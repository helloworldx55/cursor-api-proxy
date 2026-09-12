# cursor2api

Windows **Console**：托盘 + 中文设置窗。在本机拉起 **Bridge**（npm `cursor-api-proxy`），只绑定 `127.0.0.1`。不是官方 Cursor，也不附带 Agent CLI；Bridge 消耗的是 Operator 自己的 Cursor Account。

## Release

从 GitHub Releases 下载 `cursor2api-windows.zip`，解压到任意目录后运行 `cursor2api.exe`。没有 MSI/NSIS 安装器。v0.x 未签名，Windows SmartScreen 可能发出警告，这是预期现象，不要把它当成官方安装包。

zip 内嵌 Node 与 npm `cursor-api-proxy`（Bridge sidecar），不含 Agent CLI，也不预置 Cursor API Key。本机仍需 WebView2、自行安装的 Agent CLI，以及自己的 Cursor API Key 或先完成 `agent login`。

设置与 Request Summary 写在 `%APPDATA%\cursor2api\`，与解压目录无关。

本产品使用上游 npm 包 `cursor-api-proxy`（MIT），不代表 Cursor 官方认可。

## 开发

需要：Rust、Node 18+、本机已安装 Agent CLI（`cursor-agent` 或 `agent`）。

```bash
npm install
npm test
npm run tauri dev
```

设置窗 **启动** / **停止** 控制 Bridge。无 Agent CLI 时启动会被拒绝。Preferred Port 默认 8765，占用则改用更高端口，界面展示 Bound Port。退出 Console 会停掉 sidecar。
