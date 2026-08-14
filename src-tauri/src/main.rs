// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;

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

    fn bootstrap_title(&self) -> &'static str {
        "DeepSeek Harness Setup"
    }

    fn bootstrap_step(&self, step: bootstrap::Step) -> String {
        use bootstrap::Step;
        if self.is_zh {
            match step {
                Step::Download => "正在下载 Node.js…".to_string(),
                Step::Extract => "正在解压 Node.js…".to_string(),
                Step::Install => "正在安装 dsh…".to_string(),
            }
        } else {
            match step {
                Step::Download => "Downloading Node.js…".to_string(),
                Step::Extract => "Extracting Node.js…".to_string(),
                Step::Install => "Installing dsh…".to_string(),
            }
        }
    }

    fn bootstrap_failed_title(&self) -> &'static str {
        if self.is_zh { "初始化失败" } else { "Setup Failed" }
    }

    fn bootstrap_failed_msg(&self, tail: &str) -> String {
        if self.is_zh {
            format!("安装 Node.js 与 dsh 失败。\n\n{}\n\n是否重试?(选择「No」将退出应用)", tail)
        } else {
            format!("Failed to install Node.js and dsh.\n\n{}\n\nRetry? (choosing \"No\" quits the app)", tail)
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
    Private { node: PathBuf, dsh: PathBuf },
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
    if let Some((node, dsh)) = bootstrap::private_node_and_dsh(&bootstrap::toolchain_dir()) {
        return DshMode::Private { node, dsh };
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
        DshMode::Private { node, dsh } => (
            node.to_string_lossy().into_owned(),
            vec![dsh.to_string_lossy().into_owned()],
            None,
        ),
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

/// Full HTML page for the bootstrap progress window, served over a loopback
/// HTTP socket (see `serve_bootstrap_html`). WKWebView does not complete
/// navigation for `about:blank` and dev builds embed no assets, so neither
/// asset URLs nor eval-injection reliably render — a real localhost request
/// is the same pattern the main window already uses for the dsh UI.
const BOOTSTRAP_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<title>DeepSeek Harness Setup</title>
<style>
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #ffffff; color: #333333; margin: 0; display: flex; align-items: center; justify-content: center; height: 100vh; }
.wrap { width: 80%; }
p { font-size: 15px; text-align: center; margin: 0 0 18px; }
.bar { height: 6px; background: #e5e5e5; border-radius: 3px; overflow: hidden; }
#fill { height: 100%; width: 0; background: #4c8dff; border-radius: 3px; transition: width .2s ease; }
#fill.indeterminate { width: 40%; animation: slide 1.2s ease-in-out infinite; }
@keyframes slide { 0% { margin-left: -40%; } 100% { margin-left: 100%; } }
</style>
</head>
<body>
<div class="wrap">
  <p id="status">Preparing&hellip;</p>
  <div class="bar"><div id="fill"></div></div>
</div>
<script>
window.__dsbUpdate = function(state, percent) {
  document.getElementById('status').textContent = state;
  var fill = document.getElementById('fill');
  if (percent === null) {
    fill.classList.add('indeterminate');
  } else {
    fill.classList.remove('indeterminate');
    fill.style.width = Math.round(percent * 100) + '%';
  }
};
</script>
</body>
</html>"#;

/// Bind a loopback HTTP socket serving BOOTSTRAP_HTML and return its URL.
/// The listener thread lives until the process exits; every request gets the
/// same page so reloads and retries keep working.
fn serve_bootstrap_html() -> Result<String, String> {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to bind bootstrap server: {}", e))?;
    let url = format!(
        "http://{}/",
        listener.local_addr().map_err(|e| format!("bootstrap server addr: {}", e))?
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = BOOTSTRAP_HTML.as_bytes();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    Ok(url)
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
// First-launch bootstrap orchestration
// ---------------------------------------------------------------------------

/// Exit with an error dialog. Never returns.
fn fail_startup(handle: &tauri::AppHandle, i18n: &I18n, detail: &str) -> ! {
    eprintln!("[desktop] ERROR: {}", detail);
    let _ = handle
        .dialog()
        .message(i18n.start_failed_msg(detail))
        .title("DeepSeek Harness")
        .kind(MessageDialogKind::Error)
        .blocking_show();
    std::process::exit(1);
}

/// Make sure a Node.js (>=22) toolchain exists: reuse a complete private
/// toolchain or an existing system node; otherwise run the interactive
/// bootstrap (progress window + retry dialogs).
async fn ensure_toolchain(handle: &tauri::AppHandle, i18n: &I18n) -> Result<(), String> {
    if bootstrap::private_node_and_dsh(&bootstrap::toolchain_dir()).is_some() {
        return Ok(());
    }
    if find_program("node")
        .map(|p| bootstrap::node_version_ok(&p))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let bootstrap_url = serve_bootstrap_html()?;
    let win = tauri::WebviewWindowBuilder::new(
        handle,
        "bootstrap",
        tauri::WebviewUrl::External(bootstrap_url.parse().expect("invalid bootstrap url")),
    )
    .title(i18n.bootstrap_title())
    .inner_size(520.0, 240.0)
    .center()
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .build()
    .map_err(|e| format!("failed to create bootstrap window: {}", e))?;

    loop {
        let win2 = win.clone();
        let i18n2 = i18n.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            bootstrap::install(move |step, percent| {
                let msg = serde_json::to_string(&i18n2.bootstrap_step(step)).unwrap_or_default();
                let pct = percent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "null".to_string());
                let _ = win2.eval(format!("window.__dsbUpdate({}, {})", msg, pct));
            })
        })
        .await;
        let result = match result {
            Ok(r) => r,
            Err(e) => Err(format!("bootstrap thread failed: {}", e)),
        };
        match result {
            Ok(()) => {
                let _ = win.destroy();
                return Ok(());
            }
            Err(e) => {
                eprintln!("[bootstrap] ERROR: {}", e);
                let tail = &e[e.len().saturating_sub(1500)..];
                let handle2 = handle.clone();
                let i18n3 = i18n.clone();
                let tail = tail.to_string();
                let retry = tauri::async_runtime::spawn_blocking(move || {
                    handle2
                        .dialog()
                        .message(i18n3.bootstrap_failed_msg(&tail))
                        .title(i18n3.bootstrap_failed_title())
                        .kind(MessageDialogKind::Error)
                        .buttons(MessageDialogButtons::YesNo)
                        .blocking_show_with_result()
                        == MessageDialogResult::Yes
                })
                .await
                .unwrap_or(false);
                if !retry {
                    let _ = win.destroy();
                    return Err("bootstrap cancelled by user".to_string());
                }
            }
        }
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

/// Command to run npm via the private toolchain's node + bundled npm-cli.js,
/// when a complete private toolchain exists. Falls back to None otherwise.
fn private_npm_cmd(
    toolchain: &Path,
    prefix: &Path,
    extra: &[&str],
) -> Option<(String, Vec<String>)> {
    let (node, _dsh) = bootstrap::private_node_and_dsh(toolchain)?;
    let npm_cli = bootstrap::npm_cli_from_node(&node);
    if !npm_cli.exists() {
        return None;
    }
    let mut args = vec![npm_cli.to_string_lossy().into_owned()];
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push("--prefix".to_string());
    args.push(prefix.to_string_lossy().into_owned());
    Some((node.to_string_lossy().into_owned(), args))
}

/// Query the npm registry for the latest published dsh version.
fn latest_version() -> String {
    let (cmd, args) = match private_npm_cmd(
        &bootstrap::toolchain_dir(),
        &bootstrap::toolchain_dir(),
        &["view", "@deepseek-ai/dsh", "version"],
    ) {
        Some(pair) => pair,
        None => (
            resolve_program("npm"),
            vec![
                "view".to_string(),
                "@deepseek-ai/dsh".to_string(),
                "version".to_string(),
            ],
        ),
    };
    let output = Command::new(&cmd)
        .args(&args)
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
    } else if let Some((node, args)) = private_npm_cmd(
        &bootstrap::toolchain_dir(),
        &bootstrap::toolchain_dir(),
        &[
            "install",
            "--no-fund",
            "--no-audit",
            "@deepseek-ai/dsh@latest",
        ],
    ) {
        (node, args, None)
    } else {
        (
            resolve_program("npm"),
            vec![
                "install".to_string(),
                "-g".to_string(),
                "@deepseek-ai/dsh@latest".to_string(),
            ],
            None,
        )
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
    let line = format!("[{}] {}", prefix, msg);
    println!("{}", line);
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
    if id == "check_updates" {
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
            let handle = app.handle().clone();
            let i18n = (*app.state::<I18n>()).clone();
            tauri::async_runtime::spawn(async move {
                // ── 1. Toolchain (first-launch bootstrap if needed) ──
                if let Err(e) = ensure_toolchain(&handle, &i18n).await {
                    eprintln!("[desktop] ERROR: {}", e);
                    std::process::exit(1);
                }

                // ── 2. Start dsh web ────────────────────────────────
                let (child, url) = match tauri::async_runtime::spawn_blocking(start_dsh).await {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => fail_startup(&handle, &i18n, &e),
                    Err(e) => fail_startup(&handle, &i18n, &format!("task join: {}", e)),
                };
                *handle.state::<DshProcess>().0.lock().unwrap() = Some(child);

                // ── 3. Create the window, loading the dsh URL ───────
                let _window = tauri::WebviewWindowBuilder::new(
                    &handle,
                    "main",
                    tauri::WebviewUrl::External(url.parse().expect("invalid URL")),
                )
                .title("DeepSeek Harness")
                .inner_size(1200.0, 800.0)
                .build()
                .expect("failed to build main window");

                // ── 4. App menu: update / upgrade ───────────────────
                let check_item = MenuItem::with_id(
                    &handle,
                    "check_updates",
                    i18n.check_updates(),
                    true,
                    None::<&str>,
                )
                .expect("failed to build menu item");
                let submenu =
                    Submenu::with_items(&handle, "DeepSeek Harness", true, &[&check_item])
                        .expect("failed to build submenu");
                let menu = Menu::with_items(&handle, &[&submenu]).expect("failed to build menu");
                handle.set_menu(menu).expect("failed to set menu");

                // ── 5. Auto-check for updates shortly after startup ─
                let handle2 = handle.clone();
                let i18n2 = i18n.clone();
                tauri::async_runtime::spawn(async move {
                    std::thread::sleep(Duration::from_secs(5));
                    let info = check_update();
                    if info.update_available
                        && should_auto_prompt(&info.latest)
                        && ask_yes_no(
                            &handle2,
                            i18n2.update_available_title(),
                            i18n2.update_available_msg(&info.current, &info.latest),
                        )
                    {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle2, &i18n2, result);
                    }
                });
            });
            Ok(())
        })
        .on_menu_event(|app, event| {
            let i18n = (*app.state::<I18n>()).clone();
            on_menu_event(app, &i18n, event.id().as_ref());
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "bootstrap" {
                    api.prevent_close();
                }
            }
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

    fn node_bin_name() -> &'static str {
        if cfg!(windows) {
            "node.exe"
        } else {
            "node"
        }
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
    fn private_mode_runs_node_on_private_dsh_shim() {
        let mode = DshMode::Private {
            node: PathBuf::from("/x/toolchain/node-24.19.0/bin/node"),
            dsh: PathBuf::from("/x/toolchain/node_modules/.bin/dsh"),
        };
        let (cmd, args, cwd) = dsh_runner(&mode, identity);
        assert_eq!(cmd, "/x/toolchain/node-24.19.0/bin/node");
        assert_eq!(args, vec!["/x/toolchain/node_modules/.bin/dsh".to_string()]);
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

    #[test]
    fn bootstrap_copy_is_localized_zh() {
        let i18n = I18n { is_zh: true };
        assert_eq!(i18n.bootstrap_title(), "DeepSeek Harness Setup");
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Download), "正在下载 Node.js…");
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Extract), "正在解压 Node.js…");
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Install), "正在安装 dsh…");
        assert_eq!(i18n.bootstrap_failed_title(), "初始化失败");
        assert!(i18n.bootstrap_failed_msg("boom").contains("重试"));
    }

    #[test]
    fn bootstrap_copy_is_localized_en() {
        let i18n = I18n { is_zh: false };
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Download), "Downloading Node.js…");
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Extract), "Extracting Node.js…");
        assert_eq!(i18n.bootstrap_step(bootstrap::Step::Install), "Installing dsh…");
        assert_eq!(i18n.bootstrap_failed_title(), "Setup Failed");
        assert!(i18n.bootstrap_failed_msg("boom").contains("Retry"));
    }

    #[test]
    fn private_npm_cmd_builds_node_npmcli_args() {
        let root = std::env::temp_dir().join(format!("dsh-npmcmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        std::fs::create_dir_all(tc.join("node-24.19.0/bin")).unwrap();
        std::fs::write(
            tc.join("node-24.19.0").join("bin").join(node_bin_name()),
            b"",
        )
        .unwrap();
        let npm_cli = tc.join("node-24.19.0/lib/node_modules/npm/bin");
        std::fs::create_dir_all(&npm_cli).unwrap();
        std::fs::write(npm_cli.join("npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();

        let (cmd, args) = private_npm_cmd(&tc, &tc, &["view", "@deepseek-ai/dsh", "version"])
            .expect("complete private toolchain");
        assert_eq!(
            cmd,
            tc.join("node-24.19.0")
                .join("bin")
                .join(node_bin_name())
                .to_string_lossy()
        );
        assert_eq!(
            args,
            vec![
                tc.join("node-24.19.0/lib/node_modules/npm/bin/npm-cli.js")
                    .to_string_lossy()
                    .into_owned(),
                "view".to_string(),
                "@deepseek-ai/dsh".to_string(),
                "version".to_string(),
                "--prefix".to_string(),
                tc.to_string_lossy().into_owned(),
            ]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_npm_cmd_none_without_toolchain() {
        let root = std::env::temp_dir().join(format!("dsh-npmcmd-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(private_npm_cmd(&root, &root, &["view"]).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}