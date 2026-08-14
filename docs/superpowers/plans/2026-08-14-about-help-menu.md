# 关于/帮助菜单(帮助、提交反馈、导出日志)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在应用菜单加入"关于"项,新增 Help 菜单(帮助/提交反馈/导出日志),并让所有日志落盘到 `~/.dsh/desktop.log`。

**Architecture:** 三个任务:Task 1 添加 I18n 文案与 `open_url_command` 纯函数;Task 2 添加日志基础设施(`log_line`、轮转、导出文件名,均纯函数化可测);Task 3 接线(菜单、事件、替换全部 println 日志点)。

**Tech Stack:** Rust / Tauri 2 / tauri-plugin-dialog(已有,用于保存对话框)

## Global Constraints

- 只修改 `src-tauri/src/main.rs`。
- 零新增依赖;版本号用 `env!("CARGO_PKG_VERSION")`。
- URL 常量:仓库主页 `https://github.com/hialuoy/deepseek-harness-desktop`;反馈 `https://github.com/hialuoy/deepseek-harness-desktop/issues/new`。
- 日志文件 `~/.dsh/desktop.log`(HOME/USERPROFILE 兜底),启动时大小 >1MB 则轮转为 `desktop.old.log`(覆盖旧轮转)。
- 导出默认文件名 `dsh-desktop-<YYYYMMDD-HHMMSS>.log`(UTC 时间戳)。
- 现有 9 个单元测试保持通过;`cargo check`/`cargo clippy` 零告警。
- `open_url` 失败仅记日志,不弹对话框;日志追加失败静默;导出 copy 失败弹错误对话框。
- GUI 菜单视觉验证由人工完成(子代理无法看窗口),实现后提示用户。

---

### Task 1: I18n 文案 + open_url_command 纯函数

**Files:**
- Modify: `src-tauri/src/main.rs`(`impl I18n` 块与工具函数区)

**Interfaces:**
- Produces: `I18n::about()/help()/feedback()/export_logs()/help_menu() -> &'static str`;`I18n::about_msg(version: &str) -> String`;`I18n::export_logs_failed_msg(e: &str) -> String`;`fn open_url_command(os: &str, url: &str) -> (String, Vec<String>)`(Task 3 使用)

- [ ] **Step 1: 写失败测试**

