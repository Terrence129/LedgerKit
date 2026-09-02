# ADR-0001：Tauri 2、React/TypeScript 与 Rust Core

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：任务 04 的确定性选择规则已由项目所有者授权；Tauri 全部硬门禁通过时直接选择 Tauri。
>
> 关联规则/里程碑：M1、ADR-0002、M1 轻量门禁

## 背景

LedgerKit 需要在 Tauri 与 Avalonia 之间选择唯一生产栈。两个一次性样机消费同一组 M0 JSON 和同一个 10,000 行合成 XLSX，并按相同口径完成安装、启动、关闭、离线、权限、SQLite、Excel、备份和性能验证。逐项证据见 [`../benchmarks/m1/selection.md`](../benchmarks/m1/selection.md)。

Tauri 与 Avalonia Native AOT 均通过所有硬门禁。授权规则规定：只要 Tauri 同时通过全部硬门禁、Excel 黄金测试和权限边界，就选择 Tauri；只有 Tauri 失败时才选择通过门禁的 Avalonia。

## 决策驱动因素

- 确定性项目所有者授权，而非事后改变评分
- 本地优先、Core-only SQLite 和最小具名权限边界
- 安装体积、启动、完整进程树内存和默认零网络
- 已知 Excel 模板兼容性与跨平台保留路径
- 双语言长期维护成本和可测反转条件

## 方案

### 方案 A：Tauri 2 + React/TypeScript + Rust Core

标准薄安装包复用系统 Evergreen WebView2。React/TypeScript 只负责表现层与 UX 校验；Rust 负责 Application、Domain、SQLite transaction 和特权适配器。UI 只能调用逐项授权的具名 IPC。

样机薄安装包 3.302 MiB、安装载荷 12.021 MiB、冷启动 P95 1.024195 秒、idle 完整进程树 RSS P95 147.173 MB；所有硬门禁通过。代价是 Rust/TypeScript 双语言、WebView2 多进程，以及需在 WebView2 升级时重测内存和默认网络。

### 方案 B：.NET 10 + Avalonia Native AOT

单一 C# 语言和单进程模型降低跨边界维护成本。Native AOT 冷启动 P95 0.849659 秒、idle RSS P95 95.351 MB，全部硬门禁通过；但 framework-dependent 冷启动 P95 2.845878 秒失败，因此可部署方案依赖 Native AOT，并承担反射/裁剪约束与 64.855 MiB 安装载荷。

## 决策

接受方案 A，并固定：

- 桌面壳为 Tauri 2.11.5；UI 为 React 19.2.8 + TypeScript 7.0.2 + Vite 8.2.2；权威 Core 使用 Rust 1.98.0。所有版本由 lockfile 和工具链文件固定。
- 依赖方向为 `UI → typed IPC → Application → Domain`。Infrastructure 只实现 Application/Domain 定义的端口；Domain 不依赖 Tauri、React、SQLite 或 XLSX。
- SQLite、财务十进制、posting、投影、迁移、备份及 Excel 信任边界都在 Rust 侧。前端不得获得任意 SQL、posting、shell、远程 URL 或不受限文件路径能力。
- 标准包使用系统 Evergreen WebView2，具体分发见 ADR-0009。保持本地 CSP、显式 capability、按命令权限、`msOneAuthWAM` 网络控制和低内存目标适配器。
- P0 不增加本地 HTTP 服务、Node/Python sidecar、后台 daemon、图表运行时或重复状态框架。
- `spikes/tauri` 与 `spikes/avalonia` 只保留在 Git 历史；生产实现从经整理的 `app` 骨架继续。

本 ADR 只选择技术栈；其接受时并未接受样机中的支出物化投影。该后续问题已在任务 07 的生产查询实测失败后由项目所有者另行接受 ADR-0015，不改变本 ADR 的技术栈结论。

## 后果

- 正面影响：满足确定性规则；薄包和安装载荷余量大；Rust Core 使财务与特权能力位于一个可信边界；保留 Web UI 跨平台路径。
- 负面影响：两种语言和 WebView2 多进程增加调试面；idle RSS 接近 150 MB 硬门禁；Evergreen 更新可能改变内存或默认联网行为。
- 数据迁移影响：Excel staging、解析和规范事件生成全部在 Rust 适配器/Core 内，UI 只提交一次性授权 token 和高层命令。
- 测试与运维影响：每次 Tauri/WebView2 升级必须重跑完整进程树 RSS、网络、冷启动、IPC 负测与安装包门禁。

## 反转条件

以下任一情况触发新的 Proposed ADR，并以 Avalonia Native AOT 样机作为首选反转基线：

- 系统 WebView2 在目标 Windows 设备上不可可靠获得，必须内含大型 runtime 并导致硬预算失守。
- WebView2 升级后默认网络或 idle RSS 无法在不扩大权限的前提下恢复门禁。
- Rust/TypeScript 出现重复领域规则，或跨边界维护成本持续超过单人维护预算。
- Excel、SQLite、签名/打包或原生能力在 Rust 路径不稳定，而同功能 Avalonia 路径能够通过全部门禁。
- Windows 成为唯一长期平台，且项目所有者明确接受 AOT、裁剪和迁移成本。

反转必须迁移 typed DTO、端口适配器、测试和安装流程；SQLite schema 与规范 fixture 保持技术栈无关，不得改写财务答案。

## 验证

- 脱敏 fixture：M0 31 组 JSON 和 `fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx`
- 自动化测试：两套样机检查、最终 `tools/check.ps1` 与 `tools/test.ps1`
- 性能/体积/恢复验证：[`../benchmarks/m1/selection.md`](../benchmarks/m1/selection.md)
- 对账或差异桥：两套样机的规范 posting/支出 hash 相同

## 关联

- 开发计划书章节：4、10–12、15–18
- 被替代/关联 ADR：ADR-0002、ADR-0003、ADR-0007 至 ADR-0010、ADR-0015
- 相关 issue/commit：M1 技术栈选择完成提交
