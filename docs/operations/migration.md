# Excel Migration Runbook

> 状态：M5 全量规范化迁移契约、投资 cut-over、估值与质量对账已实现；真实源最终 cut-over 仍需按本 runbook 在仓库外完成。
>
> 适用范围：从用户在仓库外私人目录中明确选择的已知
> `多币种个人账本v1.3.0.xlsx` 模板进行一次性初始迁移。

## 安全原则

- 永不修改原始工作簿。
- 以文件名和已审阅的 SHA-256 确认权威源文件；当前工作站先读取 Git 忽略的 `docs/local/private-source-workbook.md` 定位原件。绝对私人路径只作为本地选择输入，不进入 tracked 仓库、公开文档、日志、诊断包或 fixture。
- 不把真实工作簿、解析输出、数据库或对账明细提交到仓库。
- 所有试运行在源文件副本、临时 staging 和新的候选数据库中完成。
- 用户确认且对账通过前，不写入或替换正式账本。
- cut-over 未决定或任一关键差异无法解释时，迁移失败而不是猜测。

## 迁移前置条件

1. 获得用户对真实源处理的明确授权；读取 `docs/local/private-source-workbook.md`，以只读方式确认源文件存在，并校验文件名和原始文件 SHA-256。记录缺失、路径不存在或哈希不匹配时停止，不得搜索相似文件替代。
2. 从已确认的原文件创建迁移副本，在桌面 Excel 中执行全量重算并保存。
3. 分别记录原文件和重算副本的哈希，并记录副本大小、修改时间、导入器版本和目标 schema 版本；当前应用目标为 Schema v7。迁移证据不得包含绝对私人路径。
4. 确认本位币、金额精度、成本基础、修订语义和支出分析 ADR。
5. 逐账户/组合选择并记录完整历史重建或明确 cut-over。
6. 准备与真实数据结构等价的脱敏黄金 fixture。

## Dry-run

1. 只读解析设置、机构、资金子账户、分类、汇率、收支流水、资金调拨和换汇流水，并在全量契约中加入投资组合、证券、证券价格、投资流水、持仓基线、检查和支出分析证据，共 15 张精确命名工作表，进入 `ImportBatch/ImportRow` staging。真实 v1.3.0 的 14 张展示/业务工作表先在仓库外只读转换为该版本化规范契约；转换副本及任何真实数据不得进入 Git。
2. 保存工作表、行号、原始文本、公式文本、缓存值和内容哈希；不得把公式本身当成权威金额。
3. 区分硬编码输入、带公式输入、派生公式、状态和展示字段。
4. 将旧业务 ID 映射到新 UUID；无稳定 ID 的流水使用同字节文件范围内的确定性导入键。
5. 运行字段、引用、重复、分类方向、汇率/价格、负持仓和 cut-over 校验。
6. 生成问题清单、规范化差异和拟议财务结果，不写正式账本。

当前规范化目标为 `ledgerkit-workbook-v1.4`，导入器为 `ledgerkit-xlsx-full-v3`。现金、调拨、换汇或投资事件若在源账本中带人工汇率，转换时必须把覆盖币种、覆盖值和非空原因作为完整三元组写入对应 override 列；不得把人工覆盖折叠成普通汇率修订，也不得因自动候选存在而丢弃覆盖。Core 必须保存自动候选、override、理由和最终汇率，并在提交后复核相同 resolution。

完整历史账户的期初金额必须作为 `收支流水` 中的显式 `OpeningBalance` 事件导入，事件日期取经审阅的源期初日期；账户主数据的 `opening_balance` 保持 `0`，避免期初与历史流水重复计入。显式 cut-over 仍由账户行的期初金额、cut-over 日期和策略生成基线，不得混用两种入口。

### 实际入口与限制

- 首次启动且尚无活库时，在 onboarding 选择“选择并分析工作簿”。文件对话框仅接受 `.xlsx`，路径只在 Rust Core 内使用，WebView 和 IPC 请求都不能提交任意路径。
- 解析和提交均通过 Tauri blocking pool 执行；UI 保持响应并通过 `aria-live` 报告进度/失败。
- 已知模板限制为 5 MiB；现金契约为 8 张、全量契约为 15 张精确命名工作表；总计最多 20,000 行、每表最多 32 列、每个文本单元格最多 4,096 字符。含宏、外部链接、未知/缺失表或表头漂移的文件拒绝进入 staging。
- 问题清单只显示稳定错误码、工作表、Excel 行号和字段，不记录单元格值或绝对路径。公式不执行；输入公式必须有可用缓存，派生、状态和展示公式只保存为证据。
- 分析结果展示旧 ID 到 UUIDv7、逐账户迁移策略、拟议事件/posting、原币余额和规范差异。只有零 blocker、现金/持仓/估值等同口径差异为零，且 Excel 可见口径到应用规范口径的差异逐项解释时才能确认。

