//! `tokensaver context` — inventory the GitHub Copilot context objects available
//! to the agent (custom instructions, prompt files, agents/chat modes, skills and
//! MCP tool configs) across the current workspace and the whole device, and
//! estimate the token cost of each one.
//!
//! The report models *how* each object costs tokens, not just its raw size:
//!
//! - **Always-on** content loads into every request. This is the case for
//!   `copilot-instructions.md`, `AGENTS.md` and `*.instructions.md` files whose
//!   `applyTo` is broad (or absent).
//! - Skills, prompts and agents only contribute their **description** to the
//!   always-on "menu" the model always sees; their full body loads on demand when
//!   the object is invoked. The description-token sum is therefore an estimate of
//!   their always-on cost.
//! - MCP `mcp.json` configs are counted in full (they are small and their tool
//!   list is exposed while the server is configured).
//!
//! Token counts reuse the active `TOKENSAVER_TOKENIZER` backend, consistent with
//! `tokensaver gain` and `tokensaver tokens`.

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::tokenizer;

/// Maximum directory depth walked from any root.
const MAX_DEPTH: usize = 12;
/// Files larger than this are skipped (they are never Copilot context objects).
const MAX_FILE_BYTES: u64 = 1_000_000;
/// Directories never descended into during a workspace scan.
const WORKSPACE_PRUNE: &[&str] = &["node_modules", "target", ".git", "dist", "build", "out", "bin", "obj", ".next"];
/// Directories never descended into during a user/device scan. `node_modules` is
/// pruned to keep the device-wide walk fast; skills bundled deep inside an
/// extension's `node_modules` are therefore not counted.
const USER_PRUNE: &[&str] = &["node_modules", "target", ".git", "dist", "build", "out"];
/// Default context-window size used for budget percentages.
const DEFAULT_WINDOW: u64 = 128_000;
/// Always-on instruction files above this token count earn a warning.
const OVERSIZED_ALWAYS_ON: u64 = 2_000;

/// The kinds of Copilot context objects the inventory recognizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Instructions,
    Prompts,
    Agents,
    Skills,
    Tools,
}

impl Category {
    /// Display order of the categories in the report.
    pub(crate) const ALL: [Category; 5] =
        [Category::Instructions, Category::Prompts, Category::Agents, Category::Skills, Category::Tools];

    /// Human-readable category label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Category::Instructions => "Instructions",
            Category::Prompts => "Prompts",
            Category::Agents => "Agents",
            Category::Skills => "Skills",
            Category::Tools => "Tools",
        }
    }

    /// Lowercase machine name used in JSON output.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Category::Instructions => "instructions",
            Category::Prompts => "prompts",
            Category::Agents => "agents",
            Category::Skills => "skills",
            Category::Tools => "tools",
        }
    }
}

/// Parses a user-supplied category name (case-insensitive, singular or plural).
pub fn parse_category(value: &str) -> Option<Category> {
    let v = value.trim().to_ascii_lowercase();
    let v = v.strip_suffix('s').unwrap_or(&v);
    match v {
        "instruction" => Some(Category::Instructions),
        "prompt" => Some(Category::Prompts),
        "agent" | "chatmode" => Some(Category::Agents),
        "skill" => Some(Category::Skills),
        "tool" | "mcp" => Some(Category::Tools),
        _ => None,
    }
}

/// One discovered context object with its token accounting.
struct Found {
    category: Category,
    path: PathBuf,
    full_tokens: u64,
    always_on_tokens: u64,
    bytes: u64,
    description: Option<String>,
    servers: Option<usize>,
    /// Number of tools an agent/chat-mode declares in its `tools:` frontmatter.
    tools_declared: Option<usize>,
    duplicate: bool,
}

/// Scope a scan root belongs to, used by the `--workspace`/`--user` filters and to
/// decide whether `node_modules` is pruned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Workspace,
    User,
}

/// A directory to scan, with its scope.
struct Root {
    path: PathBuf,
    scope: Scope,
}

/// A unit of parallel scanning work: walk `dir` recursively starting at `depth`.
struct Task {
    dir: PathBuf,
    prune: &'static [&'static str],
    depth: usize,
}

/// Parsed command-line options for `tokensaver context`.
struct Options {
    category: Option<Category>,
    top: usize,
    window: u64,
    scope: Option<Scope>,
    json: bool,
    /// Optional path to write a Markdown report to.
    md: Option<PathBuf>,
    /// Suppress progress messages on stderr.
    quiet: bool,
}

