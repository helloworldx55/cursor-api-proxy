# cursor2api

Windows **Console**：托盘 + 中文设置窗。在本机拉起 **Bridge**（npm `cursor-api-proxy`），只绑定 `127.0.0.1`。不是官方 Cursor，也不附带 Agent CLI；Bridge 消耗的是 Operator 自己的 Cursor Account。

## 开发

需要：Rust、Node 18+、本机已安装 Agent CLI（`cursor-agent` 或 `agent`）。

```bash
npm install
npm test
npm run tauri dev
```

设置窗 **启动** / **停止** 控制 Bridge。无 Agent CLI 时启动会被拒绝。Preferred Port 默认 8765，占用则改用更高端口，界面展示 Bound Port。退出 Console 会停掉 sidecar。
