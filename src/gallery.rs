//! The `token-saver gallery` (a.k.a. `marketplace`) feature.
//!
//! Harvests *user-defined* Copilot context objects — agents, skills, prompts,
//! and custom instructions — out of the user/device folders and into a local
//! gallery so they are preserved and reusable, while leaving VS Code
//! extension-provided objects untouched.
//!
//! Subcommands:
//!
//!   token-saver gallery harvest [--apply] [--quiet]   Move user objects into the gallery (dry-run by default).
//!   token-saver gallery list [category]               List items stored in the gallery.
//!   token-saver gallery show <id>                      Show details and a content preview for one item.
//!   token-saver gallery install <id> [--dir <path>] [--force]   Install an item into a workspace.
//!   token-saver gallery remove <id>                    Delete an item from the gallery.
//!   token-saver gallery serve [--port N] [--open]      Serve a browser gallery (localhost only).
//!
//! The gallery lives at `~/.token-saver/gallery`. Each item is a self-contained
//! folder under `items/<id>/` holding the payload plus a `meta` description file.

use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::assess::{self, Category};

/// Folders that should never be harvested even when found under a user root.
const HARVEST_PRUNE: &[&str] = &[".git", "node_modules", "target", "dist", "build", "out", "token-saver-gallery"];

/// Maximum recursion depth when scanning user roots for harvest candidates.
const MAX_DEPTH: usize = 12;

/// Maximum number of bytes shown in a content preview.
const PREVIEW_BYTES: usize = 4_000;

/// Maximum number of concurrent connections the gallery server will service.
const MAX_CONNECTIONS: usize = 64;

/// Maximum request body size (bytes) the gallery server will read.
const MAX_BODY_BYTES: usize = 1_000_000;

/// Whether a gallery item is a single file or a directory tree (a skill).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::File => "file",
            Kind::Dir => "dir",
        }
    }

    fn parse(value: &str) -> Option<Kind> {
        match value {
            "file" => Some(Kind::File),
            "dir" => Some(Kind::Dir),
            _ => None,
        }
    }
}

/// A stored gallery item.
#[derive(Debug, Clone)]
struct Item {
    id: String,
    category: Category,
    name: String,
    kind: Kind,
    /// File name (for `File`) or directory name (for `Dir`) of the payload.
    entry: String,
    description: String,
    source: String,
    harvested_at: u64,
}

impl Item {
    /// Absolute path to this item's payload (the harvested file or directory).
    fn payload_path(&self, root: &Path) -> PathBuf {
        root.join("items").join(&self.id).join(&self.entry)
    }
}

/// A candidate discovered during harvest, before it is moved into the gallery.
struct Candidate {
    category: Category,
    /// Absolute path of the file (or skill directory) to harvest.
    path: PathBuf,
    kind: Kind,
    name: String,
}

/// Runs the `gallery` subcommand.
pub fn run(args: &[String]) -> ExitCode {
    let Some((sub, rest)) = args.split_first() else {
        print_help();
        return ExitCode::from(2);
    };

    match sub.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
            ExitCode::SUCCESS
        }
        "harvest" => cmd_harvest(rest),
        "list" | "ls" => cmd_list(rest),
        "show" | "info" => cmd_show(rest),
        "install" | "add" => cmd_install(rest),
        "remove" | "rm" | "delete" => cmd_remove(rest),
        "serve" | "web" | "browser" => cmd_serve(rest),
        other => {
            eprintln!("token-saver: unknown gallery command '{other}'");
            print_help();
            ExitCode::from(2)
        }
    }
}

/// Prints gallery usage.
fn print_help() {
    println!(
        "token-saver gallery — a local marketplace for your Copilot context objects\n\
         \n\
         USAGE:\n\
         \x20 token-saver gallery <command> [options]\n\
         \n\
         COMMANDS:\n\
         \x20 harvest [--apply] [--quiet]   Move user-defined agents/skills/prompts/instructions\n\
         \x20                               into the gallery. Dry-run unless --apply is given.\n\
         \x20 list [category]               List stored items (optionally filtered by category).\n\
         \x20 show <id>                     Show details and a content preview for an item.\n\
         \x20 install <id> [--dir <path>] [--force]\n\
         \x20                               Install an item into a workspace (default: current dir).\n\
         \x20 remove <id>                   Delete an item from the gallery.\n\
         \x20 serve [--port N] [--open]     Serve a browser gallery on http://127.0.0.1:7878.\n\
         \n\
         The gallery is stored at ~/.token-saver/gallery. VS Code extension-provided\n\
         objects are never harvested."
    );
}

// ---------------------------------------------------------------------------
// Gallery storage
// ---------------------------------------------------------------------------

/// Returns the gallery root directory (`~/.token-saver/gallery`), if a home is known.
fn gallery_root() -> Option<PathBuf> {
    assess::home_dir().map(|home| home.join(".token-saver").join("gallery"))
}

/// Returns the gallery root or prints an error and returns `Err`.
fn require_gallery_root() -> Result<PathBuf, ExitCode> {
    gallery_root().ok_or_else(|| {
        eprintln!("token-saver: could not determine your home directory (set HOME or USERPROFILE)");
        ExitCode::FAILURE
    })
}

/// Loads all items currently stored in the gallery, sorted by category then name.
fn load_items(root: &Path) -> Vec<Item> {
    let items_dir = root.join("items");
    let mut items = Vec::new();
    let Ok(entries) = fs::read_dir(&items_dir) else {
        return items;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if let Some(item) = load_item(root, &id) {
            items.push(item);
        }
    }
    items.sort_by(|a, b| a.category.cmp(&b.category).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    items
}

/// Loads a single item by id from its `meta` file.
fn load_item(root: &Path, id: &str) -> Option<Item> {
    if !is_safe_id(id) {
        return None;
    }
    let meta_path = root.join("items").join(id).join("meta");
    let text = fs::read_to_string(&meta_path).ok()?;
    parse_meta(id, &text)
}

/// Parses an item `meta` file (simple `key=value` lines).
fn parse_meta(id: &str, text: &str) -> Option<Item> {
    let mut category = None;
    let mut name = String::new();
    let mut kind = None;
    let mut entry = String::new();
    let mut description = String::new();
    let mut source = String::new();
    let mut harvested_at = 0u64;

    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "category" => category = assess::parse_category(value),
            "name" => name = value.to_string(),
            "kind" => kind = Kind::parse(value),
            "entry" => entry = value.to_string(),
            "description" => description = value.to_string(),
            "source" => source = value.to_string(),
            "harvested_at" => harvested_at = value.parse().unwrap_or(0),
            _ => {}
        }
    }

    Some(Item { id: id.to_string(), category: category?, name, kind: kind?, entry, description, source, harvested_at })
}

/// Serializes an item into the textual `meta` representation.
fn meta_text(item: &Item) -> String {
    format!(
        "category={}\nname={}\nkind={}\nentry={}\ndescription={}\nsource={}\nharvested_at={}\n",
        item.category.key(),
        sanitize_line(&item.name),
        item.kind.as_str(),
        sanitize_line(&item.entry),
        sanitize_line(&item.description),
        sanitize_line(&item.source),
        item.harvested_at,
    )
}

// ---------------------------------------------------------------------------
// harvest
// ---------------------------------------------------------------------------