/// Runs the `context` subcommand.
pub fn run(args: &[String]) -> ExitCode {
    let opts = match parse_args(args) {
        Ok(opts) => opts,
        Err(msg) => {
            eprintln!("tokensaver: {msg}");
            eprintln!(
                "usage: tokensaver context [category] [--category <name>] [--top N] \
                 [--window N] [--workspace|--user] [--json] [--md <file>] [--quiet]"
            );
            return ExitCode::from(2);
        }
    };

    let roots = gather_roots(opts.scope);
    let candidates = scan_roots(&roots, &opts);

    let mut found = build_found(&candidates, opts.category, opts.quiet);
    mark_duplicates(&mut found);

    if let Some(path) = &opts.md {
        let markdown = build_markdown(&found, &opts, roots.len());
        match fs::write(path, markdown) {
            Ok(()) => {
                if !opts.quiet {
                    eprintln!("tokensaver: wrote Markdown report to {}", display_path(path));
                }
            }
            Err(err) => {
                eprintln!("tokensaver: could not write {}: {err}", display_path(path));
                return ExitCode::FAILURE;
            }
        }
    }

    if opts.json {
        print_json(&found, &opts);
    } else {
        print_report(&found, &opts, roots.len());
    }
    ExitCode::SUCCESS
}

/// Scans every root in parallel and returns all candidate context-object paths.
///
/// Each root's immediate child directories are turned into independent tasks that a
/// fixed pool of worker threads walks recursively, so large trees (such as the VS
/// Code extensions folders) fan out across all available cores.
fn scan_roots(roots: &[Root], opts: &Options) -> Vec<PathBuf> {
    let started = Instant::now();
    let progress = !opts.quiet;

    let mut tasks: Vec<Task> = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Seed: classify files sitting directly in each root and expand child dirs into
    // independent tasks. This top-level pass is cheap (a handful of directories).
    for root in roots {
        let prune: &'static [&'static str] = match root.scope {
            Scope::Workspace => WORKSPACE_PRUNE,
            Scope::User => USER_PRUNE,
        };
        if progress {
            eprintln!("tokensaver: scanning {}", display_path(&root.path));
        }
        let Ok(entries) = fs::read_dir(&root.path) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let name = entry.file_name();
                if prune.contains(&name.to_string_lossy().as_ref()) {
                    continue;
                }
                tasks.push(Task { dir: entry.path(), prune, depth: 1 });
            } else if file_type.is_file() && classify(&entry.path()).is_some() {
                candidates.push(entry.path());
            }
        }
    }

    let worker_count =
        thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16).min(tasks.len().max(1));

    if progress {
        eprintln!("tokensaver: walking {} directories with {} workers…", fmt_num(tasks.len() as u64), worker_count);
    }

    let queue = Arc::new(Mutex::new(tasks));
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        handles.push(thread::spawn(move || loop {
            let task = {
                let mut guard = queue.lock().unwrap();
                guard.pop()
            };
            let Some(task) = task else {
                break;
            };
            let mut local = Vec::new();
            walk(&task.dir, task.depth, task.prune, &mut local);
            if !local.is_empty() {
                let _ = tx.send(local);
            }
        }));
    }
    drop(tx);

    for batch in rx {
        candidates.extend(batch);
    }
    for handle in handles {
        let _ = handle.join();
    }

    if progress {
        eprintln!(
            "tokensaver: found {} candidate objects in {} ms",
            fmt_num(candidates.len() as u64),
            started.elapsed().as_millis()
        );
    }

    candidates
}

/// Parses the argument vector for the `context` subcommand.
fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut opts =
        Options { category: None, top: 5, window: DEFAULT_WINDOW, scope: None, json: false, md: None, quiet: false };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--category" | "-c" => {
                let value = args.get(i + 1).ok_or("--category requires a name")?;
                opts.category = Some(parse_category(value).ok_or_else(|| format!("unknown category '{value}'"))?);
                i += 2;
            }
            "--top" => {
                let value = args.get(i + 1).ok_or("--top requires a number")?;
                opts.top = value.parse().map_err(|_| "--top requires a number".to_string())?;
                i += 2;
            }
            "--window" => {
                let value = args.get(i + 1).ok_or("--window requires a number")?;
                opts.window = value.parse().map_err(|_| "--window requires a number".to_string())?;
                if opts.window == 0 {
                    return Err("--window must be greater than 0".to_string());
                }
                i += 2;
            }
            "--workspace" | "-w" => {
                opts.scope = Some(Scope::Workspace);
                i += 1;
            }
            "--user" | "-u" => {
                opts.scope = Some(Scope::User);
                i += 1;
            }
            "--json" => {
                opts.json = true;
                i += 1;
            }
            "--md" | "--out" | "-o" => {
                let value = args.get(i + 1).ok_or("--md requires a file path")?;
                opts.md = Some(PathBuf::from(value));
                i += 2;
            }
            "--quiet" | "-q" => {
                opts.quiet = true;
                i += 1;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown context option '{other}'"));
            }
            other => {
                if opts.category.is_some() {
                    return Err(format!("unexpected argument '{other}'"));
                }
                opts.category = Some(parse_category(other).ok_or_else(|| format!("unknown category '{other}'"))?);
                i += 1;
            }
        }
    }
    Ok(opts)
}

