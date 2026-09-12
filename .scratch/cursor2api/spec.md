# cursor2api Windows Console（v0 portable Release）规格

## 问题

有 Cursor Account 的 Operator，若要把 Cursor 模型变成本机 OpenAI 形态的 HTTP 入口，现在必须自己跑 Node、环境变量和上游 CLI。给陌生人下载的助手不能是聊天应用，也不能冒充 Cursor IDE；他们需要一个 Windows Console：在本机拉起锁死的 Bridge，并把 Base URL 与 Bridge Token 交给 Caller。

## 方案

cursor2api 是 Windows Console（托盘 + 中文设置窗，行话英文），以未签名 portable zip 分发。内嵌 Node 与 npm 包 `cursor-api-proxy`，Bridge 只听 `127.0.0.1`，生成 Bridge Token，永不附带 Agent CLI 或 Cursor API Key。Operator 把 Caller 配置（Bound Port、Bridge Token）贴进 Cherry Studio 等。Mac/Linux、签名、局域网绑定、agent 模式/工作区开关不在本规格内。

完整用户故事与实现/测试决策见对话中已发布的英文稿结构；执行时以 `CONTEXT.md`、ADR-0001、ADR-0002 与本仓库后续工单为准。测试主缝为 **Bridge Runtime**（检测 Agent CLI、启停 sidecar、Bound Port、无 Token 拒绝）。
