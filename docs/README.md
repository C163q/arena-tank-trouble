# 项目文档入口

本入口只链接持续维护的文档。查找某次调研、设计或实施计划时，按主题和日期到对应目录查阅。

## 常用文档

- [本地试玩与项目定位](../README.md)面向玩家，说明启动方式和基本操作。
- [静止靶试玩版开发与验证](development/playtest.md)说明代码入口和验证流程。
- [项目协作约定](../AGENTS.md)说明项目目标，以及开发代理开始工作时应阅读的材料。
- [文档编写与维护规范](documentation.md)规定文档的读者、命名、阅读范围和维护责任。
- [Codex 环境约定](ai/codex.md)说明沙箱内的命令执行与审批要求。选择子代理模型时，参阅 [Codex 模型选择](ai/codex-subagent-model-selection.md)或 [Cursor 模型选择](ai/cursor-model-selection.md)。

## 文档与本地文件放在哪里

以下路径均相对于仓库根目录，按实际内容创建。

| 路径 | 用途 |
| --- | --- |
| `docs/development/` | 面向开发者的持续维护文档，说明当前实现的结构、接口和验证方式。 |
| `docs/research/` | 只存放调研资料，如原作机制的观察记录、来源和待核实问题。 |
| `docs/superpowers/specs/` | Superpowers 设计规格，文件名为 `YYYY-MM-DD-<topic>-design.md`。 |
| `docs/superpowers/plans/` | Superpowers 实施计划，文件名为 `YYYY-MM-DD-<feature-name>.md`。 |
| `docs/local.md` | 本机环境记录，包括工具版本、工作区路径和本地参考项目。由各开发者在本机维护，Git 忽略此文件。 |
| `.tmp/` | 需要跨重启保留的本地临时文件，Git 忽略此目录。 |
| `.worktrees/` | 本地 Git 工作树的存放位置，Git 忽略此目录。 |
