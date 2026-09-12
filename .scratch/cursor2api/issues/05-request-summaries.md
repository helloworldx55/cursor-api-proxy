## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/1

## What to build

设置中展示最近 200 条 Request Summary（时间、方法、状态、远端地址、路径），不含正文与两种密钥。滚动日志约 2MB。可一键清空摘要与日志。

## Acceptance criteria

- [ ] 最多保留 200 条 Request Summary，字段符合词汇表
- [ ] 摘要与日志不含消息正文、Bridge Token、Cursor API Key
- [ ] 日志滚动上限约 2MB
- [ ] 一键清空后窗口与磁盘上的摘要/日志不再保留旧内容

## Blocked by

- #2 启停本机 Bridge（可与 #3、#4 并行）
