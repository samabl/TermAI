# ADR-0025｜GridSnapshot / RowPayload 的逐行 LineFlags 与 golden/digest 兼容（解 SD-13）

| 项 | 内容 |
| --- | --- |
| **状态** | Accepted |
| **日期** | P0 Wave 2（WS-03 渲染数据面） |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-23 §6（软换行 = 视觉裁剪）、AR-04（schema 版本化与兼容窗口）、AR-19 |
| **关联 DC** | DC-23（Log 真相 + 可重建索引）、DC-40（N-2 minor 兼容） |
| **关联 HARNESS 章节** | §4.3（依赖方向）、§7（P0 出口 E-P0-2）、§8.1（G2/G3） |
| **关联 ADR** | ADR-0023 D3（字段集以 `kernel/03` §3.3 为准）、ADR-0024 D2（VRM 归 termai-render） |
| **关联登记** | `docs/plan/p0-spec-defects.md` **SD-13**、**SD-14** |

## 1. 背景与问题

`kernel/03` §3.8 定义「逻辑行 = 由 `LineFlags::WRAPPED` 串起来的网格行链」，§3.3 的 `LineRecord` 亦携带 `LineFlags`。但 M0 落地的 core DTO v1（`crates/termai-core/src/grid.rs`）只有全局 `wrap_pending`，`RowPayload` 只有 `row` + `cells` —— **没有任何逐行标志**（SD-13）。

后果有三层，且都不轻：

1. **VRM 不可实现**：软换行/裁剪（VisualRowMap，AR-23 §6 / K-10）要求把网格行链成逻辑行；没有 WRAPPED 就无法重建，**RP-08**（切 WrapMode 产生 0 个 GridDelta 且两态复制逐字节相同）与 **UX-G17** 无法判定。
2. **digest 看不见折行**：`GridSnapshot::canonical_bytes` 只覆盖 cells/cursor/modes/scroll/title/backend。**一个硬折行的行链与两个独立行可以产生相同 digest**，于是 `ATTACH_ACK.snapshot_ref.grid_digest`（ADR-0023 D1 已落地）无法用于验证「屏幕一致」（E-P0-4）。
3. **golden 抓不到折行回归**：golden 快照同样不含折行状态。

ADR-0024 D2 已把 VRM 归入 `termai-render` 并明确「等逐行 flags 之后落地」。本 ADR 就是那个前置决定。

## 2. 可选方案

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | `GridSnapshot` 增 `row_flags: Vec<u16>`（长度 = rows）、`RowPayload` 增 `flags: u16`；定义 `LINE_WRAPPED = 1<<0`；**canonical_bytes 纳入逐行 flags**；`GRID_DTO_MINOR` 1→2；golden 保持 `TERMAI-GRID 1` 可解析（缺 flags 视为全 0） | 见 §3 |
| B（否决） | 用单元格哨兵值标记折行 | **否决**：会污染内容与复制结果，直接违反 A20 / UX-G17 的逐字节复制承诺；也破坏「宽字符尾部固定为 WIDE_CONTINUATION」的既有不变量 |
| C（否决） | 用按行键控的侧表（仿 ClusterTable） | **否决**：逐行标志是**定宽**的，平行向量更简单、无查找开销；侧表留给**变长**的 cluster 文本（`kernel/03` §3.3 的原意）。两者机制不同，不应合并 |
| D（否决） | 加字段但不纳入 canonical_bytes / digest | **否决**：那样 digest 仍分不清行链，E-P0-4 的「屏幕一致」无法用 digest 校验，golden 也抓不到折行回归——字段加了却解决不了第 1.2/1.3 条 |

## 3. 决策

### D1｜字段
- `GridSnapshot` 增 `row_flags: Vec<u16>`，长度必须等于 `rows`；解析 v1 文档（无该字段）时视为**全 0**。
- `RowPayload` 增 `flags: u16`。
- 位定义：`LINE_WRAPPED = 1 << 0`（该行被 DECAWM 硬折行，与下一行同属一个逻辑行）。**其余位保留**：写入方必须写 0；读取方**忽略未知位**（DC-40 的 N-2 前向兼容，不得因未知位失败）。

