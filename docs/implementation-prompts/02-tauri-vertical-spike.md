# 任务 02/14：Tauri M1 纵向样机

你正在执行 LedgerKit 的 Tauri M1 样机。遵循共同执行协议，确认 `main` 包含 `m0-baseline`，创建分支 `phase/m1-tauri-spike`。

用户已允许安装 Rust/Tauri 用户级工具链和必要 Windows 构建组件。只使用官方来源，记录实际版本，不提交安装器、缓存或构建产物。

## 必读材料

- 全部 M0 Accepted ADR 与黄金 fixture
- 开发计划书第 4、10–12、15–18 节
- 隐私、备份恢复与依赖约束

## 交付

在 `spikes/tauri` 建立一次性真实样机，不创建最终生产应用，不接受 ADR-0001，不改写黄金答案。

1. 从官方发布资料确认并锁定当前稳定 Rust、Tauri 2、React、TypeScript、Vite、SQLite 和候选 XLSX 库。
2. 使用 M0 fixture 完成同一纵向切片：
   - 受控打开 SQLite 和一次 schema migration。
   - 原子写入一项合成现金事件并刷新 posting/projection。
   - 读取确定性生成的 10,000 行已知模板合成 XLSX。
   - 展示分页交易列表、一个原生净资产条、11 行 Top 10+其他支出横条及同源语义表格。
   - 通过平台文件选择验证受限附件复制能力。
   - 导出标准化 XLSX。
   - 创建、验证并恢复最小密码加密备份。
   - 验证未授权命令、伪造 posting、路径越界和远程内容调用被拒绝。
   - 生成 Windows per-user 安装包。
3. 比较 Rust Excel 适配器和纯 TypeScript 适配器的已知模板兼容性、体积、许可证、维护成本与信任边界；即使使用 TypeScript 解析，财务规则也只能在 Rust Core。
4. 提交 10k 合成 XLSX 的确定性生成器、文件哈希和隐私说明，供 Avalonia 样机原样复用。
5. 测量标准薄安装包、安装后载荷、运行时内含包、直接生产依赖、Tauri 插件、IPC 数、首屏 gzip、冷启动 30 次、完整进程树 idle/peak RSS、关闭残留、导入/查询/写入/绘制耗时和 clean build 时间。

## 测试、验证与报告

- M0 黄金子集通过，规范结果哈希一致。
- 断网核心流程可用，默认网络请求为零。
- WebView 无任意 SQL、posting、shell、远程 URL 或通配文件能力。
- 安装、启动、关闭、卸载实际通过。
- 报告记录机器配置、方法、原始测量值和每项硬门禁的 pass/fail，不得调整口径美化失败。

将报告写入 `docs/benchmarks/m1/tauri.md`。通过样机自身检查后提交、合并并推送 `main`；不要选择技术栈、不要打 M1 tag、不要删除 spike。
