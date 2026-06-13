//! `token-saver` binary entry point. All logic lives in the library crate so the
//! `token-saver` and `ts` binaries can share it.

use std::process::ExitCode;

fn main() -> ExitCode {
    token_saver::run()
}
