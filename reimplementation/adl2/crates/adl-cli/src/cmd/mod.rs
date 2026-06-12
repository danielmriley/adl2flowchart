//! Subcommand implementations. Each returns `Result<ExitCode, CliError>`:
//! the dispatcher prints `CliError` to stderr and exits 2 (usage/IO),
//! while the subcommands choose their own success/diagnostic exit codes.

pub mod bridges;
pub mod check;
pub mod dot;
pub mod objects;
pub mod run;
pub mod verify;

use std::fmt;
use std::path::Path;

/// A usage- or IO-level failure (exit 2). Analysis/parse outcomes are NOT
/// errors here — they are reported with their own exit codes.
#[derive(Debug)]
pub enum CliError {
    Io {
        path: String,
        source: std::io::Error,
    },
    Write {
        path: String,
        source: std::io::Error,
    },
    Usage(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Io { path, source } => write!(f, "cannot read {path}: {source}"),
            CliError::Write { path, source } => write!(f, "cannot write {path}: {source}"),
            CliError::Usage(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for CliError {}

/// Read a file to a string, mapping IO errors to [`CliError::Io`].
pub fn read_file(path: &Path) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|source| CliError::Io {
        path: path.display().to_string(),
        source,
    })
}

/// The display name for a file in diagnostics: the file's own name, or the
/// full path if it has none.
pub fn unit_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
