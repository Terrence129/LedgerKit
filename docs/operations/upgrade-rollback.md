# Manual Upgrade and Rollback Runbook

> 状态：M6 Beta 手动升级流程；受 Accepted ADR-0010、ADR-0008 和 ADR-0013 约束。

## 边界

- P0/Beta 不含自动更新器、后台下载或启动联网检查。安装包未签名，用户必须从项目公开发布渠道手动获取并自行核对发布哈希。
- 活库只做只前进 migration，不自动降级。旧应用不得打开高于其支持版本的数据库。
- 升级、回滚和恢复都不得修改 Excel 原始迁移源；真实账本、备份、诊断和截图不得进入公开仓库。

## 升级前

1. 在旧版本“设置与数据 → 备份、恢复与导出”创建密码加密便携备份。
2. 确认结果为 verified，并把包和密码分开保存。若使用外部目录，确认设备丢失状态为 protected。
3. 正常退出 LedgerKit；退出过程另创建一致性本机快照。
4. 记录旧应用版本、支持 schema、备份 ID 和 canonical posting hash；不得记录路径、口令或私人字段。

## 手动升级

1. 退出应用后运行新 NSIS 安装包；不要同时运行两个 LedgerKit 进程。
2. 首次打开先只读识别 `application_id` 和 `user_version`。若 schema 较旧，Core 先创建并验证 migration backup，再在单 transaction 中迁移。
3. 通过 integrity、foreign-key、必需对象、schema hash、投影重建和水位检查后才开放账本。
4. 核对 ledger ID、事件水位与 canonical posting hash；失败时不要重复覆盖安装或手工改库。

## 回滚

- 若新版本尚未迁移数据库，可退出后重新安装旧版本。
- 若数据库已经升级到旧版本不支持的 schema，禁止直接用旧二进制打开。安装能够读取备份 schema 的版本，并从升级前 `.lkbackup` 恢复；恢复在候选位置验证并保留当前库的恢复前快照。
- 任一恢复失败都保留当前活库。不要复制正在写入的 `.sqlite3` 文件、删除 WAL/SHM、手工改 `user_version` 或编辑 manifest。

## 当前兼容矩阵

| 应用支持 | 打开/恢复行为 |
|---|---|
| Schema v6 | 新建 v6；v1–v5 只前进迁移；恢复 v1–v6 包 |
| Schema v1–v5 应用 | 不得打开或降级 v6；从该旧应用自己创建的兼容备份恢复 |
| 包 schema >6 | 当前应用拒绝并保持活库不变 |

## 验证命令

```powershell
pwsh -NoProfile -File tools/check.ps1
pwsh -NoProfile -File tools/build.ps1
```

任务 14 必须在独立测试位置完成安装、升级、恢复、卸载残留与未签名提示检查，并记录安装前后 canonical hash；没有该证据不得标记 Beta ready。
