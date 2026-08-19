# 应用自更新(检查并自动下载安装桌面程序本身)

日期:2026-08-20

## 背景

当前「检查更新…」只检查 dsh 命令行工具(`check_update()` → `dsh -V` 对比 `npm view`),发现新版时 `run_upgrade()` 升级 dsh。需求:同样检查 DeepSeek Harness 桌面程序本身,发现新版后**自动下载安装**(方案 B,采用 `tauri-plugin-updater` + 签名)。

本应用无前端(WebView 加载外部 dsh URL),因此自更新走 updater 插件的 **Rust API**,不引入 JS guest bindings,也不需要 capability 权限。

## 更新源

GitHub Releases 静态清单文件 `latest.json`,endpoint 固定为:

```
https://github.com/hialuoy/deepseek-harness-desktop/releases/latest/download/latest.json
```

静态清单格式(以 windows 为例):

```json
{
  "version": "1.0.2",
  "notes": "…",
  "pub_date": "2026-08-20T00:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "url": "https://github.com/hialuoy/deepseek-harness-desktop/releases/download/v1.0.2/DeepSeek Harness_1.0.2_x64-setup.exe",
      "signature": "<base64 签名>"
    }
  }
}
```

平台 key 为 `{{target}}-{{arch}}`(`windows-x86_64` / `darwin-aarch64` / `linux-x86_64` 等)。

## 签名与密钥

Tauri updater 强制签名校验,不可关闭。

- 生成:`npx tauri signer generate -w ~/.tauri/dsh-desktop.key`,产出私钥与公钥(`.pub`)。
- **公钥**内容写入 `tauri.conf.json` 的 `plugins.updater.pubkey`(可公开)。
- **私钥**只放进 CI secret `TAURI_SIGNING_PRIVATE_KEY`(路径或内容),绝不入库;丢失私钥将无法再对老用户发布更新。
- `bundle.createUpdaterArtifacts: true` 让 `tauri build` 用私钥自动签名产物并生成本地 `latest.json`,由 release workflow 上传到 Release。

## 配置(tauri.conf.json)

```jsonc
{
  "bundle": {
    "createUpdaterArtifacts": true,
    // …现有配置不动
  },
  "plugins": {
    "updater": {
      "pubkey": "<公钥内容>",
      "endpoints": [
        "https://github.com/hialuoy/deepseek-harness-desktop/releases/latest/download/latest.json"
      ],
      "windows": { "installMode": "passive" }
    }
  }
}
```

`installMode: passive`(Windows):NSIS 安装器弹小进度窗、无需用户交互,安装后自动重启应用。

## 代码改动(全部在 `src-tauri/`)

1. `Cargo.toml` 增加 `tauri-plugin-updater = "2"`。
2. `tauri.conf.json` 按上文增加 `bundle.createUpdaterArtifacts` 与 `plugins.updater`(pubkey 先用占位,待用户生成后替换)。
3. `main.rs` 的 `tauri::Builder` 注册插件 `.plugin(tauri_plugin_updater::Builder::new().build())`。
4. 新增 `check_app_update(&AppHandle) -> Result<Option<String>, String>`:`handle.updater()?.check().await`,有新版返回最新版本号,无新版返回 `None`。
5. 新增 `install_app_update(&AppHandle, update)`:`update.download_and_install(noop, noop).await`;进度交给安装器 passive 窗口,失败走 i18n 错误弹窗。
6. 菜单「检查更新…」改为:先查 dsh(原有逻辑),再查应用;应用有新版时弹窗询问 → 确认后下载安装。
7. 启动 5 秒后自动检查:同样加应用检查,按「同一版本每天最多弹一次」去重。
8. `PromptState` 增加应用字段;`should_auto_prompt` 泛化为分别记录 dsh 与应用。
9. `I18n` 新增应用更新相关双语文案(标题/提示/失败/已最新)。

## 平台行为

- Windows(`installMode: passive`):下载并验签后运行 NSIS 安装器,小进度窗自动安装,完成后自动重启。
- macOS / Linux:依赖各自安装器语义(本期实现聚焦 Windows,其余平台保留默认行为)。

## 发布流程改动(release workflow)

- build 步骤注入 `TAURI_SIGNING_PRIVATE_KEY`(来自 secret)。
- 上传产物时,额外上传签名的 `latest.json` 到 Release(否则 endpoint 404/无新版)。

## 用户手工步骤(实现完成后一次性)

1. `npx tauri signer generate -w ~/.tauri/dsh-desktop.key` 生成密钥对。
2. 把 `.pub` 内容填进 `tauri.conf.json` 的 `pubkey`。
3. 在 GitHub 仓库 Secrets 增加 `TAURI_SIGNING_PRIVATE_KEY`(私钥内容或路径)。
4. 更新 `.github/workflows/release.yml` 注入私钥并上传 `latest.json`。
5. 发布新版时,把 `latest.json` 与安装包一起作为 Release 资产上传。

## 测试

- 新增 I18n 文案单测、`PromptState`/`should_auto_prompt` 扩展单测。
- `cargo test` / `cargo clippy` / `cargo fmt` 全绿。
- 实机验证:真实签名的 `latest.json` 上线后,菜单/启动检查能下载并安装(留待用户侧在发版时验证)。
