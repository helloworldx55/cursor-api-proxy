# cursor2api

Windows 桌面 **Console**，产品名为 **cursor2api**。安装包内嵌 Node 运行时和 npm 包 `cursor-api-proxy`，在本机运行 **Bridge**，以未签名 portable zip 公开发布。它不是 Cursor IDE，不是聊天应用，也不分发 Agent CLI。Mac / Linux 延后。代码签名是 v1.0 门槛。

## Language

**cursor2api**：
对外产品名（窗口标题、托盘、zip、GitHub Releases）。
_Avoid_: Cursor Proxy, 官方 Cursor, Cursor IDE, Cursor App, cursor-api-proxy（仅指上游 npm 包）

**Console**：
Operator 安装的桌面程序：托盘图标 + 中文设置窗口（启停、健康、可复制的 Caller 配置、最近请求、日志、首次向导）。行话保持英文：Bridge、Caller、Agent CLI、Base URL、Bridge Token、Cursor API Key。
_Avoid_: 客户端, 电脑客户端, IDE, 聊天应用, widget

**Bridge**：
Console 拉起的本机 HTTP 进程，来自 npm `cursor-api-proxy`（本仓库不分叉）。只绑定 `127.0.0.1`。缺少 Agent CLI 时不得启动。
_Avoid_: proxy, 服务器, API

**Preferred Port**：
Operator 希望使用的端口（默认 8765）。若被占用，Console 尝试 8766、8767…，必须展示并复制 Bound Port，不得把过期的 Preferred Port 交给 Caller。
_Avoid_: 端口（未加限定）

**Bound Port**：
处理冲突后 Bridge 实际监听的端口。
_Avoid_: 真实端口, 实际端口（请用本词）

**Bridge Token**：
Console 生成、Caller 请求必须携带的密钥（`Authorization: Bearer`）。存在操作系统凭据库。与 Cursor API Key 不是同一物。
_Avoid_: API key（未加限定）, unused, 密码

**Cursor API Key**：
Operator 从 Cursor Dashboard 取得的 `CURSOR_API_KEY`。存在操作系统凭据库。也可以改用 Agent CLI 登录。永远不要打进 Release。
_Avoid_: token（未加限定）, Bridge Token

**Operator**：
在自己控制的机器上解压并运行 cursor2api 的人；Bridge 消耗的是此人的 Cursor Account。
_Avoid_: 用户, 客户

**Caller**：
向 Bridge 发 HTTP 请求的另一个程序。
_Avoid_: 客户端, 用户

**Cursor Account**：
被计费的 Cursor 身份，Bridge 使用其额度。cursor2api 不出售模型访问。
_Avoid_: API 用户, Cursor 用户（与 Operator 混淆）

**Agent CLI**：
Operator 机器上的 Cursor `cursor-agent` / `agent` 可执行文件。运行时必需；从不打进安装包。
_Avoid_: agent, Cursor

**App Data**：
Windows 上非秘密的 Console 状态：`%APPDATA%\cursor2api\`（窗口状态、Preferred Port、请求摘要、滚动日志）。密钥只放操作系统凭据库。zip 可解压到任意目录。
_Avoid_: 安装目录, portable 文件夹（当作设置存放处）

**Request Summary**：
一条已结束的 Caller 请求的本地记录：时间、方法、状态、远端地址、路径。最多保留 200 条。不含消息正文、不含 Cursor API Key、不含 Bridge Token。
_Avoid_: 请求日志（未加限定）, 全文记录

**Release**：
可供下载的 Windows portable zip（v0.x 未签名）。启动时检查 GitHub Releases 并提示下载，不静默替换文件。
_Avoid_: 安装器, setup, MSI

**Autostart**：
当前用户 Startup 快捷方式，在首次向导成功结束时询问，默认选是，指向刚运行的那份 exe。关闭 Autostart 即删除该快捷方式。移动 zip 目录后快捷方式失效，需重新走完向导。
_Avoid_: 服务, 安装勾选框

**最小集**：
v1 设置里仅有的 Bridge 控制：启停、健康、Preferred Port / Bound Port、Bridge Token、Cursor API Key / 登录状态、复制 Caller 配置、Request Summary、滚动日志（2MB）、Autostart。没有工作区路径、没有 agent 模式开关、没有完整 env 表单。
