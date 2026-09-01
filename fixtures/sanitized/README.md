# Sanitized Golden Fixtures

本目录只存放程序生成或人工复核的脱敏合成数据。禁止从真实工作簿复制姓名、机构名称、商户、备注、账号、余额、附件或逐笔流水。

## 建议结构

```text
fixtures/sanitized/
├─ README.md
├─ cash-cny-basic/
│  ├─ metadata.json
│  ├─ input.json
│  ├─ normalized-events.json
│  ├─ expected-postings.json
│  ├─ expected-projection.json
│  └─ expected-errors.json
└─ ...
```

每个 fixture 必须说明：

- 关联规则 ID 和 ADR；
- 输入币种、scale、日期与 sequence；
- 预期事件、posting、投影和错误码；
- `calculation_version`；
- 允许的显示容差及内部必须精确一致的字段；
- fixture 生成/复核方式。

## 最小覆盖

- CNY 与外币收支、缺 FX 和人工覆盖。
- 同币种调拨、跨币种换汇与手续费。
- 买入、部分卖出、全部卖清、卖后再买。
- 股息、预扣税和独立投资费用。
- 价格缺失、陈旧价格、禁止未来价格/汇率。
- 完整历史与明确 cut-over 两套迁移。
- 冲正、修订和投影重建。
- 支出分桶、distinct 笔数、退款、归档分类、Top 10 + 其他和缺 FX。
- 损坏备份、错误口令和恢复失败不覆盖活库。

## 变更规则

- 不得为了让失败测试变绿而直接修改预期结果。
- 改变财务结果前必须先有用户接受的 ADR，并在 fixture metadata 中记录 ADR 编号。
- 真实 Excel fixture 不得进入仓库；若需要 XLSX contract fixture，只允许从合成数据生成并完成人工隐私审查。
