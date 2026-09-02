# LedgerKit

LedgerKit 是一款正在开发的本地优先、多币种个人资产与现金流桌面应用。它的功能灵感、初始业务范围和迁移验收基线来自现有的多币种 Excel 账本，但运行时不会依赖 Excel 公式或云端服务。

公开仓库：<https://github.com/Terrence129/LedgerKit>

## 当前状态

- 阶段：M1 完成，Ready for M2。
- 技术栈：Tauri 2 + React/TypeScript + Rust Core；逐项样机选择证据见 [`docs/benchmarks/m1/selection.md`](docs/benchmarks/m1/selection.md)。
- 首发目标：Windows x64；架构保留后续跨平台能力。
- 运行时目标：一个桌面应用进程树、一个活跃的权威 SQLite 账本、零后端、零强制云依赖、零常驻辅助服务。
- `app` 是已锁定工具链和依赖的最小生产骨架；一次性双栈样机只保留在 Git 历史。

## 本地验证与构建

Windows x64 需要 Node 24.16.0、npm 11.13.0、Rust 1.98.0 MSVC 工具链、Microsoft C++ Build Tools、Windows SDK、NSIS，以及系统 Evergreen WebView2。

```powershell
pwsh -NoProfile -File tools/check.ps1
pwsh -NoProfile -File tools/test.ps1
pwsh -NoProfile -File tools/build.ps1
```

`check.ps1` 覆盖格式、严格静态检查、单测、M0 黄金 fixture、资源键一致性、权限/依赖/首屏体积预算与隐私扫描；`build.ps1` 生成 Windows per-user NSIS 薄安装包。P0 不自动联网更新，Beta 安装包未签名。

## 开始阅读

1. Agent：先读 [`AGENTS.md`](AGENTS.md)，再按 [`docs/README.md`](docs/README.md) 路由。
2. 人类协作者：先读 [`docs/多币种个人账本-开发计划书.md`](docs/多币种个人账本-开发计划书.md)。
3. 当前状态与未决问题：[`docs/agent-context.md`](docs/agent-context.md)。
4. 财务规则摘要：[`docs/financial-rules.md`](docs/financial-rules.md)。
5. 串行实施提示词：[`docs/implementation-prompts/README.md`](docs/implementation-prompts/README.md)。

## 隐私

本仓库公开时只允许提交源码、脱敏合成 fixture 和非敏感文档。真实工作簿、数据库、附件、备份、日志和分析输出不得进入版本控制。
