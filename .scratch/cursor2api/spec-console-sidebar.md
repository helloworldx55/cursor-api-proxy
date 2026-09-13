# Spec: Console Sidebar 与 Preferences

## Problem Statement

Operator 打开 cursor2api 设置窗时，最小集全挤在一屏里往下滚。找 Caller 配置、Request Summary 或 Autostart 要翻很久。Release 更新提示也夹在同一卷里。他们希望还是同一套控制，但向导结束后用左侧导航分页；Release 提示进单独一页，不要再当整窗横幅。主题和界面语言以后再说。

## Solution

首次向导未完成前，设置窗仍整块是向导，没有 Sidebar。完成后出现 Sidebar，自上而下：Bridge、凭证、Caller、记录、Autostart、偏好设置（词汇表 **Preferences**）。打开设置窗每次都进 Bridge。Preferences 的 v0 只显示 Release 检查结果、下载与忽略；不静默替换 exe。托盘不变。窗可以变宽以放下左导航。实现排在 Caller 聊天可用（#9）与完成向导可点（#10）之后。

## User Stories

1. As an Operator，I want 向导未完成时整窗只有首次向导、没有 Sidebar，so that 我必须先具备 Agent CLI 与 Bridge Token，而不会误点进日常页。
2. As an Operator，I want 「完成向导」成功后立刻看到 Sidebar 和五加一页，so that 我知道门闩已经过了。
3. As an Operator，I want 向导失败时仍停在向导（无 Sidebar），so that 我能看见错误而不是空壳。
4. As an Operator，I want Sidebar 自上而下是 Bridge、凭证、Caller、记录、Autostart、偏好设置，so that 日常 Bridge 在最上、Preferences 在最底。
5. As an Operator，I want 点 Sidebar 某一项时右边只显示那一页的最小集控制，so that 我不必滚动整份设置。
6. As an Operator，I want 打开设置窗（含从托盘「打开设置」）时右边总是 Bridge 页，so that 我先看到启停、健康、Preferred Port / Bound Port。
7. As an Operator，I want 本次会话里切到凭证或 Caller 再关窗重开后仍然落在 Bridge，so that 落地页可预期、也不往 App Data 写所选页。
8. As an Operator，I want Bridge 页上仍有健康、启停、Preferred Port、Bound Port、重新检测 Agent CLI，so that 最小集的 Bridge 控制没丢。
9. As an Operator，I want 凭证页上仍有 Cursor API Key 粘贴、保存到凭据库、以及 Agent CLI 登录状态，so that 我不必为换壳重学凭证。
10. As an Operator，I want Caller 页上仍可复制 Caller 配置、轮换 Bridge Token，且窗内不长期显示 Bridge Token，so that 我仍能交给 Cherry Studio 等 Caller。
11. As an Operator，I want 记录页上仍有最多 200 条 Request Summary、约 2MB 滚动日志、以及一键清空，so that 我能核对 Caller 请求。
12. As an Operator，I want Autostart 页仅在向导完成后出现，so that 向导结束前不能从侧栏改 Startup 快捷方式。
13. As an Operator，I want Autostart 页仍能打开或关闭指向本 exe 的快捷方式，并看到移动解压目录会弄坏 Autostart 的说明，so that 行为与现在的向导后设置一致。
14. As an Operator，I want Preferences 在界面上写「偏好设置」，so that 中文窗里不和「设置窗」整窗叠名，同时行话仍是 Preferences。
15. As an Operator，I want 启动 Console 后若 GitHub Releases 有更新，只在 Preferences 页看到版本与下载，so that Release 提示不再占据向导或其它页。
16. As an Operator，I want 在 Preferences 点下载时打开浏览器到 zip，而不改写正在运行的 exe 或解压目录，so that 更新仍是手动替换。
17. As an Operator，I want 在 Preferences 点忽略后，本次运行不再显示这条更新提示，so that 我可以继续用当前 Release。
18. As an Operator，I want 没有更新（或已忽略）时 Preferences 仍可打开，并看到并非主题/语言面板，so that 空页不会被理解成坏了。
19. As an Operator，I want Preferences v0 看不到主题或界面语言控件，so that 最小集没有这两项。
20. As an Operator，I want 关设置窗后托盘仍在，菜单仍是打开设置 / 启动 Bridge / 停止 Bridge / 退出，so that Sidebar 不取代托盘。
21. As an Operator，I want 设置窗可以比现在更宽以同时放下 Sidebar 和右页，so that 导航和 Bound Port、Caller 配置能一起看。
22. As an Operator，I want 右页切换时 Bridge 仍按原状态跑（或停），so that 换页不是 Restart sidecar。
23. As an Operator，I want 无 Agent CLI 时 Bridge 页的启动仍禁用并说明原因，so that 换壳不放宽门闩。
24. As an Operator，I want 向导上的 Autostart 勾选（默认是）完成后仍写入 Startup，so that Sidebar 出现之前这条询问还在向导里。
25. As an Operator，I want 移动 zip 目录后向导再次挡住整窗（无 Sidebar），so that 与现有 Autostart 失效规则一致。
26. As an Operator，I want 不会在 Sidebar 上看到工作区路径、agent 模式、完整 env、聊天或「以后所有功能」，so that Sidebar 只是导航壳。
27. As an Operator，I want 不会在 Sidebar 上看到名叫 Release 的单独一项，so that 更新入口只在 Preferences。
28. As an Agent implementing this spec，I want 实现明确 blocked by #9 与 #10，so that 先修 Caller 聊天与完成向导，再换壳。

