# 任务 03/14：Avalonia M1 纵向样机

你正在执行 LedgerKit 的 Avalonia M1 样机。遵循共同执行协议，确认 Tauri 报告和 10k 合成 XLSX 已在 `main`，创建分支 `phase/m1-avalonia-spike`。

## 必读材料

- M0 Accepted ADR 和全部黄金 fixture
- Tauri 样机、生成器与 `docs/benchmarks/m1/tauri.md`
- 开发计划书的轻量门禁、候选矩阵、反转条件和 M1 样机要求

## 交付

在 `spikes/avalonia` 建立 .NET 10 + Avalonia 一次性样机。必须消费 Tauri 样机生成的完全相同 XLSX 和 M0 JSON，不得另建更容易通过的输入。

1. 从官方资料确认并锁定当前稳定 Avalonia、Microsoft.Data.Sqlite、Open XML 组件和打包方式。
2. 完成与 Tauri 同形的纵向切片：受控 migration、事务写入、posting/projection、10k XLSX 读取、分页列表、净资产条、11 行原生支出横条和表格、受限文件复制、标准化 XLSX 导出、密码加密备份恢复、权限负测和 Windows per-user 安装包。
3. 不使用图表库、重量级 ORM、数据库服务器、后台服务或额外状态框架。
4. 同时测量 framework-dependent 薄包与可部署的 self-contained/trimmed 或 Native AOT 候选，区分设备已有 runtime 和应用载荷。
5. 使用与 Tauri 完全一致的指标、脚本和报告表头，额外记录裁剪/AOT 警告、反射依赖、Excel 限制、单语言维护优势与跨平台反转成本。

## 测试、验证与报告

- 相同黄金输入产生规范相同的事件、posting、投影和支出结果哈希。
- 安装、启动、关闭、卸载和断网实际通过。
- 无残留进程、默认网络请求、真实财务数据或敏感绝对路径。
- 每项硬门禁明确 pass/fail，不得修改基线。

生成 `docs/benchmarks/m1/avalonia.md`。通过后提交、合并并推送 `main`；不要接受 ADR-0001，不要打 M1 tag，不要删除任何 spike。
