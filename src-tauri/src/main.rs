// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use semver::Version;
use tauri::menu::{Menu, MenuItem, Submenu};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

/// Wrapper so we can store the dsh child process in Tauri managed state.
struct DshProcess(Mutex<Option<Child>>);

// ---------------------------------------------------------------------------
// Localization
// ---------------------------------------------------------------------------

/// Localized strings for menu items and dialogs, chosen by the system locale.
/// Chinese (`zh*`) locales get Simplified Chinese copy; everything else English.
#[derive(Clone)]
struct I18n {
    is_zh: bool,
}

impl I18n {
    fn detect() -> Self {
        let is_zh = sys_locale::get_locales().any(|l| l.to_lowercase().starts_with("zh"));
        I18n { is_zh }
    }

    fn check_updates(&self) -> &'static str {
        if self.is_zh { "检查更新…" } else { "Check for Updates…" }
    }

    fn update_available_title(&self) -> &'static str {
        if self.is_zh { "发现新版本" } else { "Update Available" }
    }

    fn up_to_date_title(&self) -> &'static str {
        if self.is_zh { "已是最新版本" } else { "Up to Date" }
    }

    fn upgrade_title(&self) -> &'static str {
        if self.is_zh { "升级 dsh" } else { "Upgrade dsh" }
    }

    fn update_available_msg(&self, current: &str, latest: &str) -> String {
        if self.is_zh {
            format!("发现 dsh 新版本。\n\n  当前版本:  {}\n  最新版本:  {}\n\n立即升级?", current, latest)
        } else {
            format!("A new version of dsh is available.\n\n  Current:  {}\n  Latest:   {}\n\nUpgrade now?", current, latest)
        }
    }

    fn up_to_date_msg(&self, current: &str) -> String {
        if self.is_zh {
            format!("dsh 已是最新版本({})。", current)
        } else {
            format!("dsh is up to date (version {}).", current)
        }
    }

    fn upgrade_success_msg(&self) -> String {
        if self.is_zh {
            "dsh 升级成功。\n\n立即重启以应用更新?".to_string()
        } else {
            "dsh upgraded successfully.\n\nRestart now to apply the update?".to_string()
        }
    }

    fn upgrade_failed_msg(&self, tail: &str) -> String {
        if self.is_zh {
            format!("升级失败(退出码非零)。\n\n{}", tail)
        } else {
            format!("Upgrade failed (exit code non-zero).\n\n{}", tail)
        }
    }

    fn upgrade_error_msg(&self, e: &str) -> String {
        if self.is_zh {
            format!("升级失败:\n{}", e)
        } else {
            format!("Upgrade failed:\n{}", e)
        }
    }

    fn about(&self) -> &'static str {
        if self.is_zh { "关于 DeepSeek Harness" } else { "About DeepSeek Harness" }
    }

    fn help(&self) -> &'static str {
        if self.is_zh { "帮助" } else { "Help" }
    }

    fn feedback(&self) -> &'static str {
        if self.is_zh { "提交反馈" } else { "Submit Feedback" }
    }

    fn export_logs(&self) -> &'static str {
        if self.is_zh { "导出日志" } else { "Export Logs" }
    }

    fn help_menu(&self) -> &'static str {
        if self.is_zh { "帮助" } else { "Help" }
    }

    fn about_msg(&self, version: &str) -> String {
        if self.is_zh {
            format!(
                "DeepSeek Harness 桌面端\n\n  版本:  {}\n  仓库:  https://github.com/hialuoy/deepseek-harness-desktop",
                version
            )
        } else {
            format!(
                "DeepSeek Harness desktop\n\n  Version:  {}\n  Repo:  https://github.com/hialuoy/deepseek-harness-desktop",
                version
            )
        }
    }

    fn export_logs_failed_msg(&self, e: &str) -> String {
        if self.is_zh {
            format!("导出日志失败:\n{}", e)
        } else {
            format!("Failed to export logs:\n{}", e)
        }
    }

    fn start_failed_msg(&self, detail: &str) -> String {
        if self.is_zh {
            format!(
                "无法启动 dsh:\n{}\n\n请确认已安装 Node.js(>=22)并全局安装 @deepseek-ai/dsh:\n  npm install -g @deepseek-ai/dsh",
                detail
            )
        } else {
            format!(
                "Failed to start dsh:\n{}\n\nMake sure Node.js (>=22) is installed and @deepseek-ai/dsh is installed globally:\n  npm install -g @deepseek-ai/dsh",
                detail
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Mode detection
// ---------------------------------------------------------------------------

/// Return the repository root (directory containing `pnpm-workspace.yaml`)
/// when the app lives inside a checkout, or `None` otherwise.
fn find_repo_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut dir = cwd.clone();
    let mut found = None;
    for _ in 0..5 {
        if dir.join("pnpm-workspace.yaml").exists() {
            found = Some(dir.clone());
            break;
        }
        if !dir.pop() {
            break;
        }
    }
    found
}

/// How dsh is provided, detected in priority order:
/// source checkout > bundled app resource > global install > npx registry.
enum DshMode {
    Source(PathBuf),
    Bundled(PathBuf),
    Global(PathBuf),
    Npx,
}

fn detect_dsh_mode() -> DshMode {
    if let Some(root) = find_repo_root() {
        return DshMode::Source(root);
    }
    let bundled_bin = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("../Resources/app/node_modules/.bin/dsh")))
        .filter(|p| p.exists());
    if let Some(bin) = bundled_bin {
        return DshMode::Bundled(bin);
    }
    if let Some(dsh) = find_program("dsh") {
        return DshMode::Global(dsh);
    }
    DshMode::Npx
}

