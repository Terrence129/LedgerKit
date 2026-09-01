# 任务 04/14：技术栈选择与最终工程骨架

你正在执行 LedgerKit 的 M1 技术门禁和最终工程初始化。遵循共同执行协议，确认两套 spike、同口径报告、安装包实测和 M0 tag 均存在，创建分支 `phase/m1-select-stack`。

## 确定性选择规则

项目所有者已授权：

1. Tauri 满足全部硬门禁、Excel 黄金测试和权限边界时选择 Tauri。
2. Tauri 任一硬门禁失败，则选择满足全部硬门禁的 Avalonia。
3. 两者都失败时立即停止，不得降低硬门禁或继续业务开发。

## 交付

1. 复核并补跑缺失指标，生成一份逐项同口径对比报告。
2. 按上述规则接受 ADR-0001，保留测量证据、选择、后果和反转条件。
3. 根据样机证据接受：
   - ADR-0007：最终 Excel 读写适配器。
   - ADR-0008：P0 活库不使用 SQLCipher；密码加密便携备份使用的版本化格式、KDF、AEAD、参数来源和依赖。
   - ADR-0009：Tauri 时固定系统 WebView2/薄包策略；Avalonia 时记录不适用。
   - ADR-0010：P0 不自动联网更新；签名和自动更新留到 P1，Beta 明确未签名。
4. 从胜出样机提炼最终 `app`，不得把实验代码未经整理直接当生产架构。依赖固定为 UI → Application → Domain，Infrastructure 只实现端口。
5. 删除两套 spike 源码但保留 benchmark、fixture、选择报告和 Git 历史。
6. 建立锁文件、固定工具链、严格 lint/nullable、最小 `unsafe` 策略、中文本地化入口、设计 token 和最小健康首页。
7. 建立 `tools/check.ps1`、`tools/test.ps1`、`tools/build.ps1`，并建立 CI 的格式、静态检查、单测、隐私扫描和生产依赖预算。
8. 建立生产依赖清单，逐项记录用途、体积、许可证、安全、维护和替代成本。
9. 更新 README、ADR 索引和 `agent-context.md` 为 M1 完成、Ready for M2。

## 验证与完成

- clean clone 后一条命令检查/测试、一条命令构建。
- 最小应用可安装、启动和关闭。
- 无数据库服务器、本地 HTTP 服务、sidecar、图表库或重复状态框架。
- Tauri 时 capability/IPC 为显式最小集合。
- 生产依赖和 M1 轻量基线满足预算。

通过后提交、合并并推送 `main`，创建并推送 annotated tag `m1-stack-selected`。任一候选栈都未通过时不得提交虚假选择。
