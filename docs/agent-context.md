# Agent Context

> 状态：M0 完成，Ready for M1
>
> M1-A 状态：Tauri 样机已通过全部硬门禁，Ready for M1-B 同口径比较
>
> M1-B 状态：Avalonia Native AOT 样机已通过全部硬门禁，Ready for M1 Gate 技术栈选择
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

## 暂定方案（尚未 Accepted）

- 技术栈首选 Tauri 2 + React/TypeScript + Rust Core + SQLite；M1 与 .NET/Avalonia 风险样机比较后接受 ADR-0001。
- Tauri 样机已验证可重建支出日聚合投影能满足 10 万事件门禁；该方案仅记录于 Proposed ADR-0015，未获接受，不得作为后续权威实现依据。
- P0 支持密码加密的便携备份；是否引入 SQLCipher 由威胁模型和 M1 PoC 决定。
- 本位币出现依赖记录后不可原地修改；需要新账本和显式迁移。

不得把本节的暂定方案描述成最终结论。

## Blockers

### M0

- 无。原 blocker 已由 ADR-0004、0005、0006、0011、0012、0014 和对应黄金 fixture 关闭；迁移实施仍必须逐账户/组合显式选择策略并完成闭环对账。

### M1

- 技术栈必须通过真实 SQLite、已知模板 XLSX、安装包、完整进程树、默认网络、启动/内存和依赖预算验证。
- 数据库加密、备份加密、WebView2 分发与发布签名策略需要 PoC/ADR。
- Tauri M1 样机报告见 [`benchmarks/m1/tauri.md`](benchmarks/m1/tauri.md)：完整进程树 idle RSS P95 为 147,173,376 bytes，正式五分钟运行观察到零远程 endpoint，冷启动 P95 为 1,024.195 ms，100k 支出 cold/warm P95 为 2.6740/2.1098 ms，全部硬门禁通过。该单栈结果仍不能据此接受 ADR-0001 或选择技术栈。
- Avalonia M1 样机报告见 [`benchmarks/m1/avalonia.md`](benchmarks/m1/avalonia.md)：framework-dependent 冷启动 P95 2,845.878 ms 未通过，但 deployable Native AOT 冷启动 P95 849.659 ms、idle RSS P95 95,350,784 bytes，正式五分钟运行观察到零远程 endpoint，100k 支出 cold/warm P95 为 2.6989/1.8187 ms，最终 AOT 安装包 17,675,474 bytes、实装载荷 68,005,284 bytes，全部硬门禁通过。双栈报告已齐备，但仍须任务 04 明确选择并接受 ADR-0001。

## 当前工作区

- 已有完整开发计划书和 agent 工作区规范。
- 已有 [`implementation-prompts/README.md`](implementation-prompts/README.md) 索引的 14 阶段串行实现提示词包，覆盖 M0 决策、M1 双栈门禁、M2–M6 实现和 Beta 候选审计；任务 01–03 已完成，任务 04–14 尚未执行。
- GitHub 公开仓库为 `Terrence129/LedgerKit`，默认分支为 `main`。
- 已创建八份 M0 Accepted ADR、Proposed ADR-0015、31 组正式合成黄金 fixture、JSON Schema、确定性生成器和 M0 检查命令；尚未创建生产应用骨架或生产数据库 schema，`spikes/tauri` 与 `spikes/avalonia` 均为一次性样机。
- 尚未建立全项目统一的 `check/test/build` 命令；M0 fixture 使用 `pwsh -NoProfile -File tools/check-m0-fixtures.ps1`，两个 M1 样机各有目录内检查与测量脚本，后续阶段仍必须报告实际运行的检查。

## 下一步建议

1. 按串行协议执行任务 04 的 M1 Gate，对两份同口径报告进行选择并更新 ADR-0001；本任务不自动启动该阶段。
2. 选择时必须保留 framework-dependent Avalonia 冷启动失败、Native AOT 约束、Tauri WebView2 约束和双栈完整载荷/维护成本，不得只比较单一最优数字。
3. ADR-0001 接受且 M1 Gate 完整关闭后，才能按选定栈进入任务 05；不要提前批量开发 UI。

## 更新规则

仅在范围、Accepted ADR、blocker、里程碑或已验证工作区能力发生持久变化时更新本文。每次更新必须同步“最后验证”日期，并链接对应证据；不要追加对话摘要或临时进度。