/// (program, base args, cwd) for a detected mode. Program names go through
/// `resolve` so callers can inject absolute-path resolution (or identity in tests).
fn dsh_runner(mode: &DshMode, resolve: impl Fn(&str) -> String) -> (String, Vec<String>, Option<PathBuf>) {
    match mode {
        DshMode::Source(root) => (resolve("pnpm"), vec!["dsh".into()], Some(root.clone())),
        DshMode::Bundled(bin) => (resolve("node"), vec![bin.to_string_lossy().into_owned()], None),
        DshMode::Global(dsh) => (dsh.to_string_lossy().into_owned(), Vec::new(), None),
        DshMode::Npx => (resolve("npx"), vec!["--yes".into(), "@deepseek-ai/dsh".into()], None),
    }
}

/// Resolve how to launch dsh web for the detected mode.
fn resolve_dsh_command() -> (String, Vec<String>, Option<PathBuf>) {
    let (cmd, mut args, cwd) = dsh_runner(&detect_dsh_mode(), resolve_program);
    args.extend(["web".into(), "--port".into(), "0".into()]);
    (cmd, args, cwd)
}

/// Common locations where node/pnpm toolchains live, probed in order.
/// Finder-launched apps inherit a bare PATH (`/usr/bin:/bin:/usr/sbin:/sbin`),
/// so the toolchain must be discovered explicitly.
fn toolchain_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    dirs.extend(newest_nvm_bins(Path::new(&home)));
    dirs.push(PathBuf::from("/usr/local/bin"));
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from(&home).join("Library/pnpm"));
        dirs.push(PathBuf::from("/opt/local/bin"));
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("npm"));
    }
    dirs.push(PathBuf::from(&home).join(".local/bin"));
    dirs
}

/// nvm-managed node bin dirs under ~/.nvm/versions/node, newest semver first.
fn newest_nvm_bins(home: &Path) -> Vec<PathBuf> {
    let nvm_root = home.join(".nvm/versions/node");
    let Ok(entries) = std::fs::read_dir(&nvm_root) else {
        return Vec::new();
    };
    sort_nvm_versions(
        entries
            .filter_map(|e| e.ok())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect(),
    )
}

