# 应用自更新(检查并自动下载安装)实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让「检查更新…」同时检查 dsh 与桌面程序本身;桌面程序发现新版时通过 `tauri-plugin-updater` 自动下载、验签、安装(Windows passive 模式)。

**Architecture:** 纯 Rust(无前端),`src-tauri/src/main.rs` 单文件内新增自更新逻辑;`src-tauri/Cargo.toml` 加插件依赖;`src-tauri/tauri.conf.json` 加 updater 配置(占位 pubkey)。启动自动检查与菜单手动检查两条路径都接入。

**Tech Stack:** Rust / Tauri 2 / tauri-plugin-updater / cargo

## Global Constraints

- 提交信息英文 conventional commits;代码注释与文档中文。
- `cargo test` / `cargo clippy`(零告警)/ `cargo fmt` 全绿。
- pubkey 用占位符,不提交私钥;私钥仅存于用户 CI secret。
- 不改动 dsh 更新逻辑(`check_update` / `run_upgrade` / `show_upgrade_progress`)的既有行为。

---

### Task 1: 依赖与配置

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: 加依赖**

在 `[dependencies]` 下新增:

```toml
tauri-plugin-updater = "2"
```

- [ ] **Step 2: 加 tauri.conf.json 配置**

`bundle` 内加 `"createUpdaterArtifacts": true`;顶层加 `plugins.updater`(pubkey 占位 `__PUBKEY_PLACEHOLDER__`,endpoints 用 GitHub Releases 静态清单,`windows.installMode = "passive"`)。

- [ ] **Step 3: 验证编译(不触发签名)**

Run: `cargo check`(工作目录 `src-tauri`)
Expected: 通过。`cargo check`/`cargo test` 不要求签名私钥。

---

### Task 2: 注册插件

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 注册 updater 插件**

在 `tauri::Builder::default()` 链上、`tauri_plugin_dialog::init()` 之后加:

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
```

- [ ] **Step 2: 引入 trait**

文件顶部加 `use tauri_plugin_updater::UpdaterExt;`(仅 `#[cfg(desktop)]` 路径使用,Windows/macOS/Linux 均可用)。

---

### Task 3: 应用更新检查与安装

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 新增 `check_app_update`**

签名:`fn check_app_update(handle: &tauri::AppHandle) -> Result<Option<String>, String>`。

实现:调用 `handle.updater().map_err(...)?`,再 `updater.check().await`;`Ok(Some(update))` 返回 `Some(update.version)`(去掉可能的 `v` 前缀并 `trim`),`Ok(None)` 返回 `None`。

- [ ] **Step 2: 新增 `install_app_update`**

签名:`async fn install_app_update(update: tauri_plugin_updater::Update) -> Result<(), String>`。

实现:调用 `update.download_and_install(|_, _| {}, || {}).await`,把错误转成中文/英文可读字符串。进度交由安装器 passive 窗口。

- [ ] **Step 3: 错误文案**

复用 `I18n::upgrade_error_msg` 风格,新增应用更新失败/已最新等文案(见 Task 4)。

---

### Task 4: I18n 与提示状态

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 新增 I18n 方法(中英双语)**

新增:
- `app_update_title()` — 「更新 DeepSeek Harness」/「Update DeepSeek Harness」
- `app_update_msg(current, latest)` — 「发现 DeepSeek Harness 新版本…,是否下载安装?」
- `app_update_failed_msg(e)` — 「应用更新失败:\n…」
- `app_up_to_date_msg(current)` — 「DeepSeek Harness 已是最新版本(…)」
- 以及「检查更新」中两者均已最新的合并文案(可选,复用现有 `up_to_date_msg`)。

- [ ] **Step 2: 扩展 `PromptState`**

增加 `last_app_prompted_version` / `last_app_prompted_at`;把 `should_auto_prompt(latest)` 泛化为两个函数(或带 `is_app: bool`),保持「同一版本 24 小时内最多弹一次」语义。状态文件仍为 `~/.dsh/desktop-update-state.json`。

- [ ] **Step 3: 补单测**

覆盖新 I18n 中英文案、`PromptState` 序列化/去重逻辑。

---

### Task 5: 接入两条检查路径

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 菜单「检查更新…」**

`on_menu_event` 的 `"check_updates"` 分支改为 `tauri::async_runtime::spawn(async move { ... })`:

1. dsh 检查:`spawn_blocking(check_update).await`(保留原提示/升级逻辑)。
2. 应用检查:`check_app_update(&handle).await`;`Some(latest)` 时 `ask_yes_no` → `install_app_update(update).await` 并按结果弹窗;`None` 时提示已最新(与 dsh 一起考虑合并提示);`Err` 时弹错误框。

- [ ] **Step 2: 启动 5 秒自动检查**

同样加应用检查:应用有新版且 `should_auto_prompt_app(&latest)` 通过时弹窗询问 → 下载安装;失败仅记日志,不打扰。

- [ ] **Step 3: 手动验证(本机)**

`cargo run`(或 release exe)后:菜单「检查更新…」在无新版时给出"已是最新"提示、不崩溃;应用检查失败(占位 pubkey)时给出友好错误而非 panic。验证后关闭窗口。

---

### Task 6: 验证与提交

- [ ] **Step 1: 全量验证**

Run(工作目录 `src-tauri`):`cargo test`、`cargo clippy --all-targets`、`cargo fmt -- --check`
Expected: 全绿、零告警。

- [ ] **Step 2: 编译验证(不打包)**

Run: `cargo check`(或 `npx tauri build --no-bundle`)
Expected: 通过(此步骤不触发签名,因为不打包 bundle)。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/src/main.rs docs/superpowers/specs/2026-08-20-app-self-update-design.md docs/superpowers/plans/2026-08-20-app-self-update.md
git commit -m "feat: add in-app self-update via tauri-plugin-updater"
```

---

## 后续(用户侧,实现完成后一次性)

1. `npx tauri signer generate -w ~/.tauri/dsh-desktop.key` 生成密钥对。
2. `.pub` 内容替换 `tauri.conf.json` 的 `pubkey` 占位。
3. GitHub Secrets 增加 `TAURI_SIGNING_PRIVATE_KEY`(私钥)。
4. 更新 `.github/workflows/release.yml`:注入私钥、上传签名的 `latest.json` 与安装包。
5. 发版验证:安装旧版 → 检查更新 → 自动下载安装新版。
