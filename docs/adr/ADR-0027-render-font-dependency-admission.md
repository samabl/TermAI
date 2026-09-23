# ADR-0027｜渲染/字体的第三方依赖准入（**Proposed**：SPDX 证据待补）

| 项 | 内容 |
| --- | --- |
| **状态** | **Proposed**（按 ADR README §3：**不可实施**） |
| **日期** | P0 Wave 2 |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-01、AR-03、AR-14、AR-21、AR-24 |
| **关联 DC** | DC-17（wgpu + rustybuzz/swash）、DC-21（依赖单向无环） |
| **关联 HARNESS 章节** | §4.3（模块与依赖方向）、§5（帧时/RSS）、§7（E-P0-2/E-P0-3）、§8.1（G5） |
| **关联 ADR** | ADR-0014（GPU T0–T3）、ADR-0015（链接边界与 SPDX 判定）、ADR-0019（准入先例）、ADR-0024（渲染第一刀/零准入）、HARNESS §11.2 CR-15 |

## 1. 背景与问题

E-P0-2 与 E-P0-3 都依赖渲染管线，而 **DC-17 指定的 wgpu / rustybuzz / swash 与窗口所需的 winit 都尚未准入**（ADR-0024 D3 明确「本次零准入」）。按 ADR-0015：进入链接边界的第三方单元必须逐项给出 **SPDX + A/R/D 判定**，**未知即拒绝**（P3）。

**当前环境无法取得这些 crate 的权威许可证元数据**（网络被限制到非公网地址；本机 cargo registry 无这些 crate 的缓存）。因此本 ADR **只能把准入做成一张待填的表 + 一条可执行程序**，而不是替谁拍板许可证——那正是 AR-21 与 ADR-0015 要防的供应链缺口。

## 2. 可选方案

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（本 ADR）** | 立一份 **Proposed** 准入 ADR：候选清单 + 判定档位 + 验收程序都写好，**SPDX 列留空并标注「待 cargo-deny 输出」**；在表填满并把状态改为 Accepted 之前，**任何 crate 不得进入 workspace 依赖** | 采纳 |
| B（否决） | 按「常识」填写 SPDX | **否决**：ADR-0015 P3 明文「不得以『作者说是 MIT』放行」；猜许可证就是把不可事后补救的风险写进产物 |
| C（否决） | 先合入依赖、过后补 ADR | **否决**：AR-21/ADR-0019 的准入纪律正是「ADR 先于合并」 |
| D（否决） | 放弃自绘 GPU，改用 WebView 渲染网格 | **否决**：直接违反 AR-01（字符网格绝不走 WebView），不可协商 |

## 3. 决策（Proposed 状态下的可执行内容）

### D1｜候选清单与链接边界判定档位（SPDX 待填）

| crate | 用途 | 边界判定（ADR-0015） | SPDX |
| --- | --- | --- | --- |
| `wgpu` | GPU 网格与外壳绘制（DC-17） | LB-01 / LB-05（运行时 + 静态链接）→ **待判定** | **待 cargo-deny 输出** |
| `winit` | 原生窗口与事件循环（仅 apps/termai-desktop） | LB-01 / LB-05 → **待判定** | **待填** |
| `rustybuzz` | 唯一 OpenType shaping 引擎（kernel/03 K-05） | LB-01 / LB-05 → **待判定** | **待填** |
| `swash` | 唯一栅格化/彩色字形引擎（kernel/03 K-05） | LB-01 / LB-05 → **待判定** | **待填** |
| `fontdb` | 字体发现/回退链 | LB-01 → **待判定** | **待填** |
| 传递依赖（如 `naga`） | 由 `cargo tree -e normal` 枚举 | 逐项按 LB-01/LB-05 | **待填** |

> **P3 规则**：任一候选若出现 `NOASSERTION` / `LicenseRef-*` / 无许可证文本 / 来源不明 → **拒绝**，并在本表登记拒绝理由，不得替换为「另一个同名 crate」。

