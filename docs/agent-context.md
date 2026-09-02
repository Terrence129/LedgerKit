# Agent Context

> 状态：M1 完成，Ready for M2
>
> M0 状态：完成
>
> M1 状态：Tauri 2 + React/TypeScript + Rust Core 已选择并建立生产骨架
>
> 最后验证：2026-09-02
>
> 用途：为新任务提供短期导航和当前事实；不是决策权威或工作日志。

## 项目概况

LedgerKit 是单用户、本地优先、隐私友好的多币种现金、投资和净资产桌面应用。功能灵感、初始业务范围和迁移验收基线来自 `多币种个人账本v1.3.0.xlsx`；应用保留其已验证的业务意图，并通过 ADR 修正已经识别的公式、口径和可维护性问题。

权威源工作簿基线：

- 文件名：`多币种个人账本v1.3.0.xlsx`
- SHA-256：`E8B7D7BB743AE79AB51980115CD8BC88DC7E2E149295F56E5B6521BFCDF5F41D`
- 真实文件不得提交、复制到 fixture 或在公开材料中披露内容。

## 已确认的边界

- 1.0 首发 Windows x64；架构保留后续 macOS/Linux 能力。
- 核心功能离线可用；无登录、后端、数据库服务器或常驻守护进程。
- 一个账本只有一个活跃的权威 SQLite 数据库；迁移临时库和备份不是并行事实源。
- 活库放操作系统本地应用数据目录，不直接运行在同步盘或网络盘。
- Excel 是一次性初始迁移源和标准化导出格式，不是运行时计算引擎或持续双向同步源。
- 权威 Excel 源文件保留在仓库外私人目录，由用户在迁移时明确选择，并以文件名和已审阅的 SHA-256 校验；绝对私人路径不作为仓库元数据，也不得进入公开文档、日志、诊断包或 fixture。
- 真实财务数据不得进入公开仓库、日志、遥测、截图或 fixture。
- 1.0 应用自有 UI 仅支持简体中文（`zh-CN`）和英文（`en-US`）；首次跟随 Windows 显示语言，非中文回退英文，用户可即时切换并持久化。语言切换不得翻译或改写业务数据、稳定标识、错误码或财务 canonical hash。
- P0 支出分析沿用 v1.3.0 的产品意图，但使用动态分类、明确缺 FX 和稳定语义修正规则缺口。
- M0 已接受模块化单体/本地 SQLite、类型化事件/确定性 posting/可重建投影、`decimal-contract-v1`、逐笔移动加权平均、修订/冲正、逐账户/组合迁移策略、冻结交易 FX/动态估值和 `expense-analysis-query/v1`；权威内容见 [`docs/adr/README.md`](adr/README.md)。
- M0 黄金基线包含开发计划书 14.2 的 31 组技术栈无关合成 fixture、六类 JSON Schema、规范序列化/哈希和跨栈消费说明；验证入口为 [`tools/check-m0-fixtures.ps1`](../tools/check-m0-fixtures.ps1)。

## 已接受的 M1 决策

- ADR-0001 已按确定性门禁选择 Tauri 2 + React/TypeScript + Rust Core；Avalonia Native AOT 作为有实测证据的反转基线。双栈逐项报告见 [`benchmarks/m1/selection.md`](benchmarks/m1/selection.md)。
- ADR-0007 已选择 Rust `calamine` + `rust_xlsxwriter` 作为隔离于 Infrastructure 的已知模板 XLSX 读写适配器。
- ADR-0008 已确定 P0 活库使用标准 SQLite、不使用 SQLCipher；便携备份使用版本化 Argon2id + AES-256-GCM 随机 data-key 封装格式。
- ADR-0009 已固定标准薄包复用系统 Evergreen WebView2；离线 runtime 包若发布必须单独计量。
- ADR-0010 已固定 P0 不自动联网更新，代码签名和自动更新留到 P1；Beta 明确未签名。

## 暂定方案（尚未 Accepted）

- Tauri 样机已验证可重建支出日聚合投影能满足 10 万事件门禁；该方案仅记录于 Proposed ADR-0015，未获接受，不得作为后续权威实现依据。
- 本位币出现依赖记录后不可原地修改；需要新账本和显式迁移。

不得把本节的暂定方案描述成最终结论。

## Blockers

### M0

- 无。原 blocker 已由 ADR-0004、0005、0006、0011、0012、0014 和对应黄金 fixture 关闭；迁移实施仍必须逐账户/组合显式选择策略并完成闭环对账。

### M1

- 无。两套样机均通过硬门禁；任务 04 已按授权规则选择 Tauri，并由 ADR-0001、0007–0010 和 [`benchmarks/m1/selection.md`](benchmarks/m1/selection.md) 关闭门禁。

## 当前工作区

- 已有完整开发计划书和 agent 工作区规范。
- 已有 [`implementation-prompts/README.md`](implementation-prompts/README.md) 索引的 14 阶段串行实现提示词包，覆盖 M0 决策、M1 双栈门禁、M2–M6 实现和 Beta 候选审计；任务 01–04 已完成，任务 05–14 尚未执行。
- GitHub 公开仓库为 `Terrence129/LedgerKit`，默认分支为 `main`。
- 已创建十三份 Accepted ADR、Proposed ADR-0015、31 组正式合成黄金 fixture、JSON Schema、确定性生成器和 M0 检查命令；生产数据库 schema 将在 M2 建立。
- 已建立 `app` 生产骨架、锁文件、固定 Node/Rust 工具链、严格 TypeScript/Clippy、最小 unsafe allowlist、两套键一致本地化资源、持久化即时语言切换、设计 token 和健康首页。UI 只有 `get_ledger_status` 与 `update_settings` 两个具名 IPC。
- 已建立统一 `tools/check.ps1`、`tools/test.ps1`、`tools/build.ps1` 和 Windows CI；生产依赖清单见 [`production-dependencies.md`](production-dependencies.md)。一次性 `spikes/` 源码已从当前树删除，仍保留在 Git 历史。

## 下一步建议

1. 按串行协议执行任务 05，建立权威 Core、Schema v1、受控 migration 和四个基础 Application 操作；本任务不自动启动该阶段。
2. M2 必须保持当前 `UI → typed IPC → Application → Domain` 方向，SQLite 和 Excel 只能由 Rust Infrastructure 实现端口。
3. ADR-0015 仍为 Proposed；任务 05 不得因为样机性能结果自行把支出日聚合投影变成生产架构。

## 更新规则

仅在范围、Accepted ADR、blocker、里程碑或已验证工作区能力发生持久变化时更新本文。每次更新必须同步“最后验证”日期，并链接对应证据；不要追加对话摘要或临时进度。