/// Builds the list of directories to scan for the requested scope filter.
fn gather_roots(scope: Option<Scope>) -> Vec<Root> {
    let mut roots: Vec<Root> = Vec::new();

    let want_workspace = scope != Some(Scope::User);
    let want_user = scope != Some(Scope::Workspace);

    if want_workspace {
        if let Ok(cwd) = env::current_dir() {
            roots.push(Root { path: cwd, scope: Scope::Workspace });
        }
    }

    if want_user {
        for path in user_roots() {
            if path.exists() {
                roots.push(Root { path, scope: Scope::User });
            }
        }
    }

    roots
}

/// Returns the candidate user/device roots for the current platform.
fn user_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let Some(home) = home_dir() else {
        return roots;
    };

    roots.push(home.join(".copilot"));
    roots.push(home.join(".agents"));
    roots.push(home.join(".vscode").join("extensions"));
    roots.push(home.join(".vscode-insiders").join("extensions"));

    for dir in vscode_prompt_dirs(&home) {
        roots.push(dir);
    }
    for dir in installed_app_skill_dirs() {
        roots.push(dir);
    }

    roots
}

/// VS Code "User/prompts" directories for stable and Insiders builds.
fn vscode_prompt_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let names = ["Code", "Code - Insiders"];

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            let base = PathBuf::from(appdata);
            for name in names {
                dirs.push(base.join(name).join("User").join("prompts"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library").join("Application Support");
        for name in names {
            dirs.push(base.join(name).join("User").join("prompts"));
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let base = home.join(".config");
        for name in names {
            dirs.push(base.join(name).join("User").join("prompts"));
        }
    }

    let _ = (home, names);
    dirs
}

/// Best-effort directories holding skills bundled with the installed VS Code app.
fn installed_app_skill_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    const SUBPATH: [&str; 7] = ["resources", "app", "extensions", "copilot", "assets", "prompts", "skills"];

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            let programs = PathBuf::from(local).join("Programs");
            for name in ["Microsoft VS Code", "Microsoft VS Code Insiders"] {
                let base = programs.join(name);
                // Some installs nest a version-hash directory before `resources`.
                if let Ok(entries) = fs::read_dir(&base) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            dirs.push(join_all(&entry.path(), &SUBPATH));
                        }
                    }
                }
                dirs.push(join_all(&base, &SUBPATH));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for name in ["Visual Studio Code", "Visual Studio Code - Insiders"] {
            let base = PathBuf::from("/Applications").join(format!("{name}.app")).join("Contents");
            dirs.push(join_all(&base, &SUBPATH));
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        for base in ["/usr/share/code", "/usr/share/code-insiders"] {
            dirs.push(join_all(Path::new(base), &SUBPATH));
        }
    }

    dirs
}

/// Joins each segment of `parts` onto `base`.
fn join_all(base: &Path, parts: &[&str]) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in parts {
        out.push(part);
    }
    out
}

/// Recursively collects context-object files under `dir`, honoring depth and prune
/// rules. Symlinks are not followed.
fn walk(dir: &Path, depth: usize, prune: &[&str], out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if prune.contains(&name.as_ref()) {
                continue;
            }
            walk(&entry.path(), depth + 1, prune, out);
        } else if file_type.is_file() && classify(&entry.path()).is_some() {
            out.push(entry.path());
        }
    }
}

/// Classifies a file path into a context category by its name, or `None`.
pub(crate) fn classify(path: &Path) -> Option<Category> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    if name == "copilot-instructions.md" || name == "agents.md" {
        return Some(Category::Instructions);
    }
    if name == "mcp.json" {
        return Some(Category::Tools);
    }
    if name == "skill.md" {
        return Some(Category::Skills);
    }
    if name.ends_with(".instructions.md") {
        return Some(Category::Instructions);
    }
    if name.ends_with(".prompt.md") {
        return Some(Category::Prompts);
    }
    if name.ends_with(".chatmode.md") || name.ends_with(".agent.md") {
        return Some(Category::Agents);
    }
    None
}

