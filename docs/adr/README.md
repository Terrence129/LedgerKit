# Architecture Decision Records

ADR 记录会改变财务结果、持久化格式、安全模型、技术栈或跨模块边界的选择。Agent 可以创建 `Proposed` ADR，但只有用户明确确认后才能改为 `Accepted`。

## 状态

- `Proposed`：正在讨论，不得作为最终实现依据。
- `Accepted`：当前权威决策。
- `Superseded`：被新的 Accepted ADR 明确替代。
- `Rejected`：已评估但未采用。

## 计划中的 ADR

| 编号 | 主题 | 状态 |
|---|---|---|
| [ADR-0001](ADR-0001-tauri-react-rust.md) | Tauri 2、React/TypeScript 与 Rust Core | Accepted |
| [ADR-0002](ADR-0002-modular-monolith-local-sqlite.md) | 模块化单体和本地 SQLite 数据位置 | Accepted |
| [ADR-0003](ADR-0003-typed-events-postings-projections.md) | 类型化业务事件、轻量分录与投影 | Accepted |
| [ADR-0004](ADR-0004-decimal-rounding-contract.md) | Decimal、币种小数位与舍入规则 | Accepted |
| [ADR-0005](ADR-0005-moving-weighted-average.md) | 证券成本基础与历史重放 | Accepted |
| [ADR-0006](ADR-0006-revision-reversal-semantics.md) | 已过账事件修订/冲正语义 | Accepted |
| [ADR-0007](ADR-0007-rust-xlsx-adapter.md) | Rust XLSX 读写适配器 | Accepted |
| [ADR-0008](ADR-0008-live-database-and-portable-backup-encryption.md) | P0 活库与密码加密便携备份 | Accepted |
| [ADR-0009](ADR-0009-system-webview2-thin-package.md) | 系统 Evergreen WebView2 与薄包分发 | Accepted |
| [ADR-0010](ADR-0010-p0-manual-update-and-unsigned-beta.md) | P0 手动更新、P1 签名与自动更新 | Accepted |
| [ADR-0011](ADR-0011-history-cutover-migration.md) | 完整历史与 cut-over 迁移策略 | Accepted |
| [ADR-0012](ADR-0012-market-data-revisions-as-of.md) | 汇率/价格修订、active 规则与估值 as-of | Accepted |
| [ADR-0013](ADR-0013-automatic-backup-retention-rpo-and-recovery-secret.md) | 自动备份保留、设备丢失 RPO 与恢复密钥 | Accepted |
| [ADR-0014](ADR-0014-expense-analysis-contract.md) | 支出分析口径、系统桶、退款、缺 FX 与查询版本 | Accepted |
| [ADR-0015](ADR-0015-rebuildable-expense-daily-projection.md) | 可重建支出日聚合投影 | Accepted |

只有实际创建 ADR 文件后才把主题改为链接。文件名使用 `ADR-NNNN-short-title.md`。

## 使用方法

1. 从 [`ADR-TEMPLATE.md`](ADR-TEMPLATE.md) 复制结构。
2. 将状态设为 `Proposed`，列出替代方案、证据和影响。
3. 为会改变财务结果的方案同时提供黄金 fixture 或差异桥。
4. 用户确认后才设为 `Accepted`，并更新本索引、`docs/financial-rules.md` 和 `docs/agent-context.md`。
5. 不直接修改历史 ADR 的结论；以新 ADR 显式 supersede。
