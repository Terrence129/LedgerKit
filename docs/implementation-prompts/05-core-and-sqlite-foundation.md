# 任务 05/14：权威 Core 与 SQLite 基础

你正在执行 LedgerKit 的 M2 Core、Schema v1 和事务基础。遵循共同执行协议，确认 `main` 包含 `m1-stack-selected`，创建分支 `phase/m2-core-persistence`。

## 必读材料

- M0/M1 全部 Accepted ADR
- `docs/financial-rules.md`
- `docs/operations/backup-restore.md`
- 开发计划书的领域模型、架构、数据位置、可靠性和测试章节

## 实现

1. Core 值类型：Decimal、Money、Currency、LocalDate、UUIDv7、Sequence、CalculationVersion、ProjectionWatermark、DomainError。
2. Decimal 完全符合 ADR-0004；JSON/IPC 使用规范十进制字符串，拒绝指数形式、非法 scale、非有限值和溢出。
3. Schema v1 覆盖：设置、主数据、BusinessEvent 和各 typed detail、FX/价格修订、FxResolution、LedgerPosting、AuditEvent、projection 元数据、ImportBatch/ImportRow、ValuationSnapshot/Line、备份状态。
4. 添加外键、业务唯一约束、active partial unique index、日期/状态检查和计划书要求的查询索引。
5. 建立显式、只前进 migration runner：只读识别 → 旧库一致性备份端口 → 单事务 migration → integrity/foreign-key/schema 检查 → 正常开放。版本过新或验证失败时只读/阻断，不修改旧库。
6. 建立事务协调器，使事件、detail、posting、audit 和 projection watermark 原子提交。
7. 建立确定性 posting 规范、canonical serialization/hash 和投影清空重建框架。
8. 实现 `create_ledger`、`open_ledger`、`get_ledger_status`、`update_settings`。活库只能位于 OS 本地应用数据目录，不能直接在同步盘或网络盘运行。
9. 出现依赖记录后冻结本位币；有交易后的账户币种和标的交易币种也不得原地重解释。
10. 建立 typed Application Facade；若选择 Tauri，IPC DTO 与 Domain 类型分离，Core 不信任前端校验。

## 测试与验收

- Decimal 规范化、精度、half-up、边界、溢出和非法输入。
- 事务任一步失败时无部分事件、posting、audit 或水位。
- 外键、业务唯一约束、active 修订和币种冻结。
- migration 成功、回滚、旧库损坏和 schema 过新。
- 改变 SQLite 物理行序后 canonical posting/hash 不变。
- 删除投影后从源事实重建完全相同。
- Domain 不依赖 UI、SQLite、桌面壳或 Excel 库。

运行统一检查、测试和构建；更新 `agent-context.md` 的持久状态，然后提交、合并并推送 `main`。
