# LedgerKit 实现提示词包

本目录把 1.0 Beta 的实施路线拆成 14 个可直接交给 Codex 的串行任务。每个提示词负责一个可验证的阶段；全部完成后，代码应达到 `1.0.0-beta.1` 候选状态。

## 使用方式

1. 可以提前打开多个以 LedgerKit 为工作目录的 session，但不要并发发送任务。
2. 严格按编号执行。只有上一阶段已合并、推送且工作区恢复干净，才能发送下一份提示词。
3. 向新 session 发送对应 Markdown 文件的完整内容；无需补充历史聊天。
4. 如果某阶段报告 blocker、测试失败或没有推送 `main`，停止后续任务，先在该 session 修复。

所有 session 共用同一 checkout，因此并发编辑、切换分支或构建会互相干扰。本提示词包不适用于并发执行；若以后改用隔离 worktree，需要另行调整 Git 流程。

## 共同执行协议

每个阶段都必须遵守以下规则：

1. 先读根目录 `AGENTS.md`、`docs/README.md`、本文件和阶段提示词列出的额外材料。
2. 运行 `git status --short --branch`，确认当前为干净的 `main`；执行 `git pull --ff-only origin main`。发现未提交改动、未完成分支或远端分叉时停止，不得覆盖或丢弃现有工作。
3. 创建阶段提示词指定的 `phase/...` 分支。不得直接在 `main` 开发。
4. 只处理当前阶段范围；不得提前实现 P1/P2，也不得读取真实 Excel。
5. 新增生产依赖前记录用途、体积、许可证、维护状态、安全影响和不用它的成本。
6. 运行阶段指定检查以及仓库已有的统一检查。不得声称未执行的测试通过。
7. 更新文档时只记录持久事实：高影响决策写 ADR，里程碑状态写 `agent-context.md`，操作流程写 runbook。
8. 完成前执行隐私检查，确保没有真实财务数据、绝对私人路径、密钥、令牌、电子邮件、数据库、备份、日志或构建产物进入提交。
9. 只有全部验收通过时才提交阶段分支，然后：切换 `main`、再次 `git pull --ff-only origin main`、以 `git merge --ff-only <阶段分支>` 合并并 `git push origin main`。
10. 如果 fast-forward 合并或推送失败，停止并报告；不得自动强推、重置、丢弃或改写他人历史。
11. 阶段失败时保留现场，不合并、不推送完成标记、不运行下一阶段。

## 已由项目所有者确认的实施输入

这些确认用于编写提示词，但仍须由对应 ADR 固化：

- M0 采用计划书推荐财务口径：`half-up`、逐笔移动加权平均、修订/冲正、冻结交易 FX resolution、动态 active 估值、既定 P0 支出口径。
- 迁移同时支持完整历史与显式 cut-over，逐账户/组合选择，不提供危险默认值。
- 允许安装 Rust/Tauri 用户级工具链及必要 Windows 构建组件。
- M1 按条件自动定栈：Tauri 全部硬门禁通过则选择 Tauri，否则选择通过门禁的 Avalonia；两者都失败则停止。
- P0 活库不使用 SQLCipher；实现密码加密便携备份。
- 目标是 Beta 候选；真实 Excel 对账、代码签名和至少四周双录观察是人工发布门禁。

## 固定应用边界

- 单桌面应用、模块化单体、一个活跃的权威 SQLite 账本。
- 零后端、零本地 HTTP 服务、零守护进程、零 Node/Python sidecar。
- UI 不获得任意 SQL、posting、shell 或不受限路径能力。
- 财务值通过规范十进制字符串跨 UI 边界；禁止 JavaScript `number` 成为权威值。
- 支出分析使用一个权威查询结果和原生 HTML/CSS/语义表格，不增加图表运行时依赖。

高层应用边界最多 25 个具名操作：

```text
create_ledger             open_ledger              get_ledger_status
update_settings           save_institution         save_cash_account
save_category             save_portfolio           save_instrument
save_fx_revision          save_price_revision      preview_event
post_event                revise_event             reverse_event
get_activity              get_overview             get_expense_analysis
get_data_quality          analyze_import            commit_import
export_data               create_backup            restore_backup
get_backup_status
```

Tauri 中这些操作是具名 IPC；Avalonia 中是同形的进程内 Application Facade。文件选择通过平台适配器产生受限的一次性授权，不能演变为任意路径接口。

## 阶段顺序

| 阶段 | 提示词 | 完成标志 |
|---|---|---|
| M0 | `01-m0-decisions-and-golden-baseline.md` | tag `m0-baseline` |
| M1-A | `02-tauri-vertical-spike.md` | Tauri 同口径报告 |
| M1-B | `03-avalonia-vertical-spike.md` | Avalonia 同口径报告 |
| M1 Gate | `04-select-stack-and-scaffold.md` | tag `m1-stack-selected` |
| M2 Foundation | `05-core-and-sqlite-foundation.md` | Core/Schema v1 |
| M2 Catalog | `06-setup-catalog-and-market-data.md` | 主数据纵切 |
| M2 Cash | `07-cash-domain-and-expense-query.md` | 现金/支出 Core |
| M2 UI | `08-cash-ui-and-activity.md` | 日常录入纵切 |
| M3 | `09-excel-cash-migration.md` | 现金迁移纵切 |
| M4 | `10-investments-vertical-slice.md` | 投资纵切 |
| M5 Core | `11-full-migration-valuation-quality.md` | 全量迁移/估值 Core |
| M5 UI | `12-overview-expense-quality-ui.md` | P0 主界面完成 |
| M6 Safety | `13-backup-restore-export-privacy.md` | 数据安全能力完成 |
| M6 Gate | `14-beta-hardening-and-candidate.md` | tag `v1.0.0-beta.1` |

## 自动化与人工完成边界

全部提示词成功后应完成全部 P0 代码、合成黄金测试、安装包和迁移/恢复演练。以下项目不会由提示词伪装成已完成：

- 使用私人 `多币种个人账本v1.3.0.xlsx` 的最终对账与 cut-over。
- Windows 代码签名证书和正式签名。
- 至少四周人工双录、完整月周期和用户最终切换确认。
- 未经用户另行授权的 GitHub Release 发布。