/// Implements `gallery harvest`.
fn cmd_harvest(args: &[String]) -> ExitCode {
    let mut apply = false;
    let mut quiet = false;
    for arg in args {
        match arg.as_str() {
            "--apply" | "-y" => apply = true,
            "--quiet" | "-q" => quiet = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("token-saver: unknown harvest option '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };

    let candidates = find_candidates();
    if candidates.is_empty() {
        println!("token-saver: no user-defined context objects found to harvest.");
        return ExitCode::SUCCESS;
    }

    if !apply {
        println!("token-saver: the following {} item(s) would be moved into the gallery:\n", candidates.len());
        for cand in &candidates {
            println!("  [{}] {}\n      from {}", cand.category.label(), cand.name, display_path(&cand.path));
        }
        println!(
            "\nThis is a dry run. Re-run with --apply to move them.\n\
             Gallery: {}",
            display_path(&root)
        );
        return ExitCode::SUCCESS;
    }

    let existing = load_items(&root);
    let mut used_ids: Vec<String> = existing.iter().map(|item| item.id.clone()).collect();
    let mut moved = 0usize;
    let mut failed = 0usize;

    for cand in &candidates {
        let id = unique_id(cand.category, &cand.name, &used_ids);
        match harvest_one(&root, cand, &id) {
            Ok(()) => {
                used_ids.push(id.clone());
                moved += 1;
                if !quiet {
                    println!("moved [{}] {} -> {}", cand.category.label(), cand.name, id);
                }
            }
            Err(err) => {
                failed += 1;
                eprintln!("token-saver: failed to harvest {}: {err}", display_path(&cand.path));
            }
        }
    }

    println!("\ntoken-saver: harvested {moved} item(s) into {}.", display_path(&root));
    if failed > 0 {
        eprintln!("token-saver: {failed} item(s) could not be harvested.");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Moves a single candidate into the gallery and writes its `meta` file.
///
/// The `meta` file is written before the payload is moved so a mid-operation
/// failure cannot strand a payload without metadata. If the move fails, the
/// freshly created item directory is removed and the source is left intact.
fn harvest_one(root: &Path, cand: &Candidate, id: &str) -> io::Result<()> {
    let item_dir = root.join("items").join(id);
    fs::create_dir_all(&item_dir)?;

    let entry = file_name_string(&cand.path);
    let dest = item_dir.join(&entry);

    let item = Item {
        id: id.to_string(),
        category: cand.category,
        name: cand.name.clone(),
        kind: cand.kind,
        entry,
        description: read_description(&cand.path, cand.category),
        source: cand.path.to_string_lossy().to_string(),
        harvested_at: now_secs(),
    };
    fs::write(item_dir.join("meta"), meta_text(&item))?;

    if let Err(err) = move_path(&cand.path, &dest) {
        let _ = fs::remove_dir_all(&item_dir);
        return Err(err);
    }
    Ok(())
}

/// Discovers user-defined context objects to harvest, de-duplicated.
fn find_candidates() -> Vec<Candidate> {
    let Some(home) = assess::home_dir() else {
        return Vec::new();
    };

    let mut files: Vec<(Category, PathBuf)> = Vec::new();

    // Recursive roots that hold only user-authored content.
    let mut recursive_roots = vec![home.join(".copilot"), home.join(".agents")];
    recursive_roots.extend(vscode_prompt_dirs(&home));
    for dir in &recursive_roots {
        walk(dir, 0, &mut files);
    }

    // Home-level instruction files (non-recursive).
    for name in ["AGENTS.md", "copilot-instructions.md"] {
        let path = home.join(name);
        if path.is_file() {
            if let Some(category) = assess::classify(&path) {
                files.push((category, path));
            }
        }
    }

    build_candidates(files)
}

/// Recursively collects classifiable files under `dir`.
fn walk(dir: &Path, depth: usize, out: &mut Vec<(Category, PathBuf)>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if HARVEST_PRUNE.contains(&name.as_str()) {
                continue;
            }
            walk(&path, depth + 1, out);
        } else if file_type.is_file() {
            if let Some(category) = assess::classify(&path) {
                out.push((category, path));
            }
        }
    }
}

/// Converts raw `(category, file)` hits into de-duplicated [`Candidate`]s.
///
/// A skill is represented by its containing directory (the parent of `SKILL.md`),
/// so its supporting assets travel with it. Everything else is a single file.
fn build_candidates(files: Vec<(Category, PathBuf)>) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    for (category, path) in files {
        let (kind, item_path) = if category == Category::Skills {
            match path.parent() {
                Some(parent) => (Kind::Dir, parent.to_path_buf()),
                None => (Kind::File, path.clone()),
            }
        } else {
            (Kind::File, path.clone())
        };

        if seen.contains(&item_path) {
            continue;
        }
        seen.push(item_path.clone());

        let name = candidate_name(category, &item_path);
        candidates.push(Candidate { category, path: item_path, kind, name });
    }

    candidates
        .sort_by(|a, b| a.category.cmp(&b.category).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    candidates
}

/// Derives a display name for a candidate from its path.
fn candidate_name(category: Category, path: &Path) -> String {
    let raw = file_name_string(path);
    if category == Category::Skills {
        return raw;
    }
    // Strip the recognized compound suffix to leave a friendly stem.
    for suffix in [".instructions.md", ".prompt.md", ".chatmode.md", ".agent.md"] {
        if let Some(stem) = raw.strip_suffix(suffix) {
            if !stem.is_empty() {
                return stem.to_string();
            }
        }
    }
    raw
}

// ---------------------------------------------------------------------------
// list / show
// ---------------------------------------------------------------------------

/// Implements `gallery list`.
fn cmd_list(args: &[String]) -> ExitCode {
    let filter = match args.first() {
        Some(value) if !value.starts_with('-') => match assess::parse_category(value) {
            Some(category) => Some(category),
            None => {
                eprintln!("token-saver: unknown category '{value}'");
                return ExitCode::from(2);
            }
        },
        _ => None,
    };

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };

    let items: Vec<Item> =
        load_items(&root).into_iter().filter(|item| filter.is_none_or(|c| c == item.category)).collect();

    if items.is_empty() {
        println!("token-saver: the gallery is empty. Run `token-saver gallery harvest --apply` to populate it.");
        return ExitCode::SUCCESS;
    }

    let id_width = items.iter().map(|item| item.id.len()).max().unwrap_or(2).max(2);
    println!("{:<id_width$}  {:<13}  NAME", "ID", "CATEGORY", id_width = id_width);
    for item in &items {
        println!("{:<id_width$}  {:<13}  {}", item.id, item.category.label(), item.name, id_width = id_width);
    }
    ExitCode::SUCCESS
}

/// Implements `gallery show`.
fn cmd_show(args: &[String]) -> ExitCode {
    let Some(id) = args.first() else {
        eprintln!("token-saver: usage: token-saver gallery show <id>");
        return ExitCode::from(2);
    };

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };

    let Some(item) = load_item(&root, id) else {
        eprintln!("token-saver: no gallery item with id '{id}'");
        return ExitCode::FAILURE;
    };

    println!("ID:          {}", item.id);
    println!("Category:    {}", item.category.label());
    println!("Name:        {}", item.name);
    println!("Kind:        {}", item.kind.as_str());
    if !item.description.is_empty() {
        println!("Description: {}", item.description);
    }
    if !item.source.is_empty() {
        println!("Source:      {}", item.source);
    }
    println!("\n--- preview ---");
    println!("{}", preview_text(&item.payload_path(&root), item.kind));
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// install
// ---------------------------------------------------------------------------

/// Implements `gallery install`.
fn cmd_install(args: &[String]) -> ExitCode {
    let mut id: Option<String> = None;
    let mut dir: Option<PathBuf> = None;
    let mut force = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" | "-d" => {
                let Some(value) = args.get(i + 1) else {
                    eprintln!("token-saver: --dir requires a path");
                    return ExitCode::from(2);
                };
                dir = Some(PathBuf::from(value));
                i += 2;
            }
            "--force" | "-f" => {
                force = true;
                i += 1;
            }
            other if other.starts_with('-') => {
                eprintln!("token-saver: unknown install option '{other}'");
                return ExitCode::from(2);
            }
            other => {
                if id.is_some() {
                    eprintln!("token-saver: unexpected argument '{other}'");
                    return ExitCode::from(2);
                }
                id = Some(other.to_string());
                i += 1;
            }
        }
    }

    let Some(id) = id else {
        eprintln!("token-saver: usage: token-saver gallery install <id> [--dir <path>] [--force]");
        return ExitCode::from(2);
    };

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };
    let workspace = dir.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match install_item(&root, &id, &workspace, force) {
        Ok(written) => {
            println!("token-saver: installed '{id}' into {}:", display_path(&workspace));
            for path in written {
                println!("  {}", display_path(&path));
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("token-saver: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Installs item `id` into `workspace`, returning the paths written.
fn install_item(root: &Path, id: &str, workspace: &Path, force: bool) -> Result<Vec<PathBuf>, String> {
    let Some(item) = load_item(root, id) else {
        return Err(format!("no gallery item with id '{id}'"));
    };
    if !is_safe_entry(&item.entry) {
        return Err(format!("gallery item '{id}' has an unsafe entry name"));
    }
    if !workspace.is_dir() {
        return Err(format!("target is not a directory: {}", display_path(workspace)));
    }

    let payload = item.payload_path(root);
    let dest = destination_path(workspace, &item);

    // Instruction files merge by appending rather than overwriting.
    if item.kind == Kind::File && is_merge_target(&item) && dest.exists() {
        append_merge(&payload, &dest).map_err(|e| format!("failed to merge into {}: {e}", display_path(&dest)))?;
        return Ok(vec![dest]);
    }

    if dest.exists() && !force {
        return Err(format!("destination already exists (use --force): {}", display_path(&dest)));
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("failed to create {}: {e}", display_path(parent)))?;
    }

    match item.kind {
        Kind::File => {
            fs::copy(&payload, &dest).map_err(|e| format!("failed to copy to {}: {e}", display_path(&dest)))?;
        }
        Kind::Dir => {
            if dest.exists() && force {
                let _ = fs::remove_dir_all(&dest);
            }
            copy_dir(&payload, &dest).map_err(|e| format!("failed to copy to {}: {e}", display_path(&dest)))?;
        }
    }
    Ok(vec![dest])
}

/// Computes the workspace-relative destination path for an item.
fn destination_path(workspace: &Path, item: &Item) -> PathBuf {
    let github = workspace.join(".github");
    match item.category {
        Category::Instructions => {
            let lower = item.entry.to_ascii_lowercase();
            if lower == "copilot-instructions.md" {
                github.join("copilot-instructions.md")
            } else if lower == "agents.md" {
                workspace.join("AGENTS.md")
            } else {
                github.join("instructions").join(&item.entry)
            }
        }
        Category::Prompts => github.join("prompts").join(&item.entry),
        Category::Agents => {
            if item.entry.to_ascii_lowercase().ends_with(".chatmode.md") {
                github.join("chatmodes").join(&item.entry)
            } else {
                github.join("agents").join(&item.entry)
            }
        }
        Category::Skills => github.join("skills").join(&item.entry),
        Category::Tools => workspace.join(".vscode").join("mcp.json"),
    }
}

/// Returns whether an item should be merged (appended) rather than overwritten.
fn is_merge_target(item: &Item) -> bool {
    let lower = item.entry.to_ascii_lowercase();
    item.category == Category::Instructions && (lower == "copilot-instructions.md" || lower == "agents.md")
}

/// Appends the gallery content to an existing instruction file under a marker.
fn append_merge(payload: &Path, dest: &Path) -> io::Result<()> {
    let addition = fs::read_to_string(payload)?;
    let mut file = fs::OpenOptions::new().append(true).open(dest)?;
    write!(file, "\n\n<!-- token-saver-gallery: appended content -->\n{addition}")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

/// Implements `gallery remove`.
fn cmd_remove(args: &[String]) -> ExitCode {
    let Some(id) = args.first() else {
        eprintln!("token-saver: usage: token-saver gallery remove <id>");
        return ExitCode::from(2);
    };
    if !is_safe_id(id) {
        eprintln!("token-saver: invalid item id '{id}'");
        return ExitCode::from(2);
    }

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };
    let item_dir = root.join("items").join(id);
    if !item_dir.is_dir() {
        eprintln!("token-saver: no gallery item with id '{id}'");
        return ExitCode::FAILURE;
    }
    match fs::remove_dir_all(&item_dir) {
        Ok(()) => {
            println!("token-saver: removed '{id}' from the gallery.");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("token-saver: failed to remove '{id}': {err}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// serve (browser gallery)
// ---------------------------------------------------------------------------

/// Implements `gallery serve`.
fn cmd_serve(args: &[String]) -> ExitCode {
    let mut port: u16 = 7878;
    let mut open = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                let Some(value) = args.get(i + 1).and_then(|v| v.parse::<u16>().ok()) else {
                    eprintln!("token-saver: --port requires a number (1-65535)");
                    return ExitCode::from(2);
                };
                port = value;
                i += 2;
            }
            "--open" | "-o" => {
                open = true;
                i += 1;
            }
            other => {
                eprintln!("token-saver: unknown serve option '{other}'");
                return ExitCode::from(2);
            }
        }
    }

    let root = match require_gallery_root() {
        Ok(root) => root,
        Err(code) => return code,
    };

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("token-saver: could not bind to 127.0.0.1:{port}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let url = format!("http://127.0.0.1:{port}/");
    println!("token-saver: gallery serving at {url} (press Ctrl+C to stop)");
    if open {
        open_browser(&url);
    }

    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));
                if active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    // Too many in-flight connections; drop this one rather than
                    // letting a flood exhaust threads.
                    continue;
                }
                let root = root.clone();
                let active = Arc::clone(&active);
                active.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || {
                    if let Err(err) = handle_connection(stream, &root, port) {
                        eprintln!("token-saver: connection error: {err}");
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(err) => eprintln!("token-saver: accept error: {err}"),
        }
    }
    ExitCode::SUCCESS
}

/// Handles a single HTTP connection.
fn handle_connection(stream: TcpStream, root: &Path, port: u16) -> io::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    let mut host: Option<String> = None;
    let mut origin: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = header_value(trimmed, "content-length") {
            content_length = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = header_value(trimmed, "host") {
            host = Some(value.trim().to_string());
        } else if let Some(value) = header_value(trimmed, "origin") {
            origin = Some(value.trim().to_string());
        }
    }

    // Reject cross-site and DNS-rebinding requests: the gallery server exposes a
    // local file-write endpoint, so only genuine loopback requests are allowed.
    if !request_is_local(host.as_deref(), origin.as_deref(), port) {
        let stream = reader.into_inner();
        return respond(stream, 403, "text/plain; charset=utf-8", b"Forbidden");
    }

    if content_length > MAX_BODY_BYTES {
        let stream = reader.into_inner();
        return respond(stream, 413, "text/plain; charset=utf-8", b"Payload too large");
    }

    let mut body = Vec::new();
    if content_length > 0 {
        body.resize(content_length, 0);
        reader.read_exact(&mut body)?;
    }
    let body = String::from_utf8_lossy(&body).to_string();

    let stream = reader.into_inner();
    route(stream, &method, &target, &body, root)
}

/// Dispatches an HTTP request to the matching handler.
fn route(stream: TcpStream, method: &str, target: &str, body: &str, root: &Path) -> io::Result<()> {
    let path = target.split('?').next().unwrap_or("/");
    match (method, path) {
        ("GET", "/") => respond(stream, 200, "text/html; charset=utf-8", index_html().as_bytes()),
        ("GET", "/api/items") => respond(stream, 200, "application/json", items_json(root).as_bytes()),
        ("POST", "/api/install") => {
            let (status, json) = api_install(root, body);
            respond(stream, status, "application/json", json.as_bytes())
        }
        ("GET", api) if api.starts_with("/api/items/") => {
            let id = &api["/api/items/".len()..];
            match item_detail_json(root, id) {
                Some(json) => respond(stream, 200, "application/json", json.as_bytes()),
                None => respond(stream, 404, "application/json", b"{\"error\":\"not found\"}"),
            }
        }
        _ => respond(stream, 404, "text/plain; charset=utf-8", b"Not found"),
    }
}

/// Writes a minimal HTTP/1.1 response and closes the connection.
fn respond(mut stream: TcpStream, status: u16, content_type: &str, body: &[u8]) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Returns the value of header `name` (case-insensitive) from a header line.
fn header_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (key, value) = line.split_once(':')?;
    key.trim().eq_ignore_ascii_case(name).then_some(value)
}