/// Reads and analyzes each unique candidate path into a [`Found`].
fn build_found(candidates: &[PathBuf], filter: Option<Category>, quiet: bool) -> Vec<Found> {
    let total = candidates.len();
    if total == 0 {
        return Vec::new();
    }

    if !quiet {
        eprintln!("tokensaver: reading and counting tokens for {} candidate objects\u{2026}", total);
    }
    let started = Instant::now();

    // Analyze each candidate (read + tokenize) in parallel; tokenization is the
    // dominant cost so this scales close to linearly with available cores. Results
    // are written into per-index slots so the final output stays deterministic
    // regardless of completion order.
    let worker_count = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16).min(total);

    let mut slots: Vec<Option<(PathBuf, Found)>> = (0..total).map(|_| None).collect();
    let next = AtomicUsize::new(0);

    thread::scope(|scope| {
        let (tx, rx) = mpsc::channel::<(usize, Option<(PathBuf, Found)>)>();
        let next = &next;
        for _ in 0..worker_count {
            let tx = tx.clone();
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                let _ = tx.send((i, analyze_one(&candidates[i], filter)));
            });
        }
        drop(tx);

        let mut processed = 0usize;
        let mut kept = 0usize;
        // Report progress at ~10% milestones, throttled so we never spam the terminal.
        let mut next_milestone = 10usize;
        let mut last_tick = Instant::now();
        for (i, analyzed) in rx {
            processed += 1;
            if analyzed.is_some() {
                kept += 1;
            }
            slots[i] = analyzed;
            if !quiet {
                let percent = processed * 100 / total;
                if percent >= next_milestone && last_tick.elapsed().as_millis() >= 250 {
                    eprintln!(
                        "tokensaver: analyzed {percent}% ({processed}/{total} objects, \
                         {kept} kept)\u{2026}"
                    );
                    next_milestone = (percent / 10) * 10 + 10;
                    last_tick = Instant::now();
                }
            }
        }
    });

    // Stitch results back together in candidate order, dropping exact duplicates
    // (distinct paths that canonicalize to the same real file).
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut found: Vec<Found> = Vec::with_capacity(slots.len());
    for (canonical, item) in slots.into_iter().flatten() {
        if seen.insert(canonical) {
            found.push(item);
        }
    }

    if !quiet {
        eprintln!(
            "tokensaver: analyzed {} objects, kept {} in {} ms",
            total,
            found.len(),
            started.elapsed().as_millis()
        );
    }

    found
}

/// Reads and analyzes a single candidate file, returning its canonical path and
/// the computed [`Found`] entry, or `None` if it is not a context object, is too
/// large, or cannot be read. Safe to call from worker threads.
fn analyze_one(path: &Path, filter: Option<Category>) -> Option<(PathBuf, Found)> {
    let category = classify(path)?;
    if let Some(want) = filter {
        if category != want {
            return None;
        }
    }
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;

    let full_tokens = tokens_of(&content);
    let front = parse_frontmatter(&content);
    let description = front.get("description").cloned();
    let apply_to = front.get("applyto").cloned();
    let desc_tokens = description.as_deref().map(tokens_of).unwrap_or(0);
    let servers = if category == Category::Tools { Some(count_servers(&content)) } else { None };
    let tools_declared = if category == Category::Agents { agent_tools(&content) } else { None };
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let always_on_tokens = always_on(category, &name, full_tokens, desc_tokens, apply_to.as_deref());

    Some((
        canonical,
        Found {
            category,
            path: path.to_path_buf(),
            full_tokens,
            always_on_tokens,
            bytes: meta.len(),
            description,
            servers,
            tools_declared,
            duplicate: false,
        },
    ))
}

/// Token count for `text` using the active tokenizer backend.
fn tokens_of(text: &str) -> u64 {
    tokenizer::select_active(tokenizer::estimate(text))
}

/// Computes the always-on token cost of an object given its category and metadata.
fn always_on(category: Category, name: &str, full_tokens: u64, desc_tokens: u64, apply_to: Option<&str>) -> u64 {
    match category {
        Category::Instructions => {
            if name == "copilot-instructions.md" || name == "agents.md" {
                full_tokens
            } else {
                match apply_to {
                    None => full_tokens,
                    Some(value) if is_broad_apply_to(value) => full_tokens,
                    Some(_) => 0,
                }
            }
        }
        Category::Skills | Category::Prompts | Category::Agents => desc_tokens,
        Category::Tools => full_tokens,
    }
}

/// Returns whether an `applyTo` glob applies to every file (so the instruction is
/// effectively always loaded).
fn is_broad_apply_to(value: &str) -> bool {
    let v = value.trim().trim_matches(['"', '\'']).trim();
    matches!(v, "" | "*" | "**" | "**/*")
}

