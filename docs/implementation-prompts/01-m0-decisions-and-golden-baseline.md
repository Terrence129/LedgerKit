# 任务 01/14：M0 财务决策与黄金基线

你正在执行 LedgerKit 的 M0 决策与黄金基线阶段。完整遵循 `AGENTS.md` 和 `docs/implementation-prompts/README.md` 的共同执行协议，创建分支 `phase/m0-decisions`。

## 必读材料

- `docs/financial-rules.md`
- `docs/adr/README.md` 与 ADR 模板
- 开发计划书第 7–9、13–14、21–24 节
- `fixtures/sanitized/README.md`

## 已获得的用户确认

项目所有者已在生成本提示词包的任务中明确接受以下推荐方向。不要再次询问，也不要把最终 ADR 留在 Proposed：

- 模块化单体和一个本地权威 SQLite 活库。
- 类型化业务事件、确定性 posting、可重建投影。
- Decimal 使用 `half-up`，禁止权威二进制浮点。
- 证券成本采用逐笔移动加权平均；清仓时 carrying cost 精确归零，重新买入不改变历史已实现盈亏。
- 已过账事件只能修订或冲正。
- 交易 `FxResolution` 过账后冻结；改变历史交易折算必须创建带理由的修订。普通动态估值使用当前 active 的 as-of 汇率/价格，已确认快照冻结修订。
- 同时支持完整历史重建和显式 cut-over；迁移时逐账户/组合选择，无安全默认值。
- 支出分析采用 `financial-rules.md` 中既定 P0 费用、桶、退款/报销、缺 FX、distinct 笔数、Top 10 和版本规则。

## 交付

1. 创建并设为 Accepted：ADR-0002、0003、0004、0005、0006、0011、0012、0014。决策者写“项目所有者”，并记录本任务中的明确授权。
2. 同步 ADR 索引、`financial-rules.md` 和 `agent-context.md`；不得改变未经确认的其他决策。
3. ADR-0004 固定跨候选栈一致的 Decimal 合约：
   - 最多 28 位有效数字，溢出返回稳定错误。
   - 普通金额输入最多 8 位小数、证券数量 12 位、单价 12 位、汇率 15 位、内部成本/换算结果 18 位。
   - 保留源精度；超限或超出币种常用精度进入显式确认/错误，不得静默截断。
   - 仅在明确的 posting、投影或显示边界舍入，并在 fixture 元数据记录边界。
   - SQLite 源事实使用规范十进制文本；性能用定标整数只能是可重建投影且必须检查溢出。
4. 将开发计划书 14.2 节全部黄金案例落成技术栈无关的合成 JSON fixture。每组至少包含 `metadata.json`、`input.json`、`normalized-events.json`、`expected-postings.json`、`expected-projection.json`、`expected-errors.json`。
5. 为支出分析提供规范 query result、policy/calculation 版本、稳定排序和 canonical hash；drilldown 使用筛选上下文，不保存无界事件 ID。
6. 添加 JSON Schema、fixture 目录约定、确定性序列化/哈希规范和跨栈消费说明。

## 测试、验证与完成

- 所有 JSON 可解析并通过 schema。
- 所有财务数字均为十进制字符串，而非 JSON number。
- 每项规则至少有正常、边界和失败案例。
- Accepted ADR、`financial-rules.md` 与黄金预期完全一致。
- fixture 只含合成数据；执行隐私扫描。

全部通过后把 `agent-context.md` 更新为 M0 完成、Ready for M1，提交、fast-forward 合并并推送 `main`，创建并推送 annotated tag `m0-baseline`。失败时不合并、不打 tag。
