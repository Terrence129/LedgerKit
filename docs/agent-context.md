# Agent Context

> 状态：M5 概览、支出分析与数据质量 UI 完成，Ready for M6 Safety
>
> M0 状态：完成
>
> M1 状态：Tauri 2 + React/TypeScript + Rust Core 已选择并建立生产骨架
>
> 最后验证：2026-09-03
>
> 用途：为新任务提供短期导航和当前事实；不是决策权威或工作日志。

## 项目概况

LedgerKit 是单用户、本地优先、隐私友好的多币种现金、投资和净资产桌面应用。功能灵感、初始业务范围和迁移验收基线来自 `多币种个人账本v1.3.0.xlsx`；应用保留其已验证的业务意图，并通过 ADR 修正已经识别的公式、口径和可维护性问题。

权威源工作簿基线：

- 文件名：`多币种个人账本v1.3.0.xlsx`
- SHA-256：`20CAFF41D7E5D08F71591CF8206DB015905BEAD40BC5AFEFD25008EE648D5820`（用户于 2026-09-03 确认为新的权威迁移源）
- 当前工作站定位：读取 Git 忽略的 `docs/local/private-source-workbook.md`；真实源调查、功能微调、最终对账或 cut-over 前必须按其中说明校验并使用副本。
- 真实文件不得提交、复制到 fixture 或在公开材料中披露内容。

## 已确认的边界

- 1.0 首发 Windows x64；架构保留后续 macOS/Linux 能力。
- 核心功能离线可用；无登录、后端、数据库服务器或常驻守护进程。
- 一个账本只有一个活跃的权威 SQLite 数据库；迁移临时库和备份不是并行事实源。
- 活库放操作系统本地应用数据目录，不直接运行在同步盘或网络盘。
- Excel 是一次性初始迁移源和标准化导出格式，不是运行时计算引擎或持续双向同步源。
- 权威 Excel 源文件保留在仓库外私人目录；当前工作站通过 Git 忽略的本机记录定位，由用户授权访问并以文件名和已审阅的 SHA-256 校验。绝对私人路径不作为 tracked 仓库元数据，也不得进入公开文档、日志、诊断包或 fixture。
- 真实财务数据不得进入公开仓库、日志、遥测、截图或 fixture。
- 1.0 应用自有 UI 仅支持简体中文（`zh-CN`）和英文（`en-US`）；首次跟随 Windows 显示语言，非中文回退英文，用户可即时切换并持久化。语言切换不得翻译或改写业务数据、稳定标识、错误码或财务 canonical hash。
- P0 支出分析沿用 v1.3.0 的产品意图，但使用动态分类、明确缺 FX 和稳定语义修正规则缺口。
- M0 已接受模块化单体/本地 SQLite、类型化事件/确定性 posting/可重建投影、`decimal-contract-v1`、逐笔移动加权平均、修订/冲正、逐账户/组合迁移策略、冻结交易 FX/动态估值和 `expense-analysis-query/v1`；权威内容见 [`docs/adr/README.md`](adr/README.md)。
- M0 黄金基线包含开发计划书 14.2 的 31 组技术栈无关合成 fixture、六类 JSON Schema、规范序列化/哈希和跨栈消费说明；验证入口为 [`tools/check-m0-fixtures.ps1`](../tools/check-m0-fixtures.ps1)。

## 已接受的 M1 决策

- ADR-0001 已按确定性门禁选择 Tauri 2 + React/TypeScript + Rust Core；Avalonia Native AOT 作为有实测证据的反转基线。双栈逐项报告见 [`benchmarks/m1/selection.md`](benchmarks/m1/selection.md)。
- ADR-0007 已选择 Rust `calamine` + `rust_xlsxwriter` 作为隔离于 Infrastructure 的已知模板 XLSX 读写适配器。
- ADR-0008 已确定 P0 活库使用标准 SQLite、不使用 SQLCipher；便携备份使用版本化 Argon2id + AES-256-GCM 随机 data-key 封装格式。
- ADR-0009 已固定标准薄包复用系统 Evergreen WebView2；离线 runtime 包若发布必须单独计量。
- ADR-0010 已固定 P0 不自动联网更新，代码签名和自动更新留到 P1；Beta 明确未签名。

