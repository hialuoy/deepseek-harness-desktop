# 删除"升级 dsh…"菜单项实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除应用菜单中的"升级 dsh…"项,只保留"检查更新…"作为唯一升级入口。

**Architecture:** 纯删除改动,只涉及 `src-tauri/src/main.rs`:移除菜单项创建、对应菜单事件分支、以及随之失效的 `I18n::upgrade()` 方法。保留 `run_upgrade`/`show_upgrade_progress`/`upgrade_*` 文案,"检查更新"确认升级后仍使用。

**Tech Stack:** Rust / Tauri 2 / cargo

## Global Constraints

- 只修改 `src-tauri/src/main.rs`,不改其他文件(设计文档已提交)。
- "检查更新…"菜单文案不变。
- 现有 9 个单元测试必须全部通过。
- `cargo check` 与 `cargo clippy` 必须零告警(dead_code 告警意味着删除不彻底)。
- 保留 `run_upgrade`、`show_upgrade_progress`、`upgrade_title`/`upgrade_success_msg`/`upgrade_failed_msg`/`upgrade_error_msg`,检查更新流程仍依赖它们。

---

### Task 1: 移除升级菜单项及其代码路径

**Files:**
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: 无(现有代码)。
- Produces: 无新符号;删除 `I18n::upgrade()` 与菜单事件 `"upgrade"` 分支。

- [ ] **Step 1: 删除 `on_menu_event` 中的 `"upgrade"` 分支**

在 `src-tauri/src/main.rs` 的 `on_menu_event` 函数中,删除以下整个分支:

```rust
        "upgrade" => {
            let handle = handle.clone();
            let i18n = i18n.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = run_upgrade();
                show_upgrade_progress(&handle, &i18n, result);
            });
        }
```

删除后 `match id` 只剩 `"check_updates"` 分支与 `_ => {}`。

- [ ] **Step 2: 从 setup 中删除 `upgrade_item` 的创建与引用**

删除这一行:

```rust
            let upgrade_item = MenuItem::with_id(app, "upgrade", i18n.upgrade(), true, None::<&str>)?;
```

并把 `Submenu::with_items` 的项列表改为只含 `check_item`:

```rust
            let submenu = Submenu::with_items(
                app,
                "DeepSeek Harness",
                true,
                &[&check_item],
            )?;
```

- [ ] **Step 3: 删除 `I18n::upgrade()` 方法**

删除:

```rust
    fn upgrade(&self) -> &'static str {
        if self.is_zh { "升级 dsh…" } else { "Upgrade dsh…" }
    }
```

- [ ] **Step 4: 验证编译与告警**

Run: `cargo check`(工作目录 `src-tauri`)
Expected: 编译通过,输出无 warning(若残留任何 `i18n.upgrade()` 引用会报错;若 `upgrade()` 方法未删会触发 dead_code 告警)。

Run: `cargo clippy`
Expected: 零告警。

- [ ] **Step 5: 运行单元测试**

Run: `cargo test`(工作目录 `src-tauri`)
Expected: 9 passed; 0 failed。

- [ ] **Step 6: 手动验证菜单**

Run: `nohup cargo run > /tmp/dsh-desktop-run.log 2>&1 &`
Expected: 应用窗口打开后,菜单栏 "DeepSeek Harness" 下只有"检查更新…"一项;点击后行为与之前一致(有新版才提示升级)。验证后关闭窗口(应用会随之退出)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat: remove direct-upgrade menu item, keep check-for-updates entry"
```
