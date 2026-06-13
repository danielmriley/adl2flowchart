//! Throughput benchmark for the streaming, chunked, deterministically-
//! merged event loop (SPEC_EVENT_PIPELINE §5). Synthetic seeded events
//! (the adl-difftest toy generator) flow through a representative ADL
//! (ex02_histograms: three histogram forms + a multi-step cutflow) using
//! the exact primitives the `smash2 run` path uses — the streaming
//! [`ChunkReader`], private per-worker `HistoSet`/`CutflowSet` partials,
//! and the ascending-chunk-index fold.
//!
//! Non-gating: this records throughput, never asserts it. Run with
//! `cargo bench -p adl-difftest --features bench`. Target ≥ 100k events/s
//! end-to-end at default jobs on this machine.

use std::hint::black_box;
use std::io::Cursor;
use std::sync::Mutex;

use adl_interp::{CutflowSet, HistoSet, Interp, RawChunkReader};
use adl_sema::{ExtDecls, Hir, analyze_str};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const EX02: &str = include_str!("../../../../../examples/tutorials/ex02_histograms.adl");

/// Mirror of the CLI's `run_streaming` core, minus the per-event stdout
/// formatting (the benchmark measures the loop, not output rendering). The
/// fold order and merge are identical to production. Returns
/// `(events, fingerprint)` — the fingerprint sums every merged histogram's
/// `fEntries` so the optimizer cannot elide the accumulation.
fn run_loop<'h>(
    jsonl: &[u8],
    ext: &'h ExtDecls,
    interp: &Interp<'h>,
    hir: &'h Hir,
    src: &str,
    jobs: usize,
) -> (u64, u64) {
    let source = Mutex::new(RawChunkReader::new(Cursor::new(jsonl)));
    let (tx, rx) = std::sync::mpsc::channel::<(usize, HistoSet<'h>, CutflowSet, u64)>();
    let mut master_h = HistoSet::new(hir);
    let mut master_c = CutflowSet::new(hir, src);
    let mut total = 0u64;

    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            let tx = tx.clone();
            let source = &source;
            scope.spawn(move || {
                loop {
                    let raw = {
                        let mut g = source.lock().expect("reader");
                        match g.next() {
                            Some(Ok(c)) => c,
                            _ => break,
                        }
                    };
                    let chunk = raw.parse(ext).expect("synthetic events parse");
                    let mut h = HistoSet::new(hir);
                    let mut c = CutflowSet::new(hir, src);
                    for se in &chunk.events {
                        let (results, traces) = interp.run_event_traced(&se.event);
                        c.record_event(&se.event, &results, &traces);
                        h.fill_event(interp, &se.event, &results);
                    }
                    let n = chunk.events.len() as u64;
                    if tx.send((chunk.index, h, c, n)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut buf: std::collections::HashMap<usize, (HistoSet<'h>, CutflowSet, u64)> =
            std::collections::HashMap::new();
        let mut next = 0usize;
        for (idx, h, c, n) in rx {
            buf.insert(idx, (h, c, n));
            while let Some((h, c, n)) = buf.remove(&next) {
                master_h.merge(&h);
                master_c.merge(&c);
                total += n;
                next += 1;
            }
        }
    });

    let fingerprint = master_h.histos.iter().map(|f| f.hist.entries()).sum();
    (total, fingerprint)
}

fn bench_event_loop(crit: &mut Criterion) {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(EX02, "ex02_histograms.adl", &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "ex02 must resolve cleanly for the benchmark"
    );

    let n_events = 200_000usize;
    let jsonl = adl_difftest::toy_jsonl(0x5EED_0612, n_events);
    let bytes = jsonl.into_bytes();
    let interp = Interp::new(&hir, &ext);
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);

    let mut group = crit.benchmark_group("event_loop");
    group.throughput(Throughput::Elements(n_events as u64));
    group.sample_size(20);

    group.bench_function("serial_jobs_1", |b| {
        b.iter(|| {
            black_box(run_loop(black_box(&bytes), &ext, &interp, &hir, EX02, 1));
        });
    });
    group.bench_function(format!("parallel_jobs_{cores}"), |b| {
        b.iter(|| {
            black_box(run_loop(
                black_box(&bytes),
                &ext,
                &interp,
                &hir,
                EX02,
                cores,
            ));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_event_loop);
criterion_main!(benches);
