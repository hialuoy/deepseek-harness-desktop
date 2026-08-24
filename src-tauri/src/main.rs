// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use semver::Version;
#[cfg(not(target_os = "windows"))]
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Manager;
use tauri_plugin_dialog::{
    DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
};
use tauri_plugin_updater::UpdaterExt;

/// Prevents overlapping npm installs when the user triggers check/update twice.
static UPGRADE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct UpgradeInProgressGuard;

impl UpgradeInProgressGuard {
    fn try_acquire() -> Option<Self> {
        if UPGRADE_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Some(Self)
        } else {
            None
        }
    }
}

impl Drop for UpgradeInProgressGuard {
    fn drop(&mut self) {
        UPGRADE_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

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
        if self.is_zh {
            "检查更新…"
        } else {
            "Check for Updates…"
        }
    }

    fn update_available_title(&self) -> &'static str {
        if self.is_zh {
            "发现新版本"
        } else {
            "Update Available"
        }
    }

    fn up_to_date_title(&self) -> &'static str {
        if self.is_zh {
            "已是最新版本"
        } else {
            "Up to Date"
        }
    }

    fn upgrade_title(&self) -> &'static str {
        if self.is_zh {
            "升级 dsh"
        } else {
            "Upgrade dsh"
        }
    }

    fn update_available_msg(&self, current: &str, latest: &str) -> String {
        if self.is_zh {
            format!(
                "发现 dsh 新版本。\n\n  当前版本:  {}\n  最新版本:  {}\n\n立即升级?",
                current, latest
            )
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

    fn app_update_title(&self) -> &'static str {
        if self.is_zh {
            "更新 DeepSeek Harness"
        } else {
            "Update DeepSeek Harness"
        }
    }

    fn app_update_msg(&self, current: &str, latest: &str) -> String {
        if self.is_zh {
            format!(
                "发现 DeepSeek Harness 新版本。\n\n  当前版本:  {}\n  最新版本:  {}\n\n是否下载并安装?",
                current, latest
            )
        } else {
            format!(
                "A new version of DeepSeek Harness is available.\n\n  Current:  {}\n  Latest:   {}\n\nDownload and install now?",
                current, latest
            )
        }
    }

    fn app_up_to_date_msg(&self, current: &str) -> String {
        if self.is_zh {
            format!("DeepSeek Harness 已是最新版本({})。", current)
        } else {
            format!("DeepSeek Harness is up to date (version {}).", current)
        }
    }

    fn app_update_failed_msg(&self, e: &str) -> String {
        if self.is_zh {
            format!("应用更新失败:\n{}", e)
        } else {
            format!("App update failed:\n{}", e)
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
        if self.is_zh {
            "关于 DeepSeek Harness"
        } else {
            "About DeepSeek Harness"
        }
    }

    fn help(&self) -> &'static str {
        if self.is_zh {
            "帮助"
        } else {
            "Help"
        }
    }

    fn feedback(&self) -> &'static str {
        if self.is_zh {
            "提交反馈"
        } else {
            "Submit Feedback"
        }
    }

    fn export_logs(&self) -> &'static str {
        if self.is_zh {
            "导出日志"
        } else {
            "Export Logs"
        }
    }

    fn help_menu(&self) -> &'static str {
        if self.is_zh {
            "帮助"
        } else {
            "Help"
        }
    }

    fn edit_menu(&self) -> &'static str {
        if self.is_zh {
            "编辑"
        } else {
            "Edit"
        }
    }

    fn undo(&self) -> &'static str {
        if self.is_zh {
            "撤销"
        } else {
            "Undo"
        }
    }

    fn redo(&self) -> &'static str {
        if self.is_zh {
            "重做"
        } else {
            "Redo"
        }
    }

    fn cut(&self) -> &'static str {
        if self.is_zh {
            "剪切"
        } else {
            "Cut"
        }
    }

    fn copy(&self) -> &'static str {
        if self.is_zh {
            "复制"
        } else {
            "Copy"
        }
    }

    fn paste(&self) -> &'static str {
        if self.is_zh {
            "粘贴"
        } else {
            "Paste"
        }
    }

    fn select_all(&self) -> &'static str {
        if self.is_zh {
            "全选"
        } else {
            "Select All"
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn hide(&self) -> &'static str {
        if self.is_zh {
            "隐藏"
        } else {
            "Hide"
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn hide_others(&self) -> &'static str {
        if self.is_zh {
            "隐藏其他"
        } else {
            "Hide Others"
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn show_all(&self) -> &'static str {
        if self.is_zh {
            "全部显示"
        } else {
            "Show All"
        }
    }

    fn quit(&self) -> &'static str {
        if self.is_zh {
            "退出"
        } else {
            "Quit"
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn services(&self) -> &'static str {
        if self.is_zh {
            "服务"
        } else {
            "Services"
        }
    }

    fn file_menu(&self) -> &'static str {
        if self.is_zh {
            "文件"
        } else {
            "File"
        }
    }

    fn view_menu(&self) -> &'static str {
        if self.is_zh {
            "显示"
        } else {
            "View"
        }
    }

    fn enter_full_screen(&self) -> &'static str {
        if self.is_zh {
            "进入全屏"
        } else {
            "Enter Full Screen"
        }
    }

    fn window_menu(&self) -> &'static str {
        if self.is_zh {
            "窗口"
        } else {
            "Window"
        }
    }

    fn minimize(&self) -> &'static str {
        if self.is_zh {
            "最小化"
        } else {
            "Minimize"
        }
    }

    fn zoom(&self) -> &'static str {
        if self.is_zh {
            "缩放"
        } else {
            "Zoom"
        }
    }

    fn close_window(&self) -> &'static str {
        if self.is_zh {
            "关闭窗口"
        } else {
            "Close Window"
        }
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
        if self.is_zh {
            "初始化失败"
        } else {
            "Setup Failed"
        }
    }

    /// Shown in the bootstrap window when the dsh install has been running
    /// for a while, so the user knows the app is working, not stuck.
    fn bootstrap_slow_msg(&self) -> String {
        if self.is_zh {
            "安装仍在进行,请耐心等待…(首次安装通常需要 1-2 分钟)".to_string()
        } else {
            "Installation is still in progress, please wait… (first install usually takes 1-2 minutes)".to_string()
        }
    }

    fn bootstrap_failed_msg(&self, tail: &str) -> String {
        if self.is_zh {
            format!(
                "安装 Node.js 与 dsh 失败。\n\n{}\n\n是否重试?(选择「No」将退出应用)",
                tail
            )
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
        .and_then(|exe| {
            exe.parent()
                .map(|p| p.join("../Resources/app/node_modules/.bin/dsh"))
        })
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
fn dsh_runner(
    mode: &DshMode,
    resolve: impl Fn(&str) -> String,
) -> (String, Vec<String>, Option<PathBuf>) {
    match mode {
        DshMode::Source(root) => (resolve("pnpm"), vec!["dsh".into()], Some(root.clone())),
        DshMode::Bundled(bin) => (
            resolve("node"),
            vec![bin.to_string_lossy().into_owned()],
            None,
        ),
        DshMode::Global(dsh) => (dsh.to_string_lossy().into_owned(), Vec::new(), None),
        DshMode::Private { node, dsh } => {
            if cfg!(windows) {
                // On Windows `dsh` is the npm `.cmd` shim; run it directly so
                // Rust wraps it in cmd.exe. Running it through node would fail
                // because the extensionless shim is a POSIX shell script.
                (dsh.to_string_lossy().into_owned(), Vec::new(), None)
            } else {
                (
                    node.to_string_lossy().into_owned(),
                    vec![dsh.to_string_lossy().into_owned()],
                    None,
                )
            }
        }
        DshMode::Npx => (
            resolve("npx"),
            vec!["--yes".into(), "@deepseek-ai/dsh".into()],
            None,
        ),
    }
}

/// Resolve how to launch dsh web for the detected mode.
fn resolve_dsh_command() -> (String, Vec<String>, Option<PathBuf>) {
    let (cmd, mut args, cwd) = dsh_runner(&detect_dsh_mode(), resolve_program);
    // dsh web opens the system browser by default; the desktop shell loads
    // the URL in its own WebView instead.
    args.extend([
        "web".into(),
        "--port".into(),
        "0".into(),
        "--no-open".into(),
    ]);
    (cmd, args, cwd)
}

const DSH_LATEST: &str = "@deepseek-ai/dsh@latest";

/// How to spawn an upgrade subprocess.
struct UpgradePlan {
    cmd: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    /// When set, use this PATH instead of [`augmented_path()`] so another
    /// node tree (e.g. the private bootstrap toolchain) cannot shadow npm.
    path: Option<String>,
}

/// Node install root for shims living in `{root}/bin/{name}` (nvm, Homebrew node).
fn node_install_root_from_shim(shim: &Path) -> Option<PathBuf> {
    let bin_dir = shim.parent()?;
    let root = bin_dir.parent()?;
    if bin_dir.file_name()?.to_string_lossy() != "bin" {
        return None;
    }
    Some(root.to_path_buf())
}

fn npm_shim_name() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn npm_global_install_args() -> Vec<String> {
    vec![
        "install".to_string(),
        "-g".to_string(),
        "--no-fund".to_string(),
        "--no-audit".to_string(),
        DSH_LATEST.to_string(),
    ]
}

/// PATH scoped to one node install so postinstall scripts pick the right `node`.
fn upgrade_path_for_node_bin(node_bin: &Path) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut parts = vec![node_bin.to_string_lossy().into_owned()];
    if cfg!(windows) {
        if let Ok(path) = std::env::var("PATH") {
            parts.push(path);
        }
    } else {
        parts.extend(["/usr/bin".to_string(), "/bin".to_string()]);
    }
    parts.join(sep)
}

/// Upgrade dsh for a global shim (`{root}/bin/dsh`). Uses that tree's npm
/// with `-g` and an isolated PATH — not `--prefix {root}`, which would install
/// into `{root}/node_modules` while nvm global packages live under
/// `{root}/lib/node_modules` where the shim points.
fn npm_upgrade_plan_for_shim(shim: &Path) -> Option<UpgradePlan> {
    let root = node_install_root_from_shim(shim)?;
    let npm = root.join("bin").join(npm_shim_name());
    if !npm.is_file() {
        return None;
    }
    let node_bin = root.join("bin");
    Some(UpgradePlan {
        cmd: npm.to_string_lossy().into_owned(),
        args: npm_global_install_args(),
        cwd: None,
        path: Some(upgrade_path_for_node_bin(&node_bin)),
    })
}

/// Upgrade command aligned with [`detect_dsh_mode`]: the install target must
/// match the dsh binary used by [`current_version`] and [`resolve_dsh_command`].
fn upgrade_runner(mode: &DshMode, resolve: impl Fn(&str) -> String) -> UpgradePlan {
    match mode {
        DshMode::Source(root) => UpgradePlan {
            cmd: resolve("sh"),
            args: vec![
                "-c".to_string(),
                // --autostash stashes and reapplies local changes across the rebase,
                // so a dirty working tree does not block the update.
                "git pull --rebase --autostash && pnpm install && pnpm run build".to_string(),
            ],
            cwd: Some(root.clone()),
            path: None,
        },
        DshMode::Private { node, .. } => {
            if let Some((node_cmd, args)) = private_npm_cmd(
                &bootstrap::toolchain_dir(),
                &bootstrap::toolchain_dir(),
                &["install", "--no-fund", "--no-audit", DSH_LATEST],
            ) {
                UpgradePlan {
                    cmd: node_cmd,
                    args,
                    cwd: None,
                    path: node.parent().map(upgrade_path_for_node_bin),
                }
            } else {
                global_npm_upgrade(resolve)
            }
        }
        DshMode::Global(dsh) => {
            npm_upgrade_plan_for_shim(dsh).unwrap_or_else(|| global_npm_upgrade(resolve))
        }
        // Bundled copy is read-only; best-effort global install until the app itself updates.
        DshMode::Bundled(bin) => {
            npm_upgrade_plan_for_shim(bin).unwrap_or_else(|| global_npm_upgrade(resolve))
        }
        DshMode::Npx => global_npm_upgrade(resolve),
    }
}

/// Fallback when no node prefix can be derived: `npm install -g`.
fn global_npm_upgrade(resolve: impl Fn(&str) -> String) -> UpgradePlan {
    UpgradePlan {
        cmd: resolve("npm"),
        args: vec![
            "install".to_string(),
            "-g".to_string(),
            "--no-fund".to_string(),
            "--no-audit".to_string(),
            DSH_LATEST.to_string(),
        ],
        cwd: None,
        path: None,
    }
}

/// Common locations where node/pnpm toolchains live, probed in order.
/// Finder-launched apps inherit a bare PATH (`/usr/bin:/bin:/usr/sbin:/sbin`),
/// so the toolchain must be discovered explicitly.
fn toolchain_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
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
            let v = name
                .strip_prefix('v')
                .and_then(|v| Version::parse(v).ok())?;
            Some((v, path))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions
        .into_iter()
        .map(|(_, path)| path.join("bin"))
        .collect()
}

