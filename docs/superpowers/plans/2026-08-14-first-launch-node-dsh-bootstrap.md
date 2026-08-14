# 首启自动安装 Node.js + dsh 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 新电脑(macOS/Windows)双击安装包后,应用首次启动自动下载并安装私有 Node.js 与 `@deepseek-ai/dsh` 到 `~/.dsh/toolchain/`,全程无需管理员权限;已有 Node 的机器行为完全不变。

**Architecture:** 新增 `src-tauri/src/bootstrap.rs` 模块(纯函数 + 下载/解压/npm 安装 I/O),`main.rs` 中新增 `DshMode::Private` 模式并重构 `setup` 为异步续接(async continuation):先 `ensure_toolchain`(需要时弹引导窗口,reqwest 流式下载 Node 官方二进制,系统 tar/PowerShell 解压,私有 npm 装 dsh,进度用 `window.eval()` 推送到 `dist/bootstrap.html`),再启动 dsh、创建主窗口。升级/版本检查命令在 Private 模式下改走私有 npm-cli.js。

**Tech Stack:** Rust / Tauri 2.11 / reqwest 0.13(blocking,默认 rustls)/ semver / cargo

## Global Constraints

- 只修改本仓库 `desktop-tauri` 内文件;新增文件:`src-tauri/src/bootstrap.rs`、`dist/bootstrap.html`、`docs/superpowers/plans/`(本文件)。
- 目标平台:macOS(arm64/x64)+ Windows(x64/arm64);Linux 仅需编译通过(`bootstrap::install` 返回 unsupported 错误即可)。
- Node 版本常量 `NODE_VERSION = "24.19.0"`(写死,当前 Node 24 LTS 最新),最低版本 `MIN_NODE_VERSION = "22.0.0"`。
- 下载源:`https://nodejs.org/dist/v<ver>/node-v<ver>-<os>-<arch>.<ext>`(darwin→tar.xz,win→zip)。
- 安装位置:`~/.dsh/toolchain/`(HOME,Windows 用 USERPROFILE 兜底)。
- 模式优先级:**Source > Bundled > Global > Private > Npx**。
- `withGlobalTauri` 保持 `false`;bootstrap 窗口不加任何 IPC capability,进度用 Rust 侧 `window.eval()` 推送。
- 对话框与状态文案必须走 `I18n` 中英双语(中文系统 `zh*` 显示中文,其余英文)。
- 现有单元测试(`cargo test` 全量)保持通过——包括执行本计划期间可能合入的其他功能(如 about/help 菜单)的测试;新增单元测试全部通过;`cargo check`、`cargo clippy` 零告警;`cargo fmt` 格式化。
- 不引入 zip/tar 解压 crate;解压用系统自带 `tar`(macOS)/ PowerShell `Expand-Archive`(Windows)。
- 提交信息用英文 conventional commits(feat:/fix:/docs:)。
- 工作目录约定:`cargo` 命令均在 `src-tauri/` 下执行(用 `workdir` 参数)。

---

### Task 1: bootstrap.rs 纯函数基础 + reqwest 依赖