## Implementation Decisions

- 遵守 ADR-0001（Tauri 2 Console 壳）、ADR-0002（npm Bridge）、ADR-0003（Sidebar + Preferences）。
- **测试与渲染共用一个缝：Console shell view。** 输入为已有 `SetupStatus`、当前选中的 Sidebar 项、`UpdatePrompt`（可空）、本进程是否已忽略该提示。输出为：整窗是向导还是 Sidebar、导航项列表（含 Autostart 是否出现、Preferences 永远在最后）、右页是哪一页、Preferences 是否展示下载提示。前端只渲染该 view，不另写一套显示规则。
- 该 view 放在现有 **console-setup** 模块（向导 / Autostart 已经在此），不新开 crate，也不把规则只写在 DOM 里。
- **release-check** 的契约不变：只产生是否提示下载，绝不改写 exe。Console 把提示从横幅改到 Preferences。
- Bridge Runtime、凭据库、Caller 配置、Request Summary、日志滚动、Autostart 快捷方式的行为不变；本 spec 只改设置窗信息架构。
- 选中页不写入 App Data；每次打开设置窗把选中项设为 Bridge。
- 忽略 Release 提示仍只在本进程内存，不持久化。
- 设置窗默认宽度加大到能放下 Sidebar + 右页；不改托盘菜单结构。
- 界面文案：导航 Bridge / 凭证 / Caller / 记录 / Autostart / 偏好设置。
- 实现顺序：#9、#10 关闭或已验证可完成向导且 Caller 聊天可用之后，再改壳。

## Testing Decisions

- 只测 Operator 可观察的 shell 行为，不测 CSS 类名、DOM id、像素。
- 主缝：console-setup 上的 shell view。仿照现有 `console_setup` 集成测试：构造 `SetupStatus`（或向导完成前后的 paths）+ 选中项 + 有无 `UpdatePrompt`，断言 mode、导航顺序、Autostart 项是否出现、默认项、Preferences 是否带下载提示、忽略后不再带提示。
- 不在本 spec 重复测 Bound Port、Bearer、Autostart `.lnk` 写入（已有测试）；若 shell 误藏 Autostart 项，由 shell view 覆盖。
- release-check 现有测试保持：更新提示与「不改写 exe」；不要求它知道 Preferences。
- 不强制新的浏览器/E2E 缝；Tauri 窗是渲染层。

## Out of Scope

- 主题、界面语言，以及 Preferences 上任何 v0 以外的控件（另开工单，仍放 Preferences 页）。
- 修复 #9（Cherry Studio 检测/聊天）与 #10（完成向导无反应）。
- 改 Bridge 协议、绑定地址、Agent CLI 分发、代码签名、Mac/Linux。
- 把 Sidebar 做成任意新功能抽屉；工作区、agent 模式、完整 env、聊天 UI。
- 自动下载并替换 exe；持久化「忽略更新」或「上次所在页」。
- 多窗口、去掉托盘、MSI/NSIS。

## Further Notes

词汇表与决策见根目录 `CONTEXT.md` 与 `docs/adr/0003-console-sidebar.md`。#11 是本规格的原始请求；施工工单由 `/to-tickets` 从本 spec 拆出，并声明 blocked by #9、#10。