## 暂定方案（尚未 Accepted）

- 本位币出现依赖记录后不可原地修改；需要新账本和显式迁移。

不得把本节的暂定方案描述成最终结论。

## Blockers

### M0

- 无。原 blocker 已由 ADR-0004、0005、0006、0011、0012、0014 和对应黄金 fixture 关闭；迁移实施仍必须逐账户/组合显式选择策略并完成闭环对账。

### M1

- 无。两套样机均通过硬门禁；任务 04 已按授权规则选择 Tauri，并由 ADR-0001、0007–0010 和 [`benchmarks/m1/selection.md`](benchmarks/m1/selection.md) 关闭门禁。

## 当前工作区

- 已有完整开发计划书和 agent 工作区规范。
- 已有 [`implementation-prompts/README.md`](implementation-prompts/README.md) 索引的 14 阶段串行实现提示词包，覆盖 M0 决策、M1 双栈门禁、M2–M6 实现和 Beta 候选审计；任务 01–12 已完成，任务 13–14 尚未执行。
- GitHub 公开仓库为 `Terrence129/LedgerKit`，默认分支为 `main`。
- 已创建十四份 Accepted ADR、31 组正式合成黄金 fixture、JSON Schema、确定性生成器和 M0 检查命令；当前生产 [`Schema v5`](persistence-schema-v1.md) 已覆盖设置、主数据、typed event detail、OpeningPosition/OpeningPerformance、市场修订/FxResolution、posting/audit、带目标 schema/分析快照的 import staging、不可变估值快照、备份状态，以及现金、支出和持仓可重建投影。
- 已建立 `app` 生产骨架、锁文件、固定 Node/Rust 工具链、严格 TypeScript/Clippy、最小 unsafe allowlist、两套键一致本地化资源、持久化即时语言切换，以及首次设置/设置与数据页面。当前共有二十五项已实现具名 IPC，现金与投资预览/过账/修订/冲正、概览、支出分析、数据质量、活动分页和一次性初始导入均通过同一 typed Facade；它们不接受任意数据库路径、SQL、posting、事件状态或前端伪造的财务派生字段。
- [`Catalog 与市场数据契约 v1`](catalog-and-market-data-v1.md) 已实现机构、现金账户、分类、组合、证券稳定 ID 与允许字段更新，非零账户停用阻断、组合/结算机构一致性、不可变汇率/价格修订、事务化 active 切换、非未来 as-of 解析及带稳定修复上下文的数据质量基础结果。
- Rust Core 已实现 ADR-0004 Decimal/Money/Currency、LocalDate、UUIDv7、Sequence、CalculationVersion、ProjectionWatermark 和稳定 DomainError；SQLite 已实现只读识别、备份端口、单事务只前进 migration、事务协调器、规范 posting/hash 与现金投影清空重建框架。活库固定在 OS 本地应用数据目录，本位币及有依赖的账户/标的币种由 Domain/Schema 双重阻断原地重解释。
- M2 Cash Core 已实现 OpeningBalance、Income/Expense、Adjustment、Transfer、CurrencyExchange 的高层命令，冻结交易/费用 FX resolution、修订/冲正、现金余额/月度收支/数据质量投影，以及 `expense-analysis-query/v1` 和有界游标活动下钻。生产事实扫描未达到 10 万事件门禁后，项目所有者接受 ADR-0015；可删除重建的日聚合投影两次复测为冷查询 0–1 ms、warm P95 0 ms、查询加序列化 0 ms、响应 2,457 bytes。
- M2 UI 已建立总览、流水、资产、数据质量、设置与数据五个稳定顶级入口；流水页完整支持收入、支出、余额调整、同币种调拨、换汇及费用的 Core 权威预览和写入。通用活动查询提供日期、类型、账户、分类、搜索和最多 100 条的游标分页，并一次有界水合业务内容、posting、冻结 FxResolution、修订/冲正关系及脱敏审计元数据；修订和冲正保留旧版本，冲正确认展示 Core 根据既有 posting 推导的反向影响。
- M3 已接入 ADR-0007 的 `calamine`/`rust_xlsxwriter` 隔离适配器和一次性 native 文件选择器。已知 8 表现金模板在 blocking worker 上只读解析到候选库 staging，逐行保存原始/规范化/公式缓存/hash 证据；dry-run 使用现有 Cash Core 生成 posting 和原币对账。用户确认后仅在无活库时单事务过账并重建投影，经完整性、对账和 SQLite backup 验证后同卷原子切换；同字节重跑幂等，修改文件禁止 merge。程序生成的三份 [`M3 合成 XLSX`](../fixtures/sanitized/m3/README.md) 覆盖正常、缺公式缓存、坏引用/日期、重复、方向错误、缺 FX 和修改文件。
- M4 已实现 SecurityTrade、Dividend 和 InvestmentExpense 的 Core 权威纵切；买卖手续费、移动加权平均、精确清仓、重开仓、股息净额和两级独立费用遵循 Accepted 规则。SQLite 按 `(portfolio_id,instrument_id)` 和稳定日期/sequence 重放持仓，历史插入、修订与通用冲正原子重建现金、posting、收益和水位；资产页展示 as-of 价格/FX 证据、陈旧警告和明确未估值原因，流水页提供四类投资录入和 typed 详情。
- M5 Core 已把全量迁移扩展到组合、标的、价格、投资流水、持仓基线、检查与支出证据；完整历史和日末显式 cut-over 均在隔离候选中通过同一 Core 原子处理现金/数量腿，并以账户、持仓、映射、事件、币种、净资产和支出差异矩阵阻断不可解释差异。`get_overview` 与 `get_data_quality` 使用明确估值日、非未来 as-of 价格/FX 和同一 SQLite snapshot；缺数据进入未估值集合，陈旧价格与稳定修复上下文可定位。已确认估值快照不可变，启动时现金/持仓投影版本或水位不匹配会先标记不可用并统一重建。
- M5 UI 已在“概览”内完成资产概览与支出分析两个页内标签；已估值净资产、构成、MTD、未估值待办、KPI、最多 11 行 Top 10 + 其他横条和完整语义表格均消费同一 Core 结果。分类占比和条宽由 Core 以整数基点跨 IPC，非法日期清空旧结果，迟到请求被丢弃；KPI/分类下钻携带版本化 `DrilldownContext`，数据质量异常可跳转到活动、汇率、价格或导入修复位置。
- 已建立统一 `tools/check.ps1`、`tools/test.ps1`、`tools/build.ps1` 和 Windows CI；生产依赖清单见 [`production-dependencies.md`](production-dependencies.md)。一次性 `spikes/` 源码已从当前树删除，仍保留在 Git 历史。

## 下一步建议

1. 按串行协议执行任务 13，实现密码加密便携备份、恢复、标准化导出与隐私诊断；25 项高层操作是硬上限，新增能力必须使用已预留的三个具名入口而不是通用文件/SQL 能力。
2. 任务 14 完成 Beta 发布门禁、安装/卸载/升级/恢复演练和隐私审计。
3. 真实 v1.3.0 的最终转换、逐账户/组合策略确认、对账和 cut-over 只在仓库外副本/候选库执行；不得用真实数据替换 M5 合成自动测试。

## 更新规则

仅在范围、Accepted ADR、blocker、里程碑或已验证工作区能力发生持久变化时更新本文。每次更新必须同步“最后验证”日期，并链接对应证据；不要追加对话摘要或临时进度。
