# ADR-0014：支出分析口径与版本化查询合约

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：项目所有者在 M0 实现提示中明确接受 `financial-rules.md` 的 P0 费用、桶、退款/报销、缺 FX、distinct 笔数、Top 10 和版本规则，并授权本 ADR 直接设为 Accepted。
>
> 关联规则/里程碑：M0、EXP-001 至 EXP-009

## 背景

Excel v1.3.0 的支出页表达了有用的产品意图，但固定分类范围、启用状态和不一致的金额/笔数公式会漏计。LedgerKit 需要一个可同时驱动 KPI、排行、表格和下钻的权威查询结果，并能解释与 Excel 可见口径的差异。

## 决策驱动因素

- 支出金额、分桶、笔数、Top 10 和下钻同源
- 归档/改名、退款、缺 FX 和一事件多贡献行为明确
- 查询结果可复现、稳定排序且跨栈哈希一致
- 响应体不携带无界事件 ID

## 方案

### 方案 A：版本化权威查询结果

Core 在同一 SQLite read snapshot 中构造规范 query result，UI 只负责表现和分页下钻。

### 方案 B：KPI、图表、表格各自查询

实现局部简单，但过滤、笔数和排序容易漂移，无法证明下钻同源。

### 方案 C：复制 Excel 固定范围公式

短期对数接近，但会保留已知漏计、名称耦合和归档分类消失问题。

## 决策

接受方案 A，并定义 `expense-analysis-query/v1`：

- P0 gross expense 只包含普通支出本金、普通收支手续费和换汇手续费。收入、余额调整、调拨本金、换汇本金、证券本金、买卖手续费、股息手续费/预扣税和独立投资费用全部排除；投资费用仍只进入投资净回报。
- 每个已折算贡献恰好进入一个桶：普通支出进入稳定 `category_id`，空分类进入 `system:uncategorized`，普通手续费进入 `system:ordinary-fee`，换汇手续费进入 `system:fx-fee`。三个系统桶不可删除或改 ID。
- 一个事件可以产生多个贡献并跨桶。总笔数是至少有一个已折算 P0 gross expense 贡献的全局 distinct event 数；桶笔数是该桶 distinct event 数，桶笔数之和可以大于总笔数。
- `refund` 与 `reimbursement` 是稳定 semantic role，按自身有效日单列 gross 金额和 distinct 笔数，默认不冲减原消费分类。未折算退款/报销分别计数，不计入支出总笔数。
- 日期范围含首尾。默认 MTD 先按本地“今天”解析成明确 `start_date`/`end_date`；query result 不保存模糊的“本月”。非法、空、带未确认时间分数或 `start > end` 返回错误并不复用旧结果。
- 完整时 `sum(bucket.amount)=total_expense`。缺 FX 时只返回 `valued_subtotal`，并满足 `sum(valued bucket.amount)=valued_subtotal`；另列未折算支出、退款和报销计数。无正已折算桶时 `largest_category=null`。
- 归档、启用、改名和重排不改变历史 `category_id`、贡献金额或筛选集合；标签和明确的并列展示次序可随当前主数据版本变化。
- 正金额桶按 `amount DESC, bucket_id ASC` 排名；零/负桶不进入 Top 10。超过十个时前十项原样返回，剩余正金额桶合成 `system:top10-other`，其金额和 distinct 计数从同一贡献集确定。完整 bucket rows 仍保留供语义表格使用。
- 规范结果包含解析后的日期、本位币、total 或 valued subtotal、global/bucket distinct counts、完整稳定排序桶、Top 10、退款/报销、未折算计数、event/master-data watermark、`expense-policy-v1`、`expense-bucket-policy-v1`、`refund-policy-v1`、calculation version、canonicalization ID 和 SHA-256。
- canonical hash 使用 `ledgerkit-canonical-json-v1`：移除顶层 `canonical_hash` 后，对已按合约排序的数组和对象执行 UTF-8、NFC、对象键 Unicode 码点升序、无空白序列化；财务数值为十进制字符串，JSON number 只允许安全非负整数计数/sequence/version。输出 `sha256:<64 个小写十六进制>`。
- 每个桶/KPI/退款/未折算项只携带有界 `drilldown_context`：解析日期、watermark、bucket/semantic role、valuation state 和版本。不得保存或返回无界事件 ID 数组；下钻把该上下文交给分页 `get_activity` 并重用相同事件有效性规则。
- 新事件或影响查询的主数据修订产生新 watermark/结果，不覆盖旧导出元数据。相同水位和 policy/calculation versions 在任意 SQLite 物理行序下必须产生相同 canonical hash。

## 后果

- 正面影响：KPI、排行、表格和下钻有一个可验证真相；已知 Excel 漏计被显式修正。
- 负面影响：查询契约较丰富，需要维护版本、水位、稳定排序和双口径迁移桥。
- 数据迁移影响：Excel 支出页只作可见基线；应用动态全量计算并逐事件解释差异。
- 测试与运维影响：必须覆盖日期、费用矩阵、distinct、退款/报销、归档、缺 FX、Top 10、物理行序和旧版本复现。

## 反转条件

P1 若增加“总费用”或退款净额视图，必须使用新的 policy version，不能改变 `expense-policy-v1` 历史含义。只有 10 万事件性能门禁失败时，才可通过新 ADR 增加可重建物化层。

## 验证

- 脱敏 fixture：`21-expense-date-validation/` 至 `31-expense-excel-difference-bridge/`
- 自动化测试：Schema、桶和计数不变量、稳定排序、canonical hash 与无界 ID 拒绝
- 性能/体积/恢复验证：10 万事件 P95 查询/下钻门禁在 M2/M5 执行
- 对账或差异桥：v1.3.0 可见结果到应用规范结果的逐事件/逐桶桥

## 关联

- 开发计划书章节：2.3、7、13、14、21
- 被替代/关联 ADR：ADR-0003、ADR-0004、ADR-0006、ADR-0012
- 相关 issue/commit：本任务完成提交