/// Rejects requests that aren't from a loopback browser context, defeating
/// DNS-rebinding and cross-site (CSRF) access to the local gallery server.
///
/// A `Host` header naming the loopback interface (and our port) is required.
/// If an `Origin` is present (cross-site fetches always send one), it must also
/// be our own loopback origin.
fn request_is_local(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let Some(host) = host else {
        return false;
    };
    if !host_is_loopback(host, port) {
        return false;
    }
    match origin {
        Some(origin) => origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .is_some_and(|rest| host_is_loopback(rest, port)),
        None => true,
    }
}

/// Returns whether `authority` (`host[:port]`) names the loopback interface on
/// our serving `port`.
fn host_is_loopback(authority: &str, port: u16) -> bool {
    let (hostname, host_port) = split_authority(authority);
    matches!(hostname, "127.0.0.1" | "localhost" | "::1") && host_port == Some(port)
}

/// Splits an HTTP authority into `(hostname, port)`, handling IPv6 literals.
fn split_authority(authority: &str) -> (&str, Option<u16>) {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: `[host]:port`.
        if let Some((host, after)) = rest.split_once(']') {
            let port = after.strip_prefix(':').and_then(|p| p.parse().ok());
            return (host, port);
        }
        return (authority, None);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()),
        None => (authority, None),
    }
}