/// Extracts simple `key: value` pairs from a leading YAML frontmatter block.
///
/// Keys are lowercased. Only the first `---`-delimited block at the very start of
/// the file is read, and only single-line scalar values are captured.
fn parse_frontmatter(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return map;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            if key.is_empty() || key.contains(' ') {
                continue;
            }
            let value = value.trim().trim_matches(['"', '\'']).trim();
            if !value.is_empty() {
                map.insert(key, value.to_string());
            }
        }
    }
    map
}

/// Approximate count of servers declared in an `mcp.json` config.
fn count_servers(content: &str) -> usize {
    content.matches("\"command\"").count() + content.matches("\"url\"").count()
}

/// Counts the tools an agent / chat-mode file declares in its `tools:` frontmatter.
///
/// Handles both the inline-array form (`tools: ['a', 'b']`) and the YAML block-list
/// form (a `tools:` line followed by `- item` entries). Returns `None` when the file
/// has no frontmatter `tools` key, and `Some(0)` when it declares an empty list.
fn agent_tools(content: &str) -> Option<usize> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return None;
    }

    let mut block: Vec<&str> = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        block.push(line);
    }

    let pos = block.iter().position(|line| {
        let t = line.trim_start();
        t == "tools:" || t.starts_with("tools:")
    })?;
    let header = block[pos].trim_start();
    let inline = header["tools:".len()..].trim();

    if !inline.is_empty() {
        // Inline array such as `tools: ['codebase', 'search', fetch]`.
        let inner = inline.trim_start_matches('[').trim_end_matches(']').trim();
        if inner.is_empty() {
            return Some(0);
        }
        return Some(inner.split(',').filter(|p| !p.trim().is_empty()).count());
    }

    // Block list: count following `- item` lines.
    let mut count = 0;
    for line in &block[pos + 1..] {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed == "-" {
            count += 1;
        } else if !line.trim().is_empty() {
            break;
        }
    }
    Some(count)
}

/// Flags objects that share an identity (same skill folder or file name) across
/// more than one location as duplicates.
fn mark_duplicates(found: &mut [Found]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for item in found.iter() {
        *counts.entry(identity(item)).or_insert(0) += 1;
    }
    for item in found.iter_mut() {
        if counts.get(&identity(item)).copied().unwrap_or(0) > 1 {
            item.duplicate = true;
        }
    }
}

/// Stable identity used for duplicate detection.
fn identity(item: &Found) -> String {
    if item.category == Category::Skills {
        item.path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "skill".to_string())
    } else {
        item.path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
    }
}

/// Renders the compact, grouped text report.
fn print_report(found: &[Found], opts: &Options, root_count: usize) {
    println!("tokensaver — Copilot context inventory");
    println!("  tokenizer:    {}", tokenizer::active_mode().label());
    println!("  scanned:      {} files across {} roots", found.len(), root_count);
    println!("  window:       {} tokens", fmt_num(opts.window));
    if let Some(cat) = opts.category {
        println!("  category:     {}", cat.label());
    }

    let mut always_on_total: u64 = 0;
    let mut full_total: u64 = 0;

    for category in Category::ALL {
        if let Some(want) = opts.category {
            if category != want {
                continue;
            }
        }
        let mut items: Vec<&Found> = found.iter().filter(|f| f.category == category).collect();
        if items.is_empty() {
            continue;
        }
        items.sort_by_key(|f| std::cmp::Reverse(f.full_tokens));

        let cat_full: u64 = items.iter().map(|f| f.full_tokens).sum();
        let cat_always: u64 = items.iter().map(|f| f.always_on_tokens).sum();
        always_on_total += cat_always;
        full_total += cat_full;

        let suffix = match category {
            Category::Skills | Category::Prompts | Category::Agents => {
                format!("{} menu", fmt_num(cat_always))
            }
            _ => format!("{} always-on", fmt_num(cat_always)),
        };
        println!();
        println!("{} ({} files, {} tokens; {})", category.label(), items.len(), fmt_num(cat_full), suffix);
        for item in items {
            let mut tags = String::new();
            if item.always_on_tokens > 0 && matches!(category, Category::Instructions | Category::Tools) {
                tags.push_str("  [always-on]");
            }
            if let Some(n) = item.servers {
                tags.push_str(&format!("  ({n} servers)"));
            }
            if let Some(n) = item.tools_declared {
                tags.push_str(&format!("  ({n} tools)"));
            }
            if item.duplicate {
                tags.push_str("  [dup]");
            }
            println!("  {:>8}  {}{}", fmt_num(item.full_tokens), display_path(&item.path), tags);
        }
    }

    print_summary(found, opts, always_on_total, full_total);
}

