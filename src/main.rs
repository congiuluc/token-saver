//! `tokensaver` binary entry point. All logic lives in the library crate so the
//! `tokensaver` and `ts` binaries can share it.

use std::process::ExitCode;

fn main() -> ExitCode {
    tokensaver::run()
}
