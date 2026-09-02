# Backup, Restore and Upgrade Runbook

> 状态：设计期 runbook；加密边界由 Accepted ADR-0008 固定，应用实现后补充实际命令和版本兼容矩阵。

## 数据边界

- 正常运行只有一个活跃的权威 SQLite 账本。
- 活库位于操作系统本地应用数据目录，不直接放在 OneDrive、Dropbox、NAS 或网络盘。
- 同步盘可以保存已经完成、关闭并验证的备份包。
- P0 备份包包含数据库、设置和 manifest；附件链接文本在数据库中，托管附件正文属于后续范围。
- 活库使用标准 SQLite，不使用 SQLCipher；便携备份必须遵守 [`ADR-0008`](../adr/ADR-0008-live-database-and-portable-backup-encryption.md) 的 `ledgerkit-portable-backup/v1` 随机 data-key、Argon2id 和双层 AES-256-GCM 格式。

## 创建备份

1. 识别账本 ID、schema、应用版本和当前事件/投影水位。
2. 使用 SQLite Online Backup API、`VACUUM INTO` 或等价一致性快照机制创建临时数据库；禁止直接复制正在写入的活库文件。
3. 对临时数据库运行可打开性、`integrity_check`、外键和 schema/version 检查。
4. 生成包含格式版本、哈希、创建时间和内容清单的 manifest。
5. 使用 ADR-0008 的强制带认证加密格式：先由 Argon2id 派生 key-encryption key 包装随机 data key，再由 data key 加密 payload；密钥不得硬编码或写入日志。
6. 写入临时目标，完成后原子重命名为最终备份包。
7. 按保留策略轮换；任何失败必须持续显示，不得把失败状态当作已保护。

## 恢复

1. 不直接覆盖活库；先选择备份并读取非敏感 manifest。
2. 在受限资源分配前校验包版本和已知 KDF 参数注册表，再校验全部认证标签、口令、哈希和 schema 兼容性；未知格式/KDF/参数必须拒绝。
3. 解包到临时位置并验证数据库完整性、外键和必需对象。
4. 使用该临时库重建/验证关键投影和水位。
5. 创建当前活库的恢复前备份并验证可打开。
6. 关闭写入连接后原子切换；启动新库并再次运行健康检查。
7. 任一步失败都保留原活库，删除或隔离无效候选，不进入部分恢复状态。

## Schema 升级

顺序固定为：

```text
只读识别旧库
→ 创建并验证一致性备份
→ 在受控 transaction 中迁移
→ integrity/foreign-key/schema 检查
→ 必要的投影重建与财务校验
→ 正常开放账本
```

不得依赖数据库插件在应用 preload/open 阶段隐式自动迁移。

## 恢复目标

- 已提交 transaction 在进程崩溃后不丢失。
- 有效备份恢复目标 ≤ 10 分钟。
- 只有用户已配置外部备份目录且最近一次备份验证成功时，才声明设备丢失 RPO ≤ 24 小时。
- 未配置或失败时明确显示“设备丢失未受保护”。

## 必测失败场景

- 损坏、截断或被篡改的备份包。
- 错误密码、未知 KDF/格式版本和遗失口令说明。
- 备份 schema 高于当前应用支持版本。
- 磁盘空间不足、目标目录失效和同步冲突。
- migration 中途失败、投影重建不一致和恢复切换失败。
