//! The streaming, chunked, deterministically-merged event loop
//! (SPEC_EVENT_PIPELINE §5).
//!
//! Events are pulled from a [`BufRead`](std::io::BufRead) in fixed-size
//! chunks of [`adl_interp::CHUNK_EVENTS`] consecutive events (constant,
//! independent of `--jobs`) and never buffered whole. `N = jobs` worker
//! threads pull chunks from a shared [`ChunkReader`](adl_interp::ChunkReader)
//! under a mutex, evaluate each chunk into a **private** partial accumulator
//! (`HistoSet` + `CutflowSet` + per-event output lines + pass bumps), and
//! hand the finished partial back tagged with its chunk index.
//!
//! The reduction is deterministic: a single-threaded fold on the main
//! thread merges partials **in ascending chunk index** (a reorder buffer
//! holds out-of-order completions until their turn). Because chunk
//! boundaries and the fold order are fixed, every `f64` addition sequence
//! is fixed — outputs are byte-identical for any `N`, including `N = 1`.
//! No atomics, no shared mutable accumulation. Per-event stdout lines flush
//! in the same ascending order, so the text/JSON event stream stays in
//! input order too.

use adl_interp::{CutflowSet, Event, HistoSet, Interp, RawChunkReader, RegionResult, StreamError};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Mutex;

/// What one event contributes to the per-event stdout stream, decided by
/// the caller (text table vs `--json` line) so this module stays output-
/// format agnostic. The first argument is the event's 0-based ordinal. An
/// empty string writes nothing for that event.
pub type FormatEvent<'f> = dyn Fn(usize, &Event, &[RegionResult]) -> String + Sync + 'f;

/// The fully-merged result of one streaming run, borrowing the HIR for the
/// lifetime `'h` of the accumulators.
pub struct RunOutput<'h> {
    pub histos: HistoSet<'h>,
    pub cutflow: CutflowSet,
    /// `(region name, pass count)` in first-seen (declaration) order.
    pub pass_counts: Vec<(String, usize)>,
    /// Total events processed.
    pub n_events: u64,
}

/// One worker's finished chunk: its private partials plus the formatted
/// per-event lines, in input order within the chunk.
struct ChunkResult<'h> {
    index: usize,
    n_events: u64,
    histos: HistoSet<'h>,
    cutflow: CutflowSet,
    lines: Vec<String>,
    /// Pass bumps in first-seen order within this chunk (region, count).
    pass: Vec<(String, usize)>,
}

