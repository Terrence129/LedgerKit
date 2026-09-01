# LedgerKit

LedgerKit 是一款规划中的本地优先、多币种个人资产与现金流桌面应用。它的功能灵感、初始业务范围和迁移验收基线来自现有的多币种 Excel 账本，但运行时不会依赖 Excel 公式或云端服务。

公开仓库：<https://github.com/Terrence129/LedgerKit>

## 当前状态

- 阶段：Ready for M0，尚未初始化应用技术栈。
- 首发目标：Windows x64；架构保留后续跨平台能力。
- 运行时目标：一个桌面应用进程树、一个活跃的权威 SQLite 账本、零后端、零强制云依赖、零常驻辅助服务。
- 当前仓库主要交付物是开发计划和 agent 工作区规范；应用源码尚未开始。

## 开始阅读

1. Agent：先读 [`AGENTS.md`](AGENTS.md)，再按 [`docs/README.md`](docs/README.md) 路由。
2. 人类协作者：先读 [`docs/多币种个人账本-开发计划书.md`](docs/多币种个人账本-开发计划书.md)。
3. 当前状态与未决问题：[`docs/agent-context.md`](docs/agent-context.md)。
4. 财务规则摘要：[`docs/financial-rules.md`](docs/financial-rules.md)。
5. 串行实施提示词：[`docs/implementation-prompts/README.md`](docs/implementation-prompts/README.md)。

## 隐私

本仓库公开时只允许提交源码、脱敏合成 fixture 和非敏感文档。真实工作簿、数据库、附件、备份、日志和分析输出不得进入版本控制。
