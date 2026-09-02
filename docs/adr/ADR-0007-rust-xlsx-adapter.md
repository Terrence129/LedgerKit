# ADR-0007：Rust XLSX 读写适配器

> 状态：Accepted
>
> 日期：2026-09-02
>
> 决策者：项目所有者
>
> 授权：任务 04 授权根据双栈样机证据接受最终 Excel 读写适配器。
>
> 关联规则/里程碑：M1、M3、M5、ADR-0001

## 背景

P0 只需要读取已知 LedgerKit 迁移模板并创建新的标准化 XLSX 导出，不承诺原地修改或完整保真任意工作簿。Tauri 样机比较了 Rust `calamine`/`rust_xlsxwriter` 与前端 TypeScript `ExcelJS`；两者均读取 10,000/10,000 行，但 Rust 路径为 66.000 ms，TypeScript 路径为 464.7339 ms 且 Node 基准 RSS 增量为 93,171,712 bytes。

## 决策驱动因素

- Excel 被视为不可信输入，解析不能扩大 WebView 权限
- 财务字符串和 staging 只能进入一个 Rust Core 信任边界
- 已知模板兼容性、UI 不冻结、导入和导出性能
- 依赖体积、许可证和单人维护成本

## 方案

### 方案 A：Rust `calamine` + `rust_xlsxwriter`

在 Infrastructure 的 `ExcelPort` 实现内完成流式读取和新文件导出。UI 只持有平台文件选择产生的一次性授权 token，解析在 Rust blocking worker 上运行。

### 方案 B：前端 `ExcelJS`

兼容共享样例，但把大型不可信解析器和工作簿对象模型放入 WebView，增加内存、npm 传递依赖和跨边界数据面；财务规则仍不能由 TypeScript 承担。

### 方案 C：Office 自动化或原地编辑

依赖本机 Office、引入进程/COM 生命周期和格式保真承诺，不符合 P0 的离线、轻量与可测试边界。

## 决策

接受方案 A：

- 已知模板读取使用 `calamine = 0.36.1`；标准化新文件导出使用 `rust_xlsxwriter = 0.99.0`，均锁精确版本且为 MIT 许可证。
- 两个库只存在于 Rust Infrastructure；Domain/Application 通过项目定义的 `ExcelPort` 与规范 staging DTO 访问，不直接依赖 XLSX 类型。
- 读取必须限制文件大小、工作表、列、行数、字符串长度、日期与 Decimal 格式；解析在 worker 线程进行，取消或失败不能产生正式账本写入。
- 不支持宏执行、公式求值、外部链接加载、远程内容、任意模板或原地覆盖源文件。原始 Excel 永不修改。
- 导出始终写到新文件，且只使用由 Core 提供的规范值。

## 后果

- 正面影响：解析与财务授权保持在 Rust 边界内；共享 10k 样例性能余量大；前端不增加 Excel 生产依赖。
- 负面影响：读写由两个专用库承担；复杂 Excel 特性必须明确拒绝或通过兼容性测试后扩展。
- 数据迁移影响：M3/M5 必须建立 staging、问题清单和对账，不能把 XLSX 行直接写入正式库。
- 测试与运维影响：库升级必须重跑 10k fixture、所有已知模板变体、恶意/损坏输入、导出重开和内存上限。

## 反转条件

若任一库无法稳定处理已审阅模板或产生标准化导出，先评估同一 Rust 信任边界内的替代库。只有 Rust 路径在体积、兼容性或维护上明确失败且新的 ADR 证明 WebView 解析仍不复制财务规则、不扩大权限时，才考虑 TypeScript 适配器。

## 验证

- 脱敏 fixture：`fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx`
- 自动化测试：10,000 行/哈希、Decimal 字符串、损坏输入和标准化导出重开测试
- 性能/体积/恢复验证：Tauri 样机导入 66.000 ms、导出 25.6141 ms
- 对账或差异桥：M3/M5 staging 对账矩阵

## 关联

- 开发计划书章节：10、13–15
- 被替代/关联 ADR：ADR-0001、ADR-0003、ADR-0011
- 相关 issue/commit：M1 技术栈选择完成提交
