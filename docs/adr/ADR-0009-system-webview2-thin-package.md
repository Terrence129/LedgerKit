# ADR-0009：系统 Evergreen WebView2 与薄包分发

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：任务 04 要求在选择 Tauri 时根据样机固定系统 WebView2/薄包策略。
>
> 关联规则/里程碑：M1、ADR-0001、轻量门禁

## 背景

Tauri 标准包可以复用系统 Evergreen WebView2，也可以下载、嵌入离线安装器或携带 fixed runtime。样机使用系统 WebView2 151.0.4129.107，薄 NSIS 为 3.302 MiB、安装载荷 12.021 MiB；内含 WebView2 的 NSIS 为 253.008 MiB，不能作为标准薄包指标。

[Tauri Windows installer 文档](https://v2.tauri.app/distribute/windows-installer/)说明 `skip` 不增加安装包且要求设备已有 WebView2，offline installer 约增加 127 MB，fixed runtime 约增加 180 MB。[Microsoft 分发文档](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution)说明 Windows 11 包含 Evergreen runtime，少量 Windows 10 设备仍可能缺失，并建议检测 runtime。

## 决策驱动因素

- 标准薄包与安装载荷硬预算
- 默认运行时零应用网络请求和离线核心功能
- WebView2 安全更新、前向兼容与维护成本
- 缺失 runtime 时可解释、可恢复的安装体验

## 方案

### 方案 A：系统 Evergreen WebView2 薄包

标准 NSIS 设置 `webviewInstallMode.type = "skip"`，不由 LedgerKit 静默联网下载。WebView2 由 Windows/Microsoft servicing 管理。

### 方案 B：标准包内含 offline/fixed runtime

可覆盖完全离线安装，但样机体积远超标准薄包门禁，并把 Chromium servicing 责任转移给 LedgerKit。

### 方案 C：安装时下载 bootstrapper

包小，但安装会产生网络依赖，不符合 LedgerKit 标准发行路径“用户明确获取运行前置”的边界。

## 决策

接受方案 A：

- Windows 1.0 标准包使用设备已有的系统 Evergreen WebView2，Tauri 配置固定为 `skip`；应用只加载本地打包资源。
- Beta 前安装验收必须在支持矩阵设备检测 WebView2。缺失时中止并给出离线、用户可执行的 Microsoft 官方 runtime 安装说明；LedgerKit 不在后台或首次启动时下载。
- 如确需完全离线安装，可单独发布 `offlineInstaller` 变体，并独立报告下载体积、安装载荷和干净机实测；不得冒充标准薄包。
- 不使用 fixed runtime 作为默认方案；Evergreen 安全更新由平台管理。每次 Tauri/Wry/WebView2 变化都要重测 30 次冷启动、五分钟完整进程树 RSS、零远程 endpoint、关闭残留和 IPC 负向边界。
- 保留样机证明必要的 `msOneAuthWAM` feature 控制、严格 CSP、禁远程 URL、窗口不活跃时低内存目标。feature 名称或 COM 接口失效时必须显式报告，不能静默删除门禁。

## 后果

- 正面影响：标准包体积小，共享 runtime 及时获得平台安全更新，不由 LedgerKit 维护 Chromium。
- 负面影响：运行时版本会独立变化；少量缺失 runtime 的 Windows 10 设备需人工前置；完整进程树包含多个 WebView2 进程。
- 数据迁移影响：无。
- 测试与运维影响：维护 WebView2 支持矩阵和升级重测；离线安装包若发布必须单独计量。

## 反转条件

若目标设备不能可靠获得/更新 Evergreen runtime，或升级后无法满足零网络、RSS、启动和安全边界，则重新评估内含离线 runtime 或 Avalonia。反转必须由新 ADR 重定体积基线，不能在普通发布中隐藏 runtime 体积。

## 验证

- 脱敏 fixture：不适用
- 自动化测试：配置/CSP/capability/unsafe allowlist 检查
- 性能/体积/恢复验证：Tauri M1 五分钟网络/RSS与两种安装包实测
- 对账或差异桥：不适用

## 关联

- 开发计划书章节：4、10、12、17
- 被替代/关联 ADR：ADR-0001、ADR-0010
- 相关 issue/commit：M1 技术栈选择完成提交
