# Sanitized Golden Fixtures

本目录只存放程序生成或人工复核的脱敏合成数据。禁止从真实工作簿复制姓名、机构名称、商户、备注、账号、余额、附件或逐笔流水。

## 固定结构

```text
fixtures/sanitized/
├─ README.md
├─ CANONICALIZATION.md
├─ schemas/
├─ 01-cny-income-expense/
│  ├─ metadata.json
│  ├─ input.json
│  ├─ normalized-events.json
│  ├─ expected-postings.json
│  ├─ expected-projection.json
│  └─ expected-errors.json
└─ 31-expense-excel-difference-bridge/
```

目录前缀 `01`–`31` 与开发计划书 14.2 的编号一一对应。每组必须且只能包含以上六个 JSON 文件；Schema 位于 `schemas/`。所有组都包含 `normal`、`boundary` 和 `failure` 场景，metadata 将三类场景映射到关联规则与 Accepted ADR。

每个 fixture 必须说明：

- 关联规则 ID 和 ADR；
- 输入币种、scale、日期与 sequence；
- 预期事件、posting、投影和错误码；
- `calculation_version`；
- 允许的显示容差及内部必须精确一致的字段；
- fixture 生成/复核方式。

`tools/generate-m0-fixtures.mjs` 是可复现生成器；默认写入 31 组文件，`--check` 只比较工作区与规范输出。生成器只使用硬编码合成数据，不读取 Excel、数据库、日志或用户目录。

## Decimal 与 JSON 合约

- 所有财务数字都是 ADR-0004 规范十进制字符串，不使用 JSON number 或指数。
- JSON number 只用于非负安全整数的 case 编号、sequence、计数、水位和 schema/version。
- metadata 固定 `decimal-contract-v1`：最多 28 位有效数字，金额/数量/单价/汇率/内部结果的最大 scale 分别为 8/12/12/15/18。
- 内部结果必须精确一致，容差为 `0`；`0.01` 只用于明确的 CNY 显示边界，不能掩盖 posting 或投影差异。
- 规范序列化、稳定排序和 SHA-256 见 [`CANONICALIZATION.md`](CANONICALIZATION.md)。

## 跨栈消费

Tauri/Rust 与 Avalonia/C# 必须使用同一目录，按以下顺序消费：

1. 以文件名选择 `schemas/` 中的 Schema 并拒绝未知字段或缺失文件。
2. 将 `input.json` 的命令交给候选栈的高层 Application/Core 接口，不直接导入预期 posting。
3. 比较实际规范事件、posting、投影和稳定错误码；财务值逐字段精确比较。
4. 使用 `ledgerkit-canonical-json-v1` 复算 posting 序列、文件和支出 query result 哈希。
5. 支出下钻只使用 query result 的筛选上下文调用分页活动查询；不得把 fixture 中的合成事件 ID 扩展成生产响应 ID 列表。

仓库当前可运行的统一 M0 检查为：

```powershell
pwsh -NoProfile -File tools/check-m0-fixtures.ps1
```

该命令对 186 个 fixture 文件执行 JSON Schema、生成器漂移、Decimal/规则覆盖、支出不变量、canonical hash 和 fixture 隐私检查。

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
