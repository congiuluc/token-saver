//! `token-saver` — run a command and print an extremely compact summary of its output.
//!
//! Usage:
//!   token-saver <command> [args...]        Run the command and print a compact summary.
//!   token-saver -x | --extreme <command>   Run and print an even more aggressive summary.
//!   token-saver --raw <command> ...        Run the command and print its raw output unchanged.
//!   token-saver - | --stdin                Read stdin and print its compact form.
//!   token-saver init [--global|--cli]      Register token-saver with GitHub Copilot.
//!   token-saver init --hook [--global]     Install a Copilot postToolUse hook.
//!   token-saver uninit [--global|--cli]    Remove what `token-saver init` configured.
//!   token-saver uninit --hook [--global]   Remove the Copilot postToolUse hook.
//!   token-saver hook                       Run as a Copilot postToolUse hook (reads stdin).
//!   token-saver gain                       Show logged token savings.
//!   token-saver gain --reset               Reset logged token savings.
//!   token-saver tokens ...                 Calculate tokens for prompt text or file content.
//!   token-saver optimize --file <path>     Compact a file's text and report token savings.
//!   token-saver context [category]         Inventory Copilot context objects and their token cost.
//!   token-saver gallery <command>          Harvest, list, install, or serve a local Copilot object gallery.
//!   token-saver update [--check|--force]   Update token-saver to the latest release.
//!   token-saver version | --version | -V   Print the installed version.
//!   token-saver -h | --help                Show usage.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;
use std::time::Instant;

pub mod assess;
pub mod format;
pub mod gallery;
pub mod hook;
pub mod init;
pub mod metrics;
pub mod optimize;
pub mod otel;
pub mod runner;
pub mod tokenizer;
pub mod update;

/// Runs the `token-saver` CLI and returns the process exit code.
pub fn run() -> ExitCode {
    let mut args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }

    match args[0].as_str() {
        "-h" | "--help" => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" | "version" => {
            println!("token-saver {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "update" | "upgrade" | "self-update" => {
            return update::run(&args[1..]);
        }
        "gallery" | "market" | "marketplace" => {
            return gallery::run(&args[1..]);
        }
        "--raw" => {
            args.remove(0);
            if args.is_empty() {
                print_usage();
                return ExitCode::from(2);
            }
            let outcome = runner::run(&args);
            print!("{}", outcome.stdout);
            eprint!("{}", outcome.stderr);
            return exit_code(outcome.code);
        }
        "init" => {
            let global = args.iter().any(|a| a == "--global" || a == "-g");
            if args.iter().any(|a| a == "--hook" || a == "--hooks") {
                return match init::run_hook(global) {
                    Ok(path) => {
                        println!("token-saver: wrote Copilot hook config at {}", path.display());
                        ExitCode::SUCCESS
                    }
                    Err(err) => {
                        eprintln!("token-saver: init failed: {err}");
                        ExitCode::from(1)
                    }
                };
            }
            let scope = if global {
                init::Scope::Global
            } else if args.iter().any(|a| a == "--cli" || a == "--agents") {
                init::Scope::Agents
            } else {
                init::Scope::Workspace
            };
            return match init::run(scope) {
                Ok(path) => {
                    println!("token-saver: updated Copilot instructions at {}", path.display());
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("token-saver: init failed: {err}");
                    ExitCode::from(1)
                }
            };
        }
        "uninit" => {
            let global = args.iter().any(|a| a == "--global" || a == "-g");
            if args.iter().any(|a| a == "--hook" || a == "--hooks") {
                return match init::uninstall_hook(global) {
                    Ok(Some(path)) => {
                        println!("token-saver: removed Copilot hook config at {}", path.display());
                        ExitCode::SUCCESS
                    }
                    Ok(None) => {
                        println!("token-saver: no Copilot hook config found");
                        ExitCode::SUCCESS
                    }
                    Err(err) => {
                        eprintln!("token-saver: uninit failed: {err}");
                        ExitCode::from(1)
                    }
                };
            }
            let scope = if global {
                init::Scope::Global
            } else if args.iter().any(|a| a == "--cli" || a == "--agents") {
                init::Scope::Agents
            } else {
                init::Scope::Workspace
            };
            return match init::uninstall(scope) {
                Ok(Some(path)) => {
                    println!("token-saver: removed Copilot instructions from {}", path.display());
                    ExitCode::SUCCESS
                }
                Ok(None) => {
                    println!("token-saver: no managed token-saver instructions found");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("token-saver: uninit failed: {err}");
                    ExitCode::from(1)
                }
            };
        }
        "hook" => {
            let extreme = args.iter().any(|a| a == "--extreme" || a == "-x");
            return hook::run(extreme);
        }
        "gain" => {
            return run_gain(&args[1..]);
        }
        "tokens" => {
            return run_tokens(&args[1..]);
        }
        "optimize" | "opt" => {
            return optimize::run(&args[1..]);
        }
        "context" | "ctx" | "assess" | "assessment" => {
            return assess::run(&args[1..]);
        }
        _ => {}
    }

    // An optional leading `--extreme`/`-x` flag tightens generic compression.
    let extreme = matches!(args[0].as_str(), "--extreme" | "-x");
    if extreme {
        args.remove(0);
        if args.is_empty() {
            print_usage();
            return ExitCode::from(2);
        }
    }

    // Stdin filter mode: summarize piped text instead of running a command.
    if matches!(args[0].as_str(), "-" | "--stdin") {
        return run_stdin(extreme);
    }

    // Some commands are rewritten to a machine-readable variant for reliable parsing
    // (e.g. `git status` -> `git status --porcelain=v1 --branch`).
    let started = Instant::now();
    let invocation = format::rewrite(&args);
    let outcome = runner::run(&invocation);

    // Summaries are keyed off the *original* command the user typed.
    let summary = format::summarize(&args, &outcome, extreme);
    let elapsed = started.elapsed();
    print!("{summary}");

    metrics::record("run", &args.join(" "), &format!("{}{}", outcome.stdout, outcome.stderr), &summary, elapsed);

    exit_code(outcome.code)
}

