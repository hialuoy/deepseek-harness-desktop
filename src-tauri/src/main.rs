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

    fn upgrade(&self) -> &'static str {
        if self.is_zh { "升级 dsh…" } else { "Upgrade dsh…" }
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
    println!(
        "[desktop] spawning: {} {} {}",
        cmd,
        args.join(" "),
        cwd.as_ref().map(|d| format!("(cwd: {})", d.display())).unwrap_or_default()
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
        println!("[dsh] {}", line);
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
        eprintln!("[desktop] ERROR: dsh did not print a URL");
        let _ = child.kill();
        return Err(DSH_NO_URL_MSG.to_string());
    }
    println!("[desktop] dsh ready at {}", url);
    Ok((child, url))
}

/// Kill the tracked dsh child process, if any.
fn kill_dsh(handle: &tauri::AppHandle) {
    let state = handle.state::<DshProcess>();
    let mut guard = state.0.lock().unwrap();
    if let Some(mut child) = guard.take() {
        println!("[desktop] shutting down dsh");
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
    println!("[desktop] version check: current={} latest={}", current, latest);
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

    println!("[desktop] upgrading: {} {}", cmd, args.join(" "));

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
        println!("[upgrade] {}", line);
        output.push_str(&line);
        output.push('\n');
    }
    let stderr = child.stderr.take().expect("stderr not piped");
    for line in BufReader::new(stderr).lines() {
        let line = line.unwrap_or_default();
        println!("[upgrade:err] {}", line);
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
        println!("[desktop] relaunching {}", exe.display());
        let _ = Command::new(exe).spawn();
    }
    handle.exit(0);
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

/// State file recording the last auto-prompt so a version nags at most once a day.
fn prompt_state_path() -> PathBuf {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".dsh").join("desktop-update-state.json")
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
        "upgrade" => {
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = run_upgrade();
                show_upgrade_progress(&handle, &i18n, result);
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
            // ── 1. Start dsh web ─────────────────────────────────────
            // A missing toolchain must explain itself in a dialog, never crash.
            let (child, url) = match start_dsh() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[desktop] ERROR: {}", e);
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
            let check_item = MenuItem::with_id(app, "check_updates", i18n.check_updates(), true, None::<&str>)?;
            let upgrade_item = MenuItem::with_id(app, "upgrade", i18n.upgrade(), true, None::<&str>)?;
            let submenu = Submenu::with_items(
                app,
                "DeepSeek Harness",
                true,
                &[&check_item, &upgrade_item],
            )?;
            let menu = Menu::with_items(app, &[&submenu])?;
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
}