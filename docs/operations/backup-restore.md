# Backup, Restore and Upgrade Runbook

> 状态：M6 已实现；加密格式由 Accepted ADR-0008 固定，自动保留、RPO 声明与恢复密钥由 Accepted ADR-0013 固定。

## 数据边界

- 正常运行只有一个活跃的权威 SQLite 账本。
- 活库位于操作系统本地应用数据目录，不直接放在 OneDrive、Dropbox、NAS 或网络盘。
- 同步盘可以保存已经完成、关闭并验证的备份包。
- P0 备份包包含数据库、设置和 manifest；附件链接文本在数据库中，托管附件正文属于后续范围。
- 活库使用标准 SQLite，不使用 SQLCipher；便携备份必须遵守 [`ADR-0008`](../adr/ADR-0008-live-database-and-portable-backup-encryption.md) 的 `ledgerkit-portable-backup/v1` 随机 data-key、Argon2id 和双层 AES-256-GCM 格式。

## 创建备份

### 用户入口

1. 打开“设置与数据 → 备份、恢复与导出”，输入至少 12 字符的独立备份密码。
2. “创建便携备份”通过一次性原生保存对话框写出 `.lkbackup`；WebView 不接收路径。
3. “选择外部目录并备份”通过一次性原生目录对话框创建并验证首个包，随后启用本会话自动备份。应用重启后密码状态重新变为 locked，需再次输入密码创建备份才能恢复本会话自动备份。
4. 只有面板显示最近 24 小时内的 verified success、且最近尝试未失败时，才把设备丢失状态视为 protected。密码遗失无法恢复。

### Core 顺序

1. 识别账本 ID、schema、应用版本和当前事件/投影水位。
2. 使用 SQLite Online Backup API、`VACUUM INTO` 或等价一致性快照机制创建临时数据库；禁止直接复制正在写入的活库文件。
3. 对临时数据库运行可打开性、`integrity_check`、外键和 schema/version 检查。
4. 生成包含格式版本、哈希、创建时间和内容清单的 manifest。
5. 使用 ADR-0008 的强制带认证加密格式：先由 Argon2id 派生 key-encryption key 包装随机 data key，再由 data key 加密 payload；密钥不得硬编码或写入日志。
6. 写入临时目标，完成后原子重命名为最终备份包。
7. 按保留策略轮换；任何失败必须持续显示，不得把失败状态当作已保护。

## 恢复

在首次设置或“设置与数据”中输入原备份密码，勾选替换确认并选择 `.lkbackup`。新设备无需预先创建空账本；已有活库时 Core 会先生成可打开的恢复前快照。界面只显示备份 ID、schema、水位和结果，不显示源路径或解密内容。

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
- 只有用户已配置外部备份目录、最近一次备份在 UTC 24 小时内验证成功、且成功后的最近尝试没有失败时，才声明设备丢失 RPO ≤ 24 小时。
- 未配置或失败时明确显示“设备丢失未受保护”。

## 保留与兼容矩阵

- 外部每日加密包保留 7 个，外部每周加密包保留 4 个；轮换只匹配应用生成的 daily/weekly 文件，不删除手动备份。
- 正常退出在本机应用数据目录保留 7 个一致性 SQLite 快照；它们不能证明设备丢失受保护。
- `ledgerkit-portable-backup/v1` 只接受 Argon2id v19 `m=65536 KiB, t=3, p=4` 与 AES-256-GCM。未知格式、算法或参数在 KDF 分配前拒绝。

| 包内 schema | 当前应用行为 |
|---|---|
| 1–6 | 在隔离候选上创建 migration 备份并只前进迁移到 v7，验证后再切换 |
| 7 | 直接验证、重建/核对投影并恢复 |
| >7 | 以 `SCHEMA_VERSION_TOO_NEW` 拒绝，活库不变 |

## 独立导出与隐私诊断

- 设置页在导出前展示隐私提示并要求显式勾选。XLSX、CSV 和 reconciliation JSON 可能包含私人字段与财务数值，应保存到受控位置。
- XLSX 以字符串单元格写出规范表；CSV 对以 `= + - @` 开头的文本增加前置单引号并始终引号转义，防止公式注入。
- diagnostics JSON 只包含 ledger ID、应用/schema/计算版本、事件/投影水位、稳定错误类别和表计数，不包含名称、备注、金额、路径、口令或密钥。
- 所有保存/选择均由一次性 native picker 授权；Core 再验证绝对路径、扩展名、父目录和“目标不存在”，不提供任意文件系统 IPC。

## 开发与复核命令

```powershell
cargo test --manifest-path app/src-tauri/Cargo.toml portable_backup
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
npm --prefix app run check
pwsh -NoProfile -File tools/check.ps1
```

## 必测失败场景

- 损坏、截断或被篡改的备份包。
- 错误密码、未知 KDF/格式版本和遗失口令说明。
- 备份 schema 高于当前应用支持版本。
- 磁盘空间不足、目标目录失效和同步冲突。
- migration 中途失败、投影重建不一致和恢复切换失败。