/// Maps a process exit code onto a [`ExitCode`], clamping to the 0-255 range.
fn exit_code(code: i32) -> ExitCode {
    ExitCode::from((code & 0xff) as u8)
}

/// Reads all of stdin and prints its compact form. Lets you shrink a large
/// log or context blob before pasting it into a prompt, e.g.
/// `Get-Content big.log | token-saver -`.
fn run_stdin(extreme: bool) -> ExitCode {
    let mut text = String::new();
    if let Err(err) = io::stdin().read_to_string(&mut text) {
        eprintln!("token-saver: failed to read stdin: {err}");
        return ExitCode::from(1);
    }
    let started = Instant::now();
    let summary = format::summarize_text(&text, extreme);
    let elapsed = started.elapsed();
    print!("{summary}");
    metrics::record("stdin", "-", &text, &summary, elapsed);
    ExitCode::SUCCESS
}

/// Prints aggregated token savings recorded in the metrics log.
fn run_gain(args: &[String]) -> ExitCode {
    if args.is_empty() {
        return print_gain();
    }

    if args.iter().all(|arg| arg == "--reset" || arg == "reset") {
        return match metrics::reset_log() {
            Ok(metrics::ResetOutcome::Disabled) => {
                println!("token-saver: gain log is disabled (TOKEN_SAVER_LOG is off)");
                ExitCode::SUCCESS
            }
            Ok(metrics::ResetOutcome::AlreadyEmpty(path)) => {
                println!("token-saver: gain stats already empty at {}", path.display());
                ExitCode::SUCCESS
            }
            Ok(metrics::ResetOutcome::Cleared(path)) => {
                println!("token-saver: reset gain stats at {}", path.display());
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("token-saver: failed to reset gain stats: {err}");
                ExitCode::from(1)
            }
        };
    }

    eprintln!("token-saver: unknown gain option(s): {}", args.join(" "));
    eprintln!("usage: token-saver gain [--reset]");
    ExitCode::from(2)
}

/// Prints aggregated token savings recorded in the metrics log.
fn print_gain() -> ExitCode {
    let totals = metrics::read_totals();
    let active_saved = totals.raw_tokens.saturating_sub(totals.out_tokens);
    let active_pct = if totals.raw_tokens > 0 { active_saved as f64 / totals.raw_tokens as f64 * 100.0 } else { 0.0 };
    let heuristic_saved = totals.raw_tokens_heuristic.saturating_sub(totals.out_tokens_heuristic);
    let heuristic_pct = if totals.raw_tokens_heuristic > 0 {
        heuristic_saved as f64 / totals.raw_tokens_heuristic as f64 * 100.0
    } else {
        0.0
    };

    println!("token-saver — token savings");
    println!("  tokenizer:    {}", tokenizer::active_mode().label());
    println!("  invocations:  {}", totals.count);
    println!("  raw chars:    {}", totals.raw_chars);
    println!("  out chars:    {}", totals.out_chars);
    println!("  raw tokens:   {}", totals.raw_tokens);
    println!("  out tokens:   {}", totals.out_tokens);
    println!("  saved:        {active_saved} ({active_pct:.1}%)");
    println!("  heuristic raw tokens:   {}", totals.raw_tokens_heuristic);
    println!("  heuristic out tokens:   {}", totals.out_tokens_heuristic);
    println!("  heuristic saved:        {heuristic_saved} ({heuristic_pct:.1}%)");

    if totals.model_token_samples > 0 {
        let model_saved = totals.raw_tokens_model.saturating_sub(totals.out_tokens_model);
        let model_pct =
            if totals.raw_tokens_model > 0 { model_saved as f64 / totals.raw_tokens_model as f64 * 100.0 } else { 0.0 };
        println!("  model raw tokens:       {} ({} samples)", totals.raw_tokens_model, totals.model_token_samples);
        println!("  model out tokens:       {}", totals.out_tokens_model);
        println!("  model saved:            {model_saved} ({model_pct:.1}%)");
    } else {
        println!("  model tokens:           n/a");
    }

    ExitCode::SUCCESS
}

