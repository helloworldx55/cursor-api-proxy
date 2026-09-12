## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/1

## What to build

首次向导：检测到 Agent CLI 且已有 Bridge Token 才算成功结束。结束时询问 Autostart，默认「是」，为当前用户写入指向本次 exe 的 Startup 快捷方式。设置中关闭 Autostart 即删除该快捷方式。需提示：移动解压目录会弄坏 Autostart。

## Acceptance criteria

- [ ] 缺少 Agent CLI 或尚无 Bridge Token 时不能走完向导
- [ ] 向导成功结束时询问 Autostart，默认选是
- [ ] 选是则当前用户 Startup 指向刚运行的 exe
- [ ] 设置中关闭 Autostart 后快捷方式消失
- [ ] 向导或设置说明移动文件夹后需重新完成向导

## Blocked by

- #3 Bridge Token
- #4 Cursor API Key
