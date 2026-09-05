<p align="center">
  <img src="app/assets/icon.svg" width="96" height="96" alt="LedgerKit logo">
</p>

<h1 align="center">LedgerKit</h1>

<p align="center">
  本地优先、离线可用的多币种个人现金、投资与净资产桌面账本。
  <br>
  A local-first, offline-capable desktop ledger for multi-currency cash, investments, and net worth.
</p>

<p align="center">
  <a href="https://github.com/Terrence129/LedgerKit/actions/workflows/ci.yml"><img src="https://github.com/Terrence129/LedgerKit/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Windows%20x64-0078D4" alt="Windows x64">
  <a href="https://github.com/Terrence129/LedgerKit/releases/tag/v1.0.0-beta.2"><img src="https://img.shields.io/badge/release-v1.0.0--beta.2-F59E0B" alt="v1.0.0-beta.2 prerelease"></a>
</p>

LedgerKit 将账本数据保存在用户电脑上的 SQLite 数据库中，不需要账号、后端服务器或强制云服务。金额、证券数量、价格、汇率、成本和盈亏全部由 Rust Core 使用精确十进制规则处理；UI 不能直接执行 SQL 或伪造财务 posting。

> 当前版本：[v1.0.0-beta.2 预发布版](https://github.com/Terrence129/LedgerKit/releases/tag/v1.0.0-beta.2)，提供 Windows x64 安装包，包含 v1.4 规范化导入及桌面交互修复。这是供试用和反馈的 Beta，不是正式稳定版；`v1.0.0-beta.1` 历史标签保持不变。

## 功能

- 多币种现金账户，以及收入、支出、余额调整、账户调拨和换汇。
- 投资买卖、股息、投资费用、逐笔移动加权成本和持仓重建。
- 按日期计算净资产，展示价格、汇率证据以及未估值原因。
- 本月至今支出、分类分析、退款语义和活动下钻。
- 已过账事件不可原地覆盖；通过可追踪的修订或冲正更正历史。
- 缺失汇率、缺失或陈旧价格、负持仓和导入问题的数据质量页面。
- 版本化 Excel staging、dry-run、问题清单、对账和原子初始导入。
- 密码加密的便携备份、验证恢复、XLSX/CSV/对账导出和脱敏诊断。
- 简体中文和英文界面；切换语言不会改写业务数据或财务结果。

## 获取与安装

### GitHub Release

从 [v1.0.0-beta.2 发布页](https://github.com/Terrence129/LedgerKit/releases/tag/v1.0.0-beta.2) 下载 `LedgerKit_1.0.0-beta.2_x64-setup.exe` 和 `SHA256SUMS.txt`。安装包不含任何私人账本；新用户首次启动创建自己的空账本。

在 PowerShell 中校验下载文件，将结果与 `SHA256SUMS.txt` 比较：

```powershell
Get-FileHash .\LedgerKit_1.0.0-beta.2_x64-setup.exe -Algorithm SHA256
```

已有账本的用户请先创建并验证加密备份，再关闭应用安装更新。不要删除本地账本目录；此版本不执行真实 Excel 的自动迁移或正式切换。

当前 Beta 安装包尚未签名，Windows 可能显示“未知发布者”。标准轻量安装包使用系统 Evergreen WebView2；少量缺少该运行时的 Windows 设备需要先通过 Microsoft 官方渠道安装 WebView2。

已知限制：投资修订的桌面编辑入口尚未提供，全部 UI 流程及长期双轨对账仍需完成验收。请保留原始资料和独立备份，不要仅依赖 Beta 保存唯一有效账目。完整说明见发布页。

### 从源码构建

构建环境：

- Windows x64
- Node.js `24.16.0`
- npm `11.13.0`
- Rust `1.98.0` MSVC 工具链
- Microsoft C++ Build Tools、Windows SDK 和 NSIS
- Evergreen WebView2 Runtime

```powershell
git clone https://github.com/Terrence129/LedgerKit.git
Set-Location LedgerKit
pwsh -NoProfile -File tools/check.ps1
pwsh -NoProfile -File tools/build.ps1
```

构建成功后，安装包位于：

```text
app/src-tauri/target/release/bundle/nsis/
```

`tools/build.ps1` 会输出安装包的完整路径、字节数和 SHA-256。

## 第一次使用

1. 启动 LedgerKit，选择简体中文或 English。
2. 新建空账本并选择三字母本位币，例如 `CNY`、`USD` 或 `SGD`。
3. 在“设置与数据”中创建机构、现金账户、分类、投资组合和证券。
4. 按需录入汇率和证券价格；LedgerKit 只使用目标日期当天或之前的有效记录。
5. 在“流水”中录入第一笔事件，先检查 Core 生成的权威预览，再确认过账。
6. 在“设置与数据”中尽快创建至少一个密码加密的设备外备份。

创建业务数据后，本位币会被冻结。需要更换本位币时，应建立新账本并执行显式迁移，不能直接重解释既有金额。

## 日常使用

LedgerKit 有五个顶级入口：

| 页面 | 用途 |
|---|---|
| 总览 | 查看估值净资产、现金、持仓、本月至今支出和质量待办；支出分析位于页内标签。 |
| 流水 | 预览并过账现金和投资事件，筛选活动，查看 posting、汇率解析和审计链。 |
| 资产 | 按日期查看组合、持仓、成本、价格/汇率证据和投资回报。 |
| 数据质量 | 定位缺汇率、缺价格、陈旧价格、负持仓、引用和导入问题。 |
| 设置与数据 | 维护主数据和市场数据，执行导入、备份、恢复及导出。 |

### 记一笔收入或支出

1. 打开“流水”，选择事件类型。
2. 填写日期、账户、分类和原币金额；如有手续费，填写手续费账户与金额。
3. 选择“预览权威影响”，检查现金变化、采用的汇率和质量提示。
4. 确认后过账。若存在阻断问题，Core 不会写入部分结果。

不要用负支出模拟退款。退款、报销、普通手续费、换汇手续费和投资费用各有明确语义与统计口径。

### 更正已过账记录

在流水详情中使用“修订”创建替代事件，或使用“冲正”创建相反影响。原事件、posting 和审计链会继续保留。不要直接修改 SQLite，也不要手动删除数据库的 WAL/SHM 文件。

### 缺少汇率或价格

LedgerKit 会明确显示“未折算”或“未估值”，并提供数据质量修复入口；不会静默使用零或 `1:1`。补录市场数据后，可重新查询相应日期的估值结果。

## 从 Excel 初始导入

公开导入器只接受版本化的规范化 `.xlsx`，不会猜测任意私人表格的结构。

1. 保留原始工作簿不变，只对副本进行整理和重算。
2. 按当前 `ledgerkit-workbook-v1.4` 的 15 表契约准备规范化工作簿；不要直接选择结构未知的私人表格。
3. 在尚无活跃账本时选择工作簿，先运行只读分析。
4. 审阅文件哈希、映射、拟议事件、问题清单和全部对账差异。
5. 只有 blocker 为零、同口径差异为零且跨口径差异逐项解释时，才确认提交。

导入失败不会替换正式账本。修改后的文件会成为新的候选，不会被增量合并进已过账账本。

## 备份、恢复与升级

- 使用不少于 12 个字符的独立密码创建 `.lkbackup`；密码遗失后无法恢复。
- 将已完成并验证的备份复制到仓库和当前设备之外的安全位置。
- 恢复操作先在候选位置验证认证、schema、完整性、外键、投影和 canonical posting hash，再原子切换。
- Beta 不自动联网检查更新。升级前先创建并验证便携备份，退出应用后再安装新版本。
- 卸载程序不等于删除账本；不要依赖卸载流程清理个人数据。

## 隐私与安全边界

- 核心功能可离线使用；P0 没有登录、同步服务、自动更新或后台守护进程。
- 一个账本只有一个活跃的权威 SQLite 数据库。
- WebView 只能调用受限的具名 IPC，不能获得任意 SQL、shell 或文件系统能力。
- 真实工作簿、数据库、备份、导出和截图不得提交到仓库或公开 Issue。
- 报告问题时，请提供稳定错误码、应用/schema 版本和脱敏 diagnostics，不要粘贴真实交易、余额、账号或私人路径。

## 开发与验证

```powershell
# 快速测试
pwsh -NoProfile -File tools/test.ps1

# 完整质量门：格式、静态检查、单测、黄金样例、隐私和性能预算
pwsh -NoProfile -File tools/check.ps1

# Windows NSIS 安装包
pwsh -NoProfile -File tools/build.ps1
```

架构采用模块化单体：React/TypeScript 负责表现层，Rust Application/Domain 是唯一财务规则权威，SQLite Infrastructure 负责事务、迁移、投影、导入和备份。生产运行时不需要 Node/Python sidecar、本地 HTTP 服务或数据库服务器。

项目设计、财务规则、ADR、实施提示词和运维手册只保存在维护者本地，不发布到 GitHub。公开贡献以本 README、源码接口、测试和 CI 结果为准。

提交 Pull Request 前必须运行完整检查，并确保没有真实财务数据、私人路径、密钥、数据库、备份、日志或构建产物进入 Git 历史。在维护者本地文档包可用时，`tools/check.ps1` 还会执行文档契约检查；普通 GitHub 克隆会跳过这些仅本地检查。

## 当前范围与限制

- 首发平台为 Windows x64；架构保留后续 macOS/Linux 能力，但尚未发布对应构建。
- 自动市场价格/汇率、云同步、移动端、Web 端、托管附件和自动更新不属于 P0。
- 应用语言范围固定为 `zh-CN` 和 `en-US`。
- 当前没有已发布的 GitHub 二进制 Release；代码签名仍是面向广泛用户正式发布前的门槛。

## License

Rust 包元数据声明为 MIT。仓库根目录目前尚未包含正式 `LICENSE` 文件；在许可证文件补齐前，请不要假定获得了超出适用法律默认范围的再分发授权。