/// Order nvm version dirs newest-first by parsed semver; drop entries that
/// don't parse as `vX.Y.Z` (aliases, dotfiles, stray files).
fn sort_nvm_versions(entries: Vec<(String, PathBuf)>) -> Vec<PathBuf> {
    let mut versions: Vec<(Version, PathBuf)> = entries
        .into_iter()
        .filter_map(|(name, path)| {
            let v = name.strip_prefix('v').and_then(|v| Version::parse(v).ok())?;
            Some((v, path))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions.into_iter().map(|(_, path)| path.join("bin")).collect()
}

/// PATH for child processes: discovered toolchain dirs first, then the ambient PATH.
fn augmented_path() -> String {
    let mut parts: Vec<String> = toolchain_dirs()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if let Ok(path) = std::env::var("PATH") {
        parts.push(path);
    }
    parts.join(":")
}

/// Executable candidate names for a program: bare name on Unix; `.exe`/`.cmd`
/// npm-style shims plus the bare name on Windows.
fn program_candidates(name: &str, windows: bool) -> Vec<String> {
    if windows {
        vec![format!("{}.exe", name), format!("{}.cmd", name), name.to_string()]
    } else {
        vec![name.to_string()]
    }
}

/// Find a program by name across toolchain dirs and the ambient PATH.
fn find_program(name: &str) -> Option<PathBuf> {
    let windows = cfg!(windows);
    for dir in toolchain_dirs() {
        for cand in program_candidates(name, windows) {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for cand in program_candidates(name, windows) {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Command entry as an absolute path when resolvable, else the bare name.
fn resolve_program(name: &str) -> String {
    find_program(name).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string())
}

/// (program, args) to open a URL in the default browser, per platform.
/// Unknown platforms fall back to xdg-open.
fn open_url_command(os: &str, url: &str) -> (String, Vec<String>) {
    match os {
        "macos" => ("open".to_string(), vec![url.to_string()]),
        "windows" => ("cmd".to_string(), vec!["/C".to_string(), "start".to_string(), url.to_string()]),
        _ => ("xdg-open".to_string(), vec![url.to_string()]),
    }
}

// ---------------------------------------------------------------------------
// dsh process management
// ---------------------------------------------------------------------------

/// Language-neutral failure detail for the "no ready URL" case; the dialog
/// wrapper (`start_failed_msg`) adds localized guidance.
const DSH_NO_URL_MSG: &str =
    "dsh did not print a ready URL (usually means Node.js is not installed or dsh is not installed correctly)";

/// Spawn dsh web and return once the URL line appears. Failure is returned,
/// never a panic — a missing toolchain must explain itself in a dialog.
fn start_dsh() -> Result<(Child, String), String> {
    let (cmd, args, cwd) = resolve_dsh_command();
    log_line(
        "desktop",
        &format!(
            "spawning: {} {} {}",
            cmd,
            args.join(" "),
            cwd.as_ref().map(|d| format!("(cwd: {})", d.display())).unwrap_or_default()
        ),
    );

    let mut proc = Command::new(&cmd);
    proc.args(&args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::inherit());
    proc.env("PATH", augmented_path());
    if let Some(ref dir) = cwd {
        proc.current_dir(dir);
    }
    let mut child = proc
        .spawn()
        .map_err(|e| format!("failed to start `{}`: {}", cmd, e))?;

    let stdout = child.stdout.take().expect("stdout not piped");
    let reader = BufReader::new(stdout);
    let mut url = String::new();
    for line in reader.lines() {
        let line = line.unwrap_or_default();
        log_line("dsh", &line);
        if let Some(rest) = line.strip_prefix("dsh web: ") {
            url = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(')')
                .to_string();
            break;
        }
    }

    if url.is_empty() {
        log_line("desktop", "ERROR: dsh did not print a URL");
        let _ = child.kill();
        return Err(DSH_NO_URL_MSG.to_string());
    }
    log_line("desktop", &format!("dsh ready at {}", url));
    Ok((child, url))
}

/// Kill the tracked dsh child process, if any.
fn kill_dsh(handle: &tauri::AppHandle) {
    let state = handle.state::<DshProcess>();
    let mut guard = state.0.lock().unwrap();
    if let Some(mut child) = guard.take() {
        log_line("desktop", "shutting down dsh");
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Ask a Yes/No question; returns true when the user pressed Yes.
fn ask_yes_no(handle: &tauri::AppHandle, title: &str, message: String) -> bool {
    handle
        .dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::YesNo)
        .blocking_show_with_result()
        == MessageDialogResult::Yes
}

// ---------------------------------------------------------------------------
// Version helpers
// ---------------------------------------------------------------------------

/// Read the running dsh version: `dsh -V` in production, `pnpm dsh -V` in source mode.
fn current_version() -> String {
    let (cmd, mut args, cwd) = dsh_runner(&detect_dsh_mode(), resolve_program);
    args.push("-V".to_string());

    let mut proc = Command::new(&cmd);
    proc.args(&mut args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::null());
    proc.env("PATH", augmented_path());
    if let Some(ref dir) = cwd {
        proc.current_dir(dir);
    }
    if let Ok(output) = proc.output() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return text;
        }
    }
    "unknown".to_string()
}

/// Query the npm registry for the latest published dsh version.
fn latest_version() -> String {
    let output = Command::new(resolve_program("npm"))
        .args(["view", "@deepseek-ai/dsh", "version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("PATH", augmented_path())
        .output();
    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() { "unknown".to_string() } else { text }
        }
        Err(_) => "unknown".to_string(),
    }
}

struct UpdateInfo {
    current: String,
    latest: String,
    update_available: bool,
}

fn check_update() -> UpdateInfo {
    let current = current_version();
    let latest = latest_version();
    let update_available = match (Version::parse(&current), Version::parse(&latest)) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    };
    log_line("desktop", &format!("version check: current={} latest={}", current, latest));
    UpdateInfo { current, latest, update_available }
}

// ---------------------------------------------------------------------------
// Upgrade
// ---------------------------------------------------------------------------

/// Run the appropriate upgrade command for the current mode, streaming lines to stdout.
fn run_upgrade() -> Result<(bool, String), String> {
    let is_source = find_repo_root().is_some();
    let (cmd, args, cwd) = if is_source {
        let root = find_repo_root().unwrap();
        (
            resolve_program("sh"),
            vec![
                "-c".to_string(),
                // --autostash stashes and reapplies local changes across the rebase,
                // so a dirty working tree does not block the update.
                "git pull --rebase --autostash && pnpm install && pnpm run build".to_string(),
            ],
            Some(root),
        )
    } else {
        (resolve_program("npm"), vec!["install".to_string(), "-g".to_string(), "@deepseek-ai/dsh@latest".to_string()], None)
    };

    log_line("desktop", &format!("upgrading: {} {}", cmd, args.join(" ")));

    let mut proc = Command::new(&cmd);
    proc.args(&args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::piped());
    proc.env("PATH", augmented_path());
    if let Some(ref dir) = cwd {
        proc.current_dir(dir);
    }
    let mut child = proc.spawn().map_err(|e| format!("failed to spawn upgrade: {}", e))?;

    let mut output = String::new();
    let stdout = child.stdout.take().expect("stdout not piped");
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap_or_default();
        log_line("upgrade", &line);
        output.push_str(&line);
        output.push('\n');
    }
    let stderr = child.stderr.take().expect("stderr not piped");
    for line in BufReader::new(stderr).lines() {
        let line = line.unwrap_or_default();
        log_line("upgrade:err", &line);
        output.push_str(&line);
        output.push('\n');
    }
    let status = child.wait().map_err(|e| format!("failed to wait: {}", e))?;
    Ok((status.success(), output))
}

/// Kill dsh, relaunch this executable, and exit.
fn restart_app(handle: &tauri::AppHandle) {
    kill_dsh(handle);
    if let Ok(exe) = std::env::current_exe() {
        log_line("desktop", &format!("relaunching {}", exe.display()));
        let _ = Command::new(exe).spawn();
    }
    handle.exit(0);
}

/// Open a URL in the default browser; failures are logged, not surfaced.
fn open_url(url: &str) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let (cmd, args) = open_url_command(os, url);
    match Command::new(&cmd).args(&args).spawn() {
        Ok(_) => log_line("desktop", &format!("opening {}", url)),
        Err(e) => log_line("desktop", &format!("failed to open {}: {}", url, e)),
    }
}

/// Copy the desktop log to a user-chosen location via a save dialog.
/// Cancellation is a no-op; copy failure shows an error dialog.
fn export_logs(handle: &tauri::AppHandle, i18n: &I18n) {
    let default = export_filename(now_unix_secs());
    let Some(path) = handle.dialog().file().set_file_name(&default).blocking_save_file() else {
        return;
    };
    let Ok(path) = path.into_path() else {
        return;
    };
    match std::fs::copy(log_path(), &path) {
        Ok(_) => log_line("desktop", &format!("logs exported to {}", path.display())),
        Err(e) => {
            let _ = handle
                .dialog()
                .message(i18n.export_logs_failed_msg(&e.to_string()))
                .title(i18n.export_logs())
                .kind(MessageDialogKind::Error)
                .blocking_show();
        }
    }
}

/// Keep only meaningful lines from an upgrade run for display: drop
/// tsdown/rollup noise (deprecation warnings, plugin timings, per-package
/// config-file chatter) so a failure dialog shows the actual error.
fn clean_output(output: &str) -> String {
    let kept: Vec<&str> = output
        .lines()
        .filter(|line| {
            let l = line.trim();
            if l.is_empty() { return false; }
            !l.contains(" WARN ")
                && !l.contains("deprecated")
                && !l.contains("PLUGIN_TIMINGS")
                && !l.contains("config file:")
                && !l.contains("Detected dependencies")
                && !l.contains("See more at")
                && !l.contains("Hint:")
                && !l.contains("entry: lib/types")
                && !l.contains("tsconfig:")
                && !l.starts_with("target:")
                && !l.starts_with("- ")
                && !l.starts_with("$ ")
                && !l.starts_with('ℹ')
                && !l.starts_with('✔')
        })
        .collect();
    kept.join("\n")
}

/// Per-user dsh config dir (`~/.dsh`), created on demand by writers.
fn dsh_dir() -> PathBuf {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".dsh")
}

