//! Process execution: runs a child command and captures its output.

use std::process::Command;

/// The captured result of running a child process.
pub struct Outcome {
    /// Captured standard output, decoded as UTF-8 (lossy).
    pub stdout: String,
    /// Captured standard error, decoded as UTF-8 (lossy).
    pub stderr: String,
    /// The process exit code, or `-1` if it was terminated by a signal,
    /// or `127` if the program could not be launched at all.
    pub code: i32,
}

/// Runs `args[0]` with `args[1..]` as arguments, capturing stdout and stderr.
///
/// On Windows, bare commands such as `npm` are shipped as `.cmd` shims; the
/// standard library resolves these through the usual `PATH`/`PATHEXT` lookup.
pub fn run(args: &[String]) -> Outcome {
    let (program, rest) = match args.split_first() {
        Some(parts) => parts,
        None => {
            return Outcome { stdout: String::new(), stderr: "token-saver: no command given\n".to_string(), code: 2 }
        }
    };

    match Command::new(program).args(rest).output() {
        Ok(output) => Outcome {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code().unwrap_or(-1),
        },
        Err(err) => Outcome {
            stdout: String::new(),
            stderr: format!("token-saver: failed to run '{program}': {err}\n"),
            code: 127,
        },
    }
}
