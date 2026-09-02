# ADR-0008：P0 活库与密码加密便携备份

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：任务 04 明确授权 P0 活库不使用 SQLCipher，并要求根据样机固定密码加密便携备份方案。
>
> 关联规则/里程碑：M1、M6、ADR-0002

## 背景与威胁模型

P0 需要应对设备丢失、备份或同步目录泄露以及离线口令猜测。应用级数据库加密不能可靠防止同一已登录 OS 账号下、能够读取进程内存或键盘输入的高权限恶意进程；活库仍依赖 Windows 账号隔离和建议启用的全盘加密。便携备份必须在离开应用本地数据目录前独立加密，且不能依赖设备安全存储才能恢复。

Tauri 样机证明 RustCrypto `argon2` + `aes-gcm` 可完成加密备份、正确密码恢复、错误密码拒绝、明文标记缺失和失败时不改变活库。生产格式在此基础上增加随机 data key 封装，并采用标准参数来源。

## 决策驱动因素

- 便携、跨设备恢复和错误口令/篡改安全失败
- 不自创密码学原语，不记录口令或密钥
- 格式、算法和参数可版本化且可安全拒绝未知值
- 对安装体积、恢复流程和单人维护成本可控

## 方案

### 方案 A：P0 SQLCipher 活库 + 加密备份

扩大 SQLite 构建、migration、密钥生命周期和恢复面，但不能覆盖同账号进程威胁；样机没有证明该成本必要。

### 方案 B：普通 SQLite 活库 + 独立加密便携备份

保持标准 SQLite 的可验证性和轻量性；依赖 OS 磁盘/账号保护活库，对离开本地目录的备份使用口令 KDF、随机 data key 和 AEAD。

### 方案 C：只压缩或只加密、无认证

不能抵抗泄露、篡改或错误口令，拒绝。

## 决策

接受方案 B，并固定 `ledgerkit-portable-backup/v1`：

1. 活库使用标准 bundled SQLite，不在 P0 引入 SQLCipher。UI 必须说明应用锁不等于磁盘加密，并建议启用 Windows 全盘加密。
2. 包为 UTF-8 JSON envelope。公开 header 只含格式版本、KDF/AEAD 标识和参数；账本 ID、schema、应用版本、创建时间、文件清单与 SHA-256 位于加密 payload 内。二进制字段使用标准 Base64。
3. 每个包由 OS CSPRNG 生成独立的 256-bit data key、128-bit salt、96-bit key-wrap nonce 和独立的 96-bit payload nonce。nonce 不得在同一 key 下复用。
4. 口令经 Argon2id v=19 派生 256-bit key-encryption key。`v1` 参数固定为 `m=65536 KiB, t=3, p=4`，salt 128 bit；这是 [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106.html) 面向内存受限环境的第二推荐配置。读取器先匹配已知格式/参数注册表再分配内存，不接受攻击者任意放大的参数。
5. key-encryption key 使用 AES-256-GCM 包装随机 data key；data key 使用独立 AES-256-GCM 加密完整 payload。两层都使用 128-bit authentication tag，并把规范 header bytes 作为 AAD；GCM 的认证加密语义依据 [NIST SP 800-38D](https://csrc.nist.gov/pubs/sp/800/38/d/final)。任一 header、wrapped key、ciphertext 或 tag 变化都必须失败。
6. 实现依赖锁定为 `argon2 0.6.0`、`aes-gcm 0.11.1`、`getrandom 0.4.3`、`zeroize 1.9.0`、`base64 0.23.1`；序列化复用 `serde/serde_json`。data key、派生 key 和口令缓冲在可行范围内主动清零，不写日志、数据库或诊断包。
7. 创建顺序是 SQLite 一致性快照 → 数据库验证 → 组包/加密到临时文件 → 重新解密并验证 manifest/数据库 → 原子发布。恢复始终在候选位置完成认证、哈希、schema、完整性、外键和投影验证，再创建恢复前备份并切换；失败保留活库。
8. 遗失口令不可恢复。设备安全存储未来只能保存便利副本，不得成为包的唯一恢复秘密。

样机使用较低的临时 Argon2 参数，证明库与恢复边界，不作为生产参数。M6 必须在目标低配设备实测上述 `v1` 参数；若无法满足恢复门禁，只能通过新的格式/策略版本和 ADR 调整，不能静默降低已创建包的参数。

## 后果

- 正面影响：便携包泄露不会直接暴露 SQLite；篡改和错误密码在切换前失败；活库保持标准 SQLite 工具链。
- 负面影响：未启用全盘加密时，活库静态文件仍可能在设备丢失后暴露；遗失密码无法恢复；双层 AEAD 增加实现和测试要求。
- 数据迁移影响：格式版本、schema 和计算版本必须兼容；未知版本只读拒绝，不尝试猜测。
- 测试与运维影响：M6 必须覆盖随机性、nonce 唯一性、header/payload/wrapped-key 篡改、错误/遗失口令、资源上限、回滚和跨版本恢复。

## 反转条件

若威胁模型新增未解锁设备上的高敏活库保护、企业合规或多用户隔离，并且 SQLCipher PoC 能通过体积、migration、备份和恢复门禁，可由新 ADR 引入新的活库格式。Argon2/AES-GCM 参数或库出现实质安全问题时创建新格式版本；旧包保持只读迁移路径，不原地重解释。

## 验证

- 脱敏 fixture：`fixtures/sanitized/20-encrypted-backup/`
- 自动化测试：Tauri 样机恢复/错误口令；M6 增加完整格式与篡改矩阵
- 性能/体积/恢复验证：M6 在参考低配设备验证 KDF 延迟、内存和 RTO ≤ 10 分钟
- 对账或差异桥：恢复后 canonical event/posting/projection hash 与备份 manifest 一致

## 关联

- 开发计划书章节：12、15、17
- 被替代/关联 ADR：ADR-0002、ADR-0010、ADR-0013
- 相关 issue/commit：M1 技术栈选择完成提交
