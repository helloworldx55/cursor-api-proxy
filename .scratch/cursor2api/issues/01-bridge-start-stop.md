## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/1

## What to build

Operator 打开 cursor2api：托盘 + 中文设置窗。无 Agent CLI 时不能 Start。有 Agent CLI 时拉起内嵌 Bridge，只绑定 `127.0.0.1`；Preferred Port 默认 8765，占用则试 8766…；界面显示 Bound Port。Stop 或退出 Console 必须停掉该 sidecar。Bridge Runtime 用测试锁住上述行为。

## Acceptance criteria

- [ ] 设置窗可 Start / Stop，托盘能看出 Bridge 是否在跑
- [ ] PATH 上没有 Agent CLI 时 Start 被拒绝，并说明原因
- [ ] 有 Agent CLI 时 Bridge 听在 `127.0.0.1`，不是 `0.0.0.0`
- [ ] Preferred Port 被占时改用更高端口，UI 展示 Bound Port 而非过期 Preferred Port
- [ ] Stop 与退出 Console 后该端口不再被本 sidecar 占用
- [ ] Bridge Runtime 有针对「无 CLI / 绑定 / 端口回退 / 停止」的外部行为测试

## Blocked by

- 无（可立刻开工）