/// Handles `POST /api/install`.
fn api_install(root: &Path, body: &str) -> (u16, String) {
    let Some(id) = json_str_field(body, "id") else {
        return (400, json_message(false, "missing 'id'"));
    };
    let Some(dir) = json_str_field(body, "dir") else {
        return (400, json_message(false, "missing 'dir'"));
    };
    let workspace = PathBuf::from(&dir);
    match install_item(root, &id, &workspace, false) {
        Ok(written) => {
            let list: Vec<String> =
                written.iter().map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy()))).collect();
            (200, format!("{{\"ok\":true,\"files\":[{}]}}", list.join(",")))
        }
        Err(err) => (400, json_message(false, &err)),
    }
}

/// Builds a JSON array of all items for the browser gallery.
fn items_json(root: &Path) -> String {
    let items = load_items(root);
    let parts: Vec<String> = items.iter().map(item_json).collect();
    format!("[{}]", parts.join(","))
}

/// Serializes one item to a JSON object.
fn item_json(item: &Item) -> String {
    format!(
        "{{\"id\":\"{}\",\"category\":\"{}\",\"name\":\"{}\",\"kind\":\"{}\",\"description\":\"{}\"}}",
        json_escape(&item.id),
        item.category.key(),
        json_escape(&item.name),
        item.kind.as_str(),
        json_escape(&item.description),
    )
}

/// Builds the JSON detail (with content preview) for one item.
fn item_detail_json(root: &Path, id: &str) -> Option<String> {
    let item = load_item(root, id)?;
    let preview = preview_text(&item.payload_path(root), item.kind);
    Some(format!(
        "{{\"id\":\"{}\",\"category\":\"{}\",\"name\":\"{}\",\"kind\":\"{}\",\"description\":\"{}\",\"source\":\"{}\",\"preview\":\"{}\"}}",
        json_escape(&item.id),
        item.category.key(),
        json_escape(&item.name),
        item.kind.as_str(),
        json_escape(&item.description),
        json_escape(&item.source),
        json_escape(&preview),
    ))
}

