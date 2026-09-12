# Issue tracker：GitHub

本仓库的工单与 spec 记在 GitHub Issues，一律用 `gh` CLI 操作。从 `git remote -v` 推断仓库（https://github.com/helloworldx55/cursor-api-proxy）。

## 约定

- **创建工单**：`gh issue create --title "..." --body "..."`。多行正文用文件或 heredoc。
- **读取工单**：`gh issue view <number> --comments`，并用 `jq` 过滤评论、拉取标签。
- **列出工单**：`gh issue list --state open --json number,title,body,labels,comments`，按需加 `--label`、`--state`。
- **评论**：`gh issue comment <number> --body "..."`
- **加/摘标签**：`gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **关闭**：`gh issue close <number> --comment "..."`

正文与标题使用中文；领域词保持 `CONTEXT.md` 中的英文。在仓库目录内运行时 `gh` 会自动选中该 remote。

## Pull requests 是否作为请求入口

**PRs as a request surface: no.**（若本仓库把外部 PR 当功能请求，把此项改为 `yes`；`/triage` 会读这个开关。）

为 `yes` 时，PR 与 Issue 走同一套标签，使用对应的 `gh pr` 命令。

GitHub 的 Issue 与 PR 共用编号：裸 `#42` 可能是任一者，先 `gh pr view 42`，失败再 `gh issue view 42`。

## 当技能说「发布到 issue tracker」

创建一张 GitHub Issue。

## 当技能说「读取相关工单」

执行 `gh issue view <number> --comments`。

## Wayfinding 操作

供 `/wayfinder` 使用。**地图**是一张 Issue，**子工单**是其子 Issue。

- **地图**：带 `wayfinder:map` 标签的一张 Issue。`gh issue create --label wayfinder:map`。
- **子工单**：用 GitHub sub-issue 挂到地图；若未启用，则在地图正文用任务列表，并在子工单顶部写 `Part of #<map>`。标签：`wayfinder:<type>`（`research` / `prototype` / `grilling` / `task`）。认领后指派给正在做的人。
- **阻塞**：优先用 GitHub 原生 issue dependencies。`gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`，其中 `<blocker-db-id>` 是 blocker 的 **database id**（`gh api repos/<owner>/<repo>/issues/<n> --jq .id`），不是 `#number`。GitHub 用 `issue_dependencies_summary.blocked_by` 表示仍打开的阻塞。不可用时退回子工单顶部的 `Blocked by: #<n>`。每个 blocker 都关闭后即解阻。
- **认领**：`gh issue edit <n> --add-assignee @me`。
- **办结**：`gh issue comment` 写答案，然后 `gh issue close`，并在地图的「已做决定」里追加指针。
