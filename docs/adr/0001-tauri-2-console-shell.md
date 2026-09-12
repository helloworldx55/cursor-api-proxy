# 用 Tauri 2 作为 Console 壳

cursor2api 必须先交付 Windows 托盘 + 设置窗口，并保留以后做 Mac/Linux 的路径，同时又要为 Bridge 内嵌一份 Node 运行时。我们选择 Tauri 2，而不是 Electron（再带一套 Chromium/Node，zip 过大）或 WinUI（等于放弃跨平台预留）。Tauri 负责托盘、窗口、拉起进程和更新提示；Node 只作为 Bridge sidecar 存在。