/// State file recording the last auto-prompt so a version nags at most once a day.
fn prompt_state_path() -> PathBuf {
    dsh_dir().join("desktop-update-state.json")
}

/// Path of the rolling desktop log file.
fn log_path() -> PathBuf {
    dsh_dir().join("desktop.log")
}

/// Max log file size before rotation (bytes).
const LOG_MAX_BYTES: u64 = 1024 * 1024;

/// True when a log file of `size` bytes exceeds the rotation cap.
fn should_rotate(size: u64, cap: u64) -> bool {
    size > cap
}

/// Rotate the log at startup: if desktop.log exceeds the cap, move it to
/// desktop.old.log (overwriting any previous rotation). Failures are ignored.
fn rotate_log_if_needed() {
    let path = log_path();
    if let Ok(meta) = std::fs::metadata(&path) {
        if should_rotate(meta.len(), LOG_MAX_BYTES) {
            let _ = std::fs::rename(&path, dsh_dir().join("desktop.old.log"));
        }
    }
}

/// Append one line to `path`, creating parent dirs and the file as needed.
fn append_log_line(path: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", line)
}

/// Write a `[prefix] msg` line to stdout and append it to the desktop log.
/// File failures are silently ignored — logging must never break the app.
fn log_line(prefix: &str, msg: &str) {
    use std::io::Write;
    let line = format!("[{}] {}", prefix, msg);
    let _ = writeln!(std::io::stdout(), "{}", line);
    let _ = append_log_line(&log_path(), &line);
}

