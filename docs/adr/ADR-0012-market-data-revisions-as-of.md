# ADR-0012：汇率/价格修订、冻结交易解析与估值 as-of

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：项目所有者在 M0 实现提示中明确接受冻结交易 `FxResolution`、动态 active 估值和冻结已确认快照，并授权本 ADR 直接设为 Accepted。
>
> 关联规则/里程碑：M0、FX-001 至 FX-005、VAL-001 至 VAL-004

## 背景

Excel 的最近汇率可能依赖物理行顺序，价格也可能晚于估值日。若修改 active 汇率或价格就静默重解释已过账交易，历史现金流和报表会失去稳定性；若所有估值都永久冻结，又无法反映用户修正后的市场数据。

## 决策驱动因素

- 交易历史稳定且更改有理由和审计链
- 普通估值能使用当前已知的最佳 as-of 数据
- 禁止未来值、行序依赖、零值和 1:1 猜测
- 已确认快照能够精确复现

## 方案

### 方案 A：交易冻结、普通估值动态、确认快照冻结

交易保存完整 `FxResolution`；估值查询当前 active as-of 修订；确认快照保存所用修订与解析 ID。

### 方案 B：所有结果始终动态

修正市场数据方便，但会静默改变已过账交易和旧导出。

### 方案 C：所有结果永久冻结

审计稳定，但日常估值无法自然采用更正后的价格/汇率，维护负担高。

## 决策

接受方案 A：

- 汇率方向固定为 `1 单位原币 = 本位币`；本位币对自身由规则解析为 `1`。汇率和价格值必须为正数。
- 每个 `(currency, base_currency, date)` 和 `(instrument, date)` 可保存多个不可变值修订，但同一键至多一个 active。替换时在一个 transaction 中停用旧修订并新增/激活新修订，禁止原地改值。
- 自动选择严格为目标日期当日或此前 active 修订中的最大日期；同日通过唯一 active 消除歧义。未来修订永不参与，物理行顺序和导入批次顺序不得影响结果。
- 每个已过账交易和手续费保存 `FxResolution`：用途、目标日、币种方向、自动候选修订、override 值与理由、最终汇率及 calculation version。过账后该 resolution 冻结。
- 改变历史交易折算只能创建带理由的事件修订；不得因 active 汇率变化静默重算旧交易。修订生成新的冻结 resolution，并由 ADR-0006 的水位语义保留旧结果。
- 普通动态估值使用当前 active 的价格和以估值日为目标的 active 汇率；估值汇率不得改用价格日期。估值日向前/后变化时重新解析。
- 已确认 `ValuationSnapshot` 冻结每行所用价格修订、FX resolution、原币/本位币值、calculation version 和源水位。重算创建新快照版本，不覆盖旧快照。
- 估值日前没有价格时为 `PRICE_MISSING_AS_OF`；缺 FX 时为 `FX_MISSING_AS_OF`。项目进入未估值/未折算集合，禁止使用零或 1:1。默认价格超过 7 个自然日标记 `STALE_PRICE`，但仍可展示；阈值变化需新 ADR 或版本化政策。
- 人工 override 必须保存自动候选（可为 null）、override、理由和最终值；理由为空返回 `FX_OVERRIDE_REASON_REQUIRED`。

## 后果

- 正面影响：交易历史稳定、普通估值及时、快照可复现，所有选择可下钻。
- 负面影响：同一估值日的动态视图可能在市场数据修订后改变，UI 必须展示数据日期和版本。
- 数据迁移影响：Excel 汇率/价格进入 revision；行序 last-row-wins 必须转为问题清单或明确 active 选择。
- 测试与运维影响：必须覆盖未来排除、同日 active 唯一、缺失、override、陈旧、交易冻结和快照重建。

## 反转条件

若未来引入自动行情，只能作为用户启用的 revision 来源，不能改变 active/as-of、交易冻结或缺失语义。陈旧阈值按资产类型变化必须版本化并提供快照差异。

## 验证

- 脱敏 fixture：`02-fx-as-of-order-independent/`、`03-fx-revision-missing-override/`、`10-missing-security-price/`、`12-valuation-as-of-stale/`、`14-valuation-date-mtd/`
- 自动化测试：active 唯一、as-of 索引、冻结 resolution、动态/快照差异和 canonical hash
- 性能/体积/恢复验证：M2/M5 价格和汇率 as-of 查询计划
- 对账或差异桥：每项估值列出目标日、修订 ID、override、最终值和版本

## 关联

- 开发计划书章节：7、8、13、14
- 被替代/关联 ADR：ADR-0004、ADR-0006、ADR-0014
- 相关 issue/commit：本任务完成提交
