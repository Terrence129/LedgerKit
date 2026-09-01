# ADR-0004：Decimal、精度与舍入合约

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：项目所有者在 M0 实现提示中明确接受 `half-up` 及本任务列出的跨候选栈 Decimal 合约，并授权本 ADR 直接设为 Accepted。
>
> 关联规则/里程碑：M0、NUM-001 至 NUM-004

## 背景

SQLite 动态类型、JavaScript `number` 和常见二进制浮点无法作为金额、数量、价格、汇率、成本或盈亏的权威表示。Tauri/Rust 与 Avalonia/C# 必须对同一输入产生逐字段完全一致的结果，不能依赖运行时默认 Decimal 行为。

## 决策驱动因素

- 跨技术栈精确一致和稳定错误
- 保留 Excel/用户输入的可解释源精度
- 防止静默截断、溢出和提前舍入
- SQLite、IPC、导出和哈希使用无歧义表示

## 方案

### 方案 A：受约束十进制合约和显式边界舍入

用 `{coefficient, scale}` 或等价 Decimal 实现统一上限，所有边界、舍入和错误由领域合约定义。

### 方案 B：各栈原生 Decimal 默认行为

开发快，但有效数字、指数、溢出和 midpoint rounding 默认值可能不同，无法保证 fixture 跨栈一致。

### 方案 C：二进制浮点加总额容差

实现简单，但逐笔误差会积累，且容差会掩盖真实迁移和 posting 差异。

## 决策

接受方案 A，并定义 `decimal-contract-v1`：

| 用途 | 最大 scale |
|---|---:|
| 普通金额输入 | 8 |
| 证券数量 | 12 |
| 证券单价 | 12 |
| 汇率 | 15 |
| 内部成本、本位币换算和派生比率 | 18 |

- 任一权威 Decimal 最多 28 位有效数字。解析、运算或舍入后超过上限返回稳定错误 `DECIMAL_PRECISION_EXCEEDED`；中间系数或目标表示无法承载时返回 `DECIMAL_OVERFLOW`。
- 输入禁止指数、`NaN`、Infinity、前导 `+` 和区域化分隔符。非法文本返回 `DECIMAL_INVALID`；超过用途 scale 返回 `DECIMAL_SCALE_EXCEEDED`。
- 源事实保留规范十进制文本及其 scale，不去掉有意义的尾随零。允许 `0` 或 `0.00`，但禁止负零；整数部分禁止无意义前导零。
- 超过币种常用精度但不超过普通金额 scale 的输入不自动改值，进入显式确认 `CURRENCY_PRECISION_CONFIRMATION_REQUIRED`；拒绝确认则不生成事件。
- 默认且唯一的 midpoint 舍入模式是 `half-up`（绝对值向最近值，恰好中点时远离零）。不得使用运行时默认 banker's rounding。
- 不在解析、标准化、事件保存或普通加减中提前舍入。原币 posting 保留已确认输入精度；当乘除结果超过 18 位 scale 时，才在明确的内部成本/换算边界以 scale 18 `half-up` 舍入。清仓成本遵循 ADR-0005 的“结转全部剩余成本”，不以均价乘法引入残值。
- 投影若使用统一 scale 定标整数，只能是可重建派生数据，写入前检查溢出；源事件、posting 审计值和 SQLite 权威财务值使用规范十进制 `TEXT`，禁止 `REAL`。
- IPC、JSON fixture、导入导出中的财务数字使用十进制字符串。显示边界按币种常用小数位 `half-up`，显示值不是源事实。
- 每个 fixture 的 metadata 必须记录适用 Decimal 合约、计算版本、舍入边界和显示容差；同一输入和算法的内部值容差为零。

稳定错误优先级为：语法 `DECIMAL_INVALID` → 用途 scale `DECIMAL_SCALE_EXCEEDED` → 有效数字 `DECIMAL_PRECISION_EXCEEDED` → 算术承载 `DECIMAL_OVERFLOW` → 币种常用精度确认。这样候选栈不会因校验顺序产生不同错误。

## 后果

- 正面影响：Rust、C#、SQLite 和 JSON 的值与错误完全可比较；舍入可定位到明确边界。
- 负面影响：需要自定义包装类型或严格配置 Decimal 库；显示值与源精度必须明确区分。
- 数据迁移影响：保留原始文本；超 scale 或超币种常用精度产生可审阅问题，不能静默规范化。
- 测试与运维影响：必须覆盖正负 midpoint、最大有效数字、每类 scale 上限、溢出、负零、指数和投影 atoms 溢出。

## 反转条件

只有受支持资产确实需要超过 28 位有效数字或既定 scale，且跨栈 PoC、SQLite 格式迁移和所有黄金差异均已评审时，才能通过新 ADR 扩展上限。任何扩展不得重新允许二进制浮点或隐式舍入。

## 验证

- 脱敏 fixture：所有 fixture；重点为 `01-cny-income-expense/`、`05-cross-currency-exchange-fee/`、`08-close-and-reopen-position/`
- 自动化测试：Schema 和语义验证器检查十进制字符串、有效数字、scale、负零与错误优先级
- 性能/体积/恢复验证：M1 候选栈用同一 fixture 比较规范输出和哈希
- 对账或差异桥：内部逐字段精确一致；Excel 浮点比较只用于独立诊断桥

## 关联

- 开发计划书章节：8、9、13.5、14
- 被替代/关联 ADR：ADR-0003、ADR-0005、ADR-0012、ADR-0014
- 相关 issue/commit：本任务完成提交