/// Current UNIX time in seconds (0 on clock failure).
fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// `YYYYMMDD-HHMMSS` (UTC) for a UNIX timestamp, via civil-from-days
/// (Howard Hinnant's algorithm) — no chrono dependency.
fn timestamp_compact(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", y, m, d, hh, mm, ss)
}

/// Default filename for an exported log bundle.
fn export_filename(secs: i64) -> String {
    format!("dsh-desktop-{}.log", timestamp_compact(secs))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PromptState {
    last_prompted_version: String,
    last_prompted_at: u64,
}

/// Decide whether the startup auto-check may prompt for this latest version:
/// false when the same version was already offered within the last 24 hours.
/// Records the prompt attempt before returning true.
fn should_auto_prompt(latest: &str) -> bool {
    let path = prompt_state_path();
    let state: PromptState = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    if state.last_prompted_version == latest && now.saturating_sub(state.last_prompted_at) < 24 * 3600 {
        return false;
    }
    let next = PromptState { last_prompted_version: latest.to_string(), last_prompted_at: now };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, serde_json::to_string(&next).unwrap_or_default());
    true
}

// ---------------------------------------------------------------------------
// Menu actions
// ---------------------------------------------------------------------------

fn show_upgrade_progress(handle: &tauri::AppHandle, i18n: &I18n, result: Result<(bool, String), String>) {
    let (ok, output) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = handle.dialog()
                .message(i18n.upgrade_error_msg(&e))
                .title(i18n.upgrade_title())
                .kind(MessageDialogKind::Error)
                .blocking_show();
            return;
        }
    };
    if ok {
        // Success shows a clean message; the build log belongs in the console.
        if ask_yes_no(handle, i18n.upgrade_title(), i18n.upgrade_success_msg()) {
            restart_app(handle);
        }
    } else {
        let cleaned = clean_output(&output);
        let tail = &cleaned[cleaned.len().saturating_sub(1500)..];
        let _ = handle
            .dialog()
            .message(i18n.upgrade_failed_msg(tail))
            .title(i18n.upgrade_title())
            .kind(MessageDialogKind::Error)
            .blocking_show();
    }
}

