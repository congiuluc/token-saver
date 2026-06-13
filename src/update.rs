//! Self-update: check GitHub Releases for a newer version and replace the
//! running binary in place.
//!
//! To preserve the crate's zero-runtime-dependency design, all network and
//! archive work is delegated to tools already present on each platform
//! (`curl`/`wget` or PowerShell for downloads, `tar`/`Expand-Archive` for
//! extraction, `sha256sum`/`shasum`/`Get-FileHash` for checksums).

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The `owner/repo` slug used to build GitHub API and download URLs.
const REPO: &str = "congiuluc/token-saver";

/// The version compiled into this binary (from `Cargo.toml`).
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Entry point for `token-saver update [--check] [--force]`.
pub fn run(args: &[String]) -> ExitCode {
    let mut check_only = false;
    let mut force = false;

    for arg in args {
        match arg.as_str() {
            "--check" | "-c" => check_only = true,
            "--force" | "-f" => force = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("token-saver: unknown update option '{other}'");
                eprintln!("usage: token-saver update [--check] [--force]");
                return ExitCode::from(2);
            }
        }
    }

    println!("token-saver: current version v{CURRENT_VERSION}");
    println!("token-saver: checking {REPO} for the latest release...");

    let latest = match fetch_latest_tag() {
        Ok(tag) => tag,
        Err(err) => {
            eprintln!("token-saver: could not check for updates: {err}");
            return ExitCode::from(1);
        }
    };

    let latest_clean = latest.trim_start_matches(['v', 'V']);
    let up_to_date = !is_newer(&latest, CURRENT_VERSION);

    if up_to_date && !force {
        println!("token-saver: already up to date (latest is v{latest_clean}).");
        return ExitCode::SUCCESS;
    }

    if up_to_date {
        println!("token-saver: reinstalling v{latest_clean} (--force).");
    } else {
        println!("token-saver: new version available: v{CURRENT_VERSION} -> v{latest_clean}");
    }

    if check_only {
        if !up_to_date {
            println!("token-saver: run `token-saver update` to install it.");
        }
        return ExitCode::SUCCESS;
    }

    match perform_update(&latest) {
        Ok(path) => {
            println!("token-saver: updated to v{latest_clean} at {}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("token-saver: update failed: {err}");
            ExitCode::from(1)
        }
    }
}

/// Downloads, verifies, and installs the latest release archive, returning the
/// path of the replaced executable.
fn perform_update(tag: &str) -> io::Result<PathBuf> {
    let target = target_triple()
        .ok_or_else(|| make_err("this platform has no prebuilt release; install with cargo or from source"))?;
    let ext = archive_ext();
    let asset_dir = format!("token-saver-{target}");
    let asset = format!("{asset_dir}.{ext}");
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");

    let work_dir = unique_temp_dir()?;
    let archive_path = work_dir.join(&asset);

    println!("token-saver: downloading {asset}...");
    download(&format!("{base}/{asset}"), &archive_path)?;

    // Best-effort checksum verification: only fails on a genuine mismatch, not
    // when the checksum file or hashing tool is unavailable.
    let sha_path = work_dir.join(format!("{asset}.sha256"));
    if download(&format!("{base}/{asset}.sha256"), &sha_path).is_ok() {
        match verify_checksum(&archive_path, &sha_path) {
            Ok(true) => println!("token-saver: checksum verified."),
            Ok(false) => {
                let _ = fs::remove_dir_all(&work_dir);
                return Err(make_err("checksum verification failed; aborting"));
            }
            Err(_) => eprintln!("token-saver: warning: could not verify checksum, continuing."),
        }
    } else {
        eprintln!("token-saver: warning: checksum file unavailable, continuing.");
    }

    println!("token-saver: extracting...");
    extract(&archive_path, &work_dir)?;

    let extracted = work_dir.join(&asset_dir);
    let current_exe = env::current_exe()?;
    let install_dir = current_exe.parent().unwrap_or_else(|| Path::new("."));

    let bin_name = exe_name("token-saver");
    let new_bin = extracted.join(&bin_name);
    if !new_bin.exists() {
        let _ = fs::remove_dir_all(&work_dir);
        return Err(make_err("downloaded archive did not contain the token-saver binary"));
    }

    replace_binary(&current_exe, &new_bin)?;

    // Best-effort: refresh the sibling `ts` alias if it lives alongside us.
    let ts_name = exe_name("ts");
    let ts_target = install_dir.join(&ts_name);
    let ts_new = extracted.join(&ts_name);
    if ts_new.exists() && ts_target.exists() {
        if let Err(e) = replace_binary(&ts_target, &ts_new) {
            eprintln!("token-saver: warning: could not update '{ts_name}': {e}");
        }
    }

    let _ = fs::remove_dir_all(&work_dir);
    Ok(current_exe)
}

