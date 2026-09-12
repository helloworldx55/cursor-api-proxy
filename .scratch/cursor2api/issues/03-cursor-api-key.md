## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/1

## What to build

设置中可粘贴 Cursor API Key（进凭据库），或识别 Operator 已 `agent login`。启动 Bridge 时带上可用凭证。Release 内不得预置任何人的 Key。日志看不到 Key。

## Acceptance criteria

- [ ] Cursor API Key 可保存到操作系统凭据库并在启动 Bridge 时注入
- [ ] 已 Agent CLI 登录时设置页能显示可用状态，不强制再贴 Key
- [ ] 安装包 / 默认配置不含 Cursor API Key
- [ ] 日志中不出现 Cursor API Key

## Blocked by

- #2 启停本机 Bridge（可与 #3 并行）
