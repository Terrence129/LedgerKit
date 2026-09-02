# Catalog 与市场数据契约 v1

本契约记录任务 06 已实现的首次设置、主数据和市场数据边界。财务口径仍以 Accepted ADR 与 `financial-rules.md` 为准。

## 写入边界

- `save_institution`、`save_cash_account`、`save_category`、`save_portfolio`、`save_instrument`、`save_fx_revision`、`save_price_revision` 是七个独立授权的高层命令；不接受 SQL、数据库路径或 posting。
- 主数据由 UUIDv7 稳定标识。改名、重排、启停和允许字段更新不替换 ID，也不删除历史引用。
- CashAccount 必须引用存在的 Institution；非零余额账户不能停用。Portfolio 的 Institution 必须与默认结算账户的 Institution 一致。
- 分类方向固定为 `income|expense`，语义固定为 `normal|refund|reimbursement`；名称不承担语义。

## 市场修订与 as-of

- 汇率与价格数值必须是正的规范 DecimalString；汇率方向为“1 单位原币 = 本位币”。本位币自身由规则返回 `1`，不允许保存自身汇率行。
- 值、日期、来源和单位构成不可变修订内容。active 状态可以切换；激活同键修订时，旧 active 在同一 transaction 中停用。
- as-of 只选择目标日或此前最大日期的 active 修订，显式排除未来记录，排序不依赖插入或物理行顺序。
- `get_ledger_status` 可携带明确 `asOfDate`，返回受限 Catalog 快照；缺汇率、缺价格、超过七日的陈旧价格及组合机构不一致问题带稳定 code、实体 ID、修复命令和字段。缺失时不生成 0 或 1:1 猜测。

## 首次设置与显示

- 新账本由用户选择本位币；应用显示固定的 OS 本地应用数据活库位置和 `backup_status` 的设备丢失保护状态。
- 首次语言由 Windows 显示语言提示解析为 `zh-CN` 或 `en-US`；非中文回退英文。用户选择写入壳层缓存和已打开账本，应用即时重渲染。
- 语言资源键必须一致。切换语言只改变应用资源，不修改用户业务文本、DecimalString、UUID、错误码或 canonical hash。
