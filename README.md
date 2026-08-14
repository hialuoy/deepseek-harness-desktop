# Desktop Shell (Route A · Tauri)

DeepSeek Harness 的轻量级桌面壳,基于 Tauri v2:spawn `dsh web --port 0`,解析端口,用系统原生 WebView 加载 Web UI。

比 Electron 壳省 ~100MB 内存(共享系统 WebView),体积更小(~5MB vs ~200MB)。

## 架构

```
Tauri Rust 主进程
  ├─ spawn: pnpm dsh web --port 0  (源码模式)
  │  或: dsh web --port 0          (npm 模式)
  ├─ 从 stdout 解析 URL → dsh web: http://127.0.0.1:<port>
  └─ 系统原生 WebView 加载该 URL
```

## 前置条件

- Rust >= 1.70 (`rustup` + stable 工具链)
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Linux: `libwebkit2gtk-4.1-dev` + `libgtk-3-dev`
- Node.js >= 22.19.0 (用于运行 `dsh`)
- 如果从源码仓库运行:先 `pnpm install && pnpm run build`

## 使用

```sh
cd desktop-tauri

# 安装 Tauri CLI
pnpm install

# 开发模式(带热重载)
pnpm dev

# 打包为 .app / .dmg / .deb / .AppImage
pnpm build
```

## 检查更新 / 升级 dsh

应用启动 5 秒后自动检查 npm 最新版本,菜单栏 **DeepSeek Harness → Check for Updates…** 可手动检查。

- **界面语言**:根据系统语言自动选择——中文系统(`zh*`)显示「检查更新… / 升级 dsh…」及中文对话框,其他语言显示英文(`sys-locale` 检测)
- **版本比较**:semver 对比 `dsh -V`(当前)与 `npm view @deepseek-ai/dsh version`(最新)
- **源码模式**升级:自动执行 `git pull --rebase --autostash && pnpm install && pnpm run build`
- **生产模式**升级:自动执行 `npm install -g @deepseek-ai/dsh@latest`
- **升级完成后**:询问是否重启应用(杀掉 dsh 子进程 → 重新拉起应用)

## 与 Electron 版的区别

| | Electron 版 | Tauri 版 |
|---|---|---|
| 壳体积 | ~200MB | ~5MB |
| 内存 | +~150MB | +~30MB(共享 WebView) |
| 依赖 | Node.js | Rust + 系统 WebView |
| 打包 | `electron-builder` | `cargo tauri build` |
| 自定义 | JS/TS | Rust |

## 路线图

- [ ] 系统托盘图标
- [ ] 开机自启
- [ ] 文件关联(`dsh://` 深链)
- [ ] 迁移到路线 B(内嵌 host + IPC bridge)