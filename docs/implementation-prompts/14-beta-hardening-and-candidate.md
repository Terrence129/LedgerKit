# 任务 14/14：Beta 总审计、性能、打包与候选版本

你正在执行 LedgerKit `1.0.0-beta.1` 总审计与候选构建。遵循共同执行协议，确认前 13 个阶段均已在 `main`，创建分支 `phase/m6-beta-hardening`。

## 必读材料

- 全部 Accepted ADR 与 `docs/financial-rules.md`
- 开发计划书的 P0、轻量门禁、测试、风险、Definition of Done 和发布章节
- migration 与 backup/restore runbook

目标不是增加范围，而是发现、修复并证明全部 P0 可交付。

## 执行

1. 在临时位置建立 clean clone，验证从零恢复依赖、检查、测试和打包。
2. 对照 P0 功能矩阵审计实现；发现 P0 缺口直接补齐，但不得引入 P1/P2。
3. 运行 Domain unit/property、SQLite integration/migration/rebuild、黄金、Excel contract、UI component、核心 E2E、备份恢复、权限、隐私和断网测试。
4. 使用 10 万合成事件测量账户/时间线、支出查询、常规保存/筛选/切页、数据库体积、UI 延迟和 IPC 载荷。
5. 测量候选包：
   - 标准薄安装包、安装后应用载荷和可选完整 runtime 包。
   - 冷启动 30 次 P95。
   - 启动 5 分钟后的完整进程树 idle RSS P95。
   - 退出 10 秒后残留进程。
   - 默认运行时网络请求。
   - clean clone 到测试+打包时间。
6. 核对直接生产依赖 ≤25、Tauri 插件 ≤8、特权操作 ≤25、首屏资源和支出页面增量预算。
7. 执行安全负测：伪造 posting、任意 SQL、路径越界、远程内容、未授权窗口/命令、恶意 XLSX 和日志泄漏。
8. 对五个顶级入口分别以 `zh-CN`/`en-US` 执行资源完整性、语言切换/重启恢复、键盘、焦点、语义、缩放、高对比和 reduced motion 审计；确认业务数据与 canonical hash 不随语言变化。
9. 生成 SBOM、许可证清单、性能/轻量报告、用户手册、导入指南、备份恢复指南、已知问题、升级/回滚和 Beta 发布说明。
10. 设置版本 `1.0.0-beta.1`，生成 Windows x64 per-user Beta 安装包及 SHA-256。不要提交安装包，文档只记录可复现构建和本地 artifact 路径规则。
11. 明确剩余人工发布门禁：
    - 私人 v1.3.0 Excel 最终对账和 cut-over。
    - Windows 代码签名证书。
    - 至少四周人工双录和一个完整月周期。
    - 用户最终切换确认。
12. 不创建 GitHub Release；是否发布安装包由用户另行授权。

## 完成门禁

任一硬目标失败时先修复并重测。无法修复则保留阶段分支，不合并、不打 tag，并给出精确 blocker；不得降低预算、删除测试或把失败写成已知问题后继续发布。

只有全部自动化硬门禁通过时：

- 更新 `agent-context.md` 为 Beta Candidate，并记录仍需人工完成的门禁。
- 提交、fast-forward 合并并推送 `main`。
- 创建并推送 annotated tag `v1.0.0-beta.1`。
