# 任务 07/14：现金事件、修订/冲正与支出查询 Core

你正在执行 LedgerKit 的现金 Domain、现金投影和 `ExpenseAnalysisQuery`。遵循共同执行协议，确认主数据纵切已在 `main`，创建分支 `phase/m2-cash-analysis-core`。

## 必读材料

- `docs/financial-rules.md`
- ADR-0003、0004、0006、0012、0014
- 现金、FX、支出分析和修订黄金 fixture
- 开发计划书第 6–8、14–15 节

## 实现

1. 实现 `EventCommand` 的 OpeningBalance、IncomeExpense、Adjustment、Transfer、CurrencyExchange。
2. 实现 `preview_event`、`post_event`、`revise_event`、`reverse_event`。
3. 收入、支出和费用输入正数，符号由 Domain 派生；只有 Adjustment 接受显式正负增量。
4. 同币种调拨要求不同账户、相同币种和相同金额；跨币种换汇本金排除收入/支出，手续费单独过账并按自身账户币种处理。
5. 每个交易和费用保存完整冻结的 FxResolution；人工覆盖保留自动候选、覆盖值、理由、最终值和计算版本。
6. 修订事件指向 superseded event；冲正指向 reversed event。成对冲正净影响为零；退款/报销必须使用 semantic role，不得伪装成冲正。
7. 实现可重建的现金余额、月度收支和现金数据质量投影。
8. 实现只读 `get_expense_analysis`：
   - 日期含首尾。
   - 只纳入普通支出、普通手续费和换汇手续费。
   - 用户分类、系统费用桶、未分类、归档分类、退款/报销和未折算集合。
   - 全局 distinct event 与桶内 distinct event。
   - 稳定 Top 10 + 其他。
   - 单一 SQLite read snapshot。
   - event watermark、calculation/bucket/refund policy version 和 canonical hash。
   - 返回 `DrilldownContext`，不返回无界事件 ID。
   - 单次聚合、无 N+1、响应不超过 32 KiB。
9. 实现有上限、游标分页的 `get_activity` 后端。

## 测试与验收

- 执行全部现金、FX、调拨、换汇、修订、冲正和支出黄金 fixture。
- 完整时分桶合计等于总支出；缺 FX 时已折算分桶等于已折算小计。
- 一个事件跨桶时全局笔数为 1，桶内可各计 1。
- 退款/报销改名、跨期、无关联、关联原消费和缺 FX。
- 归档分类、未分类、换汇费和超过 10 个正金额分类。
- 改变物理行序/导入顺序不改变同版本 canonical hash。
- 10 万事件 warm/cold SQL、IPC 延迟和响应大小达到门禁。

性能不达标时先优化索引和查询计划；未经新 ADR 不得增加持久化支出投影。通过后提交、合并并推送 `main`。
