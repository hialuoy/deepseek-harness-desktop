# AGENTS.md

DeepSeek Harness 桌面壳(Route A · Tauri v2)。独立 git 仓库(`hialuoy/deepseek-harness-desktop`),物理上位于 monorepo 的 `desktop-tauri/` 子目录,但**不是** pnpm workspace 成员——父仓库的 AGENTS.md/CLAUDE.md 不适用于本仓库,也不要修改父仓库文件。

## 架构要点

- 无前端、无 Tauri command/IPC:全部逻辑在 `src-tauri/src/main.rs` 单文件内。应用 spawn `dsh web --port 0`,从 stdout 解析 `dsh web: http://127.0.0.1:<port>` 行,用原生 WebView 加载该 URL。
- 窗口在 `setup` 中动态创建(`WebviewUrl::External`);`tauri.conf.json` 的 `windows` 为空、`devUrl` 是 `about:blank`,`dist/index.html` 只是占位页。CSP 只允许 127.0.0.1。
- dsh 模式探测优先级:Source(pnpm-workspace.yaml 向上最多 5 层)> Bundled(`Resources/app`)> Global > Npx。升级命令随模式不同:Source 模式在仓库根跑 `git pull --rebase --autostash && pnpm install && pnpm run build`,其余 `npm install -g @deepseek-ai/dsh@latest`。
- Finder 启动的 app PATH 极简,因此 `toolchain_dirs()` 显式探测 nvm、`/usr/local/bin`、`/opt/homebrew/bin` 等;Windows 程序名需带 `.exe`/`.cmd` shim。
- i18n:所有用户可见文案硬编码在 `main.rs` 的 `I18n` 结构体,中英双语(按 `sys-locale` 的 `zh*` 判断)。新增文案必须双语,并补对应单元测试。
- 运行时状态:`~/.dsh/desktop.log`(超 1MB 轮转为 `desktop.old.log`)、`~/.dsh/desktop-update-state.json`(同一版本每天最多自动弹一次更新提示)。

## 常用命令

- `pnpm dev` / `pnpm build`(package.json 脚本,分别映射 `cargo tauri dev/build`);仓库内无 lockfile,CI 用 `npm install` + `npx tauri build --no-bundle`。
- cargo 命令都在 `src-tauri/` 目录下执行。验证顺序:`cargo test`(17 个测试全在 main.rs 内)→ `cargo check` → `cargo clippy`(要求零告警)→ `cargo fmt`。
- 版本号三处必须一致,否则 `cargo tauri build` 失败:`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`。
- 在 monorepo 内运行 `pnpm dev` 会命中 Source 模式,要求父仓库已 `pnpm install && pnpm run build`。

## 工作流约定

- 特性开发先写规格再写实施计划,存于 `docs/superpowers/specs/` 和 `docs/superpowers/plans/`;实现前先读对应 plan(其中可能有未完成的任务)。
- 提交信息用英文 conventional commits(`feat:`/`fix:`/`docs:`/`chore:`/`ci:`);文档与对话用中文。
- CI:push/PR 跑 `build.yml` 三平台编译验证。发布:推 `v*` 标签或手动触发 release workflow(手动时版本号取 package.json),产物是 GitHub Release 草稿,需在 Releases 页手动发布。
