# DeepSeek Harness Desktop

**DeepSeek Harness** 的官方桌面应用——以原生 WebView 加载 DeepSeek Harness 工作台,轻量、低内存、无需技术背景即可安装使用。

The official desktop app for **DeepSeek Harness** — a lightweight native shell that runs the DeepSeek Harness workspace in your system's built-in WebView. Small, fast, and installs without any technical setup.

---

## ✨ 功能亮点 / Highlights

- **零依赖安装**:新电脑双击安装包即用,首次启动自动下载安装运行所需组件(私有 Node.js + dsh),全程无需管理员权限。 / **Zero-setup install**: the first launch downloads everything it needs (private Node.js + dsh) — no admin rights required.
- **轻量省内存**:仅 ~5MB 安装包,共享系统 WebView,比 Electron 版省约 100MB 内存。 / **Lightweight**: ~5MB installer, shares the system WebView (~100MB less memory than the Electron version).
- **自动更新**:启动后自动检查 dsh 新版本,一键升级。 / **Auto-updates**: checks for new dsh versions on startup, one-click upgrade.
- **界面语言自适应**:中文系统显示中文,其他语言显示英文。 / **Language-aware**: Chinese UI on Chinese systems, English elsewhere.

---

## 📦 安装 / Installation

### macOS

1. 下载 `DeepSeek Harness_<version>_<arch>.dmg`(Apple Silicon 选 `aarch64`,Intel 选 `x64`)
2. 打开 DMG,把 **DeepSeek Harness.app** 拖入 Applications 文件夹
3. 首次打开时,若出现「无法验证开发者」提示,右键应用 → **打开**(未签名/未公证的测试版本;正式发布版会消除此提示)

### Windows

1. 下载 `DeepSeek Harness_<version>_x64-setup.exe`(或 `.msi`)
2. 双击运行,按提示完成安装
3. 从开始菜单启动 DeepSeek Harness

### Linux(实验性)

1. 下载 `.deb` / `.rpm` / `.AppImage` 对应包
2. 安装后从应用菜单启动(需系统已安装 WebKitGTK)

---

## 🚀 首次启动 / First Launch

首次启动会显示一个引导窗口,自动完成以下步骤(需联网,通常 1-2 分钟):

1. 下载并安装私有 Node.js 到用户目录(`~/.dsh/toolchain/`)
2. 安装 dsh 命令行工具
3. 启动 DeepSeek Harness 工作台

> 安装过程中请耐心等待;如果超过 30 秒,窗口会显示「安装仍在进行,请耐心等待…」,这是正常现象。

> The first launch shows a setup window and installs a private Node.js and dsh into your user directory (`~/.dsh/toolchain/`). It needs internet and usually takes 1-2 minutes. If the install runs long, the window will show a "please wait" notice — that's normal.

安装完成后,后续启动无需联网,秒开。

After the first install, subsequent launches are instant and work offline.

---

## 🔄 更新 / Updates

- **自动检查**:应用启动 5 秒后自动检查 dsh 是否有新版本;有新版本时弹窗询问是否升级。
- **手动检查**:菜单栏 **DeepSeek Harness → Check for Updates…**(中文系统显示「检查更新…」)
- **升级**:确认后自动升级 dsh,完成后询问是否重启应用

> The app checks for updates 5 seconds after launch. You can also check manually via the app menu (**DeepSeek Harness → Check for Updates…** on macOS). Upgrading dsh is one click.

---

## ❓ 常见问题 / FAQ

### 安装或启动失败?

- **安装卡在「正在安装 dsh…」**:首次安装需要下载数百个依赖包,1-2 分钟是正常的。窗口会在 30 秒后提示"请耐心等待"。
- **提示无法启动 dsh**:请确认网络正常,然后关闭应用重新打开。若仍失败,可在终端执行 `npm install -g @deepseek-ai/dsh` 后重试。

### 应用打不开(macOS)?

- 未签名版本会被 macOS 拦截:右键应用 → **打开**。或到 **系统设置 → 隐私与安全性** 中允许。

### 数据存在哪里?

- 应用数据与配置存放在 `~/.dsh/` 目录;日志在 `~/.dsh/desktop.log`。

### 如何卸载?

- **macOS**:把 DeepSeek Harness.app 拖入废纸篓;如需清理数据,删除 `~/.dsh/` 目录。
- **Windows**:控制面板 → 卸载程序 → DeepSeek Harness;如需清理数据,删除 `%USERPROFILE%\.dsh` 目录。

---

## 🛠 开发者 / For Developers

### 前置条件 / Prerequisites

- Rust >= 1.70(`rustup` + stable)
- macOS:Xcode Command Line Tools(`xcode-select --install`)
- Linux:`libwebkit2gtk-4.1-dev` + `libgtk-3-dev`
- Node.js >= 22.19.0(运行 dsh)

### 开发 / Development

```sh
cd desktop-tauri
npm install          # 安装 Tauri CLI
npm run dev          # 开发模式(等价 cargo tauri dev)
npm run build        # 打包 .app/.dmg/.deb/.AppImage
```

> 注意:若在 monorepo 仓库内运行 `npm run dev`,会命中 Source 模式,要求父仓库已 `pnpm install && pnpm run build`。

### 测试与验证 / Test & Lint

```sh
cd src-tauri
cargo test       # 单元测试
cargo clippy     # 零告警
cargo fmt -- --check
```

### 发布新版本 / Release

1. 更新版本号(三处必须一致):`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`
2. 提交并推送代码
3. 打标签推送,触发 CI 三平台构建:

```sh
git tag v1.0.0
git push origin v1.0.0
```

4. 到 GitHub **Actions** 页观察构建;完成后在 **Releases** 页把草稿发布,即可获得各平台安装包。

---

## 📄 License

MIT