/// PATH for child processes: private toolchain node bin first, then the
/// discovered toolchain dirs, then the ambient PATH.
fn augmented_path() -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut parts: Vec<String> = Vec::new();
    // Private node bin FIRST: npm `.cmd` shims resolve `node` by name, and in
    // private mode this is the only node guaranteed to be >=22. A stale global
    // or nvm node (the <22 version that triggered bootstrap) must not shadow it.
    parts.extend(
        bootstrap::private_node_bin_dirs(&bootstrap::toolchain_dir())
            .iter()
            .map(|d| d.to_string_lossy().into_owned()),
    );
    parts.extend(
        toolchain_dirs()
            .iter()
            .map(|d| d.to_string_lossy().into_owned()),
    );
    if let Ok(path) = std::env::var("PATH") {
        parts.push(path);
    }
    parts.join(sep)
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

/// Full HTML page for the startup loading window, shown immediately while dsh
/// boots in the background. The `__STATUS__` placeholder is replaced with
/// localized copy by `loading_html`.
const LOADING_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<title>DeepSeek Harness</title>
<style>
:root {
  --boot-bg: #fff;
  --boot-label-primary: #0f1115;
  --boot-label-tertiary: #81858c;
  --boot-border: rgb(0 0 0 / 10%);
  --boot-brand: #0f1115;
}
@media (prefers-color-scheme: dark) {
  :root {
    --boot-bg: #151517;
    --boot-label-primary: #f9fafb;
    --boot-label-tertiary: #adb2b8;
    --boot-border: rgb(255 255 255 / 12%);
    --boot-brand: #f9fafb;
  }
}
html, body { height: 100%; margin: 0; }
body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Helvetica Neue", Helvetica, Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  color: var(--boot-label-primary);
  background: var(--boot-bg);
}
.boot { height: 100%; display: grid; place-items: center; }
.card { display: flex; flex-direction: column; align-items: center; gap: 16px; }
.wordmark { font-size: 16px; line-height: 24px; font-weight: 600; letter-spacing: 0.08em; color: var(--boot-label-primary); }
.hint { font-size: 12px; line-height: 18px; color: var(--boot-label-tertiary); }
.spinner {
  position: relative;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid var(--boot-border);
  animation: spin 0.8s linear infinite;
}
.spinner::after {
  content: '';
  position: absolute;
  inset: -2px;
  border-radius: inherit;
  background: conic-gradient(var(--boot-brand) var(--boot-arc, 72deg), transparent 0);
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 0);
  mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 0);
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
</head>
<body>
<div class="boot">
  <div class="card">
    <div class="wordmark">HARNESS</div>
    <div class="spinner"></div>
    <div class="hint">Starting…</div>
  </div>
