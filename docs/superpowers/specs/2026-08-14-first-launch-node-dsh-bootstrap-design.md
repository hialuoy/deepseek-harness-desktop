# 首启自动安装 Node.js + dsh(免依赖引导)

日期:2026-08-14

## 背景

当前 Tauri 壳在全新电脑上无法"即装即用":运行时依赖系统已装 Node.js(>=22),且 dsh 来自源码 / 全局安装 / npx 拉取。目标:macOS 与 Windows 新电脑双击安装包后,应用首次启动自动下载并安装私有 Node.js 与 `@deepseek-ai/dsh`,全程无需管理员权限;已有 Node 的机器行为完全不变。

## 目标平台

- macOS(arm64 / x64)
- Windows(x64 / arm64)

## 架构

```
main() setup
  ├─ 1. ensure_toolchain()          ← 新增,start_dsh 之前
  │     find_program("node") 且版本 >= 22? ──是──> 跳过(现有行为不变)
  │     否 → bootstrap:
  │        a. 清理旧半成品目录,打开引导窗口(状态文字 + 进度条,不可关闭)
  │        b. reqwest 流式下载 Node 官方二进制(带进度回调)
  │           macOS:   node-v<LTS>-darwin-{arm64,x64}.tar.xz(系统 tar 解压)
  │           Windows: node-v<LTS>-win-{x64,arm64}.zip(PowerShell Expand-Archive)
  │        c. 解压到 ~/.dsh/toolchain/node-<ver>/
  │        d. <私有node> <prefix>/node_modules/npm/bin/npm-cli.js
  │           install --prefix ~/.dsh/toolchain @deepseek-ai/dsh
  │           (用 node 直接跑 npm-cli.js,绕开 Windows .cmd shim 问题)
  │        e. 成功后关闭引导窗口
  │     失败 → 原生错误对话框(截断日志)+ 重试 / 退出
  ├─ 2. start_dsh()(新增 DshMode::Private)
  ├─ 3. 创建主窗口
  └─ 4. 菜单 / 更新检查(现有逻辑)
```

## 模式检测变更

- 新增 `DshMode::Private`:检测 `~/.dsh/toolchain/node-*/bin/node` 与 `~/.dsh/toolchain/node_modules/.bin/dsh` 同时存在。
- runner:`<node绝对路径> <prefix>/node_modules/.bin/dsh`(node 直接执行脚本,跨平台;node 忽略 shebang 行)。
- 新优先级:**Source > Bundled > Global > Private > Npx**。
- 引导触发条件:找不到 node >= 22(机器上有旧 node 也引导私有版,Private 优先级高于 Npx,避免旧 node 跑 npx 失败)。
- `toolchain_dirs()` 无需变更(Private 模式用绝对路径,不依赖 PATH)。

## 关键决策

- Node 版本来源:代码中写死一个 LTS 版本常量(实现时取当时最新 LTS,如 `v22.17.0`),不额外请求 `dist/index.json`(YAGNI;后续可随应用更新升级该常量)。
- 下载:新增 `reqwest` 依赖(流式 + content-length 进度);解压用系统自带工具(tar / PowerShell),不引入 zip/tar crate。
- 安装位置:`~/.dsh/toolchain/`(用户目录,无需提权);`~/.dsh` 已用于现有 `desktop-update-state.json`。
- 引导窗口:独立小窗口(约 520×240)加载内嵌 HTML,状态文本 + 进度条;通过 Tauri event 更新,文案走现有 I18n(中/英)。
- 错误处理:下载 / 解压 / npm 失败 → 原生对话框(截断日志)+「重试 / 退出」;重试先清理半成品目录;日志统一 `[bootstrap]` 前缀,npm 输出复用 `clean_output` 过滤风格。

## 升级流适配

- `run_upgrade()`:Private 模式下改为 `<私有node> npm-cli.js install --prefix ~/.dsh/toolchain @deepseek-ai/dsh@latest`(不再 `npm -g`)。
- `latest_version()`:npm view 命令同样改走私有 npm-cli.js(系统无 npm 时也能用)。
- `current_version()` 已走 `dsh_runner`,Private 模式自动生效,无需改。

## 测试

- 单元测试(纯函数):下载 URL / 归档名构造(平台 × arch 矩阵)、LTS 常量、prefix 路径拼接、`dsh_runner` Private 参数、detect 优先级顺序、npm-cli.js 路径。
- 手动验证:删除 `~/.dsh/toolchain` 后启动走完整引导;保留系统 node 时验证不触发引导。
- CI 不变。

## 边界情况

- 下载中断残留半成品 → bootstrap 开始前清理 `~/.dsh/toolchain` 旧目录。
- 引导窗口关闭按钮 → 拦截,提示改用「退出」按钮。
- 代理 / 无网:reqwest 走系统默认,不做自定义代理配置(YAGNI)。
- 磁盘空间不足 / 无权限:写入失败按通用错误路径弹窗展示。

## 非目标

- 不做 Linux 引导(webkit2gtk 依赖是另一问题)。
- 不做安装器内下载(NSIS/pack 脚本)与 CI 打包 Node 进安装包。
- 不提供系统级安装选项。