## Cut-over 门禁

### A. 完整历史重建

- 导入完整历史业务事件。
- 证明最早现金余额/调整与全部证券现金腿闭环。
- 避免历史现金腿与期初余额重复计入。

### B. 明确 cut-over

- cut-over 前事件只作为迁移证据，不生成 posting。
- 在 cut-over 日创建 OpeningBalance、OpeningPosition 和 OpeningPerformance。
- 对零持仓但有历史业绩的标的保留记录；组合级费用不得静默摊派。

任何策略都必须按账户、组合和标的证明现金、数量、剩余成本、已实现盈亏、净股息、独立费用和净资产闭环。

## 提交与切换

1. 用户审阅问题清单和对账报告。
2. 在新的候选数据库中单事务提交源事件。
3. 重建 posting 与全部投影。
4. 再次运行逐事件、账户、持仓、机构、币种、净资产和支出分析对账。
5. 创建并验证候选数据库的一致性备份。
6. 只有全部通过后才原子切换正式账本；失败则保留旧账本并丢弃候选库。

真实 v1.3.0 的仓库外转换与最终切换还必须在提交前创建一个密码加密的 `ledgerkit-portable-backup/v1` 包，并按 backup/restore runbook 重新解密验证；本机 migration/恢复前 SQLite 快照不能替代外部设备丢失保护。

初始导入使用 opaque `batchId` 作为唯一提交授权。候选数据库和 staging 位于应用本地数据目录的 `import-staging` 子目录，与最终 `ledger.sqlite3` 同卷；dry-run 已在隔离候选中通过同一 Cash/Investment Core 过账完整拟议事件，因此可同时生成账户、持仓、机构、币种、总净资产与支出差异矩阵。确认后在同一事务绑定 batch、恢复源账户生命周期并统一重建投影，经 `integrity_check`/外键检查、全部矩阵复核和 SQLite backup API 验证后，以同卷 rename 原子切换。已有活库只允许同一已提交 batch 返回幂等结果，任何不同文件哈希的候选都以 `IMPORT_MODIFIED_MERGE_FORBIDDEN` 拒绝，不做增量 merge。

## 对账证据

- 输入行统计与每行状态。
- 主数据映射。
- 每个事件的现金/数量腿。
- 每个账户的原币余额。
- 每个 `(portfolio, instrument)` 的数量、成本与回报组成。
- 所用汇率、价格、目标日期和计算版本。
- 机构、币种、总净资产和数据质量检查。
- Excel 可见结果与应用规范结果的逐事件/逐桶差异桥。

## 失败处理

- 不修改源文件。
- 不覆盖旧数据库。
- 保存脱敏错误类别、行号和计数；不把真实输入值写入日志。
- 修复映射或输入后重新 dry-run；修改/另存的工作簿视为新的导入候选，不增量 merge 到已过账账本。
- 文件对话框取消返回 `IMPORT_CANCELLED`；模板、大小、确认、blocker、对账、备份和切换失败均使用稳定错误码。切换前失败只留下隔离候选，不创建或覆盖 `ledger.sqlite3`；修正源文件后重新分析会生成新 batch。
- 如果应用在切换后但 UI 刷新前退出，重新启动并打开账本即可；`import_batches.status='committed'`、事件的 `import_batch_id`、投影水位和规范 posting hash 用于确认提交已完成。

## 开发与复核命令

以下命令只使用 `fixtures/sanitized/m3` 与 `fixtures/sanitized/m5` 中的程序生成合成文件，不读取真实工作簿：

```powershell
cargo run --manifest-path app/src-tauri/Cargo.toml --example generate_m3_fixtures
cargo run --manifest-path app/src-tauri/Cargo.toml --example generate_m5_fixtures
cargo test --manifest-path app/src-tauri/Cargo.toml infrastructure::sqlite::import_store::tests
npm --prefix app run check
pwsh -NoProfile -File tools/check.ps1
```

确定性复核应在连续两次生成后比较各组 `.xlsx` 的 SHA-256；任何字节漂移都视为 fixture 失败。M5 合成文件分别固定完整历史、显式 cut-over 和阻断路径；显式 cut-over 采用日末边界，cut-over 当日及之前的流水只作证据，Opening 事件在该日建立新账本基线。