### D2｜依赖位置与方向（立项义务，AGENTS §3）

- 新增 **`crates/termai-gpu`**（T1）：GPU 后端探测、T0–T3 降级、draw/present。
- `crates/termai-render` 的 **`→vt` 与 `→gpu` 两条边**在此登记（ADR-0024 D1 暂缓的就是这两条）：`termai-render -> [termai-core, termai-vt, termai-gpu]`。
- **`apps/termai-desktop`（T2）** 承载窗口/事件循环与 IME 宿主；`apps/*` 永不被库依赖。
- 同步义务：K4 允许边、K7 CODEOWNERS、准入表**必须同 PR 更新**。

### D3｜验收程序（把 Proposed 升为 Accepted 的唯一路径）

1. 在可联网环境执行 `cargo deny check licenses advisories bans sources`，把输出与 `deny.toml` 作为证据；
2. 用 `cargo tree -e normal --no-dev` 枚举**发布闭包内**的全部第三方单元，逐项填 SPDX 与 A/R/D；
3. 任何一项为 R/D → 该候选**不得准入**，回到方案选型；
4. 表填满后把本 ADR 状态改为 **Accepted**，并在 24h 内同步 `kernel/03` / spec 07 与 K4 允许边（HARNESS §12）；
5. **在此之前**：workspace 依赖不得出现上述任何 crate（K5 不覆盖此点，靠本 ADR + CODEOWNERS 复核）。

## 4. 理由
1. **A1**：把「设计上已决定要用」（DC-17）与「供应链上尚未许可」两种状态分开记录，既不假装已准入，也不丢设计意图。
2. **A2**：wgpu/字体栈是帧时、网格对齐、DPI 与 CJK 回退的直接承载者；它们的选型不该被许可证手续**反向**决定，但许可证必须是**前置门**。
3. **可执行**：把准入从「一场讨论」变成「一次填表 + 一条命令」，使 E-P0-2 的第一个 PR 有明确入口。

## 5. 后果
### 正面
- 任何人拿到可联网环境都能在**不做设计决策**的前提下完成准入；
- 「未知即拒绝」在流程上被保住：表没填满时，依赖进不来。
### 负面（必须接受的代价）
1. **本 ADR 自身不可实施**；E-P0-2 的代码路径在表填满前**不能开始**（这是有意的，不是遗漏）。
2. 候选清单可能不完整（传递依赖只能由 `cargo tree` 枚举），因此 D3 第 2 步是**强制**的。
3. wgpu 的依赖闭包较大，G5（SBOM/cargo-deny）与 G4（RSS/帧时）都要重新评估，成本高于 M0 的 6 项依赖。

## 6. 反方记录与复议条件
- **反方（先跑起来派）**：先用 wgpu 写出一个可运行的窗口，再补许可证。
  **驳回**：一旦产物分发出去，许可问题**不可事后补救**（ADR-0015 背景已写明）。
  **复议条件**：无——除非 AR-21 或 ADR-0015 被 TSC 按章程修订。
- **反方（换技术栈派）**：若某候选被判 D，是否改用软件光栅？
  **复议条件**：若 wgpu 被判 D，可按 ADR-0014 的 T2/T3 降级路径评估软件光栅作为**唯一**后端，但那就变成一次新的架构选择（AR-01/DC-17 需 ADR 修订），不得作为「顺手降级」。

## 7. 关联决策与实现位置
| 项 | 落点 |
| --- | --- |
| 准入表 | 本文件 §3 D1；填满后同步 ADR-0019 的登记方式 |
| 新 crate | `crates/termai-gpu`（T1）；`apps/termai-desktop`（T2） |
| 允许边 | `tools/kernel-gates/check.mjs` 的 ALLOWED_LIB_EDGES |
| CODEOWNERS | `/crates/termai-gpu/`、`/apps/termai-desktop/` |
| 复核 | G5（cargo-deny + SBOM）；`cargo tree -e normal --no-dev` 反向依赖检查 |