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
| ADR-0001 | Tauri 2 或 .NET/Avalonia | Proposed |
| ADR-0002 | 模块化单体和本地 SQLite 数据位置 | Proposed |
| ADR-0003 | 类型化业务事件、轻量分录与投影 | Proposed |
| ADR-0004 | Decimal、币种小数位与舍入规则 | Proposed |
| ADR-0005 | 证券成本基础与历史重放 | Proposed |
| ADR-0006 | 已过账事件修订/冲正语义 | Proposed |
| ADR-0007 | Excel 解析与导出库 | Proposed |
| ADR-0008 | 数据库与备份加密 | Proposed |
| ADR-0009 | WebView2 分发策略 | Proposed |
| ADR-0010 | 自动更新与签名 | Proposed |
| ADR-0011 | 完整历史与 cut-over 迁移策略 | Proposed |
| ADR-0012 | 汇率/价格修订、active 规则与估值 as-of | Proposed |
| ADR-0013 | 自动备份保留、设备丢失 RPO 与恢复密钥 | Proposed |
| ADR-0014 | 支出分析口径、系统桶、退款、缺 FX 与查询版本 | Proposed |

只有实际创建 ADR 文件后才把主题改为链接。文件名使用 `ADR-NNNN-short-title.md`。

## 使用方法

1. 从 [`ADR-TEMPLATE.md`](ADR-TEMPLATE.md) 复制结构。
2. 将状态设为 `Proposed`，列出替代方案、证据和影响。
3. 为会改变财务结果的方案同时提供黄金 fixture 或差异桥。
4. 用户确认后才设为 `Accepted`，并更新本索引、`docs/financial-rules.md` 和 `docs/agent-context.md`。
5. 不直接修改历史 ADR 的结论；以新 ADR 显式 supersede。
