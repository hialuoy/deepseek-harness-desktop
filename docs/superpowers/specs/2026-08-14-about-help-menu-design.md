# 关于/帮助菜单(帮助、提交反馈、导出日志)

日期:2026-08-14

## 背景

当前应用菜单只有一个 "DeepSeek Harness" 子菜单(仅"检查更新…"一项)。用户希望补充标准菜单项:关于对话框,以及 Help 菜单中的帮助(打开仓库主页)、提交反馈(打开 GitHub Issues)、导出日志(保存应用日志副本)。

## 菜单结构

- 应用菜单 "DeepSeek Harness":
  - **关于 DeepSeek Harness** → 弹 Info 对话框:应用名、版本号(`env!("CARGO_PKG_VERSION")`)、仓库地址 https://github.com/hialuoy/deepseek-harness-desktop(纯文本,不可点击)
  - **检查更新…**(保持不变)
- 新增 Help 菜单:
  - **帮助** → 浏览器打开 https://github.com/hialuoy/deepseek-harness-desktop
  - **提交反馈** → 浏览器打开 https://github.com/hialuoy/deepseek-harness-desktop/issues/new
  - **导出日志** → 保存对话框,把日志文件复制到用户所选位置,默认文件名 `dsh-desktop-<YYYYMMDD-HHMMSS>.log`

## 组件(全部在 src-tauri/src/main.rs)

1. **I18n 扩展**:新增 `about()`、`help()`、`feedback()`、`export_logs()` 文案与 `about_msg(version)` 对话框内容,中英双语,沿用现有 `is_zh` 模式。
2. **`open_url(url: &str)`**:按平台执行 `open <url>`(macOS)/ `xdg-open <url>`(Linux)/ `cmd /C start <url>`(Windows),失败仅写日志,不弹错误对话框(浏览器打不开属罕见环境问题,不打断用户)。
3. **`log_line(prefix: &str, msg: &str)`**:输出到 stdout 并追加到日志文件。日志文件路径 `~/.dsh/desktop.log`(`HOME`/`USERPROFILE` 兜底,目录不存在则创建)。启动时若文件超过 1MB,轮转为 `desktop.old.log`(覆盖旧轮转文件)。
4. **日志点替换**:现有 `println!`/`eprintln!` 日志输出点全部改为 `log_line`(含 [desktop]、[dsh]、[upgrade]、[upgrade:err]、版本检查等)。dsh 子进程 stdout 行经现有 BufReader 流入 `log_line`,自动进入日志文件。
5. **菜单事件**:`"about"` 弹对话框;`"help"`/`"feedback"` 调 `open_url`;`"export_logs"` 调 `blocking_save_file` 保存对话框 + `fs::copy` 日志文件(日志文件不存在时复制出一个空文件,不报错)。
6. **版本号**:`env!("CARGO_PKG_VERSION")` 编译期常量。

## 错误处理

- `open_url` spawn 失败:`log_line` 记录,静默。
- 日志追加失败(磁盘满/权限):静默,不中断应用运行。
- 导出失败(用户取消或 copy 失败):取消则无事发生;copy 失败弹 Info 对话框提示失败原因。

## 测试

- 纯函数化可测部分:平台命令构造(`open_url_command(os, url)`)、日志轮转判断(`should_rotate(size, cap)`)、导出默认文件名(`export_filename(now)`)。
- 现有 9 个单元测试全部保持通过;`cargo check`/`cargo clippy` 零告警。
- 手动验证:菜单项显示与点击行为(GUI,由人工完成)。