/// Returns the static HTML for the browser gallery (CSS + JS inlined, no deps).
fn index_html() -> String {
    // Dynamic content is rendered with textContent in JS to avoid XSS.
        r####"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>token-saver gallery</title>
<style>
    :root {
        --bg: #0d1117;
        --bg-elev: #161b22;
        --surface: #0f1724;
        --text: #e6edf3;
        --muted: #8b949e;
        --line: #30363d;
        --blue: #58a6ff;
        --cobalt: #1f6feb;
        --purple: #a371f7;
        --pink: #db61a2;
        --rose: #f778ba;
        --radius: 14px;
        --shadow: 0 16px 50px rgba(0, 0, 0, 0.35);
    }

    * { box-sizing: border-box; }

    html, body { height: 100%; }

    body {
        margin: 0;
        color: var(--text);
        background:
            radial-gradient(1200px 500px at -5% -10%, rgba(88, 166, 255, 0.24), transparent 55%),
            radial-gradient(900px 420px at 105% -12%, rgba(163, 113, 247, 0.22), transparent 52%),
            linear-gradient(180deg, #090c12 0%, var(--bg) 55%, #0b111b 100%);
        font-family: "Segoe UI", "Inter", "Avenir Next", "SF Pro Text", system-ui, -apple-system, sans-serif;
        line-height: 1.5;
    }

    a { color: var(--blue); }

    .skip {
        position: absolute;
        left: -9999px;
        top: 0;
        background: #fff;
        color: #111;
        padding: .5rem .75rem;
        border-radius: 8px;
    }
    .skip:focus { left: 10px; top: 10px; z-index: 1000; }

    .wrap {
        min-height: 100%;
        display: grid;
        grid-template-rows: auto 1fr;
    }

    .hero {
        padding: 1.25rem 1.1rem .95rem;
        border-bottom: 1px solid #ffffff1f;
        backdrop-filter: blur(6px);
        background: linear-gradient(90deg, rgba(13, 17, 23, 0.84), rgba(13, 17, 23, 0.62));
    }

    .hero-inner {
        max-width: 1280px;
        margin: 0 auto;
        display: grid;
        grid-template-columns: auto 1fr;
        align-items: center;
        gap: .9rem 1.1rem;
    }

    .logo {
        width: 54px;
        height: 54px;
        border-radius: 50%;
        background: conic-gradient(from 130deg, var(--blue), var(--purple), var(--pink), var(--rose), var(--cobalt), var(--blue));
        box-shadow: 0 0 0 2px #ffffff12 inset, 0 12px 28px rgba(24, 88, 179, 0.36);
        display: grid;
        place-items: center;
        font-weight: 800;
        font-size: 1.1rem;
        letter-spacing: .03em;
        color: #ffffff;
        text-shadow: 0 1px 6px rgba(0, 0, 0, .45);
    }

    h1 {
        margin: 0;
        font-size: clamp(1.25rem, 2.3vw, 1.6rem);
        line-height: 1.2;
    }

    .subtitle {
        margin: .28rem 0 0;
        color: var(--muted);
        font-size: .95rem;
    }

    .hero-actions {
        grid-column: 1 / -1;
        display: flex;
        flex-wrap: wrap;
        gap: .6rem;
        align-items: center;
    }

    .btn {
        border: 1px solid transparent;
        border-radius: 11px;
        padding: .53rem .82rem;
        font-size: .88rem;
        font-weight: 650;
        text-decoration: none;
        cursor: pointer;
        transition: transform .14s ease, border-color .14s ease, background-color .14s ease;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: .35rem;
    }

    .btn:focus-visible,
    .item:focus-visible,
    .field:focus-visible,
    .select:focus-visible {
        outline: 3px solid rgba(88, 166, 255, 0.45);
        outline-offset: 2px;
    }

    .btn-primary {
        color: #091321;
        background: linear-gradient(130deg, var(--blue), #86c2ff);
    }
    .btn-primary:hover { transform: translateY(-1px); }

    .btn-ghost {
        color: var(--text);
        background: #ffffff0a;
        border-color: #ffffff2a;
    }
    .btn-ghost:hover {
        background: #ffffff12;
        border-color: #ffffff44;
        transform: translateY(-1px);
    }

    .btn-insiders {
        color: #150a27;
        background: linear-gradient(130deg, var(--purple), var(--pink));
    }

    .split {
        position: relative;
        display: inline-flex;
    }

    .split-main {
        border-radius: 11px 0 0 11px;
    }

    .split-toggle {
        border-left: 1px solid #ffffff3d;
        border-radius: 0 11px 11px 0;
        min-width: 2.15rem;
        padding: .53rem .55rem;
    }

    .split-menu {
        position: absolute;
        right: 0;
        top: calc(100% + .4rem);
        min-width: 185px;
        border-radius: 10px;
        border: 1px solid #ffffff24;
        background: #0f1724;
        box-shadow: 0 14px 26px #0008;
        overflow: hidden;
        z-index: 30;
    }

    .split-menu[hidden] { display: none; }

    .menu-item {
        width: 100%;
        border: 0;
        border-radius: 0;
        text-align: left;
        padding: .6rem .7rem;
        color: #dbe7f3;
        background: transparent;
        font-size: .88rem;
    }

    .menu-item:hover {
        background: #ffffff14;
    }

    .layout {
        max-width: 1280px;
        width: 100%;
        margin: 0 auto;
        padding: .95rem 1.1rem 1.15rem;
        display: grid;
        grid-template-columns: minmax(300px, 370px) minmax(0, 1fr);
        gap: .95rem;
        align-items: stretch;
    }

    .panel {
        background: linear-gradient(180deg, rgba(22, 27, 34, 0.96), rgba(15, 23, 36, 0.96));
        border: 1px solid #ffffff1f;
        border-radius: var(--radius);
        box-shadow: var(--shadow);
        min-height: 0;
    }

    .list-panel {
        display: grid;
        grid-template-rows: auto auto 1fr;
        overflow: hidden;
    }

    .panel-head {
        padding: .9rem .9rem .55rem;
        border-bottom: 1px solid #ffffff14;
    }

    .panel-title {
        margin: 0;
        font-size: 1rem;
        letter-spacing: .02em;
    }

    .toolbar {
        padding: .6rem .9rem .75rem;
        border-bottom: 1px solid #ffffff12;
        display: grid;
        gap: .5rem;
    }

    .field,
    .select {
        width: 100%;
        border-radius: 10px;
        border: 1px solid #ffffff2b;
        background: #0c1320;
        color: var(--text);
        padding: .52rem .66rem;
        font: inherit;
    }

    .hint {
        margin: .2rem 0 0;
        font-size: .79rem;
        color: var(--muted);
    }

    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip: rect(0, 0, 0, 0);
        border: 0;
        white-space: nowrap;
    }

    .toolbar-actions {
        display: flex;
        gap: .45rem;
        align-items: center;
        justify-content: space-between;
    }

    .btn-subtle {
        border: 1px solid #ffffff2b;
        background: #ffffff08;
        color: #d8e3ef;
        border-radius: 10px;
        padding: .42rem .62rem;
        font-size: .8rem;
    }

    .btn-subtle:hover {
        background: #ffffff12;
        border-color: #ffffff45;
    }

    .list {
        overflow: auto;
        padding: .45rem;
    }

    .group {
        margin: .25rem .24rem .2rem;
        color: #b8c5d2;
        font-size: .72rem;
        letter-spacing: .09em;
        text-transform: uppercase;
    }

    .item {
        width: 100%;
        text-align: left;
        border: 1px solid transparent;
        background: transparent;
        color: var(--text);
        border-radius: 11px;
        padding: .58rem .62rem;
        margin-bottom: .35rem;
        cursor: pointer;
    }

    .item:hover {
        border-color: #ffffff2a;
        background: #ffffff08;
    }

    .item.active {
        border-color: rgba(88, 166, 255, 0.62);
        background: linear-gradient(120deg, rgba(88, 166, 255, 0.19), rgba(163, 113, 247, 0.16));
    }

    .item-name {
        font-size: .93rem;
        font-weight: 650;
    }

    .item-desc {
        margin-top: .08rem;
        font-size: .8rem;
        color: #b1bdc9;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .empty {
        margin: .5rem;
        padding: 1rem;
        border: 1px dashed #ffffff30;
        border-radius: 12px;
        color: #b5c0cb;
        font-size: .9rem;
    }

    .detail {
        display: grid;
        grid-template-rows: auto auto 1fr;
        min-height: 0;
    }

    .detail-head {
        padding: 1rem 1rem .7rem;
        border-bottom: 1px solid #ffffff14;
    }

    .detail-name {
        margin: 0;
        font-size: clamp(1.15rem, 2vw, 1.35rem);
        line-height: 1.25;
    }

    .chips {
        margin-top: .45rem;
        display: flex;
        flex-wrap: wrap;
        gap: .45rem;
        color: #bdd0e3;
    }

    .chip {
        border: 1px solid #ffffff28;
        border-radius: 999px;
        font-size: .75rem;
        padding: .2rem .55rem;
        background: #ffffff08;
    }

    .install {
        padding: .9rem 1rem;
        border-bottom: 1px solid #ffffff14;
        display: grid;
        gap: .55rem;
    }

    .install-grid {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: .5rem;
        align-items: center;
    }

    .status {
        min-height: 1.15rem;
        color: #b5e3c8;
        font-size: .84rem;
    }

    .status.error {
        color: #ffb2b2;
    }

    pre {
        margin: 0;
        padding: 1rem;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-word;
        background: linear-gradient(180deg, #0a111c, #09101a);
        color: #dce5ee;
        border-radius: 0 0 var(--radius) var(--radius);
        font-family: "Cascadia Code", "Consolas", monospace;
        font-size: .86rem;
    }

    @media (max-width: 930px) {
        .layout {
            grid-template-columns: 1fr;
            padding-top: .75rem;
        }
        .list-panel { max-height: 45vh; }
        .hero-inner { grid-template-columns: 1fr; }
        .logo { width: 48px; height: 48px; }
    }

    @media (prefers-reduced-motion: no-preference) {
        .panel,
        .hero {
            animation: rise .45s ease both;
        }

        .list .item {
            animation: fade .22s ease both;
        }

        @keyframes rise {
            from { opacity: 0; transform: translateY(7px); }
            to { opacity: 1; transform: translateY(0); }
        }

        @keyframes fade {
            from { opacity: .5; transform: translateY(2px); }
            to { opacity: 1; transform: translateY(0); }
        }
    }
</style>
</head>
<body>
<a class="skip" href="#detail">Skip to item details</a>
<div class="wrap">
    <header class="hero">
        <div class="hero-inner">
            <div class="logo" aria-hidden="true">TS</div>
            <div>
                <h1>token-saver gallery</h1>
                <p class="subtitle">Browse and install harvested Copilot prompts, agents, skills, instructions, and tools.</p>
            </div>
            <nav class="hero-actions" aria-label="Quick actions">
                <a class="btn btn-ghost" href="https://awesome-copilot.github.com/" target="_blank" rel="noreferrer">Awesome GitHub Copilot</a>
            </nav>
        </div>
    </header>

    <main class="layout">
        <section class="panel list-panel" aria-label="Gallery items">
            <div class="panel-head"><h2 class="panel-title">Items</h2></div>
            <div class="toolbar">
                <label>
                    <span class="hint">Search</span>
                    <input id="search" class="field" type="search" placeholder="Find by name, id, or description" autocomplete="off">
                </label>
                <label>
                    <span class="hint">Category</span>
                    <select id="category" class="select">
                        <option value="">All categories</option>
                    </select>
                </label>
                <div class="toolbar-actions">
                    <button id="clear-filters" class="btn-subtle" type="button">Clear filters</button>
                    <span class="hint">Shortcut: /</span>
                </div>
                <p id="count" class="hint" role="status" aria-live="polite"></p>
            </div>
            <div id="list" class="list"><div class="empty">Loading...</div></div>
        </section>

        <section class="panel detail" id="detail" aria-label="Item details">
            <div id="detail-head" class="detail-head">
                <h2 class="detail-name">Select an item</h2>
                <p class="hint">Choose an entry from the left to preview and install.</p>
            </div>
            <div id="install" class="install">
                <label>
                    <span class="hint">Workspace folder (absolute path)</span>
                    <div class="install-grid">
                        <input id="target-dir" class="field" placeholder="d:\\dev\\my-repo" aria-describedby="target-help">
                        <div class="split">
                            <button id="install-btn" class="btn btn-primary split-main" type="button" disabled>Install to VS Code</button>
                            <button id="install-target-toggle" class="btn btn-primary split-toggle" type="button" aria-haspopup="true" aria-expanded="false" aria-label="Choose install target" disabled>▾</button>
                            <div id="install-target-menu" class="split-menu" hidden>
                                <button id="install-target-vscode" class="menu-item" type="button">VS Code</button>
                                <button id="install-target-insiders" class="menu-item" type="button">VS Code Insiders</button>
                            </div>
                        </div>
                    </div>
                </label>
                <div id="target-help" class="hint">Path is remembered locally in your browser session.</div>
                <div id="status" class="status" role="status" aria-live="polite"></div>
            </div>
            <pre id="preview">Select an item to view details.</pre>
        </section>
    </main>
</div>

<script>
let items = [];
let filtered = [];
let activeId = null;
let visibleIds = [];

const listEl = document.getElementById('list');
const countEl = document.getElementById('count');
const searchEl = document.getElementById('search');
const categoryEl = document.getElementById('category');
const clearFiltersEl = document.getElementById('clear-filters');
const detailHeadEl = document.getElementById('detail-head');
const previewEl = document.getElementById('preview');
const statusEl = document.getElementById('status');
const installBtn = document.getElementById('install-btn');
const installTargetToggleEl = document.getElementById('install-target-toggle');
const installTargetMenuEl = document.getElementById('install-target-menu');
const installTargetVsCodeEl = document.getElementById('install-target-vscode');
const installTargetInsidersEl = document.getElementById('install-target-insiders');
const targetDirEl = document.getElementById('target-dir');
let installTarget = 'VS Code';
const LAST_DIR_KEY = 'token-saver-gallery-target-dir';

searchEl.addEventListener('input', applyFilters);
categoryEl.addEventListener('change', applyFilters);
clearFiltersEl.addEventListener('click', () => {
    searchEl.value = '';
    categoryEl.value = '';
    applyFilters();
    searchEl.focus();
});
installBtn.addEventListener('click', () => {
    if (!activeId) {
        setStatus('Select an item first.', false);
        return;
    }
    install(activeId, targetDirEl.value, installTarget);
});

installTargetToggleEl.addEventListener('click', (ev) => {
    ev.stopPropagation();
    const next = installTargetMenuEl.hidden;
    installTargetMenuEl.hidden = !next;
    installTargetToggleEl.setAttribute('aria-expanded', String(next));
});

installTargetVsCodeEl.addEventListener('click', () => {
    setInstallTarget('VS Code');
    hideInstallMenu();
});

installTargetInsidersEl.addEventListener('click', () => {
    setInstallTarget('VS Code Insiders');
    hideInstallMenu();
});

document.addEventListener('click', () => hideInstallMenu());
document.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape') {
        hideInstallMenu();
    }
    if ((ev.key === 'ArrowDown' || ev.key === 'ArrowUp') && !isTypingContext(document.activeElement) && !ev.altKey && !ev.ctrlKey && !ev.metaKey) {
        ev.preventDefault();
        moveSelection(ev.key === 'ArrowDown' ? 1 : -1);
    }
    if (ev.key === 'Enter' && !isTypingContext(document.activeElement) && activeId && !ev.altKey && !ev.ctrlKey && !ev.metaKey) {
        ev.preventDefault();
        showDetail(activeId, { focusItem: true });
    }
    if (ev.key === '/' && document.activeElement !== searchEl && !isTypingContext(document.activeElement)) {
        ev.preventDefault();
        searchEl.focus();
    }
});