fn on_menu_event(handle: &tauri::AppHandle, i18n: &I18n, id: &str) {
    match id {
        "about" => {
            let _ = handle
                .dialog()
                .message(i18n.about_msg(env!("CARGO_PKG_VERSION")))
                .title(i18n.about())
                .kind(MessageDialogKind::Info)
                .blocking_show();
        }
        "help" => open_url("https://github.com/hialuoy/deepseek-harness-desktop"),
        "feedback" => open_url("https://github.com/hialuoy/deepseek-harness-desktop/issues/new"),
        "export_logs" => export_logs(handle, i18n),
        "check_updates" => {
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let info = check_update();
                if info.update_available {
                    if ask_yes_no(
                        &handle,
                        i18n.update_available_title(),
                        i18n.update_available_msg(&info.current, &info.latest),
                    ) {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle, &i18n, result);
                    }
                } else {
                    let _ = handle
                        .dialog()
                        .message(i18n.up_to_date_msg(&info.current))
                        .title(i18n.up_to_date_title())
                        .kind(MessageDialogKind::Info)
                        .blocking_show();
                }
            });
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// App entry
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .manage(DshProcess(Mutex::new(None)))
        .manage(I18n::detect())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            rotate_log_if_needed();
            // ── 1. Start dsh web ─────────────────────────────────────
            // A missing toolchain must explain itself in a dialog, never crash.
            let (child, url) = match start_dsh() {
                Ok(v) => v,
                Err(e) => {
                    log_line("desktop", &format!("ERROR: {}", e));
                    let i18n = (*app.state::<I18n>()).clone();
                    let _ = app
                        .dialog()
                        .message(i18n.start_failed_msg(&e))
                        .title("DeepSeek Harness")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    std::process::exit(1);
                }
            };
            *app.state::<DshProcess>().0.lock().unwrap() = Some(child);

            // ── 2. Create the window, loading the dsh URL ───────────
            let _window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External(url.parse().expect("invalid URL")),
            )
            .title("DeepSeek Harness")
            .inner_size(1200.0, 800.0)
            .build()?;

            // ── 3. App menu: update / upgrade ────────────────────────
            let i18n = (*app.state::<I18n>()).clone();
            let about_item = MenuItem::with_id(app, "about", i18n.about(), true, None::<&str>)?;
            let check_item = MenuItem::with_id(app, "check_updates", i18n.check_updates(), true, None::<&str>)?;
            let submenu = Submenu::with_items(
                app,
                "DeepSeek Harness",
                true,
                &[&about_item, &check_item],
            )?;
            let help_item = MenuItem::with_id(app, "help", i18n.help(), true, None::<&str>)?;
            let feedback_item = MenuItem::with_id(app, "feedback", i18n.feedback(), true, None::<&str>)?;
            let export_item = MenuItem::with_id(app, "export_logs", i18n.export_logs(), true, None::<&str>)?;
            let help_submenu = Submenu::with_items(
                app,
                i18n.help_menu(),
                true,
                &[&help_item, &feedback_item, &export_item],
            )?;
            let menu = Menu::with_items(app, &[&submenu, &help_submenu])?;
            app.set_menu(menu)?;

            // ── 4. Auto-check for updates shortly after startup ──────
            // Nags at most once per day per version: a dismissed prompt stays
            // quiet until the version changes or a day passes. The manual
            // menu item always checks immediately.
            {
                let handle = app.handle().clone();
                let i18n = i18n.clone();
                tauri::async_runtime::spawn(async move {
                    std::thread::sleep(Duration::from_secs(5));
                    let info = check_update();
                    if info.update_available
                        && should_auto_prompt(&info.latest)
                        && ask_yes_no(
                            &handle,
                            i18n.update_available_title(),
                            i18n.update_available_msg(&info.current, &info.latest),
                        )
                    {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle, &i18n, result);
                    }
                });
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            let i18n = (*app.state::<I18n>()).clone();
            on_menu_event(app, &i18n, event.id().as_ref());
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    kill_dsh(window.app_handle());
                    std::process::exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(name: &str) -> String {
        name.to_string()
    }

    #[test]
    fn nvm_versions_sort_newest_first_by_semver() {
        let entries = vec![
            ("v22.9.0".to_string(), PathBuf::from("a")),
            ("v22.10.0".to_string(), PathBuf::from("b")),
            ("v20.3.1".to_string(), PathBuf::from("c")),
        ];
        assert_eq!(
            sort_nvm_versions(entries),
            vec![
                PathBuf::from("b/bin"),
                PathBuf::from("a/bin"),
                PathBuf::from("c/bin"),
            ]
        );
    }

    #[test]
    fn nvm_entries_that_are_not_versions_are_dropped() {
        let entries = vec![
            ("default".to_string(), PathBuf::from("a")),
            ("v22.9.0".to_string(), PathBuf::from("b")),
            (".lts".to_string(), PathBuf::from("c")),
        ];
        assert_eq!(sort_nvm_versions(entries), vec![PathBuf::from("b/bin")]);
    }

    #[test]
    fn unix_program_candidates_are_just_the_name() {
        assert_eq!(program_candidates("node", false), vec!["node".to_string()]);
    }

    #[test]
    fn windows_program_candidates_include_shims() {
        assert_eq!(
            program_candidates("npm", true),
            vec!["npm.exe".to_string(), "npm.cmd".to_string(), "npm".to_string()]
        );
    }

    #[test]
    fn source_mode_runs_pnpm_dsh_in_repo_root() {
        let root = PathBuf::from("/repo");
        let (cmd, args, cwd) = dsh_runner(&DshMode::Source(root.clone()), identity);
        assert_eq!(cmd, "pnpm");
        assert_eq!(args, vec!["dsh".to_string()]);
        assert_eq!(cwd, Some(root));
    }

    #[test]
    fn bundled_mode_runs_node_on_bundled_bin() {
        let bin = PathBuf::from("/app/Resources/app/node_modules/.bin/dsh");
        let (cmd, args, cwd) = dsh_runner(&DshMode::Bundled(bin.clone()), identity);
        assert_eq!(cmd, "node");
        assert_eq!(args, vec![bin.to_string_lossy().into_owned()]);
        assert_eq!(cwd, None);
    }

    #[test]
    fn global_mode_runs_resolved_dsh_directly() {
        let dsh = PathBuf::from("/opt/homebrew/bin/dsh");
        let (cmd, args, cwd) = dsh_runner(&DshMode::Global(dsh.clone()), identity);
        assert_eq!(cmd, dsh.to_string_lossy().into_owned());
        assert!(args.is_empty());
        assert_eq!(cwd, None);
    }

    #[test]
    fn npx_mode_falls_back_to_registry() {
        let (cmd, args, cwd) = dsh_runner(&DshMode::Npx, identity);
        assert_eq!(cmd, "npx");
        assert_eq!(args, vec!["--yes".to_string(), "@deepseek-ai/dsh".to_string()]);
        assert_eq!(cwd, None);
    }

    #[test]
    fn no_url_error_detail_is_language_neutral_ascii() {
        assert!(DSH_NO_URL_MSG.is_ascii());
        assert!(DSH_NO_URL_MSG.contains("ready URL"));
    }

    #[test]
    fn i18n_help_menu_items_zh_and_en() {
        let zh = I18n { is_zh: true };
        let en = I18n { is_zh: false };
        assert_eq!(zh.about(), "关于 DeepSeek Harness");
        assert_eq!(en.about(), "About DeepSeek Harness");
        assert_eq!(zh.help(), "帮助");
        assert_eq!(en.help(), "Help");
        assert_eq!(zh.feedback(), "提交反馈");
        assert_eq!(en.feedback(), "Submit Feedback");
        assert_eq!(zh.export_logs(), "导出日志");
        assert_eq!(en.export_logs(), "Export Logs");
        assert_eq!(zh.help_menu(), "帮助");
        assert_eq!(en.help_menu(), "Help");
    }

    #[test]
    fn about_msg_contains_version_and_repo() {
        let zh = I18n { is_zh: true };
        let msg = zh.about_msg("0.1.0");
        assert!(msg.contains("0.1.0"));
        assert!(msg.contains("https://github.com/hialuoy/deepseek-harness-desktop"));
        let en_msg = I18n { is_zh: false }.about_msg("0.1.0");
        assert!(en_msg.contains("0.1.0"));
        assert!(en_msg.contains("https://github.com/hialuoy/deepseek-harness-desktop"));
    }

    #[test]
    fn export_logs_failed_msg_zh_and_en() {
        assert_eq!(
            I18n { is_zh: true }.export_logs_failed_msg("boom"),
            "导出日志失败:\nboom"
        );
        assert_eq!(
            I18n { is_zh: false }.export_logs_failed_msg("boom"),
            "Failed to export logs:\nboom"
        );
    }

    #[test]
    fn open_url_command_per_os() {
        let url = "https://example.com";
        assert_eq!(open_url_command("macos", url), ("open".to_string(), vec![url.to_string()]));
        assert_eq!(
            open_url_command("windows", url),
            ("cmd".to_string(), vec!["/C".to_string(), "start".to_string(), url.to_string()])
        );
        assert_eq!(open_url_command("linux", url), ("xdg-open".to_string(), vec![url.to_string()]));
        assert_eq!(open_url_command("freebsd", url), ("xdg-open".to_string(), vec![url.to_string()]));
    }

    #[test]
    fn timestamp_compact_known_epochs_utc() {
        assert_eq!(timestamp_compact(0), "19700101-000000");
        assert_eq!(timestamp_compact(946_684_800), "20000101-000000");
        assert_eq!(timestamp_compact(951_782_400), "20000229-000000");
        assert_eq!(timestamp_compact(951_868_800), "20000301-000000");
        assert_eq!(timestamp_compact(951_868_800 + 36_000), "20000301-100000");
    }

    #[test]
    fn export_filename_format() {
        assert_eq!(export_filename(951_868_800), "dsh-desktop-20000301-000000.log");
    }

    #[test]
    fn should_rotate_only_above_cap() {
        assert!(!should_rotate(1024 * 1024, 1024 * 1024));
        assert!(should_rotate(1024 * 1024 + 1, 1024 * 1024));
    }

    #[test]
    fn append_log_line_creates_dirs_and_appends() {
        let dir = std::env::temp_dir().join(format!("dsh-log-test-{}", std::process::id()));
        let path = dir.join("nested").join("test.log");
        let _ = std::fs::remove_dir_all(&dir);
        append_log_line(&path, "first").unwrap();
        append_log_line(&path, "second").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\nsecond\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}