/// Prints the always-on baseline, budget percentages, top consumers and warnings.
fn print_summary(found: &[Found], opts: &Options, always_on_total: u64, full_total: u64) {
    println!();
    println!("Summary");
    println!(
        "  always-on baseline:  {} tokens  ({} of {})",
        fmt_num(always_on_total),
        pct(always_on_total, opts.window),
        fmt_num(opts.window)
    );
    println!(
        "  full catalog:        {} tokens  ({} of {})",
        fmt_num(full_total),
        pct(full_total, opts.window),
        fmt_num(opts.window)
    );

    if opts.top > 0 {
        let mut top: Vec<&Found> = found.iter().collect();
        top.sort_by_key(|f| std::cmp::Reverse(f.full_tokens));
        let shown = top.len().min(opts.top);
        if shown > 0 {
            println!("  top consumers:");
            for item in top.into_iter().take(shown) {
                println!("    {:>8}  {}", fmt_num(item.full_tokens), display_path(&item.path));
            }
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    for item in found {
        if item.category == Category::Instructions && item.always_on_tokens >= OVERSIZED_ALWAYS_ON {
            warnings.push(format!(
                "{} is {} tokens and loads on every request",
                display_path(&item.path),
                fmt_num(item.always_on_tokens)
            ));
        }
    }
    let mut dup_seen: HashSet<String> = HashSet::new();
    for item in found.iter().filter(|f| f.duplicate) {
        let id = identity(item);
        if dup_seen.insert(id.clone()) {
            let count = found.iter().filter(|f| f.duplicate && identity(f) == id).count();
            warnings.push(format!(
                "duplicate {} '{}' in {} locations",
                item.category.label().to_ascii_lowercase(),
                id,
                count
            ));
        }
    }
    if !warnings.is_empty() {
        println!("  warnings:");
        for warning in warnings {
            println!("    ! {warning}");
        }
    }
}

/// Builds a Markdown report of the inventory for export with `--md`.
fn build_markdown(found: &[Found], opts: &Options, root_count: usize) -> String {
    let mut always_on_total: u64 = 0;
    let mut full_total: u64 = 0;
    for item in found {
        if let Some(want) = opts.category {
            if item.category != want {
                continue;
            }
        }
        always_on_total += item.always_on_tokens;
        full_total += item.full_tokens;
    }

    let mut md = String::new();
    md.push_str("# Copilot context inventory\n\n");
    md.push_str(&format!("- **Tokenizer:** {}\n", tokenizer::active_mode().label()));
    md.push_str(&format!("- **Scanned:** {} objects across {} roots\n", fmt_num(found.len() as u64), root_count));
    md.push_str(&format!("- **Window:** {} tokens\n", fmt_num(opts.window)));
    if let Some(cat) = opts.category {
        md.push_str(&format!("- **Category filter:** {}\n", cat.label()));
    }
    md.push_str(&format!(
        "- **Always-on baseline:** {} tokens ({} of window)\n",
        fmt_num(always_on_total),
        pct(always_on_total, opts.window)
    ));
    md.push_str(&format!(
        "- **Full catalog:** {} tokens ({} of window)\n",
        fmt_num(full_total),
        pct(full_total, opts.window)
    ));

    for category in Category::ALL {
        if let Some(want) = opts.category {
            if category != want {
                continue;
            }
        }
        let mut items: Vec<&Found> = found.iter().filter(|f| f.category == category).collect();
        if items.is_empty() {
            continue;
        }
        items.sort_by_key(|f| std::cmp::Reverse(f.full_tokens));
        let cat_full: u64 = items.iter().map(|f| f.full_tokens).sum();

        md.push_str(&format!("\n## {} ({} files, {} tokens)\n\n", category.label(), items.len(), fmt_num(cat_full)));
        md.push_str("| Full tokens | Always-on | Notes | Path |\n");
        md.push_str("| ---: | ---: | --- | --- |\n");
        for item in items {
            let mut notes: Vec<String> = Vec::new();
            if let Some(n) = item.servers {
                notes.push(format!("{n} servers"));
            }
            if let Some(n) = item.tools_declared {
                notes.push(format!("{n} tools"));
            }
            if item.duplicate {
                notes.push("duplicate".to_string());
            }
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                fmt_num(item.full_tokens),
                fmt_num(item.always_on_tokens),
                if notes.is_empty() { "—".to_string() } else { notes.join(", ") },
                display_path(&item.path)
            ));
        }
    }

    if opts.top > 0 {
        let mut top: Vec<&Found> = found.iter().collect();
        top.sort_by_key(|f| std::cmp::Reverse(f.full_tokens));
        let shown = top.len().min(opts.top);
        if shown > 0 {
            md.push_str("\n## Top consumers\n\n");
            md.push_str("| Full tokens | Path |\n| ---: | --- |\n");
            for item in top.into_iter().take(shown) {
                md.push_str(&format!("| {} | {} |\n", fmt_num(item.full_tokens), display_path(&item.path)));
            }
        }
    }

    md
}

