//! Error handling shared by the tools' `main` functions.

use std::io;
use std::process::ExitCode;

/// Why a tool stopped early.
pub enum Error {
    /// Stdout was closed by the reader (e.g. `tool | head`); not a failure.
    StdoutClosed,
    /// Anything else, as a message for the user.
    Message(String),
}

pub type Result<T = ()> = std::result::Result<T, Error>;

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::Message(msg)
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::Message(msg.to_string())
    }
}

/// Lets writes to stdout use a bare `?`. Other I/O errors should be mapped to
/// a message saying what was being read or written instead.
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::BrokenPipe {
            Error::StdoutClosed
        } else {
            Error::Message(format!("failed to write to stdout: {e}"))
        }
    }
}

/// Runs a tool's body, reporting any error as `tool: message` on stderr with
/// a failing exit code.
pub fn run(tool: &str, body: impl FnOnce() -> Result) -> ExitCode {
    match body() {
        Ok(()) | Err(Error::StdoutClosed) => ExitCode::SUCCESS,
        Err(Error::Message(msg)) => {
            eprintln!("{tool}: {msg}");
            ExitCode::FAILURE
        }
    }
}
