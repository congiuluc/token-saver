//! `ts` is a short alias for the `token-saver` binary; both share the library crate.

use std::process::ExitCode;

fn main() -> ExitCode {
    token_saver::run()
}