**Files:**
- Create: `src-tauri/src/bootstrap.rs`
- Modify: `src-tauri/Cargo.toml`(新增 reqwest 依赖)
- Modify: `src-tauri/src/main.rs`(顶部声明 `mod bootstrap;`)
- Test: `src-tauri/src/bootstrap.rs`(文件内 `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `semver::Version`(Cargo.toml 已有)。
- Produces:
  - `pub const NODE_VERSION: &str = "24.19.0";`
  - `pub const MIN_NODE_VERSION: &str = "22.0.0";`
  - `pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";`
  - `pub fn home_dir() -> PathBuf`
  - `pub fn toolchain_dir() -> PathBuf`
  - `pub fn node_dir(toolchain: &Path, version: &str) -> PathBuf`
  - `pub fn node_bin(toolchain: &Path, version: &str) -> PathBuf`(Windows 下文件名 `node.exe`,其余 `node`)
  - `pub fn npm_cli_js(toolchain: &Path, version: &str) -> PathBuf`
  - `pub fn npm_cli_from_node(node: &Path) -> PathBuf`
  - `pub fn platform_arch() -> Option<(&'static str, &'static str)>`
  - `pub fn archive_name(version: &str, os: &str, arch: &str) -> String`
  - `pub fn archive_url(version: &str, os: &str, arch: &str) -> String`
  - `pub fn extracted_top_dir(version: &str, os: &str, arch: &str) -> String`
  - `pub fn parse_node_version_ok(output: &str) -> bool`

- [ ] **Step 1: 写失败测试(先建文件,只含测试与桩函数)**

创建 `src-tauri/src/bootstrap.rs`,先写入测试和最小桩(函数体 `todo!()` 会编译失败,先用 `unimplemented!()` 也不可测——按 TDD 先写测试、桩返回空值以便编译,测试先失败):

```rust
//! First-launch bootstrap: download Node.js and install @deepseek-ai/dsh
//! into a user-local toolchain (~/.dsh/toolchain).

use std::path::{Path, PathBuf};

use semver::Version;

pub const NODE_VERSION: &str = "24.19.0";
pub const MIN_NODE_VERSION: &str = "22.0.0";
pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";

pub fn home_dir() -> PathBuf {
    PathBuf::new()
}

pub fn toolchain_dir() -> PathBuf {
    home_dir().join(".dsh").join("toolchain")
}

pub fn node_dir(toolchain: &Path, version: &str) -> PathBuf {
    toolchain.join(format!("node-{}", version))
}

pub fn node_bin(toolchain: &Path, version: &str) -> PathBuf {
    PathBuf::new()
}

pub fn npm_cli_js(toolchain: &Path, version: &str) -> PathBuf {
    PathBuf::new()
}

pub fn npm_cli_from_node(node: &Path) -> PathBuf {
    PathBuf::new()
}

pub fn platform_arch() -> Option<(&'static str, &'static str)> {
    None
}

pub fn archive_name(version: &str, os: &str, arch: &str) -> String {
    String::new()
}

pub fn archive_url(version: &str, os: &str, arch: &str) -> String {
    String::new()
}

pub fn extracted_top_dir(version: &str, os: &str, arch: &str) -> String {
    String::new()
}

pub fn parse_node_version_ok(output: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_name_for_macos_arm64_is_tar_xz() {
        assert_eq!(archive_name("24.19.0", "darwin", "arm64"), "node-v24.19.0-darwin-arm64.tar.xz");
    }

    #[test]
    fn archive_name_for_windows_x64_is_zip() {
        assert_eq!(archive_name("24.19.0", "win", "x64"), "node-v24.19.0-win-x64.zip");
    }

    #[test]
    fn archive_url_points_at_official_dist() {
        assert_eq!(
            archive_url("24.19.0", "darwin", "arm64"),
            "https://nodejs.org/dist/v24.19.0/node-v24.19.0-darwin-arm64.tar.xz"
        );
    }

    #[test]
    fn extracted_top_dir_matches_archive_stem() {
        assert_eq!(extracted_top_dir("24.19.0", "darwin", "arm64"), "node-v24.19.0-darwin-arm64");
        assert_eq!(extracted_top_dir("24.19.0", "win", "x64"), "node-v24.19.0-win-x64");
    }

    #[test]
    fn node_bin_points_into_version_dir() {
        let tc = PathBuf::from("/home/u/.dsh/toolchain");
        let name = if cfg!(windows) { "node.exe" } else { "node" };
        assert_eq!(node_bin(&tc, "24.19.0"), tc.join("node-24.19.0").join("bin").join(name));
    }

    #[test]
    fn npm_cli_from_node_derives_lib_path() {
        let node = PathBuf::from("/x/toolchain/node-24.19.0/bin/node");
        assert_eq!(
            npm_cli_from_node(&node),
            PathBuf::from("/x/toolchain/node-24.19.0/lib/node_modules/npm/bin/npm-cli.js")
        );
    }

    #[test]
    fn platform_arch_maps_native_values() {
        let (os, arch) = platform_arch().expect("supported platform");
        assert!(os == "darwin" || os == "win");
        assert!(arch == "x64" || arch == "arm64");
    }

    #[test]
    fn node_version_parse_accepts_22_or_newer() {
        assert!(parse_node_version_ok("v22.9.0\n"));
        assert!(parse_node_version_ok("v24.19.0\n"));
    }

    #[test]
    fn node_version_parse_rejects_older_or_garbage() {
        assert!(!parse_node_version_ok("v20.3.1\n"));
        assert!(!parse_node_version_ok("v18.0.0\n"));
        assert!(!parse_node_version_ok("garbage"));
        assert!(!parse_node_version_ok(""));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test bootstrap::tests`(workdir `src-tauri`)
Expected: 编译失败(`platform_arch().expect` 返回 None 会 panic)或多个断言 FAIL。

- [ ] **Step 3: 实现函数使测试通过**

把 `bootstrap.rs` 中的桩替换为真实实现:

```rust
//! First-launch bootstrap: download Node.js and install @deepseek-ai/dsh
//! into a user-local toolchain (~/.dsh/toolchain).

use std::path::{Path, PathBuf};

use semver::Version;

pub const NODE_VERSION: &str = "24.19.0";
pub const MIN_NODE_VERSION: &str = "22.0.0";
pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";

/// HOME on Unix, USERPROFILE on Windows — mirrors main.rs state-file lookup.
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn toolchain_dir() -> PathBuf {
    home_dir().join(".dsh").join("toolchain")
}

pub fn node_dir(toolchain: &Path, version: &str) -> PathBuf {
    toolchain.join(format!("node-{}", version))
}

pub fn node_bin(toolchain: &Path, version: &str) -> PathBuf {
    node_dir(toolchain, version)
        .join("bin")
        .join(if cfg!(windows) { "node.exe" } else { "node" })
}

pub fn npm_cli_js(toolchain: &Path, version: &str) -> PathBuf {
    node_dir(toolchain, version)
        .join("lib")
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js")
}

/// Derive the bundled npm-cli.js path from the node binary location,
/// so it works for whatever version dir actually exists.
pub fn npm_cli_from_node(node: &Path) -> PathBuf {
    node.parent()
        .unwrap_or(Path::new(""))
        .parent()
        .unwrap_or(Path::new(""))
        .join("lib")
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js")
}

/// (os, arch) naming used by nodejs.org release files; None on unsupported platforms.
pub fn platform_arch() -> Option<(&'static str, &'static str)> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        return None;
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    Some((os, arch))
}

pub fn archive_name(version: &str, os: &str, arch: &str) -> String {
    let ext = if os == "darwin" { "tar.xz" } else { "zip" };
    format!("node-v{}-{}-{}.{}", version, os, arch, ext)
}

pub fn archive_url(version: &str, os: &str, arch: &str) -> String {
    format!("https://nodejs.org/dist/v{}/{}", version, archive_name(version, os, arch))
}

/// Top-level directory name inside the downloaded archive.
pub fn extracted_top_dir(version: &str, os: &str, arch: &str) -> String {
    format!("node-v{}-{}-{}", version, os, arch)
}

/// True when `node -v` output parses as >= MIN_NODE_VERSION.
pub fn parse_node_version_ok(output: &str) -> bool {
    let v = output.trim().strip_prefix('v').unwrap_or(output.trim());
    match Version::parse(v) {
        Ok(parsed) => parsed >= Version::parse(MIN_NODE_VERSION).unwrap(),
        Err(_) => false,
    }
}
```

保持 Step 1 写入的 `#[cfg(test)] mod tests { ... }` 整个模块不动,只替换其上方(文件中)的函数实现。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test bootstrap::tests`(workdir `src-tauri`)
Expected: 9 个测试全部 PASS。

- [ ] **Step 5: 添加 reqwest 依赖并声明模块**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 段添加(默认 feature 已含 rustls 版 TLS,无需系统 OpenSSL):

```toml
reqwest = { version = "0.13", features = ["blocking"] }
```

在 `src-tauri/src/main.rs` 顶部(`use` 语句之前)添加:

```rust
mod bootstrap;
```

- [ ] **Step 6: 验证全量编译与告警**

Run: `cargo check`(workdir `src-tauri`)
Expected: 编译通过,零告警(此时 `bootstrap` 模块部分函数未用,可能触发 dead_code 告警;如有,在未使用的函数上临时加 `#[allow(dead_code)]`,后续任务会消费它们——若编译器未告警则不加)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/bootstrap.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/main.rs
git commit -m "feat: bootstrap module scaffolding with pure helpers for node toolchain"
```

---

### Task 2: bootstrap.rs 下载/解压/npm 安装 I/O

**Files:**
- Modify: `src-tauri/src/bootstrap.rs`
- Test: `src-tauri/src/bootstrap.rs`(测试模块追加)

**Interfaces:**
- Consumes: Task 1 的全部产物。
- Produces:
  - `pub fn node_version_ok(node: &Path) -> bool`
  - `pub fn private_node_and_dsh(toolchain: &Path) -> Option<(PathBuf, PathBuf)>`
  - `pub fn npm_install_args(prefix: &Path) -> Vec<String>`
  - `pub fn install_dsh(node: &Path, npm_cli: &Path, prefix: &Path) -> Result<String, String>`
  - `pub fn download_node(url: &str, dest: &Path, on_progress: impl FnMut(u64, Option<u64>)) -> Result<(), String>`
  - `pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String>`
  - `pub fn move_into_node_dir(toolchain: &Path, version: &str, os: &str, arch: &str) -> Result<(), String>`
  - `pub enum Step { Download, Extract, Install }`(供进度回调与 I18n 使用)
  - `pub fn install(mut on_progress: impl FnMut(Step, Option<f64>)) -> Result<(), String>`

- [ ] **Step 1: 写失败测试(追加到 bootstrap.rs 的 tests 模块)**

追加以下测试(用临时目录构造私有工具链结构,不依赖网络):

```rust
    #[test]
    fn private_node_and_dsh_requires_both_parts() {
        let root = std::env::temp_dir().join(format!("dsh-bootstrap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        std::fs::create_dir_all(tc.join("node-24.19.0/bin")).unwrap();
        std::fs::write(tc.join("node-24.19.0/bin/node"), b"").unwrap();
        assert!(private_node_and_dsh(&tc).is_none());
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();
        assert!(private_node_and_dsh(&tc).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_node_and_dsh_picks_newest_version() {
        let root = std::env::temp_dir().join(format!("dsh-bootstrap-newest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        for v in ["node-22.9.0", "node-24.19.0"] {
            std::fs::create_dir_all(tc.join(v).join("bin")).unwrap();
            std::fs::write(tc.join(v).join("bin/node"), b"").unwrap();
        }
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();
        let (node, _dsh) = private_node_and_dsh(&tc).expect("both parts present");
        assert!(node.to_string_lossy().contains("node-24.19.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn npm_install_args_target_private_prefix() {
        let args = npm_install_args(Path::new("/home/u/.dsh/toolchain"));
        assert_eq!(
            args,
            vec![
                "install".to_string(),
                "--prefix".to_string(),
                "/home/u/.dsh/toolchain".to_string(),
                "--no-fund".to_string(),
                "--no-audit".to_string(),
                "@deepseek-ai/dsh".to_string(),
            ]
        );
    }

    #[test]
    fn install_dsh_errors_when_node_missing() {
        let result = install_dsh(Path::new("/nonexistent/node"), Path::new("/nonexistent/npm-cli.js"), Path::new("/tmp"));
        assert!(result.is_err());
    }
```

`node_version_ok`、`private_node_and_dsh`、`npm_install_args`、`install_dsh` 四个函数此时尚不存在,测试先以编译失败方式失败。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test bootstrap::tests`(workdir `src-tauri`)
Expected: 新增 4 个测试 FAIL。

- [ ] **Step 3: 实现函数**

在 `bootstrap.rs` 追加实现(依赖:`use std::io::{Read, Write}; use std::process::Command;`):

```rust
/// Run `node -v` and check it meets MIN_NODE_VERSION.
pub fn node_version_ok(node: &Path) -> bool {
    Command::new(node)
        .arg("-v")
        .output()
        .map(|o| parse_node_version_ok(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false)
}

/// Locate a complete private toolchain: newest node-<ver>/bin/node plus the
/// installed dsh shim, if both exist.
pub fn private_node_and_dsh(toolchain: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut versions: Vec<(Version, PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(toolchain).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let ver = name.strip_prefix("node-").and_then(|v| Version::parse(v).ok())?;
        versions.push((ver, e.path()));
    }
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    let (_ver, dir) = versions.into_iter().next()?;
    let node = dir
        .join("bin")
        .join(if cfg!(windows) { "node.exe" } else { "node" });
    let dsh = toolchain.join("node_modules").join(".bin").join("dsh");
    if node.is_file() && dsh.exists() {
        Some((node, dsh))
    } else {
        None
    }
}

/// Args for `node npm-cli.js <args...>` installing dsh into the private prefix.
pub fn npm_install_args(prefix: &Path) -> Vec<String> {
    vec![
        "install".to_string(),
        "--prefix".to_string(),
        prefix.to_string_lossy().into_owned(),
        "--no-fund".to_string(),
        "--no-audit".to_string(),
        DSH_PACKAGE.to_string(),
    ]
}

/// Run `<node> <npm-cli.js> install ...`; Ok(output) on success, Err(tail) on failure.
pub fn install_dsh(node: &Path, npm_cli: &Path, prefix: &Path) -> Result<String, String> {
    let output = Command::new(node)
        .arg(npm_cli)
        .args(npm_install_args(prefix))
        .output()
        .map_err(|e| format!("failed to run npm: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "npm install failed (exit {:?}):\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Stream-download `url` to `dest`, calling `on_progress(bytes, total)` at most
/// once per MiB (and once at completion).
pub fn download_node(
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .read_timeout(Some(std::time::Duration::from_secs(600)))
        .build()
        .map_err(|e| format!("http client: {}", e))?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length();
    let mut file = std::fs::File::create(dest).map_err(|e| format!("create {}: {}", dest.display(), e))?;
    let mut downloaded: u64 = 0;
    let mut since_emit: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("read: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| format!("write: {}", e))?;
        downloaded += n as u64;
        since_emit += n as u64;
        let done = total.map(|t| downloaded >= t).unwrap_or(false);
        if since_emit >= 1024 * 1024 || done {
            since_emit = 0;
            on_progress(downloaded, total);
        }
    }
    file.flush().map_err(|e| format!("flush: {}", e))?;
    Ok(())
}

/// Extract the downloaded archive into `dest_dir` using system tools:
/// `tar -xJf` on macOS, PowerShell `Expand-Archive` on Windows.
pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| format!("mkdir {}: {}", dest_dir.display(), e))?;
    let status = if cfg!(target_os = "macos") {
        Command::new("tar")
            .arg("-xJf")
            .arg(archive)
            .arg("-C")
            .arg(dest_dir)
            .status()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive.display(),
                dest_dir.display()
            ))
            .status()
    } else {
        return Err("unsupported platform".to_string());
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("extract failed with status: {}", s)),
        Err(e) => Err(format!("failed to run extractor: {}", e)),
    }
}

/// Rename the archive's top-level dir to the canonical `node-<version>` name.
pub fn move_into_node_dir(toolchain: &Path, version: &str, os: &str, arch: &str) -> Result<(), String> {
    let from = toolchain.join(extracted_top_dir(version, os, arch));
    let to = node_dir(toolchain, version);
    std::fs::rename(&from, &to)
        .map_err(|e| format!("rename {} -> {}: {}", from.display(), to.display(), e))
}

/// Bootstrap progress states, surfaced to the UI (localized by the caller).
pub enum Step {
    Download,
    Extract,
    Install,
}

/// Full bootstrap: clean any half-installed toolchain, download, extract,
/// install dsh, verify. `on_progress(step, percent)` — percent is None while
/// the step is indeterminate.
pub fn install(mut on_progress: impl FnMut(Step, Option<f64>)) -> Result<(), String> {
    let (os, arch) = platform_arch().ok_or("unsupported platform/arch")?;
    let toolchain = toolchain_dir();
    if toolchain.exists() {
        std::fs::remove_dir_all(&toolchain).map_err(|e| format!("cleanup {}: {}", toolchain.display(), e))?;
    }
    std::fs::create_dir_all(&toolchain).map_err(|e| format!("mkdir {}: {}", toolchain.display(), e))?;

    let archive = toolchain.join(archive_name(NODE_VERSION, os, arch));
    let url = archive_url(NODE_VERSION, os, arch);
    println!("[bootstrap] downloading {}", url);
    download_node(&url, &archive, |downloaded, total| {
        let pct = match total {
            Some(t) if t > 0 => downloaded as f64 / t as f64,
            _ => 0.0,
        };
        on_progress(Step::Download, Some(pct));
    })?;

    on_progress(Step::Extract, None);
    extract_archive(&archive, &toolchain)?;
    move_into_node_dir(&toolchain, NODE_VERSION, os, arch)?;
    let _ = std::fs::remove_file(&archive);

    on_progress(Step::Install, None);
    let node = node_bin(&toolchain, NODE_VERSION);
    let npm_cli = npm_cli_js(&toolchain, NODE_VERSION);
    println!("[bootstrap] installing {} via {}", DSH_PACKAGE, npm_cli.display());
    install_dsh(&node, &npm_cli, &toolchain)?;

    if private_node_and_dsh(&toolchain).is_none() {
        return Err("toolchain incomplete after install".to_string());
    }
    println!("[bootstrap] toolchain ready at {}", toolchain.display());
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test bootstrap::tests`(workdir `src-tauri`)
Expected: 全部 PASS(13 个)。

- [ ] **Step 5: 验证编译与告警**

Run: `cargo check && cargo clippy`(workdir `src-tauri`)
Expected: 零告警(此任务结束后 `install`/`download_node` 等可能仍未被 main.rs 使用,如触发 dead_code 告警,临时加 `#[allow(dead_code)]` 于模块级 `#![allow(dead_code)]`?不允许——改为在下一任务前保持,若告警则在本任务先加 `#[allow(dead_code)]` 到具体函数,后续任务删除)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/bootstrap.rs
git commit -m "feat: bootstrap download, extract and npm install functions"
```

---

### Task 3: DshMode::Private 模式检测与 runner

**Files:**
- Modify: `src-tauri/src/main.rs`(enum、`detect_dsh_mode`、`dsh_runner` 及测试)

**Interfaces:**
- Consumes: `bootstrap::private_node_and_dsh`、`bootstrap::toolchain_dir`(Task 2)。
- Produces: `DshMode::Private { node: PathBuf, dsh: PathBuf }`,在 `detect_dsh_mode` 中位于 Global 之后、Npx 之前;`dsh_runner` 新增匹配分支。

- [ ] **Step 1: 写失败测试(追加到 main.rs 测试模块)**

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test private_mode_runs_node_on_private_dsh_shim`(workdir `src-tauri`)
Expected: 编译错误(`DshMode` 无 `Private` 变体)。

- [ ] **Step 3: 实现**

修改 `src-tauri/src/main.rs`:

1. `DshMode` 枚举(约 main.rs:130)改为:

```rust
enum DshMode {
    Source(PathBuf),
    Bundled(PathBuf),
    Global(PathBuf),
    Private { node: PathBuf, dsh: PathBuf },
    Npx,
}
```

2. `detect_dsh_mode`(约 main.rs:137)在 Global 检查之后、`DshMode::Npx` 之前插入:

```rust
    if let Some((node, dsh)) = bootstrap::private_node_and_dsh(&bootstrap::toolchain_dir()) {
        return DshMode::Private { node, dsh };
    }
```

3. `dsh_runner`(约 main.rs:156)新增分支:

```rust
        DshMode::Private { node, dsh } => (
            node.to_string_lossy().into_owned(),
            vec![dsh.to_string_lossy().into_owned()],
            None,
        ),
```

- [ ] **Step 4: 运行全部测试确认通过**

Run: `cargo test`(workdir `src-tauri`)
Expected: 全部 PASS(main.rs 现有测试 + 新增 1 + bootstrap 模块 13)。

- [ ] **Step 5: 验证编译与告警**

Run: `cargo check && cargo clippy`(workdir `src-tauri`)
Expected: 零告警(删除 Task 2 遗留的 `#[allow(dead_code)]`,因为 `private_node_and_dsh` 现已被使用)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs src-tauri/src/bootstrap.rs
git commit -m "feat: add DshMode::Private for user-local toolchain"
```

---

### Task 4: 引导窗口 HTML 与 I18n 文案

**Files:**
- Create: `dist/bootstrap.html`
- Modify: `src-tauri/src/main.rs`(`I18n` 新增方法 + 测试)

**Interfaces:**
- Consumes: `bootstrap::Step`(Task 2)。
- Produces:
  - `dist/bootstrap.html`:定义全局函数 `window.__dsbUpdate(state, percent)`(percent 为 null 表示不确定进度)。
  - `I18n::bootstrap_title() -> &'static str`
  - `I18n::bootstrap_step(step: bootstrap::Step) -> String`
  - `I18n::bootstrap_failed_title() -> &'static str`
  - `I18n::bootstrap_failed_msg(tail: &str) -> String`

- [ ] **Step 1: 写失败测试(追加到 main.rs 测试模块;I18n 为 pub 结构,is_zh 字段同模块可见)**

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test bootstrap_copy`(workdir `src-tauri`)
Expected: 编译错误(`I18n` 无这些方法)。

- [ ] **Step 3: 实现 I18n 方法**

在 `impl I18n`(约 main.rs:29)追加:

```rust
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
```

- [ ] **Step 4: 创建 dist/bootstrap.html**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <title>DeepSeek Harness Setup</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background: #1a1a2e; color: #eee; margin: 0; display: flex; align-items: center; justify-content: center; height: 100vh; }
    .wrap { width: 80%; }
    p { font-size: 15px; text-align: center; margin: 0 0 18px; }
    .bar { height: 6px; background: #2d2d4a; border-radius: 3px; overflow: hidden; }
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
    function __dsbUpdate(state, percent) {
      document.getElementById('status').textContent = state;
      var fill = document.getElementById('fill');
      if (percent === null) {
        fill.classList.add('indeterminate');
      } else {
        fill.classList.remove('indeterminate');
        fill.style.width = Math.round(percent * 100) + '%';
      }
    }
  </script>
</body>
</html>
```

说明:CSP 已含 `script-src 'self' 'unsafe-inline'` 与 `style-src 'self' 'unsafe-inline'`(tauri.conf.json:13),内联 script/style 可运行;该页面不需要 `__TAURI__`(进度由 Rust 侧 `eval` 推送),因此 `withGlobalTauri: false` 与 capabilities 都不用改。

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test bootstrap_copy`(workdir `src-tauri`)
Expected: 2 个新测试 PASS;`cargo test` 全量 PASS。

- [ ] **Step 6: 验证编译与告警**

Run: `cargo check && cargo clippy`(workdir `src-tauri`)
Expected: 零告警(新 I18n 方法暂未被调用时如触发 dead_code,下一任务即消费,可先加 `#[allow(dead_code)]` 或直接等 Task 5 一并验证)。

- [ ] **Step 7: Commit**

```bash
git add dist/bootstrap.html src-tauri/src/main.rs
git commit -m "feat: bootstrap window html and localized setup copy"
```

---

### Task 5: ensure_toolchain 编排与 setup 异步重构

**Files:**
- Modify: `src-tauri/src/main.rs`(`ensure_toolchain`、`fail_startup`、`setup`、`on_window_event`)

**Interfaces:**
- Consumes: Task 1-4 全部产物。
- Produces: `async fn ensure_toolchain(handle: &tauri::AppHandle, i18n: &I18n) -> Result<(), String>`;setup 内异步续接流程。

- [ ] **Step 1: 新增 `ensure_toolchain` 与 `fail_startup`**

在 `main.rs` 的 dsh process management 段之前新增:

```rust
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

    let win = tauri::WebviewWindowBuilder::new(
        handle,
        "bootstrap",
        tauri::WebviewUrl::App("bootstrap.html".into()),
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
                let _ = win2.eval(&format!("window.__dsbUpdate({}, {})", msg, pct));
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
```

注意:成功路径用 `win.destroy()`(跳过 CloseRequested,因为下一步会在事件处理里拦截该窗口的用户关闭);`blocking_show` 从 spawn_blocking 调用与现有升级流程模式一致(main.rs 现有 `on_menu_event`)。

- [ ] **Step 2: 重构 setup 为异步续接**

**合并注意:** 若执行本任务时 setup 中已存在其他菜单项(如已合入的 about/help 菜单 `help_menu`/`feedback`/`export_logs` 及对应 `on_menu_event` 分支),替换 setup 时必须原样保留那些菜单项与事件分支,只改动启动流程(引导 → dsh → 主窗口)的结构;下面代码中的菜单段是最小基线,按现状合并。

把 `setup` 闭包体(当前 main.rs 中 `setup(|app| { ... })`)整体替换为:

```rust
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
                let submenu = Submenu::with_items(&handle, "DeepSeek Harness", true, &[&check_item])
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
```

- [ ] **Step 3: 拦截 bootstrap 窗口的关闭事件**

把 `on_window_event` 闭包(main.rs:684-691)替换为:

```rust
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
```

- [ ] **Step 4: 编译、告警与测试**

Run: `cargo check`(workdir `src-tauri`)
Expected: 编译通过。

Run: `cargo clippy`(workdir `src-tauri`)
Expected: 零告警。

Run: `cargo fmt -- --check`(workdir `src-tauri`)
Expected: 无差异;若有,执行 `cargo fmt` 后重跑。

Run: `cargo test`(workdir `src-tauri`)
Expected: 全部 PASS(main.rs 现有测试 + 本次新增,加 bootstrap 模块 13)。

- [ ] **Step 5: 手动端到端验证引导流程(macOS)**

前置:确认本机 shell 的 PATH 能找到 node(`which node`),然后:

```sh
rm -rf ~/.dsh/toolchain
env PATH=/usr/bin:/bin cargo run
```

(工作目录 `src-tauri`。`PATH=/usr/bin:/bin` 让 `find_program("node")` 在 PATH 中找不到 node,强制触发引导。注意:`toolchain_dirs()` 还会探测 `/usr/local/bin`、`/opt/homebrew/bin` 等绝对路径,这些不受 PATH 影响——若应用没弹引导窗口,说明在这些目录里找到了 node;此时可临时 `mv /opt/homebrew/bin/node /opt/homebrew/bin/node.bak`(以及 `/usr/local/bin/node` 同理)后重试,验证完务必移回,或改用干净虚拟机/新机器验证。)

Expected:
1. 弹出「DeepSeek Harness Setup」引导窗口,状态依次为「正在下载 Node.js…」→「正在解压 Node.js…」→「正在安装 dsh…」,下载阶段进度条随百分比增长;
2. 引导窗口自动销毁,主窗口正常打开并显示 dsh Web UI(来自 `~/.dsh/toolchain` 私有工具链);
3. 日志(`[bootstrap]` 前缀)显示下载、安装过程;
4. `ls ~/.dsh/toolchain` 可见 `node-24.19.0/` 与 `node_modules/`。

然后正常启动(不加 PATH 限制):

```sh
cargo run
```

Expected: 无引导窗口(私有工具链已存在),直接进入主窗口。

清理测试残留(可选):

```sh
rm -rf ~/.dsh/toolchain
```

- [ ] **Step 6: 手动验证失败重试路径(macOS,可选)**

```sh
rm -rf ~/.dsh/toolchain
env PATH=/usr/bin:/bin HTTPS_PROXY=http://127.0.0.1:1 cargo run
```

(代理指向不可达端口模拟下载失败。)

Expected: 下载失败后弹出「初始化失败」错误对话框(含日志尾部),点「Yes」重试、「No」退出应用;点 No 后进程退出、引导窗口销毁。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: first-launch bootstrap orchestration with progress window and retry"
```

---

### Task 6: 升级/版本检查走私有 npm

**Files:**
- Modify: `src-tauri/src/main.rs`(`private_npm_cmd`、`run_upgrade`、`latest_version` 及测试)

**Interfaces:**
- Consumes: `bootstrap::private_node_and_dsh`、`bootstrap::npm_cli_from_node`、`bootstrap::toolchain_dir`(Task 2)。
- Produces: `fn private_npm_cmd(toolchain: &Path, prefix: &Path, extra: &[&str]) -> Option<(String, Vec<String>)>`。

- [ ] **Step 1: 写失败测试(追加到 main.rs 测试模块)**

```rust
    #[test]
    fn private_npm_cmd_builds_node_npmcli_args() {
        let root = std::env::temp_dir().join(format!("dsh-npmcmd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        std::fs::create_dir_all(tc.join("node-24.19.0/bin")).unwrap();
        std::fs::write(tc.join("node-24.19.0/bin/node"), b"").unwrap();
        let npm_cli = tc.join("node-24.19.0/lib/node_modules/npm/bin");
        std::fs::create_dir_all(&npm_cli).unwrap();
        std::fs::write(npm_cli.join("npm-cli.js"), b"").unwrap();
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();

        let (cmd, args) = private_npm_cmd(&tc, &tc, &["view", "@deepseek-ai/dsh", "version"])
            .expect("complete private toolchain");
        assert_eq!(cmd, tc.join("node-24.19.0/bin/node").to_string_lossy());
        assert_eq!(
            args,
            vec![
                tc.join("node-24.19.0/lib/node_modules/npm/bin/npm-cli.js").to_string_lossy().into_owned(),
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test private_npm_cmd`(workdir `src-tauri`)
Expected: 编译错误(`private_npm_cmd` 未定义)。

- [ ] **Step 3: 实现 `private_npm_cmd` 并接入 `run_upgrade` / `latest_version`**

在 main.rs 版本辅助段(latest_version 之前)新增:

```rust
/// Command to run npm via the private toolchain's node + bundled npm-cli.js,
/// when a complete private toolchain exists. Falls back to None otherwise.
fn private_npm_cmd(toolchain: &Path, prefix: &Path, extra: &[&str]) -> Option<(String, Vec<String>)> {
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
```

`run_upgrade`(约 main.rs:418)的 else 分支改为:

```rust
    } else if let Some((node, args)) = private_npm_cmd(
        &bootstrap::toolchain_dir(),
        &bootstrap::toolchain_dir(),
        &["install", "--no-fund", "--no-audit", "@deepseek-ai/dsh@latest"],
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
```

`latest_version`(约 main.rs:380)改为先取私有 npm 命令:

```rust
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
```

(需要确认 `Path` 已 use——main.rs 顶部已有 `use std::path::{Path, PathBuf};`,无需新增。)

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test private_npm_cmd`(workdir `src-tauri`)
Expected: 2 个新测试 PASS。

Run: `cargo test`(workdir `src-tauri`)
Expected: 全量 PASS。

- [ ] **Step 5: 验证编译与告警**

Run: `cargo check && cargo clippy && cargo fmt -- --check`(workdir `src-tauri`)
Expected: 全部干净。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: run upgrade and version check via private npm when available"
```

---

### Task 7: README 更新与全量验证

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: 无。
- Produces: 无。

- [ ] **Step 1: 更新 README「前置条件」与「使用」段**

`README.md` 的「前置条件」改为区分最终用户与开发者:

```markdown
## 前置条件

- 最终用户:无。首次启动会自动下载安装私有 Node.js 与 dsh 到 `~/.dsh/toolchain/`(需网络,无需管理员权限);机器已有 Node.js(>=22)时直接复用。
- 开发者(从源码运行):
  - Rust >= 1.70(`rustup` + stable 工具链)
  - macOS:Xcode Command Line Tools(`xcode-select --install`)
  - Linux:`libwebkit2gtk-4.1-dev` + `libgtk-3-dev`
  - Node.js >= 22.19.0(用于运行 `dsh`)
  - 先 `pnpm install && pnpm run build`(仓库根)
```

把「发布」段末尾的注替换为:

```markdown
> 注:应用首启时自动下载 Node.js 官方二进制(当前 LTS v24.19.0)与 `@deepseek-ai/dsh` 到用户目录,目标机器只需网络。macOS 未签名产物会有 Gatekeeper 提示,需配置签名/公证。
```

- [ ] **Step 2: 全量验证**

Run: `cargo test`(workdir `src-tauri`)
Expected: 全部 PASS。

Run: `cargo clippy`(workdir `src-tauri`)
Expected: 零告警。

Run: `cargo fmt -- --check`(workdir `src-tauri`)
Expected: 无差异。

Run: `git status --short`
Expected: 除 `README.md` 外无未提交改动。

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update prerequisites for first-launch bootstrap"
```

---

## 完成后的行为总结

- 新电脑(macOS/Windows):双击安装包 → 首启引导窗口下载/安装 → 直接可用;后续启动秒开。
- 已有 Node >= 22 的机器:跳过引导,行为与现在一致(Source/Bundled/Global/Npx 优先级不变,仅新增 Private 档位)。
- 升级菜单:Private 模式下用私有 npm 升级 dsh;其他模式不变。

## 实施修订记录(执行期间发现并裁定)

- **Task 2**:`private_node_and_dsh` 的 `?`-in-loop 写法是计划缺陷——工具链目录中总有 `node_modules/` 等非 `node-*` 条目,`?` 会从函数返回 `None`,导致完整工具链永远检测不到。已按 skip 模式修正(评审裁定 JUSTIFIED)。
- **Task 2**:reqwest 0.13 blocking `ClientBuilder` 没有 `read_timeout`,只有 `timeout`;计划中的 `read_timeout` 改为 `timeout`(评审裁定 JUSTIFIED)。
