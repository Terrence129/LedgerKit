# 任务 06/14：首次设置、主数据与市场数据

你正在执行 LedgerKit 的首次设置和 Catalog/Market Data 纵向切片。遵循共同执行协议，确认 Core/Schema v1 已在 `main`，创建分支 `phase/m2-catalog-reference`。

## 必读材料

- `docs/financial-rules.md` 的 NUM、DATE、FX、VAL、CASH 和分类规则
- ADR-0002、0003、0004、0012
- 开发计划书的首次设置、机构/账户、分类、主数据、数据质量和可访问性要求

## 实现

完成 Domain → SQLite → Application → UI 的完整纵切：

1. 首次设置：新建空账本、选择本位币、显示活库位置和设备丢失保护状态；首次按 Windows 显示语言选择 `zh-CN` 或 `en-US`，非中文回退英文。
2. Institution、CashAccount、Category、Portfolio、SecurityInstrument 的创建、修改允许项、启停/归档和稳定 ID。
3. FxRateRevision 和 SecurityPriceRevision 的不可变修订、active 切换、正数校验和 as-of 查询。
4. 实现 `save_institution`、`save_cash_account`、`save_category`、`save_portfolio`、`save_instrument`、`save_fx_revision`、`save_price_revision`。
5. 分类 kind、semantic_role、稳定 sort order 和归档规则；分类方向与新流水类型不一致时必须可由后续 Core 阻断。
6. 账户币种、机构、用途和业务 ID 约束；组合机构与默认结算账户机构一致。非零账户不得直接关闭。
7. 设置与数据页面提供简体中文/英文语言切换以及双语表单、列表、空态、错误态和键盘操作；应用自有页面即时重渲染，选择持久化并在重启后恢复，不把 Excel 工作表逐页复制成导航。
8. 建立数据质量基础结果：缺汇率、缺价格、陈旧价格、悬空映射和非法 active 状态，并返回稳定修复上下文。
9. Tauri 时每个写入命令单独授权，不开放任意 SQL、shell 或通配路径。

## 测试与验收

- 汇率/价格只取目标日或此前最大日期，未来值排除，结果不依赖插入顺序。
- 本位币对自身汇率恒为 1。
- 同一日期允许历史修订但最多一个 active。
- 缺汇率/价格显式未折算/未估值，不使用 0 或 1:1。
- 分类改名、排序、停用不改变稳定 ID 和历史引用。
- 账户/组合约束和错误码稳定。
- UI 错误与字段关联，支持键盘、200% 缩放和非颜色状态表达。
- `zh-CN`/`en-US` 资源键集合一致；核心设置流程在两种语言下通过，切换语言不改写用户业务文本、权威 DecimalString、稳定 ID、错误码或 canonical hash。

通过统一检查后提交、合并并推送 `main`。
