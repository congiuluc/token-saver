//! `ts` is a short alias for the `tokensaver` binary; both share the library crate.

use std::process::ExitCode;

fn main() -> ExitCode {
    tokensaver::run()
}