targetDirEl.addEventListener('change', () => {
    const value = targetDirEl.value.trim();
    if (value) {
        try { localStorage.setItem(LAST_DIR_KEY, value); } catch {}
    }
});

function isTypingContext(el) {
    if (!el) return false;
    const tag = (el.tagName || '').toUpperCase();
    return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable;
}

function hideInstallMenu() {
    installTargetMenuEl.hidden = true;
    installTargetToggleEl.setAttribute('aria-expanded', 'false');
}

function setInstallTarget(target) {
    installTarget = target;
    installBtn.textContent = 'Install to ' + target;
}

function setStatus(text, ok) {
    statusEl.textContent = text;
    statusEl.classList.toggle('error', !ok && !!text);
    statusEl.style.color = ok ? '#b5e3c8' : '#ffb2b2';
}

function resetDetail() {
    detailHeadEl.innerHTML = '<h2 class="detail-name">Select an item</h2><p class="hint">Choose an entry from the left to preview and install.</p>';
    previewEl.textContent = 'Select an item to view details.';
    installBtn.disabled = true;
    installTargetToggleEl.disabled = true;
    hideInstallMenu();
    setInstallTarget('VS Code');
    setStatus('', true);
}

async function load() {
    try {
        const remembered = localStorage.getItem(LAST_DIR_KEY);
        if (remembered) targetDirEl.value = remembered;
    } catch {}

    try {
        const res = await fetch('/api/items');
        if (!res.ok) {
            throw new Error('bad response');
        }
        items = await res.json();
    } catch {
        items = [];
        const msg = document.createElement('div');
        msg.className = 'empty';
        msg.textContent = 'Could not load items. Refresh the page or restart `token-saver gallery serve`.';
        listEl.textContent = '';
        listEl.appendChild(msg);
        countEl.textContent = 'load failed';
        return;
    }

    const cats = Array.from(new Set(items.map(it => it.category))).sort();
    for (const c of cats) {
        const opt = document.createElement('option');
        opt.value = c;
        opt.textContent = c;
        categoryEl.appendChild(opt);
    }

    applyFilters();
    if (!items.length) resetDetail();
}

