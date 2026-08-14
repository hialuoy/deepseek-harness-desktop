# 删除"升级 dsh…"菜单项,只保留"检查更新…"

日期:2026-08-14

## 背景

应用菜单当前有两个条目:

- **检查更新…**(`check_updates`):查询当前/最新版本,仅在有新版时弹窗询问是否升级。
- **升级 dsh…**(`upgrade`):不检查版本,直接执行升级(源模式 `git pull --rebase --autostash && pnpm install && pnpm run build`,生产模式 `npm i -g @deepseek-ai/dsh@latest`)。

用户反馈:直接升级入口容易误点(且检查失败时无法判断是否有新版),希望删除"升级 dsh…"项,只保留"检查更新…"。

## 改动

全部位于 `src-tauri/src/main.rs`:

1. `setup` 中删除 `upgrade_item` 的创建与 `Submenu::with_items` 中的引用,菜单只含 `check_item`。
2. `on_menu_event` 删除 `"upgrade"` 匹配分支。
3. `I18n` 删除 `upgrade()` 方法(删除菜单项后无调用方,避免死代码)。

保留不动:

- `run_upgrade`、`show_upgrade_progress`、`upgrade_title`/`upgrade_success_msg`/`upgrade_failed_msg`/`upgrade_error_msg`——"检查更新"发现新版并确认后仍会执行升级并展示结果。
- 启动时每日自动检查(`should_auto_prompt`)及其升级提示流程。
- "检查更新…"菜单文案不变。

## 权衡

- 已是最新版本(或版本检查因网络失败返回 unknown)时,不再有手动强制升级/重装入口。这是有意为之:升级入口统一经由版本检查,行为更可预期。
- 后续如需"修复安装/强制重装",可另加显式入口。

## 测试

- 现有 9 个单元测试全部保持通过(本次未改动被测逻辑)。
- `cargo check` / `cargo clippy` 无告警。
- 手动验证:运行应用,菜单仅显示"检查更新…"。
