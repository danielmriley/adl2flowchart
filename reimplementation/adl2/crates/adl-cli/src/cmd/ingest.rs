//! `smash2 ingest` — materialize a profile-mapped ROOT event file as
//! canonical JSONL, and/or emit the independent uproot oracle script
//! (SPEC_EVENT_PIPELINE §1.1).
//!
//! - `smash2 ingest events.root --profile delphes -o events.jsonl`
//!   reads natively (oxyroot) and writes byte-deterministic JSONL.
//! - `smash2 ingest --profile delphes --emit-script DIR` writes
//!   `to_jsonl.py` (uproot 5.x), which must reproduce the native bytes
//!   exactly — the no-Rust fallback and the CI oracle.
//!
//! Mapping diagnostics go to stderr; `--verbose` adds the profile's
//! `[DECIDE]` choices and full dropped-leaf detail. Exit 1 when the file
//! cannot be ingested faithfully (the reader refuses rather than guesses).

use crate::cmd::CliError;
use adl_ingest::{IngestDiag, Profile};
use std::path::Path;
use std::process::ExitCode;

pub fn run(
    input: Option<&Path>,
    profile_name: &str,
    output: Option<&Path>,
    emit_script: Option<&Path>,
    verbose: bool,
) -> Result<ExitCode, CliError> {
    let Some(profile) = adl_ingest::by_name(profile_name) else {
        return Err(CliError::Usage(format!(
            "unknown profile `{profile_name}` (known: {})",
            adl_ingest::KNOWN_PROFILES.join(", ")
        )));
    };
    if output.is_none() && emit_script.is_none() {
        return Err(CliError::Usage(
            "nothing to do: pass `-o FILE` to materialize JSONL and/or `--emit-script DIR`"
                .to_owned(),
        ));
    }
    if output.is_some() && input.is_none() {
        return Err(CliError::Usage(
            "`-o` needs a ROOT input file to ingest".to_owned(),
        ));
    }

    if verbose {
        print_profile_choices(&profile);
    }

    if let Some(dir) = emit_script {
        std::fs::create_dir_all(dir).map_err(|source| CliError::Write {
            path: dir.display().to_string(),
            source,
        })?;
        let path = dir.join("to_jsonl.py");
        std::fs::write(&path, adl_ingest::to_jsonl_py(&profile)).map_err(|source| {
            CliError::Write {
                path: path.display().to_string(),
                source,
            }
        })?;
        if verbose {
            eprintln!("wrote {}", path.display());
        }
    }

    if let (Some(input), Some(output)) = (input, output) {
        let ingested = match adl_ingest::read_root(input, &profile) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("{}: {e}", input.display());
                return Ok(ExitCode::from(1));
            }
        };
        print_diags(&ingested.diags, &ingested.profile_id, verbose);
        std::fs::write(output, ingested.jsonl()).map_err(|source| CliError::Write {
            path: output.display().to_string(),
            source,
        })?;
        if verbose {
            eprintln!("wrote {} ({} events)", output.display(), ingested.entries);
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Surface the profile's `[DECIDE]` choices on stderr (SPEC_EVENT_PIPELINE
/// §1.2: per-run choices are explicit, here and later in §6 provenance).
pub fn print_profile_choices(profile: &Profile) {
    eprintln!("profile {}:", profile.id());
    for (key, value) in profile.decides() {
        eprintln!("  {key} = {value}");
    }
}

/// Print mapping diagnostics to stderr in their deterministic order;
/// `--verbose` includes verbose-only notes and full leaf lists.
pub fn print_diags(diags: &[IngestDiag], profile_id: &str, verbose: bool) {
    for d in diags {
        if d.verbose_only() && !verbose {
            continue;
        }
        eprintln!("profile {profile_id}: {d}");
        if verbose && let Some(detail) = d.verbose_detail() {
            eprintln!("  {detail}");
        }
    }
}