/// Calculates word counts for prompt text or file content.
fn run_tokens(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("token-saver: tokens requires one of --prompt, --file, or --stdin");
        return ExitCode::from(2);
    }

    let mut text: Option<String> = None;
    let mut source = String::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--prompt" | "-p" => {
                if i + 1 >= args.len() {
                    eprintln!("token-saver: --prompt requires a value");
                    return ExitCode::from(2);
                }
                if text.is_some() {
                    eprintln!("token-saver: use only one source: --prompt, --file, or --stdin");
                    return ExitCode::from(2);
                }
                text = Some(args[i + 1].clone());
                source = "prompt".to_string();
                i += 2;
            }
            "--file" | "-f" => {
                if i + 1 >= args.len() {
                    eprintln!("token-saver: --file requires a path");
                    return ExitCode::from(2);
                }
                if text.is_some() {
                    eprintln!("token-saver: use only one source: --prompt, --file, or --stdin");
                    return ExitCode::from(2);
                }
                let path = &args[i + 1];
                match fs::read_to_string(path) {
                    Ok(content) => {
                        text = Some(content);
                        source = format!("file:{path}");
                    }
                    Err(err) => {
                        eprintln!("token-saver: failed to read file '{path}': {err}");
                        return ExitCode::from(1);
                    }
                }
                i += 2;
            }
            "--stdin" | "-" => {
                if text.is_some() {
                    eprintln!("token-saver: use only one source: --prompt, --file, or --stdin");
                    return ExitCode::from(2);
                }
                let mut stdin_text = String::new();
                if let Err(err) = io::stdin().read_to_string(&mut stdin_text) {
                    eprintln!("token-saver: failed to read stdin: {err}");
                    return ExitCode::from(1);
                }
                text = Some(stdin_text);
                source = "stdin".to_string();
                i += 1;
            }
            unknown => {
                eprintln!("token-saver: unknown tokens option '{unknown}'");
                return ExitCode::from(2);
            }
        }
    }

    let Some(text) = text else {
        eprintln!("token-saver: tokens requires one of --prompt, --file, or --stdin");
        return ExitCode::from(2);
    };

    let words = tokenizer::count_words(&text);
    let per_line_words = tokenizer::count_words_per_line(&text);

    println!("token-saver — word count");
    println!("  source:       {source}");
    println!("  chars:        {}", text.chars().count());
    println!("  bytes:        {}", text.len());
    println!("  words:        {words}");
    println!("  lines:        {}", per_line_words.len());
    println!("  by line:");
    for (idx, line_words) in per_line_words.iter().enumerate() {
        println!("    L{}: {}", idx + 1, line_words);
    }

    ExitCode::SUCCESS
}
fn print_usage() {
    eprintln!(
        "token-saver — summarize command output\n\
         \n\
         USAGE:\n\
         \x20 token-saver <command> [args...]      Run and print a compact summary\n\
         \x20 token-saver -x | --extreme <cmd>     Run and print a maximally compressed summary\n\
         \x20 token-saver --raw <command> ...      Run and print raw output (no summary)\n\
         \x20 token-saver - | --stdin             Read stdin and print its compact form\n\
         \x20 token-saver init [--global|--cli]   Register token-saver with GitHub Copilot\n\
         \x20 token-saver init --hook [--global]  Install a Copilot postToolUse hook\n\
         \x20 token-saver uninit [--global|--cli] Remove what token-saver init configured\n\
         \x20 token-saver uninit --hook [--global] Remove the Copilot postToolUse hook\n\
         \x20 token-saver hook                    Run as a Copilot postToolUse hook (reads stdin)\n\
         \x20 token-saver gain                    Show logged token savings\n\
         \x20 token-saver gain --reset            Reset logged token savings\n\
         \x20 token-saver optimize --file <p>     Compact file text; --preview shows it + token diff\n\
         \x20 token-saver context [category]      Inventory Copilot context objects + token cost\n\
         \x20 token-saver gallery <command>      Harvest/list/install Copilot objects; serve a browser gallery\n\
         \x20 token-saver update [--check|--force] Update token-saver to the latest release\n\
         \x20 token-saver version | -V            Print the installed version\n\
         \x20 token-saver -h | --help              Show this help\n\
         \n\
         EXAMPLES:\n\
         \x20 token-saver git status\n\
         \x20 token-saver cargo test\n\
         \x20 token-saver docker ps\n\
         \x20 token-saver kubectl get pods"
    );
}