在 `src-tauri/src/main.rs` 底部 `#[cfg(test)] mod tests` 内(现有测试之后)追加:

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`(workdir `src-tauri`)
Expected: 编译失败,`cannot find method about/help/... in I18n`、`cannot find function open_url_command`。

- [ ] **Step 3: 实现最小代码**

在 `impl I18n` 块内(现有 `upgrade_error_msg` 之后)添加:

```rust
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
```

在 `resolve_program` 函数之后添加:

```rust
/// (program, args) to open a URL in the default browser, per platform.
/// Unknown platforms fall back to xdg-open.
fn open_url_command(os: &str, url: &str) -> (String, Vec<String>) {
    match os {
        "macos" => ("open".to_string(), vec![url.to_string()]),
        "windows" => ("cmd".to_string(), vec!["/C".to_string(), "start".to_string(), url.to_string()]),
        _ => ("xdg-open".to_string(), vec![url.to_string()]),
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`(workdir `src-tauri`)
Expected: 13 passed; 0 failed(9 旧 + 4 新)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: i18n copy and open_url_command for about/help menu"
```

---

### Task 2: 日志基础设施(落盘、轮转、导出文件名)

**Files:**
- Modify: `src-tauri/src/main.rs`(工具函数区与 `prompt_state_path`)

**Interfaces:**
- Consumes: 无。
- Produces: `fn dsh_dir() -> PathBuf`;`fn log_path() -> PathBuf`;`fn should_rotate(size: u64, cap: u64) -> bool`;`fn rotate_log_if_needed()`;`fn append_log_line(path: &Path, line: &str) -> std::io::Result<()>`;`fn log_line(prefix: &str, msg: &str)`;`fn now_unix_secs() -> i64`;`fn timestamp_compact(secs: i64) -> String`;`fn export_filename(secs: i64) -> String`(Task 3 使用)

- [ ] **Step 1: 写失败测试**

在测试模块追加:

```rust
    #[test]
    fn timestamp_compact_known_epochs_utc() {
        assert_eq!(timestamp_compact(0), "19700101-000000");
        assert_eq!(timestamp_compact(946_684_800), "20000101-000000");
        assert_eq!(timestamp_compact(951_696_000), "20000229-000000");
        assert_eq!(timestamp_compact(951_782_400), "20000301-000000");
        assert_eq!(timestamp_compact(951_782_400 + 36_000), "20000301-100000");
    }

    #[test]
    fn export_filename_format() {
        assert_eq!(export_filename(951_782_400), "dsh-desktop-20000301-000000.log");
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test`(workdir `src-tauri`)
Expected: 编译失败,`cannot find function timestamp_compact/export_filename/should_rotate/append_log_line`。

- [ ] **Step 3: 实现最小代码**

在工具函数区(`prompt_state_path` 附近)添加,并把 `prompt_state_path` 改为复用 `dsh_dir()`:

```rust
/// Per-user dsh config dir (`~/.dsh`), created on demand by writers.
fn dsh_dir() -> PathBuf {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    PathBuf::from(home).join(".dsh")
}

/// State file recording the last auto-prompt so a version nags at most once a day.
fn prompt_state_path() -> PathBuf {
    dsh_dir().join("desktop-update-state.json")
}
```

继续添加日志函数:

```rust
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
```

注意:现有 `prompt_state_path` 函数体要替换成上面的两行版本(行为等价)。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test`(workdir `src-tauri`)
Expected: 18 passed; 0 failed(13 旧 + 5 新)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: file-backed logging with rotation and export filename helpers"
```

---

### Task 3: 接线(菜单、事件、open_url、导出动作、日志点替换)

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: Task 1 的 `I18n` 方法、`open_url_command`;Task 2 的 `log_line`、`rotate_log_if_needed`、`log_path`、`export_filename`、`now_unix_secs`。
- Produces: 无新公开符号。

- [ ] **Step 1: 启动时轮转日志**

在 `.setup(|app| {` 第一行后插入:

```rust
            rotate_log_if_needed();
```

- [ ] **Step 2: 替换全部 11 处 println!/eprintln! 为 log_line**

先替换 spawn 日志(现为多行 `println!(...)`,整段替换):

```rust
    log_line(
        "desktop",
        &format!(
            "spawning: {} {} {}",
            cmd,
            args.join(" "),
            cwd.as_ref().map(|d| format!("(cwd: {})", d.display())).unwrap_or_default()
        ),
    );
```

其余 10 处按以下对照逐一替换(格式串保持不变):

| 位置(现行为) | 替换为 |
|---|---|
| `println!("[dsh] {}", line);` | `log_line("dsh", &line);` |
| `eprintln!("[desktop] ERROR: dsh did not print a URL");` | `log_line("desktop", "ERROR: dsh did not print a URL");` |
| `println!("[desktop] dsh ready at {}", url);` | `log_line("desktop", &format!("dsh ready at {}", url));` |
| `println!("[desktop] shutting down dsh");` | `log_line("desktop", "shutting down dsh");` |
| `println!("[desktop] version check: current={} latest={}", current, latest);` | `log_line("desktop", &format!("version check: current={} latest={}", current, latest));` |
| `println!("[desktop] upgrading: {} {}", cmd, args.join(" "));` | `log_line("desktop", &format!("upgrading: {} {}", cmd, args.join(" ")));` |
| `println!("[upgrade] {}", line);` | `log_line("upgrade", &line);` |
| `println!("[upgrade:err] {}", line);` | `log_line("upgrade:err", &line);` |
| `println!("[desktop] relaunching {}", exe.display());` | `log_line("desktop", &format!("relaunching {}", exe.display()));` |
| `eprintln!("[desktop] ERROR: {}", e);`(setup 错误分支) | `log_line("desktop", &format!("ERROR: {}", e));` |

- [ ] **Step 3: 添加 open_url 与 export_logs 动作函数**

在 `restart_app` 函数之后添加:

```rust
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
```

- [ ] **Step 4: 添加菜单项**

把 setup 中的菜单构造块(当前是 `check_item` + `submenu`)替换为:

```rust
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
```

- [ ] **Step 5: 添加菜单事件分支**

把 `on_menu_event` 函数体(当前是 `if id == "check_updates" { ... }`)替换为 match:

```rust
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
```

(原 `check_updates` 分支代码原样保留,只改外层结构。)

- [ ] **Step 6: 验证**

Run: `cargo check`(workdir `src-tauri`)
Expected: 编译通过,无 warning。

Run: `cargo clippy`(workdir `src-tauri`)
Expected: 零告警。

Run: `cargo test`(workdir `src-tauri`)
Expected: 18 passed; 0 failed。

Run: `grep -n "println!\|eprintln!" src-tauri/src/main.rs`
Expected: 无输出(全部替换完毕)。

GUI 验证(菜单显示与点击、导出对话框)留给人工,在报告与收尾提示中说明。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: about and help menu (help, feedback, export logs)"
```