/// Queries the GitHub API for the latest release tag (e.g. `v0.2.0`).
fn fetch_latest_tag() -> io::Result<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let tmp = unique_temp_dir()?;
    let body_path = tmp.join("release.json");
    download(&url, &body_path)?;
    let body = fs::read_to_string(&body_path);
    let _ = fs::remove_dir_all(&tmp);
    let body = body?;
    parse_tag(&body).ok_or_else(|| make_err("no release found (could not read tag_name)"))
}

/// Extracts the `tag_name` field from a GitHub release JSON payload without a
/// JSON dependency.
fn parse_tag(json: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let start = json.find(key)? + key.len();
    let rest = &json[start..];
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    let open = after.find('"')?;
    let value = &after[open + 1..];
    let close = value.find('"')?;
    let tag = value[..close].trim().to_string();
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

/// Parses a version such as `1.2.3` or `v1.2.3` into `(major, minor, patch)`,
/// ignoring any pre-release or build metadata.
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Returns `true` when `latest` is a strictly higher version than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Returns the Rust target triple of the running platform, or `None` if no
/// prebuilt release is published for it.
fn target_triple() -> Option<String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return None;
    };

    let os = if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else {
        return None;
    };

    Some(format!("{arch}-{os}"))
}

/// The archive extension used for this platform's release asset.
fn archive_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    }
}

/// Appends `.exe` to a binary name on Windows.
fn exe_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// Downloads `url` to `dest` using the first available system tool.
fn download(url: &str, dest: &Path) -> io::Result<()> {
    let dest_str = dest.to_string_lossy().to_string();

    let mut candidates: Vec<(&str, Vec<String>)> = vec![
        (
            "curl",
            vec!["-fsSL".into(), "-A".into(), "token-saver-updater".into(), url.into(), "-o".into(), dest_str.clone()],
        ),
        (
            "wget",
            vec!["-q".into(), "-U".into(), "token-saver-updater".into(), "-O".into(), dest_str.clone(), url.into()],
        ),
    ];

    if cfg!(target_os = "windows") {
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; \
             [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; \
             Invoke-WebRequest -Uri '{url}' -OutFile '{dest_str}' \
             -Headers @{{'User-Agent'='token-saver-updater'}} -UseBasicParsing"
        );
        candidates.push(("powershell", vec!["-NoProfile".into(), "-Command".into(), script]));
    }

    let mut last_err: Option<io::Error> = None;
    for (program, args) in &candidates {
        match Command::new(program).args(args).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                last_err = Some(make_err(&format!("{program} exited with status {status}")));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| make_err("no downloader found (need curl, wget, or PowerShell)")))
}

/// Extracts `archive` into `dest_dir` using the platform archive tool.
fn extract(archive: &Path, dest_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dest_dir)?;
    let archive_str = archive.to_string_lossy().to_string();
    let dest_str = dest_dir.to_string_lossy().to_string();

    if archive_str.ends_with(".zip") {
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; \
             Expand-Archive -Path '{archive_str}' -DestinationPath '{dest_str}' -Force"
        );
        run_required("powershell", &["-NoProfile", "-Command", &script])
    } else {
        run_required("tar", &["-xzf", &archive_str, "-C", &dest_str])
    }
}

/// Verifies that `archive` matches the hash stored in `sha_file`.
fn verify_checksum(archive: &Path, sha_file: &Path) -> io::Result<bool> {
    let expected = fs::read_to_string(sha_file)?.split_whitespace().next().unwrap_or("").to_lowercase();
    if expected.is_empty() {
        return Err(make_err("checksum file was empty"));
    }
    let actual = compute_sha256(archive).ok_or_else(|| make_err("no SHA-256 tool available"))?;
    Ok(actual.eq_ignore_ascii_case(&expected))
}

