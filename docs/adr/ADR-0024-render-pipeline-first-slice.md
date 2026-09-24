# ADR-0024｜渲染管线第一刀：termai-render 的依赖位置与零新增依赖切片

| 项 | 内容 |
| --- | --- |
| **状态** | Accepted |
| **日期** | P0 Wave 2（WS-03 起桩） |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-01（混合分层：网格绝不经 Web）、AR-03（内核不依赖 AI/网络/UI）、AR-19、AR-23 §6（软换行=视觉裁剪）、AR-24 |
| **关联 DC** | DC-17（wgpu + rustybuzz/swash）、DC-21（依赖单向无环）、DC-25（受限控件集） |
| **关联 HARNESS 章节** | §4.2（L0–L4 分层）、§4.3（模块与依赖方向）、§5（帧时/RSS）、§7（P0 出口 E-P0-2/E-P0-3）、§8.1（G3/G4） |
| **关联 ADR** | ADR-0001（混合架构）、ADR-0014（GPU T0–T3）、ADR-0015（链接边界与准入）、ADR-0019（内核 crate 布局与准入）、ADR-0023（字段集冻结）、HARNESS §11.2 CR-15（命名口径） |

## 1. 背景与问题

P0 出口的两条（**E-P0-2** 三平台 IME/CJK 矩阵、**E-P0-3** 性能门禁进 CI）都依赖渲染管线（`kernel/03`）；而渲染管线目前**一行代码都没有**（M0 明确把 kernel/03 排除在切片外），是 P0 关键路径的头部。

整条管线一次性落地会立刻卡在两处**不该在第一步阻塞**的地方：

1. **第三方依赖准入需要 SPDX 证据**。DC-17 指定 wgpu / rustybuzz / swash，窗口需要 winit。按 ADR-0015，进入链接边界的依赖必须逐项给出 SPDX 与 A/R/D 判定，**未知即拒绝**；而当前环境无法取得这些 crate 的权威许可证元数据（网络受限、本机 cargo registry 无缓存）。此时"先写进去再说"直接违反 AR-21 / ADR-0015 与 AGENTS §7.4。
2. **管线里有一大块是纯逻辑、零新依赖**：渲染镜像（应用 `GridSnapshot` / `GridDelta`、rev 缺口自愈）、damage 到帧的输入、VisualRowMap（软换行/裁剪是**显示层**映射）。这些只依赖 `termai-core` 的既有 DTO，**不需要 wgpu/字体栈**。

失败代价：若因依赖未准入而整体搁置，P0 的关键路径就在等一个与本机环境无关的手续；若绕过准入直接引入，则违反不可协商的许可与供应链纪律。

## 2. 可选方案

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | 建立 `crates/termai-render`，第一刀只做**纯逻辑切片**（镜像 + rev 自愈 + damage 帧输入），依赖**仅** `termai-core`；第三方依赖**一个都不准入**；wgpu/字体栈/winit 留待后续 ADR（须附 cargo-deny 输出） | 见 §3 |
| B（否决） | 等依赖准入齐备再开渲染 | **否决**：关键路径被一个**与本机环境相关**的手续阻塞；镜像/VRM 的逻辑与 GPU 无关，等它们只是浪费 P0 关键路径 |
| C（否决） | 现在就把 wgpu/rustybuzz/swash/winit 写进 `[workspace.dependencies]`，"以后再补 ADR" | **否决**：ADR-0015 P3「未知即拒绝」是硬规则；无 SPDX 即无判定；这正是 AR-21 要防的供应链缺口 |
| D（否决） | 把镜像逻辑塞进 `termai-vt` 或 `apps/termai` | **否决**：违反 kernel/03 K-02 的跨进程切点（sessiond 不做 shaping/镜像，UI 不解析 VT）与 DC-21；也会把渲染状态与终端真源焊死 |

## 3. 决策

### D1｜新增 `crates/termai-render`，本切片依赖位置 = 仅 `termai-core`

- HARNESS §4.3 的 `term-render(→vt,gpu)` 中，`termai-vt` 与 `termai-gpu` 两条边**本切片不建立**（不用就不声明，避免"声明了却没用"的假依赖）；它们在 **shaping 切片**（需要 vt 的唯一列宽表与 ClusterTable 语义）与 **GPU 切片**（termai-gpu）落地时各自登记，并同步 K4 允许边。
- 依 AGENTS §3：本 ADR 即为该 crate 的依赖位置登记；K4 允许边表增加 `termai-render -> [termai-core]`；CODEOWNERS 增加 `/crates/termai-render/` → **T1 Core Kernel**（spec 07 §3.1.2）。
- 本切片**不产生任何像素**，因此**不**主张 G3 视觉回归、帧时、网格对齐任何一条通过。

