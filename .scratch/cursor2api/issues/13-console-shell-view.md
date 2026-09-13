## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/12

## What to build

console-setup 提供可测的 Console shell view：向导未完成则整窗是向导、没有 Sidebar；完成后给出 Sidebar 项（Bridge、凭证、Caller、记录、Autostart、Preferences）、默认选中 Bridge、Autostart 仅向导完成后出现、Preferences 是否展示 Release 提示（含本进程忽略）。不改设置窗外观、不改 Bridge。

## Acceptance criteria

- [ ] 向导未完成：view 为整窗向导，无 Sidebar 项
- [ ] 向导完成后：Sidebar 自上而下为 Bridge、凭证、Caller、记录、Autostart、Preferences
- [ ] 默认选中项为 Bridge
- [ ] Autostart 项仅在向导完成后出现
- [ ] 有未忽略的 UpdatePrompt 时 Preferences 带下载提示；忽略后不再带
- [ ] 测试只断言上述 Operator 可观察规则，不绑定 DOM

## Blocked by

None (can start immediately)
