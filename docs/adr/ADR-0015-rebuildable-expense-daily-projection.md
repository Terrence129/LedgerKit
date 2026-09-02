# ADR-0015：可重建支出日聚合投影

> 状态：Proposed
>
> 日期：2026-09-02
>
> 决策者：待确认
>
> 关联规则/里程碑：M1、`expense-analysis-query/v1`、EXP-001 至 EXP-009

## 背景

ADR-0014 要求支出 KPI、桶、Top 10、语义表格和下钻上下文来自同一个版本化权威查询结果，并明确只有 10 万事件性能门禁失败时才可通过新 ADR 增加可重建物化层。

Tauri M1 样机先对 100,000 个合成事件直接扫描事实表。原始 SQL 冷查询为 1,259.3991 ms，warm P95 为 1,326.8799 ms，分别超过 150 ms 和 50 ms 门禁。把 SQLite 浮点聚合替换成 Rust Decimal 自定义聚合后，冷查询约 59.76 ms，但 warm P95 约 86.14 ms，仍不满足 warm 门禁。样机随后验证了按有效日和稳定桶预聚合、查询时继续由 Rust Decimal 合成规范结果的方案；30 次 fresh-connection P95 为 2.6740 ms，warm P95 为 2.1098 ms。

这些数字只来自合成 M1 样机，不构成 Accepted 架构决策。样机实现用于证明可行性和测量反转成本。

## 决策驱动因素

- 保持 ADR-0014 的规范结果、排序、计数、缺 FX 和 canonical hash 不变
- 权威事实仍是类型化事件和确定性 posting，投影可以完全删除后重建
- 事件、posting、投影增量和 watermark 必须处于同一 SQLite transaction
- 避免二进制浮点进入金额计算或持久化
- 满足 100,000 事件 cold P95 ≤ 150 ms、warm P95 ≤ 50 ms
- 支持 schema 迁移、备份恢复和投影版本升级

## 方案

### 方案 A：每次查询扫描完整事实历史

优点是派生状态最少。缺点是已实测无法达到门禁，即使自定义 Decimal 聚合也不能稳定满足 warm 目标。

### 方案 B：可重建的按日/桶支出投影

写入时按 `effective_date + bucket_id` 更新仅含规范十进制字符串和 distinct 计数所需状态的投影，并同步推进版本化 watermark。查询从投影读取，在 Rust Core 中完成 Decimal 汇总、Top 10、系统桶、缺 FX 和 canonical hash。投影可删除并由权威事件确定性重建。

优点是查询余量大，且不改变规范结果；缺点是增加 schema、写路径和重建逻辑，必须持续验证事务原子性和重建等价性。

### 方案 C：外部缓存或 UI 缓存

会引入第二事实口径、失效竞态或常驻服务，不符合本地单体和单一权威 Core 边界。

## 决策

待项目所有者决定。若接受，选择方案 B，并要求：

- 投影仅为派生数据，不是新的权威事实；删除投影不得丢失业务信息。
- 所有金额以规范十进制字符串持久化，并由 Rust Decimal 运算；禁止 SQLite `REAL` 或 JavaScript number 承担权威汇总。
- 新事件的事实、posting、投影增量和 watermark 在同一 transaction 内提交。
- schema/projection version 不匹配时先创建迁移前备份，再执行受控重建。
- 查询输出继续严格遵守 `expense-analysis-query/v1`；不得因投影改变任何 M0 黄金答案或 canonical hash。

## 后果

- 正面影响：样机中 100,000 事件 cold/warm P95 降至 2.6740/2.1098 ms，同时响应保持 17,698 bytes。
- 负面影响：写路径、迁移和恢复增加一个派生结构；每个影响支出口径的事件类型都要维护投影映射。
- 数据迁移影响：旧库升级时需备份后创建并全量重建投影；事实表和 posting 不改写。
- 测试与运维影响：必须测试中断回滚、删除后重建、物理行序无关、版本不匹配、备份恢复后重建及 100,000 事件性能。

## 反转条件

若未来按事实扫描在目标硬件上稳定满足门禁，或新的 Accepted 查询版本不能由该粒度无损表达，可删除投影并回退到事实重放。反转只删除派生表并更新 schema/projection version，不迁移或改写权威事件。

## 验证

- 脱敏 fixture：M0 `21-expense-date-validation/` 至 `31-expense-excel-difference-bridge/`，以及 M1 确定性 100,000 事件生成器
- 自动化测试：投影删除/重建结果与 hash 完全相同；failpoint 同时回滚事件、posting、投影和 watermark
- 性能/体积/恢复验证：100,000 事件 cold P95 2.6740 ms、warm P95 2.1098 ms、DB 54,038,528 bytes；仍需在最终技术栈和迁移阶段复测
- 对账或差异桥：`expense-analysis-query/v1` 的 M0 canonical hash 保持不变

## 关联

- 开发计划书章节：4、14、15、17
- 被替代/关联 ADR：ADR-0003、ADR-0004、ADR-0014
- 相关 issue/commit：Tauri M1 纵向样机完成提交