/// Run the streaming parallel loop and the deterministic fold.
///
/// `jobs` is clamped to at least 1. `format` is invoked per event to
/// produce its stdout line; per-event lines are written to `out` in input
/// order during the fold. `make_histos`/`make_cutflow` build a fresh zero
/// accumulator (one per worker plus the master) from the shared HIR.
///
/// # Errors
/// Returns the earliest-by-chunk [`StreamError`] (I/O or malformed event)
/// seen by any worker; callers treat any error as fatal.
pub fn run_streaming<'h, R, FH, FC>(
    reader: R,
    interp: &'h Interp<'h>,
    make_histos: FH,
    make_cutflow: FC,
    format: &FormatEvent<'_>,
    jobs: usize,
    out: &mut dyn Write,
) -> Result<RunOutput<'h>, StreamError>
where
    R: BufRead + Send,
    FH: Fn() -> HistoSet<'h> + Sync,
    FC: Fn() -> CutflowSet + Sync,
{
    let jobs = jobs.max(1);
    let ext = interp.ext();
    let source = Mutex::new(RawChunkReader::new(reader));
    // The earliest-by-chunk error any worker hit; kept minimal so the
    // surfaced error is deterministic regardless of completion order.
    let first_err: Mutex<Option<(usize, StreamError)>> = Mutex::new(None);
    let (tx, rx) = std::sync::mpsc::channel::<ChunkResult<'h>>();

    let mut master_histos = make_histos();
    let mut master_cutflow = make_cutflow();
    let mut pass_counts: Vec<(String, usize)> = Vec::new();
    let mut n_events: u64 = 0;

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let tx = tx.clone();
            let source = &source;
            let first_err = &first_err;
            let make_histos = &make_histos;
            let make_cutflow = &make_cutflow;
            scope.spawn(move || {
                loop {
                    // Pull one *unparsed* chunk under the lock, then release
                    // it before the (heavier) JSON parse + evaluation —
                    // workers serialize only on the cheap line-read, so
                    // parsing and compute run fully in parallel.
                    //
                    // No early-out on a recorded error: the reader hands out
                    // chunks strictly in order, so every chunk before a bad
                    // one is already in flight and must still be parsed to
                    // surface a *possibly earlier* bad line. `record_err`
                    // keeps the minimum line, so the reported error is the
                    // earliest in the file regardless of scheduling.
                    let raw = {
                        let mut guard = source.lock().expect("chunk reader mutex");
                        match guard.next() {
                            Some(Ok(c)) => c,
                            Some(Err(e)) => {
                                record_err(first_err, usize::MAX, StreamError::Io(e));
                                break;
                            }
                            None => break,
                        }
                    };
                    let chunk = match raw.parse(ext) {
                        Ok(c) => c,
                        Err(e) => {
                            // Report by the offending 1-based line so the
                            // earliest bad line is surfaced regardless of
                            // which worker hit it first.
                            let line = e.line();
                            record_err(first_err, line, StreamError::Event(e));
                            continue;
                        }
                    };
                    let mut histos = make_histos();
                    let mut cutflow = make_cutflow();
                    let mut lines = Vec::new();
                    let mut pass: Vec<(String, usize)> = Vec::new();
                    for se in &chunk.events {
                        let event = &se.event;
                        let (results, traces) = interp.run_event_traced(event);
                        cutflow.record_event(event, &results, &traces);
                        histos.fill_event(interp, event, &results);
                        for r in &results {
                            bump(&mut pass, &r.name, matches!(r.pass, Ok(true)));
                        }
                        let s = format(se.ordinal, event, &results);
                        if !s.is_empty() {
                            lines.push(s);
                        }
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let cr = ChunkResult {
                        index: chunk.index,
                        n_events: chunk.events.len() as u64,
                        histos,
                        cutflow,
                        lines,
                        pass,
                    };
                    if tx.send(cr).is_err() {
                        break; // receiver gone (fold finished/aborted)
                    }
                }
            });
        }
        drop(tx); // close the channel once all workers hold their clones

        // Deterministic fold: merge chunks strictly in ascending index. A
        // reorder buffer parks out-of-order completions until their turn.
        let mut reorder: HashMap<usize, ChunkResult<'h>> = HashMap::new();
        let mut next = 0usize;
        for result in rx {
            reorder.insert(result.index, result);
            while let Some(cr) = reorder.remove(&next) {
                master_histos.merge(&cr.histos);
                master_cutflow.merge(&cr.cutflow);
                for (region, count) in &cr.pass {
                    bump_by(&mut pass_counts, region, *count);
                }
                n_events += cr.n_events;
                for line in &cr.lines {
                    let _ = writeln!(out, "{line}");
                }
                next += 1;
            }
        }
    });

    if let Some((_, e)) = first_err.into_inner().expect("first-error mutex") {
        return Err(e);
    }

    Ok(RunOutput {
        histos: master_histos,
        cutflow: master_cutflow,
        pass_counts,
        n_events,
    })
}

/// Record an error keyed by its 1-based source line, keeping the
/// **earliest** line — so the reported error is deterministic regardless of
/// which worker hit it first or in what order. An I/O error (line
/// `usize::MAX`) only wins if nothing earlier was recorded.
fn record_err(slot: &Mutex<Option<(usize, StreamError)>>, line: usize, e: StreamError) {
    let mut g = slot.lock().expect("first-error mutex");
    match &*g {
        Some((prev, _)) if *prev <= line => {}
        _ => *g = Some((line, e)),
    }
}

fn bump(counts: &mut Vec<(String, usize)>, name: &str, passed: bool) {
    bump_by(counts, name, usize::from(passed));
}

fn bump_by(counts: &mut Vec<(String, usize)>, name: &str, by: usize) {
    if let Some(c) = counts.iter_mut().find(|(n, _)| n == name) {
        c.1 += by;
    } else {
        counts.push((name.to_owned(), by));
    }
}
