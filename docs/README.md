# LedgerKit 文档导航

本目录采用渐进式读取：先判断任务类型，再加载必要文档。不要让 agent 每次任务都通读全部计划与操作材料。

## 权威性

发生冲突时遵循：当前用户指令 → 适用 `AGENTS.md` → Accepted ADR → 财务规则与黄金样例 → 开发计划书 → agent context / runbook / README。

如果 `docs/financial-rules.md` 与黄金样例不一致，视为阻断问题；不得自行选择对实现更方便的一方。

## 文档地图

| 文档 | 用途 | 何时阅读 |
|---|---|---|
| [`../AGENTS.md`](../AGENTS.md) | 仓库级强制工作约束 | 每个任务开始时 |
| [`多币种个人账本-开发计划书.md`](多币种个人账本-开发计划书.md) | 产品、架构、迁移、测试与交付的完整基线 | 产品/架构/里程碑任务 |
| [`agent-context.md`](agent-context.md) | 当前状态、暂定决策、blocker 和下一步 | 每个非平凡任务 |
| [`financial-rules.md`](financial-rules.md) | 可测试的财务规则摘要 | 任何财务或报表任务 |
| [`adr/README.md`](adr/README.md) | ADR 状态索引和使用方法 | 高影响决策或相关实现 |
| [`implementation-prompts/README.md`](implementation-prompts/README.md) | 从 M0 到 1.0 Beta 的 14 阶段串行执行提示词 | 准备或执行应用实施任务 |
| [`operations/migration.md`](operations/migration.md) | Excel staging、cut-over、对账和切换 | 导入与迁移任务 |
| [`operations/backup-restore.md`](operations/backup-restore.md) | 一致性备份、恢复和升级安全顺序 | 数据安全与发布任务 |
| [`local/README.md`](local/README.md) | 本机私有源文件定位规则；实际路径保存在 Git 忽略文件中 | 真实源调查、微调、最终迁移与 cut-over |
| [`production-dependencies.md`](production-dependencies.md) | 已锁定生产依赖、预算与延期适配器 | 新增/升级依赖或发布审计 |
| [`persistence-schema-v1.md`](persistence-schema-v1.md) | Schema v1 基线、v2/v3 前向迁移、事务、staging、路径和重建边界 | Core/SQLite、迁移与后续 ledger 阶段 |
| [`catalog-and-market-data-v1.md`](catalog-and-market-data-v1.md) | 首次设置、主数据、市场修订、as-of 与质量修复契约 | Catalog、市场数据、估值与设置 UI |
| [`../fixtures/sanitized/README.md`](../fixtures/sanitized/README.md) | 脱敏黄金样例格式和变更规则 | 测试与财务实现任务 |

## 更新原则

- `AGENTS.md`：只有工作约束变化时更新。
- `agent-context.md`：只有持久项目状态变化时更新，不记录逐日过程。
- `financial-rules.md`：只随 Accepted ADR 或已确认规则变化更新。
- ADR：记录重要选择、替代方案、后果和反转条件。
- runbook：记录安全、可重复执行的操作步骤。
- 计划书：保存全局范围与里程碑，不承担每次实施进度日志。