function applyFilters() {
    const q = searchEl.value.trim().toLowerCase();
    const cat = categoryEl.value;
    filtered = items.filter(it => {
        if (cat && it.category !== cat) return false;
        if (!q) return true;
        const hay = (it.name + ' ' + it.id + ' ' + (it.description || '')).toLowerCase();
        return hay.includes(q);
    });

    if (activeId && !filtered.some(it => it.id === activeId)) {
        activeId = null;
        resetDetail();
    }

    renderList();
}

function renderList() {
    listEl.textContent = '';
    visibleIds = [];

    if (!filtered.length) {
        const e = document.createElement('div');
        e.className = 'empty';
        e.textContent = items.length
            ? 'No items match the current filters.'
            : 'The gallery is empty. Run: token-saver gallery harvest --apply';
        listEl.appendChild(e);
        countEl.textContent = items.length ? '0 results' : '0 items';
        return;
    }

    countEl.textContent = filtered.length + (filtered.length === 1 ? ' result' : ' results');

    const groups = {};
    for (const it of filtered) {
        if (!groups[it.category]) groups[it.category] = [];
        groups[it.category].push(it);
    }

    for (const cat of Object.keys(groups).sort()) {
        const g = document.createElement('div');
        g.className = 'group';
        g.textContent = cat;
        listEl.appendChild(g);

        for (const it of groups[cat]) {
            const button = document.createElement('button');
            button.type = 'button';
            button.className = 'item' + (it.id === activeId ? ' active' : '');
            button.setAttribute('aria-current', it.id === activeId ? 'true' : 'false');
            button.setAttribute('aria-selected', it.id === activeId ? 'true' : 'false');
            button.tabIndex = it.id === activeId ? 0 : -1;
            button.dataset.id = it.id;

            const name = document.createElement('div');
            name.className = 'item-name';
            name.textContent = it.name;

            const desc = document.createElement('div');
            desc.className = 'item-desc';
            desc.textContent = it.description || it.id;

            button.appendChild(name);
            button.appendChild(desc);
            button.addEventListener('click', () => showDetail(it.id));
            listEl.appendChild(button);
            visibleIds.push(it.id);
        }
    }
}

function focusActiveItem() {
    if (!activeId) return;
    const buttons = listEl.querySelectorAll('.item');
    for (const button of buttons) {
        if (button.dataset.id === activeId) {
            button.focus({ preventScroll: true });
            button.scrollIntoView({ block: 'nearest' });
            return;
        }
    }
}

function moveSelection(delta) {
    if (!visibleIds.length) return;
    const current = visibleIds.indexOf(activeId);
    const next = current === -1
        ? (delta > 0 ? 0 : visibleIds.length - 1)
        : (current + delta + visibleIds.length) % visibleIds.length;
    showDetail(visibleIds[next], { focusItem: true });
}

async function showDetail(id, options) {
    const focusItem = !!(options && options.focusItem);
    activeId = id;
    renderList();
    if (focusItem) {
        focusActiveItem();
    }
    setStatus('', true);

    let it;
    try {
        const res = await fetch('/api/items/' + encodeURIComponent(id));
        if (!res.ok) {
            setStatus('Could not load item details.', false);
            return;
        }
        it = await res.json();
    } catch {
        setStatus('Could not load item details.', false);
        return;
    }

    installBtn.disabled = false;
    installTargetToggleEl.disabled = false;

    detailHeadEl.textContent = '';
    const h = document.createElement('h2');
    h.className = 'detail-name';
    h.textContent = it.name;
    detailHeadEl.appendChild(h);

    const chips = document.createElement('div');
    chips.className = 'chips';

    const c1 = document.createElement('span');
    c1.className = 'chip';
    c1.textContent = it.category;
    chips.appendChild(c1);

    const c2 = document.createElement('span');
    c2.className = 'chip';
    c2.textContent = it.kind;
    chips.appendChild(c2);

    if (it.source) {
        const c3 = document.createElement('span');
        c3.className = 'chip';
        c3.textContent = it.source;
        chips.appendChild(c3);
    }

    detailHeadEl.appendChild(chips);
    previewEl.textContent = it.preview || '(no preview)';
}

async function install(id, dir, targetEditor) {
    const target = (dir || '').trim();
    if (!target) {
        setStatus('Enter a folder path first.', false);
        return;
    }

    setStatus('Installing to ' + targetEditor + '...', true);
    installBtn.disabled = true;
    installTargetToggleEl.disabled = true;
    hideInstallMenu();
    try {
        const res = await fetch('/api/install', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id, dir: target })
        });
        const data = await res.json();
        if (data.ok) {
            setStatus('Installed to ' + targetEditor + ': ' + (data.files || []).join(', '), true);
        } else {
            setStatus('Error: ' + (data.message || 'failed'), false);
        }
    } catch {
        setStatus('Error: request failed.', false);
    } finally {
        installBtn.disabled = false;
        installTargetToggleEl.disabled = false;
    }
}

load();
</script>
</body>
</html>
"####
        .to_string()
}

/// Best-effort attempt to open the default browser at `url`.
fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(url).spawn();
    if let Err(err) = result {
        eprintln!("token-saver: could not open browser: {err}");
    }
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

/// Moves a file or directory, falling back to copy + delete across filesystems.
fn move_path(src: &Path, dest: &Path) -> io::Result<()> {
    if fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    if src.is_dir() {
        copy_dir(src, dest)?;
        fs::remove_dir_all(src)?;
    } else {
        fs::copy(src, dest)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

/// Recursively copies a directory tree from `src` to `dest`.
fn copy_dir(src: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Reads a short description for an item from its content.
fn read_description(path: &Path, category: Category) -> String {
    let file = if category == Category::Skills { path.join("SKILL.md") } else { path.to_path_buf() };
    let Ok(text) = fs::read_to_string(&file) else {
        return String::new();
    };
    extract_description(&text)
}

/// Extracts a one-line description from front matter or the first prose line.
fn extract_description(text: &str) -> String {
    // YAML front matter `description:` field.
    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let front = &rest[..end];
            for line in front.lines() {
                if let Some(value) = line.trim().strip_prefix("description:") {
                    return sanitize_line(value.trim().trim_matches(['"', '\'']));
                }
            }
        }
    }
    // Otherwise the first non-empty, non-heading, non-comment line.
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") || trimmed.starts_with("<!--") {
            continue;
        }
        return sanitize_line(&truncate(trimmed, 160));
    }
    String::new()
}

/// Produces a bounded text preview for an item.
fn preview_text(path: &Path, kind: Kind) -> String {
    let file = if kind == Kind::Dir { path.join("SKILL.md") } else { path.to_path_buf() };
    match fs::read_to_string(&file) {
        Ok(text) => truncate(&text, PREVIEW_BYTES),
        Err(_) if kind == Kind::Dir => format!("(directory: {})", display_path(path)),
        Err(err) => format!("(could not read: {err})"),
    }
}

// ---------------------------------------------------------------------------
// Small utilities
// ---------------------------------------------------------------------------