/// Emits a machine-readable JSON document of the inventory.
fn print_json(found: &[Found], opts: &Options) {
    let mut out = String::from("{");
    out.push_str(&format!("\"tokenizer\":\"{}\",", tokenizer::active_mode().label()));
    out.push_str(&format!("\"window\":{},", opts.window));

    let always_on_total: u64 = found.iter().map(|f| f.always_on_tokens).sum();
    let full_total: u64 = found.iter().map(|f| f.full_tokens).sum();
    out.push_str(&format!("\"alwaysOnTokens\":{always_on_total},"));
    out.push_str(&format!("\"fullTokens\":{full_total},"));

    out.push_str("\"items\":[");
    for (i, item) in found.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!("\"category\":\"{}\",", item.category.key()));
        out.push_str(&format!("\"path\":\"{}\",", json_escape(&display_path(&item.path))));
        out.push_str(&format!("\"fullTokens\":{},", item.full_tokens));
        out.push_str(&format!("\"alwaysOnTokens\":{},", item.always_on_tokens));
        out.push_str(&format!("\"bytes\":{},", item.bytes));
        out.push_str(&format!("\"duplicate\":{}", item.duplicate));
        if let Some(n) = item.servers {
            out.push_str(&format!(",\"servers\":{n}"));
        }
        if let Some(n) = item.tools_declared {
            out.push_str(&format!(",\"tools\":{n}"));
        }
        if let Some(desc) = &item.description {
            out.push_str(&format!(",\"description\":\"{}\"", json_escape(desc)));
        }
        out.push('}');
    }
    out.push_str("]}");
    println!("{out}");
}

