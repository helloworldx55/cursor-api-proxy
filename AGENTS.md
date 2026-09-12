# cursor2api

## Agent skills

### Issue tracker

工单在 helloworldx55/cursor-api-proxy 的 GitHub Issues，用 `gh` 操作。见 `docs/agents/issue-tracker.md`。

### Triage labels

默认角色：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。见 `docs/agents/triage-labels.md`。

### Domain docs

单上下文：根目录 `CONTEXT.md` 与 `docs/adr/`。见 `docs/agents/domain.md`。

### 文档语言

本仓库面向人与 agent 的文档一律用**中文**撰写，包括 `CONTEXT.md`、ADR、`AGENTS.md`、`docs/agents/`、spec、GitHub Issue / 工单正文与评论。领域词必须使用 `CONTEXT.md` 里的英文词（如 Bridge、Console、Operator、Caller、Agent CLI、Bridge Token、Cursor API Key、Preferred Port、Bound Port、App Data、Request Summary、Release、Autostart），不要译成「桥梁」「客户端」「用户」等。代码标识符、命令、URL、环境变量保持英文。
