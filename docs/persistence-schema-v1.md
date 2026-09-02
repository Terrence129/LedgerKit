# LedgerKit Persistence Schema v1

> 状态：M2 Foundation 已实现
>
> 权威边界：受 ADR-0002、ADR-0003、ADR-0004、ADR-0006、ADR-0011 和 ADR-0012 约束；本文记录物理实现，不改变财务口径。

## 标识与版本

- SQLite `application_id = 0x4C4B4954`（`LKIT`），`user_version = 1`。
- migration 历史保存应用版本和 Schema v1 SQL 的 SHA-256；posting 使用 `ledger-calculation-v1`，现金投影使用 `cash-balance-projection-v1`。
- 权威财务字段为 ADR-0004 规范十进制 `TEXT`，不使用 SQLite `REAL`。

## 表组

| 表组 | Schema v1 表 |
|---|---|
| 账本与设置 | `ledger_metadata`、`app_settings`、`migration_history`、`backup_status` |
| 主数据 | `institutions`、`cash_accounts`、`categories`、`portfolios`、`security_instruments` |
| 事件与 typed detail | `business_events`、`opening_balance_details`、`income_expense_details`、`transfer_details`、`currency_exchange_details`、`security_trade_details`、`dividend_details`、`investment_expense_details`、`opening_position_details`、`opening_performance_details` |
| 市场与折算 | `fx_rate_revisions`、`security_price_revisions`、`fx_resolutions` |
| 分录与审计 | `ledger_postings`、`audit_events` |
| 可重建投影 | `projection_metadata`、`cash_balance_projection`、`holding_projection` |
| 导入 staging | `import_batches`、`import_rows` |
| 估值审计 | `valuation_snapshots`、`valuation_snapshot_lines` |

Schema 使用外键、业务 ID/自然键唯一约束、日期与枚举状态检查。汇率和价格由 partial unique index 保证同一业务键至多一个 active 修订。活动、修订/冲正、费用分类、posting、持仓、as-of 市场数据、导入和估值均有对应查询索引。

## 写入与冻结

- `EventTransactionPort` 只接受已通过 Domain 校验的 `PreparedEventCommit`；事件、唯一 typed detail、确定性 posting、audit、现金投影和投影水位在同一个 SQLite transaction 中提交。
- `LedgerPosting` 没有 IPC 或独立写入入口。规范序列先按 `(effective_date, sequence, event_id, posting_id)` 排序，再以 `ledgerkit-canonical-json-v1` 序列化并计算 SHA-256。
- 本位币在出现现金账户、标的、事件、市场数据或估值依赖后冻结。账户产生 posting 后不能更改币种；标的产生交易或价格修订后不能更改交易币种。
- `ProjectionRebuilder` 先将投影标记不可用、清空派生行，再按稳定事件/posting 顺序重建并原子推进版本与水位。当前 Foundation 实现现金余额投影；后续投影必须沿用同一框架。

## 创建、打开与迁移

高层 Facade 暴露 `create_ledger`、`open_ledger`、`get_ledger_status`、`update_settings`。IPC DTO 与 Domain 类型分离，并拒绝未知字段。创建/打开命令不接受数据库路径；活库固定由 Tauri `app_local_data_dir` 派生为 `ledger.sqlite3`。相对路径、UNC 路径和已知同步盘目录不得成为活库根目录。

打开顺序固定为：

```text
只读识别 application_id / user_version / integrity / foreign keys / schema
→ 旧版本通过 MigrationBackupPort 创建并验证一致性 SQLite 备份
→ 单 transaction 只前进 migration
→ transaction 内 integrity / foreign-key / 必需表与索引检查
→ commit 后以 WAL + synchronous=FULL 正常开放
```

版本高于应用、损坏、非 LedgerKit、备份失败、migration 失败或 Schema 校验失败均阻断打开。失败不得把旧库当作已升级；不执行自动降级、强制修复或猜测性 migration。