/// Computes the SHA-256 of `path` using the first available system tool.
fn compute_sha256(path: &Path) -> Option<String> {
    let p = path.to_string_lossy().to_string();

    if let Some(out) = capture("sha256sum", &[&p]) {
        return out.split_whitespace().next().map(str::to_lowercase);
    }
    if let Some(out) = capture("shasum", &["-a", "256", &p]) {
        return out.split_whitespace().next().map(str::to_lowercase);
    }
    if cfg!(target_os = "windows") {
        let script = format!("(Get-FileHash -Algorithm SHA256 '{p}').Hash");
        if let Some(out) = capture("powershell", &["-NoProfile", "-Command", &script]) {
            return out.split_whitespace().next().map(str::to_lowercase);
        }
    }
    None
}

/// Replaces the binary at `target` with the file at `src`.
fn replace_binary(target: &Path, src: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let staging = target.with_extension("new");
        fs::copy(src, &staging)?;
        let mut perms = fs::metadata(&staging)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&staging, perms)?;
        fs::rename(&staging, target)?;
        Ok(())
    }

    #[cfg(windows)]
    {
        // A running executable cannot be overwritten on Windows, so move it
        // aside first; it can be removed on the next run.
        let backup = target.with_extension("old");
        let _ = fs::remove_file(&backup);
        if target.exists() {
            fs::rename(target, &backup)?;
        }
        match fs::copy(src, target) {
            Ok(_) => {
                let _ = fs::remove_file(&backup);
                Ok(())
            }
            Err(e) => {
                let _ = fs::rename(&backup, target);
                Err(e)
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        fs::copy(src, target)?;
        Ok(())
    }
}

/// Runs a command and returns an error if it is missing or exits non-zero.
fn run_required(program: &str, args: &[&str]) -> io::Result<()> {
    let status = Command::new(program).args(args).status().map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            make_err(&format!("required tool '{program}' not found"))
        } else {
            e
        }
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(make_err(&format!("{program} exited with status {status}")))
    }
}

/// Runs a command and captures its stdout on success.
fn capture(program: &str, args: &[&str]) -> Option<String> {
    match Command::new(program).args(args).output() {
        Ok(out) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        _ => None,
    }
}

/// Creates a fresh, process-unique temporary directory.
fn unique_temp_dir() -> io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = env::temp_dir().join(format!("token-saver-update-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Builds an `io::Error` with the given message.
fn make_err(message: &str) -> io::Error {
    io::Error::other(message.to_string())
}

/// Prints help for the `update` subcommand.
fn print_help() {
    println!(
        "token-saver update — update token-saver to the latest release\n\
         \n\
         USAGE:\n\
         \x20 token-saver update            Check for and install the latest version\n\
         \x20 token-saver update --check    Only report whether a newer version exists\n\
         \x20 token-saver update --force    Reinstall even if already up to date\n\
         \n\
         The latest release is fetched from https://github.com/{REPO}/releases."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_prefixed_versions() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("V2.0.0"), Some((2, 0, 0)));
    }

    #[test]
    fn fills_missing_components_with_zero() {
        assert_eq!(parse_version("1"), Some((1, 0, 0)));
        assert_eq!(parse_version("1.4"), Some((1, 4, 0)));
    }

    #[test]
    fn ignores_prerelease_and_build_metadata() {
        assert_eq!(parse_version("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3+build.5"), Some((1, 2, 3)));
    }

    #[test]
    fn rejects_garbage_versions() {
        assert_eq!(parse_version("not-a-version"), None);
    }

    #[test]
    fn detects_newer_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn rejects_same_or_older_versions() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
    }

    #[test]
    fn extracts_tag_name_from_json() {
        let json = r#"{"url":"x","tag_name": "v1.4.0", "name":"release"}"#;
        assert_eq!(parse_tag(json), Some("v1.4.0".to_string()));
    }

    #[test]
    fn extracts_tag_name_without_spaces() {
        let json = r#"{"tag_name":"v2.0.1"}"#;
        assert_eq!(parse_tag(json), Some("v2.0.1".to_string()));
    }

    #[test]
    fn returns_none_when_tag_missing() {
        assert_eq!(parse_tag(r#"{"name":"no tag here"}"#), None);
    }

    #[test]
    fn target_triple_is_known_on_supported_platforms() {
        if cfg!(any(target_os = "windows", target_os = "macos", target_os = "linux"))
            && cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        {
            assert!(target_triple().is_some());
        }
    }
}