/// Escapes a string for inclusion in a JSON string literal.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Renders a path relative to the cwd or home directory for compact display.
fn display_path(path: &Path) -> String {
    if let Ok(cwd) = env::current_dir() {
        if path == cwd {
            return ".".to_string();
        }
        if let Ok(rel) = path.strip_prefix(&cwd) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    if let Some(home) = home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.to_string_lossy().replace('\\', "/"));
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

/// Formats `n` with thousands separators.
fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Formats `n` as a percentage of `window`.
fn pct(n: u64, window: u64) -> String {
    if window == 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", n as f64 / window as f64 * 100.0)
}

/// Returns the user's home directory, honoring `USERPROFILE` then `HOME`.
pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_maps_known_filenames() {
        assert_eq!(classify(Path::new("x/copilot-instructions.md")), Some(Category::Instructions));
        assert_eq!(classify(Path::new("AGENTS.md")), Some(Category::Instructions));
        assert_eq!(classify(Path::new("a/react.instructions.md")), Some(Category::Instructions));
        assert_eq!(classify(Path::new("a/build.prompt.md")), Some(Category::Prompts));
        assert_eq!(classify(Path::new("a/plan.chatmode.md")), Some(Category::Agents));
        assert_eq!(classify(Path::new("a/explore.agent.md")), Some(Category::Agents));
        assert_eq!(classify(Path::new("skills/docx/SKILL.md")), Some(Category::Skills));
        assert_eq!(classify(Path::new(".vscode/mcp.json")), Some(Category::Tools));
        assert_eq!(classify(Path::new("src/main.rs")), None);
        assert_eq!(classify(Path::new("README.md")), None);
    }

    #[test]
    fn parse_category_accepts_singular_plural_and_case() {
        assert_eq!(parse_category("agents"), Some(Category::Agents));
        assert_eq!(parse_category("Agent"), Some(Category::Agents));
        assert_eq!(parse_category("SKILLS"), Some(Category::Skills));
        assert_eq!(parse_category("instruction"), Some(Category::Instructions));
        assert_eq!(parse_category("mcp"), Some(Category::Tools));
        assert_eq!(parse_category("nope"), None);
    }

    #[test]
    fn frontmatter_extracts_description_and_applyto() {
        let content = "---\ndescription: A test skill\napplyTo: \"**/*.ts\"\n---\n# Body\n";
        let front = parse_frontmatter(content);
        assert_eq!(front.get("description").map(String::as_str), Some("A test skill"));
        assert_eq!(front.get("applyto").map(String::as_str), Some("**/*.ts"));
    }

    #[test]
    fn frontmatter_absent_returns_empty() {
        assert!(parse_frontmatter("# Just a heading\n").is_empty());
    }

    #[test]
    fn broad_apply_to_detection() {
        assert!(is_broad_apply_to("**"));
        assert!(is_broad_apply_to("\"**\""));
        assert!(is_broad_apply_to("**/*"));
        assert!(is_broad_apply_to("*"));
        assert!(!is_broad_apply_to("**/*.ts"));
        assert!(!is_broad_apply_to("src/**"));
    }

    #[test]
    fn always_on_rules_per_category() {
        // copilot-instructions.md is always full.
        assert_eq!(always_on(Category::Instructions, "copilot-instructions.md", 100, 10, None), 100);
        // scoped instructions contribute nothing always-on.
        assert_eq!(always_on(Category::Instructions, "x.instructions.md", 100, 10, Some("**/*.ts")), 0);
        // broad instructions are always-on in full.
        assert_eq!(always_on(Category::Instructions, "x.instructions.md", 100, 10, Some("**")), 100);
        // skills contribute only their description.
        assert_eq!(always_on(Category::Skills, "skill.md", 5000, 42, None), 42);
        // tools count in full.
        assert_eq!(always_on(Category::Tools, "mcp.json", 80, 0, None), 80);
    }

    #[test]
    fn count_servers_counts_command_and_url() {
        let json = r#"{"servers":{"a":{"command":"x"},"b":{"url":"y"}}}"#;
        assert_eq!(count_servers(json), 2);
    }

    #[test]
    fn agent_tools_counts_inline_and_block_lists() {
        let inline = "---\ndescription: x\ntools: ['codebase', 'search', fetch]\n---\n# Body\n";
        assert_eq!(agent_tools(inline), Some(3));

        let block = "---\ntools:\n  - codebase\n  - search\n  - edit\ndescription: x\n---\n";
        assert_eq!(agent_tools(block), Some(3));

        let empty = "---\ntools: []\n---\n";
        assert_eq!(agent_tools(empty), Some(0));

        let none = "---\ndescription: x\n---\n";
        assert_eq!(agent_tools(none), None);

        assert_eq!(agent_tools("# no frontmatter\n"), None);
    }

    #[test]
    fn fmt_num_groups_thousands() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(999), "999");
        assert_eq!(fmt_num(1000), "1,000");
        assert_eq!(fmt_num(1234567), "1,234,567");
    }

    #[test]
    fn duplicate_detection_flags_repeated_skill_folders() {
        let mut found = vec![
            Found {
                category: Category::Skills,
                path: PathBuf::from("/a/docx/SKILL.md"),
                full_tokens: 1,
                always_on_tokens: 0,
                bytes: 1,
                description: None,
                servers: None,
                tools_declared: None,
                duplicate: false,
            },
            Found {
                category: Category::Skills,
                path: PathBuf::from("/b/docx/SKILL.md"),
                full_tokens: 1,
                always_on_tokens: 0,
                bytes: 1,
                description: None,
                servers: None,
                tools_declared: None,
                duplicate: false,
            },
            Found {
                category: Category::Skills,
                path: PathBuf::from("/c/pptx/SKILL.md"),
                full_tokens: 1,
                always_on_tokens: 0,
                bytes: 1,
                description: None,
                servers: None,
                tools_declared: None,
                duplicate: false,
            },
        ];
        mark_duplicates(&mut found);
        assert!(found[0].duplicate);
        assert!(found[1].duplicate);
        assert!(!found[2].duplicate);
    }

    #[test]
    fn walk_collects_matches_and_prunes() {
        let base = std::env::temp_dir().join(format!("tokensaver-assess-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".github")).unwrap();
        fs::create_dir_all(base.join("node_modules").join("pkg")).unwrap();
        fs::write(base.join(".github").join("copilot-instructions.md"), "x").unwrap();
        fs::write(base.join("AGENTS.md"), "x").unwrap();
        fs::write(base.join("node_modules").join("pkg").join("SKILL.md"), "x").unwrap();
        fs::write(base.join("README.md"), "x").unwrap();

        let mut out = Vec::new();
        walk(&base, 0, WORKSPACE_PRUNE, &mut out);

        let names: Vec<String> = out.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert!(names.iter().any(|n| n == "copilot-instructions.md"));
        assert!(names.iter().any(|n| n == "AGENTS.md"));
        // Pruned: SKILL.md under node_modules must not appear.
        assert!(!names.iter().any(|n| n == "SKILL.md"));
        assert!(!names.iter().any(|n| n == "README.md"));

        let _ = fs::remove_dir_all(&base);
    }
}