</div>
</body>
</html>"#;

/// 自定义标题栏注入脚本模板。`__LABELS__` 占位符会被 [`titlebar_script`]
/// 替换为按系统语言生成的菜单文案 JSON。脚本在每次页面导航时由 WebView
/// 注入,负责在页面顶部渲染「菜单 + 最小化/最大化/关闭」这一行。
///
/// 仅 Windows 使用:该平台把原生标题栏(含三按钮)与菜单栏分成两行,无法
/// 通过配置合并,因此关闭原生装饰后由前端自绘标题栏;其余平台保留原生
/// 标题栏与原生菜单,不走此脚本。
#[cfg(target_os = "windows")]
const TITLEBAR_SCRIPT_TEMPLATE: &str = r#"(function () {
  if (window.__DSH_TITLEBAR__) return;
  window.__DSH_TITLEBAR__ = true;

  // 由 titlebar_script 注入的中英双语菜单文案。
  var LABELS = __LABELS__;
  var BAR_HEIGHT = 40;
  var maxBtnEl = null;

  // 通过 Tauri 内部 IPC 桥调用后端命令(与 withGlobalTauri 无关)。
  function invoke(cmd, args) {
    try {
      return window.__TAURI_INTERNALS__.invoke(cmd, args || {});
    } catch (e) {
      return Promise.reject(e);
    }
  }

  // 编辑类命令尽量贴近 WebView 原生编辑菜单行为。
  function exec(cmd) {
    return function () {
      try { document.execCommand(cmd); } catch (e) {}
    };
  }

  var CSS = [
    '#__dsh_titlebar__{--tb-bg:#f6f6f8;--tb-fg:#1b1b1f;--tb-hover:rgba(0,0,0,0.06);--tb-border:rgba(0,0,0,0.08);position:fixed;top:0;left:0;right:0;height:' + BAR_HEIGHT + 'px;display:flex;align-items:stretch;z-index:2147483000;background:var(--tb-bg);color:var(--tb-fg);font:13px/1 -apple-system,BlinkMacSystemFont,"Segoe UI","Microsoft YaHei",sans-serif;-webkit-user-select:none;user-select:none;}',
    '@media (prefers-color-scheme:dark){#__dsh_titlebar__{--tb-bg:#1f1f21;--tb-fg:#e9e9ec;--tb-hover:rgba(255,255,255,0.08);--tb-border:rgba(255,255,255,0.10);}}',
    '#__dsh_titlebar__ .tb-menus{display:flex;align-items:center;gap:2px;padding:0 8px;}',
    '#__dsh_titlebar__ .tb-menu{position:relative;display:flex;}',
    // 菜单按钮的悬浮背景尽量贴合文字(仅横向留少量余白),窗口按钮 .tb-wb 保持通高不变。
    '#__dsh_titlebar__ .tb-btn{display:flex;align-items:center;gap:7px;height:26px;padding:0 4px;border-radius:5px;background:none;border:none;color:inherit;font:inherit;cursor:pointer;}',
    '#__dsh_titlebar__ .tb-btn:hover,#__dsh_titlebar__ .tb-btn.open{background:var(--tb-hover);}',
    '#__dsh_titlebar__ .tb-app .tb-btn{font-weight:600;}',
    '#__dsh_titlebar__ .tb-appicon{width:16px;height:16px;border-radius:3px;display:block;pointer-events:none;}',
    '#__dsh_titlebar__ .tb-dropdown{position:absolute;top:calc(100% + 2px);left:0;min-width:232px;display:none;background:var(--tb-bg);border:1px solid var(--tb-border);border-radius:10px;box-shadow:0 12px 32px rgba(0,0,0,0.20);padding:6px;z-index:2147483001;}',
    '#__dsh_titlebar__ .tb-menu.open .tb-dropdown{display:block;}',
    '#__dsh_titlebar__ .tb-item{display:block;width:100%;padding:6px 10px;border-radius:4px;background:none;border:none;color:inherit;font:inherit;text-align:left;cursor:pointer;white-space:nowrap;}',
    '#__dsh_titlebar__ .tb-item:hover{background:var(--tb-hover);}',
    '#__dsh_titlebar__ .tb-sep{height:1px;margin:5px 8px;background:var(--tb-border);}',
    '#__dsh_titlebar__ .tb-drag{flex:1;}',
    '#__dsh_titlebar__ .tb-wins{display:flex;align-items:stretch;height:100%;}',
    '#__dsh_titlebar__ .tb-wb{display:flex;align-items:center;justify-content:center;width:46px;height:100%;border-radius:0;background:none;border:none;color:inherit;cursor:pointer;}',
    '#__dsh_titlebar__ .tb-wb:hover{background:var(--tb-hover);}',
    '#__dsh_titlebar__ .tb-wb.tb-close:hover{background:#e81123;color:#ffffff;}',
    '#__dsh_titlebar__ svg{display:block;}'
  ].join('\n');

  // 简洁自绘窗口按钮图标,随 currentColor 适配深浅主题;应用图标使用程序真实图标图片。
  var ICONS = {
    app: '__APP_ICON__',
    min: '<svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1"/></svg>',
    max: '<svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
    restore: '<svg width="10" height="10" viewBox="0 0 10 10"><path d="M2.5 2.5 V0.5 H9.5 V7.5 H7.5" fill="none" stroke="currentColor" stroke-width="1"/><rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/></svg>',
    close: '<svg width="10" height="10" viewBox="0 0 10 10"><path d="M0.5 0.5 L9.5 9.5 M9.5 0.5 L0.5 9.5" stroke="currentColor" stroke-width="1"/></svg>'
  };

  function item(label, action) { return { label: label, action: action }; }
  function separator() { return { separator: true }; }

  function buildMenus() {
    return [
      { label: '', icon: ICONS.app, items: [
        item(LABELS.about, function () { invoke('menu_action', { action: 'about' }); }),
        item(LABELS.checkUpdates, function () { invoke('menu_action', { action: 'check_updates' }); }),
        separator(),
        item(LABELS.quit, function () { invoke('menu_action', { action: 'quit' }); })
      ] },
      { label: LABELS.file, items: [
        item(LABELS.closeWindow, function () { invoke('window_close'); })
      ] },
      { label: LABELS.edit, items: [
        item(LABELS.undo, exec('undo')),
        item(LABELS.redo, exec('redo')),
        separator(),
        item(LABELS.cut, exec('cut')),
        item(LABELS.copy, exec('copy')),
        item(LABELS.paste, exec('paste')),
        separator(),
        item(LABELS.selectAll, exec('selectAll'))
      ] },
      { label: LABELS.view, items: [
        item(LABELS.fullscreen, function () { invoke('window_toggle_fullscreen'); })
      ] },
      { label: LABELS.windowMenu, items: [
        item(LABELS.minimize, function () { invoke('window_minimize'); }),
        item(LABELS.zoom, function () { toggleMaximize(); }),
        separator(),
        item(LABELS.closeWindow, function () { invoke('window_close'); })
      ] },
      { label: LABELS.help, items: [
        item(LABELS.helpItem, function () { invoke('menu_action', { action: 'help' }); }),
        item(LABELS.feedback, function () { invoke('menu_action', { action: 'feedback' }); }),
        item(LABELS.exportLogs, function () { invoke('menu_action', { action: 'export_logs' }); })
      ] }
    ];
  }

  function toggleMaximize() {
    invoke('window_toggle_maximize').then(function (isMax) {
      if (maxBtnEl) maxBtnEl.innerHTML = isMax ? ICONS.restore : ICONS.max;
    });
  }

  function closeAllMenus(root) {
    var open = root.querySelectorAll('.tb-menu.open');
    for (var i = 0; i < open.length; i++) open[i].classList.remove('open');
  }

  function mount() {
    if (document.getElementById('__dsh_titlebar__')) return;

    var style = document.createElement('style');
    style.textContent = CSS;
    (document.head || document.documentElement).appendChild(style);

    // 标题栏固定悬浮在顶部,内容区向下让出标题栏高度。
    document.body.style.boxSizing = 'border-box';
    document.body.style.paddingTop = BAR_HEIGHT + 'px';

    var bar = document.createElement('div');
    bar.id = '__dsh_titlebar__';

    var menusWrap = document.createElement('div');
    menusWrap.className = 'tb-menus';

    buildMenus().forEach(function (menu) {
      var holder = document.createElement('div');
      holder.className = menu.icon ? 'tb-menu tb-app' : 'tb-menu';

      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'tb-btn';
      if (menu.icon) {
        if (menu.icon.indexOf('data:') === 0) {
          var img = document.createElement('img');
          img.className = 'tb-appicon';
          img.src = menu.icon;
          img.alt = '';
          btn.appendChild(img);
        } else {
          var tmp = document.createElement('span');
          tmp.innerHTML = menu.icon;
          btn.appendChild(tmp);
        }
        if (menu.label) {
          var span = document.createElement('span');
          span.textContent = menu.label;
          btn.appendChild(span);
        }
      } else {
        btn.textContent = menu.label;
      }
      btn.addEventListener('click', function (e) {
        e.stopPropagation();
        var wasOpen = holder.classList.contains('open');
        closeAllMenus(bar);
        if (!wasOpen) holder.classList.add('open');
      });

      var dd = document.createElement('div');
      dd.className = 'tb-dropdown';
      menu.items.forEach(function (mi) {
        if (mi.separator) {
          var sep = document.createElement('div');
          sep.className = 'tb-sep';
          dd.appendChild(sep);
          return;
        }
        var it = document.createElement('button');
        it.type = 'button';
        it.className = 'tb-item';
        it.textContent = mi.label;
        it.addEventListener('click', function () {
          closeAllMenus(bar);
          mi.action();
        });
        dd.appendChild(it);
      });

      holder.appendChild(btn);
      holder.appendChild(dd);
      menusWrap.appendChild(holder);
    });

    var drag = document.createElement('div');
    drag.className = 'tb-drag';
    // 只有鼠标实际移动超过阈值才进入 OS 拖动循环:立即 start_dragging 会
    // 阻塞 JS 事件,导致空白区域双击切换最大化(全屏)永远不触发。
    drag.addEventListener('mousedown', function (e) {
      if (e.button !== 0) return;
      var startX = e.clientX, startY = e.clientY;
      function onMove(ev) {
        if (Math.abs(ev.clientX - startX) + Math.abs(ev.clientY - startY) > 3) {
          cleanup();
          invoke('window_start_dragging');
        }
      }
      function onUp() { cleanup(); }
      function cleanup() {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
      }
      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    });
    drag.addEventListener('dblclick', toggleMaximize);

    var wins = document.createElement('div');
    wins.className = 'tb-wins';

    function winBtn(icon, extraClass, onClick) {
      var b = document.createElement('button');
      b.type = 'button';
      b.className = 'tb-wb' + (extraClass ? ' ' + extraClass : '');
      b.innerHTML = icon;
      b.addEventListener('click', onClick);
      return b;
    }

    var minBtn = winBtn(ICONS.min, '', function () { invoke('window_minimize'); });
    var maxBtn = winBtn(ICONS.max, '', toggleMaximize);
    maxBtnEl = maxBtn;
    var closeBtn = winBtn(ICONS.close, 'tb-close', function () { invoke('window_close'); });

    wins.appendChild(minBtn);
    wins.appendChild(maxBtn);
    wins.appendChild(closeBtn);

    bar.appendChild(menusWrap);
    bar.appendChild(drag);
    bar.appendChild(wins);

    document.body.appendChild(bar);

    document.addEventListener('click', function () { closeAllMenus(bar); });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();"#;

/// 生成按系统语言本地化的标题栏注入脚本,把菜单文案 JSON 与程序图标注入模板。
#[cfg(target_os = "windows")]
fn titlebar_script(i18n: &I18n) -> String {
    let labels = serde_json::json!({
        "about": i18n.about(),
        "checkUpdates": i18n.check_updates(),
        "quit": i18n.quit(),
        "file": i18n.file_menu(),
        "closeWindow": i18n.close_window(),
        "edit": i18n.edit_menu(),
        "undo": i18n.undo(),
        "redo": i18n.redo(),
        "cut": i18n.cut(),
        "copy": i18n.copy(),
        "paste": i18n.paste(),
        "selectAll": i18n.select_all(),
        "view": i18n.view_menu(),
        "fullscreen": i18n.enter_full_screen(),
        "windowMenu": i18n.window_menu(),
        "minimize": i18n.minimize(),
        "zoom": i18n.zoom(),
        "help": i18n.help_menu(),
        "helpItem": i18n.help(),
        "feedback": i18n.feedback(),
        "exportLogs": i18n.export_logs(),
    })
    .to_string();
    TITLEBAR_SCRIPT_TEMPLATE
        .replace("__LABELS__", &labels)
        .replace("__APP_ICON__", &icon_data_url())
}

/// 编译期把程序图标(32×32 PNG)打包进二进制,运行时转为 base64 data URL,
/// 供注入脚本在自绘标题栏中显示真实的应用图标。
#[cfg(target_os = "windows")]
fn icon_data_url() -> String {
    const ICON_PNG: &[u8] = include_bytes!("../icons/32x32.png");
    format!("data:image/png;base64,{}", base64_encode(ICON_PNG))
}

/// 标准 base64 编码(无换行),避免为单一的图标编码引入额外依赖。
#[cfg(target_os = "windows")]
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Bind a loopback HTTP socket serving `html` and return its URL. The listener
/// thread lives until the process exits; every request gets the same page so
/// reloads and retries keep working.
fn serve_html(html: String) -> Result<String, String> {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("failed to bind local server: {}", e))?;
    let url = format!(
        "http://{}/",
        listener
            .local_addr()
            .map_err(|e| format!("local server addr: {}", e))?
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = html.as_bytes();
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

/// Serve the bootstrap progress page over a loopback HTTP socket.
fn serve_bootstrap_html() -> Result<String, String> {
    serve_html(BOOTSTRAP_HTML.to_string())
}

/// Serve the startup loading page over a loopback HTTP socket.
fn serve_loading_html() -> Result<String, String> {
    serve_html(LOADING_HTML.to_string())
}

/// Executable candidate names for a program: bare name on Unix; `.exe`/`.cmd`
/// npm-style shims plus the bare name on Windows.
fn program_candidates(name: &str, windows: bool) -> Vec<String> {
    if windows {
        vec![
            format!("{}.exe", name),
            format!("{}.cmd", name),
            name.to_string(),
        ]
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
    find_program(name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string())
}

/// (program, args) to open a URL in the default browser, per platform.
/// Unknown platforms fall back to xdg-open.
fn open_url_command(os: &str, url: &str) -> (String, Vec<String>) {
    match os {
        "macos" => ("open".to_string(), vec![url.to_string()]),
        "windows" => (
            "cmd".to_string(),
            vec!["/C".to_string(), "start".to_string(), url.to_string()],
        ),
        _ => ("xdg-open".to_string(), vec![url.to_string()]),
    }
}

// ---------------------------------------------------------------------------
// First-launch bootstrap orchestration
// ---------------------------------------------------------------------------

/// Exit with an error dialog. Never returns.
fn fail_startup(handle: &tauri::AppHandle, i18n: &I18n, detail: &str) -> ! {
    log_line("desktop", &format!("ERROR: {}", detail));
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
            // First time we reach the install step, arm a one-shot notice that
            // fires if the install is still running a while later, so a slow
            // first-time setup doesn't look like a hang.
            let slow_armed = std::cell::Cell::new(false);
            bootstrap::install(move |step, percent| {
                let msg = serde_json::to_string(&i18n2.bootstrap_step(step)).unwrap_or_default();
                let pct = percent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "null".to_string());
                let _ = win2.eval(format!("window.__dsbUpdate({}, {})", msg, pct));
                if matches!(&step, bootstrap::Step::Install) && !slow_armed.replace(true) {
                    let win3 = win2.clone();
                    let slow_msg = i18n2.bootstrap_slow_msg();
                    tauri::async_runtime::spawn(async move {
                        std::thread::sleep(Duration::from_secs(30));
                        let _ = win3.eval(format!(
                            "window.__dsbUpdate({}, null)",
                            serde_json::to_string(&slow_msg).unwrap_or_default()
                        ));
                    });
                }
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
    log_line(
        "desktop",
        &format!(
            "spawning: {} {} {}",
            cmd,
            args.join(" "),
            cwd.as_ref()
                .map(|d| format!("(cwd: {})", d.display()))
                .unwrap_or_default()
        ),
    );

    let mut proc = Command::new(&cmd);
    proc.args(&args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::inherit());
    proc.env("PATH", augmented_path());
    bootstrap::hide_console(&mut proc);
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
    bootstrap::hide_console(&mut proc);
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
    let mut proc = Command::new(&cmd);
    proc.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("PATH", augmented_path());
    bootstrap::hide_console(&mut proc);
    let output = proc.output();
    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                "unknown".to_string()
            } else {
                text
            }
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
    log_line(
        "desktop",
        &format!("version check: current={} latest={}", current, latest),
    );
    UpdateInfo {
        current,
        latest,
        update_available,
    }
}

// ---------------------------------------------------------------------------
// Desktop app self-update
// ---------------------------------------------------------------------------

/// Check the configured updater endpoint for a newer desktop-app release.
/// Returns the parsed `Update` when available; `None` when already current.
async fn check_app_update(
    handle: &tauri::AppHandle,
) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let updater = handle
        .updater()
        .map_err(|e| format!("updater init failed: {}", e))?;
    updater
        .check()
        .await
        .map_err(|e| format!("update check failed: {}", e))
}

/// Download, verify and install an app update. Progress is shown by the
/// installer itself (Windows `passive` mode opens a small progress window).
async fn install_app_update(update: tauri_plugin_updater::Update) -> Result<(), String> {
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Upgrade
// ---------------------------------------------------------------------------

/// Run the appropriate upgrade command for the current mode, streaming lines to stdout.
fn run_upgrade() -> Result<(bool, String), String> {
    let _busy = UpgradeInProgressGuard::try_acquire()
        .ok_or_else(|| "upgrade already in progress".to_string())?;
    let plan = upgrade_runner(&detect_dsh_mode(), resolve_program);

    log_line(
        "desktop",
        &format!("upgrading: {} {}", plan.cmd, plan.args.join(" ")),
    );

    let mut proc = Command::new(&plan.cmd);
    proc.args(&plan.args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::piped());
    proc.env("PATH", plan.path.as_deref().unwrap_or(&augmented_path()));
    bootstrap::hide_console(&mut proc);
    if let Some(ref dir) = plan.cwd {
        proc.current_dir(dir);
    }
    let mut child = proc
        .spawn()
        .map_err(|e| format!("failed to spawn upgrade: {}", e))?;

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
    let mut proc = Command::new(&cmd);
    proc.args(&args);
    bootstrap::hide_console(&mut proc);
    match proc.spawn() {
        Ok(_) => log_line("desktop", &format!("opening {}", url)),
        Err(e) => log_line("desktop", &format!("failed to open {}: {}", url, e)),
    }
}

/// Copy the desktop log to a user-chosen location via a save dialog.
/// Cancellation is a no-op; copy failure shows an error dialog.
fn export_logs(handle: &tauri::AppHandle, i18n: &I18n) {
    let default = export_filename(now_unix_secs());
    let Some(path) = handle
        .dialog()
        .file()
        .set_file_name(&default)
        .blocking_save_file()
    else {
        return;
    };
    let Ok(path) = path.into_path() else {
        log_line(
            "desktop",
            "save dialog returned a non-filesystem path; export cancelled",
        );
        return;
    };
    let result = if log_path().exists() {
        std::fs::copy(log_path(), &path).map(|_| ())
    } else {
        std::fs::write(&path, b"")
    };
    match result {
        Ok(()) => log_line("desktop", &format!("logs exported to {}", path.display())),
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
            if l.is_empty() {
                return false;
            }
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
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
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
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", line)
}

/// Write a `[timestamp] [prefix] msg` line to stdout and append it to the
/// desktop log. File failures are silently ignored — logging must never break
/// the app.
fn log_line(prefix: &str, msg: &str) {
    use std::io::Write;
    let line = format!("[{}] [{}] {}", now_timestamp(), prefix, msg);
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

/// Format a log timestamp as `YYYY-MM-DD HH:MM:SS.mmm` (UTC) from a UNIX
/// second count plus a millisecond sub-second. Extracted from
/// `timestamp_compact` so the format is a pure, testable function.
fn format_timestamp(secs: i64, millis: u32) -> String {
    let base = timestamp_compact(secs); // YYYYMMDD-HHMMSS
    format!(
        "{}-{}-{} {}:{}:{}.{:03}",
        &base[..4],
        &base[4..6],
        &base[6..8],
        &base[9..11],
        &base[11..13],
        &base[13..15],
        millis
    )
}

/// Current wall-clock time as a millisecond-precision log timestamp (UTC).
fn now_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_timestamp(now.as_secs() as i64, now.subsec_millis())
}

/// Default filename for an exported log bundle.
fn export_filename(secs: i64) -> String {
    format!("dsh-desktop-{}.log", timestamp_compact(secs))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PromptState {
    last_prompted_version: String,
    last_prompted_at: u64,
    last_app_prompted_version: String,
    last_app_prompted_at: u64,
}

/// Pure 24h dedup decision (extracted for testability): true when `latest`
/// differs from the recorded version or the record is stale.
fn should_prompt_given(recorded_version: &str, recorded_at: u64, latest: &str, now: u64) -> bool {
    recorded_version != latest || now.saturating_sub(recorded_at) >= 24 * 3600
}

/// Decide whether the startup auto-check may prompt for this version of dsh
/// (`is_app == false`) or the desktop app (`is_app == true`): false when the
/// same version was already offered within the last 24 hours. Records the
/// prompt attempt before returning true.
fn should_auto_prompt(latest: &str, is_app: bool) -> bool {
    let path = prompt_state_path();
    let mut state: PromptState = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let (last_version, last_at) = if is_app {
        (
            &mut state.last_app_prompted_version,
            &mut state.last_app_prompted_at,
        )
    } else {
        (
            &mut state.last_prompted_version,
            &mut state.last_prompted_at,
        )
    };
    if !should_prompt_given(last_version, *last_at, latest, now) {
        return false;
    }
    *last_version = latest.to_string();
    *last_at = now;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, serde_json::to_string(&state).unwrap_or_default());
    true
}

// ---------------------------------------------------------------------------
// Menu actions
// ---------------------------------------------------------------------------

fn show_upgrade_progress(
    handle: &tauri::AppHandle,
    i18n: &I18n,
    result: Result<(bool, String), String>,
) {
    let (ok, output) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = handle
                .dialog()
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
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = handle
                    .dialog()
                    .message(i18n.about_msg(env!("CARGO_PKG_VERSION")))
                    .title(i18n.about())
                    .kind(MessageDialogKind::Info)
                    .blocking_show();
            });
        }
        "help" => open_url("https://github.com/hialuoy/deepseek-harness-desktop"),
        "feedback" => open_url("https://github.com/hialuoy/deepseek-harness-desktop/issues/new"),
        "export_logs" => {
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || export_logs(&handle, &i18n));
        }
        "check_updates" => {
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || {
                // dsh check
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

                // desktop app self-check (async updater driven via block_on)
                match tauri::async_runtime::block_on(check_app_update(&handle)) {
                    Ok(Some(update)) => {
                        let latest = update.version.clone();
                        if ask_yes_no(
                            &handle,
                            i18n.app_update_title(),
                            i18n.app_update_msg(env!("CARGO_PKG_VERSION"), &latest),
                        ) {
                            if let Err(e) =
                                tauri::async_runtime::block_on(install_app_update(update))
                            {
                                let _ = handle
                                    .dialog()
                                    .message(i18n.app_update_failed_msg(&e))
                                    .title(i18n.app_update_title())
                                    .kind(MessageDialogKind::Error)
                                    .blocking_show();
                            }
                        }
                    }
                    Ok(None) => {
                        let _ = handle
                            .dialog()
                            .message(i18n.app_up_to_date_msg(env!("CARGO_PKG_VERSION")))
                            .title(i18n.up_to_date_title())
                            .kind(MessageDialogKind::Info)
                            .blocking_show();
                    }
                    Err(e) => {
                        // Manifest missing or endpoint unreachable — not actionable
                        // for the user; startup auto-check logs the same way.
                        log_line("desktop", &format!("app update check failed: {}", e));
                    }
                }
            });
        }
        // 自绘标题栏的「退出」菜单项:结束 dsh 子进程后退出应用。
        "quit" => {
            kill_dsh(handle);
            handle.exit(0);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 自定义标题栏的窗口控制命令(仅 Windows 自绘标题栏通过 IPC 调用;其余
// 平台命令同样注册但不会被触发)。
// ---------------------------------------------------------------------------

/// 最小化当前窗口。
#[tauri::command]
fn window_minimize(window: tauri::WebviewWindow) {
    let _ = window.minimize();
}

/// 切换当前窗口最大化/还原,返回切换后是否处于最大化状态。
#[tauri::command]
fn window_toggle_maximize(window: tauri::WebviewWindow) -> bool {
    match window.is_maximized() {
        Ok(true) => {
            let _ = window.unmaximize();
            false
        }
        Ok(false) => {
            let _ = window.maximize();
            true
        }
        Err(_) => false,
    }
}

/// 关闭当前窗口。
#[tauri::command]
fn window_close(window: tauri::WebviewWindow) {
    let _ = window.close();
}

/// 切换当前窗口全屏,返回切换后是否处于全屏状态。
#[tauri::command]
fn window_toggle_fullscreen(window: tauri::WebviewWindow) -> bool {
    match window.is_fullscreen() {
        Ok(fullscreen) => {
            let _ = window.set_fullscreen(!fullscreen);
            !fullscreen
        }
        Err(_) => false,
    }
}

/// 从自绘标题栏的空白区发起窗口拖动。
#[tauri::command]
fn window_start_dragging(window: tauri::WebviewWindow) {
    let _ = window.start_dragging();
}

/// 分发自绘标题栏菜单项到既有的菜单处理逻辑。
#[tauri::command]
fn menu_action(app: tauri::AppHandle, action: String) {
    let i18n = (*app.state::<I18n>()).clone();
    on_menu_event(&app, &i18n, &action);
}

/// 构建并设置原生菜单栏。仅非 Windows 平台使用:Windows 改用自绘标题栏
/// 菜单(见 `TITLEBAR_SCRIPT_TEMPLATE`),因此不设置原生菜单。
#[cfg(not(target_os = "windows"))]
fn setup_native_menu(handle: &tauri::AppHandle, i18n: &I18n) {
    let about_item = MenuItem::with_id(handle, "about", i18n.about(), true, None::<&str>)
        .expect("failed to build menu item");
    let check_item = MenuItem::with_id(
        handle,
        "check_updates",
        i18n.check_updates(),
        true,
        None::<&str>,
    )
    .expect("failed to build menu item");
    let submenu = Submenu::with_items(
        handle,
        "DeepSeek Harness",
        true,
        &[
            &about_item,
            &check_item,
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::services(handle, Some(i18n.services()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::hide(handle, Some(i18n.hide()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::hide_others(handle, Some(i18n.hide_others()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::show_all(handle, Some(i18n.show_all()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::quit(handle, Some(i18n.quit()))
                .expect("failed to build menu item"),
        ],
    )
    .expect("failed to build submenu");
    let help_item = MenuItem::with_id(handle, "help", i18n.help(), true, None::<&str>)
        .expect("failed to build menu item");
    let feedback_item = MenuItem::with_id(handle, "feedback", i18n.feedback(), true, None::<&str>)
        .expect("failed to build menu item");
    let export_item = MenuItem::with_id(
        handle,
        "export_logs",
        i18n.export_logs(),
        true,
        None::<&str>,
    )
    .expect("failed to build menu item");
    let help_submenu = Submenu::with_items(
        handle,
        i18n.help_menu(),
        true,
        &[&help_item, &feedback_item, &export_item],
    )
    .expect("failed to build submenu");

    // 编辑菜单:恢复 macOS Cmd+C/Cmd+V 剪贴板快捷键。
    let edit_submenu = Submenu::with_items(
        handle,
        i18n.edit_menu(),
        true,
        &[
            &PredefinedMenuItem::undo(handle, Some(i18n.undo()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::redo(handle, Some(i18n.redo()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::cut(handle, Some(i18n.cut())).expect("failed to build menu item"),
            &PredefinedMenuItem::copy(handle, Some(i18n.copy()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::paste(handle, Some(i18n.paste()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::select_all(handle, Some(i18n.select_all()))
                .expect("failed to build menu item"),
        ],
    )
    .expect("failed to build submenu");
    let file_submenu = Submenu::with_items(
        handle,
        i18n.file_menu(),
        true,
        &[
            &PredefinedMenuItem::close_window(handle, Some(i18n.close_window()))
                .expect("failed to build menu item"),
        ],
    )
    .expect("failed to build submenu");
    let view_submenu = Submenu::with_items(
        handle,
        i18n.view_menu(),
        true,
        &[
            &PredefinedMenuItem::fullscreen(handle, Some(i18n.enter_full_screen()))
                .expect("failed to build menu item"),
        ],
    )
    .expect("failed to build submenu");
    let window_submenu = Submenu::with_items(
        handle,
        i18n.window_menu(),
        true,
        &[
            &PredefinedMenuItem::minimize(handle, Some(i18n.minimize()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::maximize(handle, Some(i18n.zoom()))
                .expect("failed to build menu item"),
            &PredefinedMenuItem::separator(handle).expect("failed to build separator"),
            &PredefinedMenuItem::close_window(handle, Some(i18n.close_window()))
                .expect("failed to build menu item"),
        ],
    )
    .expect("failed to build submenu");
    let menu = Menu::with_items(
        handle,
        &[
            &submenu,
            &file_submenu,
            &edit_submenu,
            &view_submenu,
            &window_submenu,
            &help_submenu,
        ],
    )
    .expect("failed to build menu");
    handle.set_menu(menu).expect("failed to set menu");
}

// ---------------------------------------------------------------------------
// App entry
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .manage(DshProcess(Mutex::new(None)))
        .manage(I18n::detect())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            window_minimize,
            window_toggle_maximize,
            window_close,
            window_toggle_fullscreen,
            window_start_dragging,
            menu_action,
        ])
        .setup(|app| {
            rotate_log_if_needed();
            let handle = app.handle().clone();
            let i18n = (*app.state::<I18n>()).clone();

            // 立即显示主窗口(加载页),避免等待 dsh 启动期间一片空白。
            // 窗口先渲染 spinner,dsh 就绪后再导航到实际 UI。
            let loading_url = serve_loading_html().expect("failed to serve loading page");
            let window = {
                let builder = tauri::WebviewWindowBuilder::new(
                    &handle,
                    "main",
                    tauri::WebviewUrl::External(loading_url.parse().expect("invalid loading url")),
                )
                .title("DeepSeek Harness")
                .inner_size(1200.0, 800.0)
                .center();
                // Windows 关闭原生标题栏与菜单栏,改由注入的自绘标题栏呈现
                // 「菜单 + 最小化/最大化/关闭」一行;shadow 保留 Win11 圆角。
                #[cfg(target_os = "windows")]
                let builder = builder
                    .decorations(false)
                    .shadow(true)
                    .initialization_script(titlebar_script(&i18n));
                builder.build().expect("failed to build main window")
            };

            tauri::async_runtime::spawn(async move {
                // ── 1. Toolchain (first-launch bootstrap if needed) ──
                if let Err(e) = ensure_toolchain(&handle, &i18n).await {
                    log_line("desktop", &format!("ERROR: {}", e));
                    std::process::exit(1);
                }

                // ── 2. Start dsh web ────────────────────────────────
                let (child, url) = match tauri::async_runtime::spawn_blocking(start_dsh).await {
                    Ok(Ok(v)) => v,
                    Ok(Err(e)) => fail_startup(&handle, &i18n, &e),
                    Err(e) => fail_startup(&handle, &i18n, &format!("task join: {}", e)),
                };
                *handle.state::<DshProcess>().0.lock().unwrap() = Some(child);

                // ── 3. 把已在 setup 中显示的加载页窗口导航到 dsh URL ──
                // dsh 就绪后多停 1 秒,让加载页动画完整展示,也给 dsh 页面留出首次渲染缓冲。
                std::thread::sleep(Duration::from_secs(1));
                if let Err(e) = window.navigate(url.parse().expect("invalid dsh url")) {
                    log_line("desktop", &format!("failed to navigate to dsh: {}", e));
                }

                // ── 4. Native menu(非 Windows);Windows 用自绘标题栏菜单 ──
                #[cfg(not(target_os = "windows"))]
                setup_native_menu(&handle, &i18n);

                // ── 5. Auto-check for updates shortly after startup ─
                let handle2 = handle.clone();
                let i18n2 = i18n.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    std::thread::sleep(Duration::from_secs(5));

                    // dsh auto-check
                    let info = check_update();
                    if info.update_available
                        && should_auto_prompt(&info.latest, false)
                        && ask_yes_no(
                            &handle2,
                            i18n2.update_available_title(),
                            i18n2.update_available_msg(&info.current, &info.latest),
                        )
                    {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle2, &i18n2, result);
                    }

                    // desktop app auto-check (once per version per day)
                    match tauri::async_runtime::block_on(check_app_update(&handle2)) {
                        Ok(Some(update)) => {
                            let latest = update.version.clone();
                            if should_auto_prompt(&latest, true)
                                && ask_yes_no(
                                    &handle2,
                                    i18n2.app_update_title(),
                                    i18n2.app_update_msg(env!("CARGO_PKG_VERSION"), &latest),
                                )
                            {
                                if let Err(e) =
                                    tauri::async_runtime::block_on(install_app_update(update))
                                {
                                    let _ = handle2
                                        .dialog()
                                        .message(i18n2.app_update_failed_msg(&e))
                                        .title(i18n2.app_update_title())
                                        .kind(MessageDialogKind::Error)
                                        .blocking_show();
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(e) => log_line("desktop", &format!("app update check failed: {}", e)),
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
            vec![
                "npm.exe".to_string(),
                "npm.cmd".to_string(),
                "npm".to_string()
            ]
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
        if cfg!(windows) {
            // The npm `.cmd` shim is invoked directly (cmd.exe wraps it).
            assert_eq!(cmd, "/x/toolchain/node_modules/.bin/dsh");
            assert!(args.is_empty());
        } else {
            assert_eq!(cmd, "/x/toolchain/node-24.19.0/bin/node");
            assert_eq!(args, vec!["/x/toolchain/node_modules/.bin/dsh".to_string()]);
        }
        assert_eq!(cwd, None);
    }

    #[test]
    fn npx_mode_falls_back_to_registry() {
        let (cmd, args, cwd) = dsh_runner(&DshMode::Npx, identity);
        assert_eq!(cmd, "npx");
        assert_eq!(
            args,
            vec!["--yes".to_string(), "@deepseek-ai/dsh".to_string()]
        );
        assert_eq!(cwd, None);
    }

    #[test]
    fn node_install_root_from_bin_shim() {
        let dsh = PathBuf::from("/nvm/versions/node/v22.23.1/bin/dsh");
        assert_eq!(
            node_install_root_from_shim(&dsh).unwrap(),
            PathBuf::from("/nvm/versions/node/v22.23.1")
        );
    }

    #[test]
    fn global_upgrade_targets_detected_node_with_isolated_path() {
        let root = std::env::temp_dir().join(format!("dsh-upg-global-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let prefix = root.join("node-v22.23.1");
        std::fs::create_dir_all(prefix.join("bin")).unwrap();
        std::fs::write(prefix.join("bin").join(npm_shim_name()), b"").unwrap();
        std::fs::write(prefix.join("bin").join("dsh"), b"").unwrap();

        let dsh = prefix.join("bin").join("dsh");
        let plan = npm_upgrade_plan_for_shim(&dsh).expect("npm beside dsh");
        assert_eq!(
            plan.cmd,
            prefix.join("bin").join(npm_shim_name()).to_string_lossy()
        );
        assert_eq!(
            plan.args,
            vec![
                "install".to_string(),
                "-g".to_string(),
                "--no-fund".to_string(),
                "--no-audit".to_string(),
                "@deepseek-ai/dsh@latest".to_string(),
            ]
        );
        assert!(plan
            .path
            .as_ref()
            .unwrap()
            .starts_with(prefix.join("bin").to_string_lossy().as_ref()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn global_upgrade_falls_back_without_local_npm() {
        let dsh = PathBuf::from("/opt/homebrew/bin/dsh");
        let plan = upgrade_runner(&DshMode::Global(dsh), identity);
        assert_eq!(plan.cmd, "npm");
        assert_eq!(plan.args[0], "install");
        assert!(plan.args.contains(&"-g".to_string()));
        assert_eq!(plan.cwd, None);
    }

    #[test]
    fn source_upgrade_runs_git_pull_in_repo_root() {
        let root = PathBuf::from("/repo");
        let plan = upgrade_runner(&DshMode::Source(root.clone()), identity);
        assert_eq!(plan.cmd, "sh");
        assert!(plan.args[1].contains("git pull"));
        assert_eq!(plan.cwd, Some(root));
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
    fn i18n_edit_menu_items_zh_and_en() {
        let zh = I18n { is_zh: true };
        let en = I18n { is_zh: false };
        assert_eq!(zh.edit_menu(), "编辑");
        assert_eq!(en.edit_menu(), "Edit");
        assert_eq!(zh.undo(), "撤销");
        assert_eq!(en.undo(), "Undo");
        assert_eq!(zh.redo(), "重做");
        assert_eq!(en.redo(), "Redo");
        assert_eq!(zh.cut(), "剪切");
        assert_eq!(en.cut(), "Cut");
        assert_eq!(zh.copy(), "复制");
        assert_eq!(en.copy(), "Copy");
        assert_eq!(zh.paste(), "粘贴");
        assert_eq!(en.paste(), "Paste");
        assert_eq!(zh.select_all(), "全选");
        assert_eq!(en.select_all(), "Select All");
    }

    #[test]
    fn i18n_standard_menu_items_zh_and_en() {
        let zh = I18n { is_zh: true };
        let en = I18n { is_zh: false };
        assert_eq!(zh.quit(), "退出");
        assert_eq!(en.quit(), "Quit");
        assert_eq!(zh.file_menu(), "文件");
        assert_eq!(en.file_menu(), "File");
        assert_eq!(zh.view_menu(), "显示");
        assert_eq!(en.view_menu(), "View");
        assert_eq!(zh.enter_full_screen(), "进入全屏");
        assert_eq!(en.enter_full_screen(), "Enter Full Screen");
        assert_eq!(zh.window_menu(), "窗口");
        assert_eq!(en.window_menu(), "Window");
        assert_eq!(zh.minimize(), "最小化");
        assert_eq!(en.minimize(), "Minimize");
        assert_eq!(zh.zoom(), "缩放");
        assert_eq!(en.zoom(), "Zoom");
        assert_eq!(zh.close_window(), "关闭窗口");
        assert_eq!(en.close_window(), "Close Window");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn i18n_macos_menu_items_zh_and_en() {
        let zh = I18n { is_zh: true };
        let en = I18n { is_zh: false };
        assert_eq!(zh.hide(), "隐藏");
        assert_eq!(en.hide(), "Hide");
        assert_eq!(zh.hide_others(), "隐藏其他");
        assert_eq!(en.hide_others(), "Hide Others");
        assert_eq!(zh.show_all(), "全部显示");
        assert_eq!(en.show_all(), "Show All");
        assert_eq!(zh.services(), "服务");
        assert_eq!(en.services(), "Services");
    }

    #[test]
    fn app_update_i18n_zh_and_en() {
        let zh = I18n { is_zh: true };
        let en = I18n { is_zh: false };
        assert_eq!(zh.app_update_title(), "更新 DeepSeek Harness");
        assert_eq!(en.app_update_title(), "Update DeepSeek Harness");
        assert_eq!(
            zh.app_update_msg("1.0.1", "1.0.2"),
            "发现 DeepSeek Harness 新版本。\n\n  当前版本:  1.0.1\n  最新版本:  1.0.2\n\n是否下载并安装?"
        );
        assert_eq!(
            en.app_update_msg("1.0.1", "1.0.2"),
            "A new version of DeepSeek Harness is available.\n\n  Current:  1.0.1\n  Latest:   1.0.2\n\nDownload and install now?"
        );
        assert_eq!(
            zh.app_up_to_date_msg("1.0.1"),
            "DeepSeek Harness 已是最新版本(1.0.1)。"
        );
        assert_eq!(
            en.app_up_to_date_msg("1.0.1"),
            "DeepSeek Harness is up to date (version 1.0.1)."
        );
        assert_eq!(zh.app_update_failed_msg("boom"), "应用更新失败:\nboom");
        assert_eq!(en.app_update_failed_msg("boom"), "App update failed:\nboom");
    }

    #[test]
    fn should_prompt_given_24h_dedup_rules() {
        let day = 24 * 3600;
        // Same version, still fresh → suppress.
        assert!(!should_prompt_given("1.0.2", 100, "1.0.2", 100 + day - 1));
        // Same version, stale (>=24h) → prompt.
        assert!(should_prompt_given("1.0.2", 100, "1.0.2", 100 + day));
        // Different version → prompt regardless of age.
        assert!(should_prompt_given("1.0.2", 100, "1.0.3", 100 + 1));
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
        assert_eq!(
            open_url_command("macos", url),
            ("open".to_string(), vec![url.to_string()])
        );
        assert_eq!(
            open_url_command("windows", url),
            (
                "cmd".to_string(),
                vec!["/C".to_string(), "start".to_string(), url.to_string()]
            )
        );
        assert_eq!(
            open_url_command("linux", url),
            ("xdg-open".to_string(), vec![url.to_string()])
        );
        assert_eq!(
            open_url_command("freebsd", url),
            ("xdg-open".to_string(), vec![url.to_string()])
        );
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
    fn format_timestamp_known_epochs_utc() {
        assert_eq!(format_timestamp(0, 0), "1970-01-01 00:00:00.000");
        assert_eq!(
            format_timestamp(946_684_800, 123),
            "2000-01-01 00:00:00.123"
        );
        assert_eq!(
            format_timestamp(951_868_800 + 36_000, 7),
            "2000-03-01 10:00:00.007"
        );
    }

    #[test]
    fn export_filename_format() {
        assert_eq!(
            export_filename(951_868_800),
            "dsh-desktop-20000301-000000.log"
        );
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
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Download),
            "正在下载 Node.js…"
        );
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Extract),
            "正在解压 Node.js…"
        );
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Install),
            "正在安装 dsh…"
        );
        assert_eq!(i18n.bootstrap_failed_title(), "初始化失败");
        assert!(i18n.bootstrap_failed_msg("boom").contains("重试"));
        assert!(i18n.bootstrap_slow_msg().contains("耐心等待"));
    }

    #[test]
    fn loading_html_matches_dsh_boot_page() {
        assert!(LOADING_HTML.contains("HARNESS"));
        assert!(LOADING_HTML.contains("Starting…"));
        assert!(LOADING_HTML.contains("wordmark"));
        assert!(LOADING_HTML.contains("spinner"));
        assert!(LOADING_HTML.contains("conic-gradient"));
        assert!(LOADING_HTML.contains("72deg"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn titlebar_script_zh_contains_localized_menu_labels() {
        let script = titlebar_script(&I18n { is_zh: true });
        assert!(script.contains("\"file\":\"文件\""));
        assert!(script.contains("\"about\":\"关于 DeepSeek Harness\""));
        assert!(script.contains("\"minimize\":\"最小化\""));
        assert!(script.contains("\"exportLogs\":\"导出日志\""));
        // 窗口控制命令名必须与后端 command 函数名一致。
        assert!(script.contains("window_minimize"));
        assert!(script.contains("window_toggle_maximize"));
        assert!(script.contains("window_close"));
        assert!(script.contains("window_start_dragging"));
        // 应用图标已替换为程序图标的 data URL。
        assert!(script.contains("data:image/png;base64,"));
        assert!(!script.contains("__APP_ICON__"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn titlebar_script_en_contains_localized_menu_labels() {
        let script = titlebar_script(&I18n { is_zh: false });
        assert!(script.contains("\"file\":\"File\""));
        assert!(script.contains("\"about\":\"About DeepSeek Harness\""));
        assert!(script.contains("\"minimize\":\"Minimize\""));
        assert!(script.contains("\"exportLogs\":\"Export Logs\""));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn bootstrap_copy_is_localized_en() {
        let i18n = I18n { is_zh: false };
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Download),
            "Downloading Node.js…"
        );
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Extract),
            "Extracting Node.js…"
        );
        assert_eq!(
            i18n.bootstrap_step(bootstrap::Step::Install),
            "Installing dsh…"
        );
        assert_eq!(i18n.bootstrap_failed_title(), "Setup Failed");
        assert!(i18n.bootstrap_failed_msg("boom").contains("Retry"));
        assert!(i18n.bootstrap_slow_msg().contains("please wait"));
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
        std::fs::write(
            tc.join("node_modules/.bin")
                .join(bootstrap::dsh_shim_name()),
            b"",
        )
        .unwrap();

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
                tc.join("node-24.19.0")
                    .join("lib")
                    .join("node_modules")
                    .join("npm")
                    .join("bin")
                    .join("npm-cli.js")
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
