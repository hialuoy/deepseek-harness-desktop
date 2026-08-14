// Prevents additional console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
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

/// Resolve how to launch dsh: source mode, bundled mode, or npx fallback.
fn resolve_dsh_command() -> (String, Vec<String>, Option<PathBuf>) {
    if let Some(root) = find_repo_root() {
        return ("pnpm".into(), vec!["dsh".into(), "web".into(), "--port".into(), "0".into()], Some(root));
    }
    let bundled_bin = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("../Resources/app/node_modules/.bin/dsh")))
        .filter(|p| p.exists());
    if let Some(bin) = bundled_bin {
        return ("node".into(), vec![bin.to_string_lossy().into_owned(), "web".into(), "--port".into(), "0".into()], None);
    }
    ("npx".into(), vec!["--yes".into(), "@deepseek-ai/dsh".into(), "web".into(), "--port".into(), "0".into()], None)
}

// ---------------------------------------------------------------------------
// dsh process management
// ---------------------------------------------------------------------------

/// Spawn dsh web and return once the URL line appears.
fn start_dsh() -> (Child, String) {
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
    if let Some(ref dir) = cwd {
        proc.current_dir(dir);
    }
    let mut child = proc.spawn().expect("Failed to start dsh web");

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
        std::process::exit(1);
    }
    println!("[desktop] dsh ready at {}", url);
    (child, url)
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
    let (cmd, mut args, cwd) = if find_repo_root().is_some() {
        ("pnpm".to_string(), vec!["dsh".to_string(), "-V".to_string()], find_repo_root())
    } else {
        let bundled_bin = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.join("../Resources/app/node_modules/.bin/dsh")))
            .filter(|p| p.exists());
        if let Some(bin) = bundled_bin {
            ("node".to_string(), vec![bin.to_string_lossy().into_owned(), "-V".to_string()], None)
        } else {
            ("npx".to_string(), vec!["--yes".to_string(), "@deepseek-ai/dsh".to_string(), "-V".to_string()], None)
        }
    };

    let mut proc = Command::new(&cmd);
    proc.args(&mut args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::null());
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
    let output = Command::new("npm")
        .args(["view", "@deepseek-ai/dsh", "version"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "git pull --rebase && pnpm install && pnpm run build".to_string(),
            ],
            Some(root),
        )
    } else {
        ("npm".to_string(), vec!["install".to_string(), "-g".to_string(), "@deepseek-ai/dsh@latest".to_string()], None)
    };

    println!("[desktop] upgrading: {} {}", cmd, args.join(" "));

    let mut proc = Command::new(&cmd);
    proc.args(&args);
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::piped());
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

// ---------------------------------------------------------------------------
// Menu actions
// ---------------------------------------------------------------------------

fn show_upgrade_progress(handle: &tauri::AppHandle, result: Result<(bool, String), String>) {
    let (ok, output) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = handle.dialog()
                .message(format!("Upgrade failed:\n{}", e))
                .title("Upgrade dsh")
                .kind(MessageDialogKind::Error)
                .blocking_show();
            return;
        }
    };
    let tail = output.trim_end();
    let tail = &tail[tail.len().saturating_sub(2000)..];
    if ok {
        if ask_yes_no(
            handle,
            "Upgrade dsh",
            format!("dsh upgraded successfully.\n\n{tail}\n\nRestart now to apply the update?"),
        ) {
            restart_app(handle);
        }
    } else {
        let _ = handle
            .dialog()
            .message(format!("Upgrade failed (exit code non-zero).\n\n{tail}"))
            .title("Upgrade dsh")
            .kind(MessageDialogKind::Error)
            .blocking_show();
    }
}

fn on_menu_event(handle: &tauri::AppHandle, id: &str) {
    match id {
        "check_updates" => {
            let handle = handle.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let info = check_update();
                if info.update_available {
                    if ask_yes_no(
                        &handle,
                        "Update Available",
                        format!(
                            "A new version of dsh is available.\n\n  Current:  {}\n  Latest:   {}\n\nUpgrade now?",
                            info.current, info.latest
                        ),
                    ) {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle, result);
                    }
                } else {
                    let _ = handle
                        .dialog()
                        .message(format!("dsh is up to date (version {}).", info.current))
                        .title("Up to Date")
                        .kind(MessageDialogKind::Info)
                        .blocking_show();
                }
            });
        }
        "upgrade" => {
            let handle = handle.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = run_upgrade();
                show_upgrade_progress(&handle, result);
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // ── 1. Start dsh web ─────────────────────────────────────
            let (child, url) = start_dsh();
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
            let check_item = MenuItem::with_id(app, "check_updates", "Check for Updates…", true, None::<&str>)?;
            let upgrade_item = MenuItem::with_id(app, "upgrade", "Upgrade dsh…", true, None::<&str>)?;
            let submenu = Submenu::with_items(
                app,
                "DeepSeek Harness",
                true,
                &[&check_item, &upgrade_item],
            )?;
            let menu = Menu::with_items(app, &[&submenu])?;
            app.set_menu(menu)?;

            // ── 4. Auto-check for updates shortly after startup ──────
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    std::thread::sleep(Duration::from_secs(5));
                    let info = check_update();
                    if info.update_available
                        && ask_yes_no(
                            &handle,
                            "Update Available",
                            format!(
                                "A new version of dsh is available.\n\n  Current:  {}\n  Latest:   {}\n\nUpgrade now?",
                                info.current, info.latest
                            ),
                        )
                    {
                        let result = run_upgrade();
                        show_upgrade_progress(&handle, result);
                    }
                });
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            on_menu_event(app, event.id().as_ref());
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    kill_dsh(&window.app_handle());
                    std::process::exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}