/// Returns the final path component as an owned `String`.
fn file_name_string(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Generates a unique gallery id from a category and name, avoiding collisions.
fn unique_id(category: Category, name: &str, used: &[String]) -> String {
    let base = format!("{}-{}", category.key(), slugify(name));
    if !used.contains(&base) {
        return base;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !used.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Converts a name into a filesystem- and URL-safe slug.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

/// Validates that a gallery id is a safe single path segment.
fn is_safe_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 {
        return false;
    }
    if Path::new(id).components().count() != 1 {
        return false;
    }
    !matches!(Path::new(id).components().next(), Some(Component::ParentDir | Component::RootDir))
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && id != "."
        && id != ".."
}

/// Validates that an item `entry` is a single, non-traversing path component.
///
/// Unlike [`is_safe_id`], entries are real file/directory names and may contain
/// spaces or Unicode, so only path separators and traversal are rejected.
fn is_safe_entry(entry: &str) -> bool {
    if entry.is_empty() || entry.len() > 255 {
        return false;
    }
    let mut components = Path::new(entry).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// Collapses a value to a single line for storage in `meta` and JSON.
fn sanitize_line(value: &str) -> String {
    value.replace(['\n', '\r', '\t'], " ").trim().to_string()
}

/// Truncates a string to at most `max` bytes on a char boundary.
fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Escapes a string for embedding inside a JSON string literal.
fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
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

/// Builds a small `{ "ok": bool, "message": str }` JSON object.
fn json_message(ok: bool, message: &str) -> String {
    format!("{{\"ok\":{ok},\"message\":\"{}\"}}", json_escape(message))
}

/// Extracts a string field value from a small JSON object body (naive).
fn json_str_field(body: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let start = body.find(&key)? + key.len();
    let rest = &body[start..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let after = after.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = after.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Returns the VS Code `User/prompts` directories for stable and Insiders builds.
fn vscode_prompt_dirs(home: &Path) -> Vec<PathBuf> {
    let names = ["Code", "Code - Insiders"];
    let mut dirs = Vec::new();

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
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = home.join(".config");
        for name in names {
            dirs.push(base.join(name).join("User").join("prompts"));
        }
    }

    let _ = home;
    dirs
}

/// Returns the current Unix time in seconds.
fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Renders a path for display.
fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_makes_safe_slugs() {
        assert_eq!(slugify("My Cool Skill!"), "my-cool-skill");
        assert_eq!(slugify("a__b  c"), "a-b-c");
        assert_eq!(slugify("---"), "item");
        assert_eq!(slugify(""), "item");
    }

    #[test]
    fn unique_id_avoids_collisions() {
        let used = vec!["skills-foo".to_string(), "skills-foo-2".to_string()];
        assert_eq!(unique_id(Category::Skills, "foo", &used), "skills-foo-3");
        assert_eq!(unique_id(Category::Prompts, "bar", &used), "prompts-bar");
    }

    #[test]
    fn is_safe_entry_rejects_traversal() {
        assert!(is_safe_entry("copilot-instructions.md"));
        assert!(is_safe_entry("My Prompt.prompt.md"));
        assert!(!is_safe_entry("../evil.md"));
        assert!(!is_safe_entry("a/b.md"));
        assert!(!is_safe_entry(""));
    }

    #[test]
    fn request_is_local_blocks_cross_site() {
        // Loopback Host with no Origin is allowed.
        assert!(request_is_local(Some("127.0.0.1:7878"), None, 7878));
        assert!(request_is_local(Some("localhost:7878"), None, 7878));
        // Matching loopback Origin is allowed.
        assert!(request_is_local(Some("127.0.0.1:7878"), Some("http://127.0.0.1:7878"), 7878));
        // Missing Host is rejected.
        assert!(!request_is_local(None, None, 7878));
        // Wrong port is rejected.
        assert!(!request_is_local(Some("127.0.0.1:9999"), None, 7878));
        // Cross-site Origin (CSRF) is rejected.
        assert!(!request_is_local(Some("127.0.0.1:7878"), Some("http://evil.example"), 7878));
        // Non-loopback Host (DNS rebinding) is rejected.
        assert!(!request_is_local(Some("evil.example:7878"), None, 7878));
    }

    #[test]
    fn is_safe_id_rejects_traversal() {
        assert!(is_safe_id("skills-foo"));
        assert!(is_safe_id("agents-my_agent.v2"));
        assert!(!is_safe_id("../escape"));
        assert!(!is_safe_id("a/b"));
        assert!(!is_safe_id(".."));
        assert!(!is_safe_id(""));
    }

    #[test]
    fn meta_round_trips() {
        let item = Item {
            id: "skills-foo".to_string(),
            category: Category::Skills,
            name: "Foo".to_string(),
            kind: Kind::Dir,
            entry: "foo".to_string(),
            description: "a\tdesc".to_string(),
            source: "/home/u/.agents/skills/foo".to_string(),
            harvested_at: 123,
        };
        let text = meta_text(&item);
        let parsed = parse_meta("skills-foo", &text).expect("parse");
        assert_eq!(parsed.category, Category::Skills);
        assert_eq!(parsed.name, "Foo");
        assert_eq!(parsed.kind, Kind::Dir);
        assert_eq!(parsed.entry, "foo");
        assert_eq!(parsed.description, "a desc");
        assert_eq!(parsed.harvested_at, 123);
    }

    #[test]
    fn candidate_name_strips_suffixes() {
        assert_eq!(candidate_name(Category::Prompts, Path::new("/x/review.prompt.md")), "review");
        assert_eq!(candidate_name(Category::Instructions, Path::new("/x/style.instructions.md")), "style");
        assert_eq!(candidate_name(Category::Agents, Path::new("/x/planner.agent.md")), "planner");
        assert_eq!(candidate_name(Category::Skills, Path::new("/x/my-skill")), "my-skill");
    }

    #[test]
    fn destination_paths_follow_conventions() {
        let ws = Path::new("/ws");
        let mk = |category: Category, entry: &str| Item {
            id: "x".to_string(),
            category,
            name: "n".to_string(),
            kind: Kind::File,
            entry: entry.to_string(),
            description: String::new(),
            source: String::new(),
            harvested_at: 0,
        };
        assert_eq!(
            destination_path(ws, &mk(Category::Prompts, "r.prompt.md")),
            Path::new("/ws/.github/prompts/r.prompt.md")
        );
        assert_eq!(
            destination_path(ws, &mk(Category::Instructions, "copilot-instructions.md")),
            Path::new("/ws/.github/copilot-instructions.md")
        );
        assert_eq!(destination_path(ws, &mk(Category::Instructions, "agents.md")), Path::new("/ws/AGENTS.md"));
        assert_eq!(
            destination_path(ws, &mk(Category::Agents, "p.chatmode.md")),
            Path::new("/ws/.github/chatmodes/p.chatmode.md")
        );
        assert_eq!(destination_path(ws, &mk(Category::Tools, "mcp.json")), Path::new("/ws/.vscode/mcp.json"));
    }

    #[test]
    fn extract_description_reads_front_matter() {
        let text = "---\ndescription: A helpful skill\napplyTo: '**'\n---\n# Title\nbody";
        assert_eq!(extract_description(text), "A helpful skill");
    }

    #[test]
    fn extract_description_falls_back_to_first_line() {
        let text = "# Heading\n\nThis is the intro line.\nmore";
        assert_eq!(extract_description(text), "This is the intro line.");
    }

    #[test]
    fn json_escape_handles_specials() {
        assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }

    #[test]
    fn json_str_field_parses_values() {
        let body = "{\"id\":\"skills-foo\",\"dir\":\"C:\\\\ws\"}";
        assert_eq!(json_str_field(body, "id").as_deref(), Some("skills-foo"));
        assert_eq!(json_str_field(body, "dir").as_deref(), Some("C:\\ws"));
        assert_eq!(json_str_field(body, "missing"), None);
    }

    #[test]
    fn truncate_respects_limit() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "hel…");
    }
}