### D2｜软换行/裁剪属 `termai-render`，且必须**零 GridDelta**

- `WrapMode { Fold, Clip }` 与 VisualRowMap 归本 crate；切换**不得**产生 GridDelta、不得改 rev、不得改列数与复制字节（AR-23 §6 / kernel/03 K-10 / RP-08）。
- **前置缺口已登记**：当前 core DTO 的 `GridSnapshot` / `RowPayload` **没有逐行 LineFlags（WRAPPED）**，无法把网格行链成"逻辑行"，因此 VRM **本切片不实现**（见 `docs/plan/p0-spec-defects.md` **SD-13**）。不许用"按列宽猜折行"之类近似替代——那会直接破坏 A20/UX-G17 的复制保真。

### D3｜第三方依赖准入：本次**零准入**

- wgpu / winit / rustybuzz / swash / fontdb 等**仍未准入**（ADR-0015 P3）。它们必须由后续 ADR 逐项登记 SPDX 与 A/R/D 判定，并附 `cargo-deny` 输出；本 ADR 不构成任何准入。
- `[workspace.dependencies]` 本次**不新增任何第三方条目**。

## 4. 理由

1. **A1/A2**：把"能做且该先做"的逻辑与"需要供应链证据"的依赖解耦，既不越过许可红线，也不让 P0 关键路径空转。
2. **A3**：镜像与 rev 自愈正是"状态结构化"在 UI 侧的落点；它先冻结**应用顺序**（scroll → payload → cursor），shaping/GPU 才能并行接入。
3. **可判定**：本切片可用纯 Rust 单测判定（rev 缺口、陈旧 rev、scroll 行槽位重映射、payload 覆盖），**不需要参考机、不需要 GPU**，符合"每一步留下可执行证据"。

## 5. 后果

### 正面
- P0 关键路径（渲染）有了第一个**可测**落点，且不引入任何新的供应链风险。
- `GridDelta` 的应用顺序与失败语义被实现并测试冻结，后续 UI/插件只读镜像可复用同一实现。
- 顺带把两个契约缺口变成**有编号的登记**（SD-13 逐行 LineFlags 缺失、SD-14 scroll 字段重复），而不是实现者的口头约定。

### 负面（必须接受的代价）
1. 本切片**看不到任何像素**：E-P0-2 / E-P0-3 的状态不因本 ADR 改变（仍为未实现/未判定）。
2. `termai-render` 的 `→vt` 边尚未建立，意味着"唯一列宽表"的接线要等 shaping 切片；在此之前该 crate **不得**做任何宽度计算。
3. 新增 crate 增加了 K4/K7 的维护面（每次新增都要同步 ADR + 门禁 + CODEOWNERS），这是 AGENTS §3 有意保留的流程摩擦。

## 6. 反方记录与复议条件

- **反方（急进派）**：先把 wgpu 拉进来跑通一个像素，比写"看不见的镜像"更能推进 P0。
  **驳回理由**：无 SPDX 判定就引入 wgpu 及其传来的依赖树（naga 等）属 ADR-0015 明确禁止的"未知即准入"；且没有镜像与 rev 语义，GPU 侧一接上就要重写应用顺序。
  **复议条件**：若取得 cargo-deny/SPDX 证据并落地后续准入 ADR，可立即并行推进 GPU 切片；本 ADR 的 D1 允许在该时点把 `→gpu` 边与本切片合并。
- **反方（保守派）**：VRM 未落地就不该先建 crate。
  **驳回理由**：镜像是 VRM 与 GPU 的公共前置；SD-13 的字段补充属"实现对齐已冻结设计（ADR-0023 D3）"，不应阻塞镜像。
  **复议条件**：若 SD-13 的字段方案最终被否（例如改为由 sessiond 直接下发逻辑行分组），则 VRM 落点需重评，但镜像本身不受影响。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| D1 crate | `crates/termai-render/`（Cargo.toml 依赖 `termai-core`）；根 `Cargo.toml` 的 `[workspace] members` 与 `[workspace.dependencies]`；K4 允许边 `termai-render -> [termai-core]`；CODEOWNERS `/crates/termai-render/` |
| D1 本切片 | `src/mirror.rs`（GridSnapshot/GridDelta 应用 + rev 自愈 + damage 帧输入）+ 单测 |
| D2 VRM | 待 **SD-13** 冻结逐行 LineFlags 后落地；本 ADR 只定归属与"零 GridDelta"约束 |
| D3 准入 | 无；后续 ADR 须附 `cargo-deny` 输出 |
