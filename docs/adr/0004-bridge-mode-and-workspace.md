# 最小集纳入 Bridge Mode 与 Bridge Workspace

Cherry Studio 这类 Caller 不会自动带 `X-Cursor-Mode` / `X-Cursor-Workspace`，所以 Console 必须给出整台 Bridge 的默认 `--mode` 和工作目录，而不能继续把这两项排除在最小集之外。我们把 **Bridge Mode**（出厂 `agent`，界面问答 / 智能体 / 计划）和 **Bridge Workspace**（未选时用 App Data 默认目录）放在已有 Bridge 页，不新增 Sidebar 项，以免和 ADR-0003 打架；曾考虑只改上游默认、或把工作区交给 Caller，都会让 Operator 在 exe 目录里误写文件，或根本切不到 `agent`。
