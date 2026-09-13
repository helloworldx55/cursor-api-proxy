## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/12

## What to build

设置窗在向导完成后用 Sidebar 分页展示最小集；打开窗总是 Bridge 页；Preferences（界面「偏好设置」）v0 只放 Release 检查结果、下载与忽略，去掉整窗横幅；窗可变宽；托盘菜单不变。前端只渲染 console-setup 的 shell view。

## Acceptance criteria

- [ ] 向导未完成：整窗向导，无 Sidebar
- [ ] 向导完成后：可见 Sidebar，可在 Bridge / 凭证 / Caller / 记录 / Autostart / 偏好设置之间换页，右页只显示该页控制
- [ ] 打开设置窗（含托盘「打开设置」）总是进 Bridge
- [ ] Autostart 页仅向导完成后出现
- [ ] Preferences 展示 Release 提示或「无更新/已忽略」；下载不改写 exe；忽略仅本进程
- [ ] 不再出现整窗 Release 横幅
- [ ] 无主题、无界面语言控件
- [ ] 托盘仍为打开设置 / 启停 Bridge / 退出

## Blocked by

- #13
- #9
- #10