### D2｜digest / golden 兼容
- `canonical_bytes` 在 cell 区块之后追加逐行 flags（row-major、u16 小端）。**这是 digest 语义变更**，因此 `GRID_DTO_MINOR` 由 1 升到 **2**（minor 只增字段，DC-40）。
- golden `TERMAI-GRID 1` **保持可解析**：缺 flags 的行按 0 处理；**写出时始终带 flags**。既有 golden 期望值需要重新生成，并在提交信息与 SD-13 处置里显式记录（**不得**以「哈希变了就改测试」了事：必须说明这是 D2 的有意契约变更）。
- 任何跨版本比较 digest 的地方（attach 校验、回放断言）必须按版本处理，不得跨 minor 直接比。

### D3｜消费方
- `termai-render` 用 `row_flags` 重建逻辑行并落地 VRM（Fold/Clip）；VRM **不得**产生任何 GridDelta、不得改 rev、不得改列数与复制字节（AR-23 §6 / K-10 / RP-08）。
- `termai-vt` 是 flags 的产生方（唯一的列宽与折行真源，K-04）；`termai-render` 不得自行推断折行。

### D4｜边界
- 本 ADR **不新增任何第三方依赖**；不改变 SD-14 的 scroll 双承载处置（仍以 `GridDelta.scroll` 为准）。

## 4. 理由
1. **A2（正确性优先）**：折行是网格语义的一部分；把它排除在 digest 之外，等于让「屏幕一致」这条 P0 承诺只能靠人工比对。
2. **A3（状态结构化）**：逻辑行是 AI 上下文、复制与命中测试的共同基础；结构化到 DTO 里，三个消费方才能共享同一真源。
3. **可判定**：本决策让 RP-08 与 UX-G17 从「不可判定」变为可写用例；且全部可用纯 Rust 单测判定，不需要 GPU 或参考机。
4. **代价可控**：minor 字段新增 + digest 版本升级，落在 DC-40 的兼容窗口内；没有引入新机制。

## 5. 后果

### 正面
- VRM 的前置被解除；RP-08 / UX-G17 可写用例。
- digest 覆盖折行状态，`ATTACH_ACK.snapshot_ref.grid_digest` 第一次真正能用于校验「客户端与 sessiond 看到同一屏」。
- 逐行 flags 为后续（bidi 行标记、提示符行归属）留下位空间，而不必再动一次 DTO。

### 负面（必须接受的代价）
1. **digest 语义变更**：既有 golden 期望值与任何持久化的 digest 需重新生成；`GRID_DTO_MINOR` 升到 2。
2. **实现面扩大**：`GridSnapshot::new`、canonical 编解码、golden 读写、`termai-vt` 的 Grid 都要同步；漏一处就会出现「flags 全 0」的静默退化——必须由用例锁住非零 flags 的往返。
3. 与 W1-B 的 G1 一致性语料存在**一次性的 golden 重生成**，需要在其收口后执行，避免两边同时改 golden。

## 6. 反方记录与复议条件

- **反方（保守派）**：折行开关是显示层的事，不该进网格 DTO。
  **驳回理由**：**硬**折行（DECAWM）本身就是网格层行为（`kernel/03` §3.3 / `kernel/01` 的 `LineFlags::WRAPPED`）；进 DTO 的是硬折行链，**不是**软换行开关。软换行仍纯属显示层（K-10）。
  **复议条件**：若设计改为「由 sessiond 直接下发逻辑行分组」而网格行不再暴露折行链，可复议撤销逐行 flags（届时须同时修订 `kernel/03` §3.3/§3.8 与 RP-08 的判定载体）。
- **反方（兼容派）**：改 digest 会破坏既有回放基线。
  **驳回理由**：M0 的字段集本来就是**临时**冻结（ADR-0023 D3 之前的状态），且 AR-04/DC-40 明确允许 minor 增字段；真正的风险是"偷偷改哈希"，本 ADR 用显式版本升级 + 重生成记录来消除它。
  **复议条件**：若重生成暴露出无法解释的差异（非 flags 引起的哈希漂移），先停下来定位，不得直接覆盖基线。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| D1 字段 | `crates/termai-core/src/grid.rs`（`row_flags` / `RowPayload.flags` / `LINE_WRAPPED` / `GRID_DTO_MINOR=2`） |
| D2 digest | 同文件的 `canonical_bytes`；`crates/termai-vt` 的 golden 读写与语料重生成 |
| D3 产生方 | `crates/termai-vt`（Grid 在 DECAWM 折行时置位；列为唯一真源） |
| D3 消费方 | `crates/termai-render`（逻辑行重建 + VRM，下一切片） |
| 登记回填 | `docs/plan/p0-spec-defects.md` SD-13 处置更新为「ADR-0025」；SD-14 不变 |
