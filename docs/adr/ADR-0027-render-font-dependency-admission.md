# ADR-0027｜渲染/字体的第三方依赖准入（**Accepted**：许可证通过；公告按 ADR-0031 的**时间盒例外**处置）

| 项 | 内容 |
| --- | --- |
| **状态** | **Accepted**（第 264 轮）——第 262 轮采证：许可证 / 来源 / bans 全部通过，**`advisories` 失败**（`rustybuzz` RUSTSEC-2026-0206、`ttf-parser` RUSTSEC-2026-0192）；所有者按 **ADR-0031** 采纳**时间盒例外**（登记于 `docs/audit/waivers.json` W-01，到期 2027-03-23 或 2 个 minor，先到者为准，且**到期即红**）。**准入解除**，但该例外在 SBOM / 发布说明中保持可见 |
| **日期** | P0 Wave 2 |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-01、AR-03、AR-14、AR-21、AR-24 |
| **关联 DC** | DC-17（wgpu + rustybuzz/swash）、DC-21（依赖单向无环） |
| **关联 HARNESS 章节** | §4.3（模块与依赖方向）、§5（帧时/RSS）、§7（E-P0-2/E-P0-3）、§8.1（G5） |
| **关联 ADR** | ADR-0014（GPU T0–T3）、ADR-0015（链接边界与 SPDX 判定）、ADR-0019（准入先例）、ADR-0024（渲染第一刀/零准入）、HARNESS §11.2 CR-15 |

## 1. 背景与问题

E-P0-2 与 E-P0-3 都依赖渲染管线，而 **DC-17 指定的 wgpu / rustybuzz / swash 与窗口所需的 winit 都尚未准入**（ADR-0024 D3 明确「本次零准入」）。按 ADR-0015：进入链接边界的第三方单元必须逐项给出 **SPDX + A/R/D 判定**，**未知即拒绝**（P3）。

~~**当前环境无法取得这些 crate 的权威许可证元数据**（网络被限制到非公网地址；本机 cargo registry 无这些 crate 的缓存）。~~ → **第 262 轮更正：crates.io 与 github.com 已可达**，SPDX 证据已采集（[ADR-0027-spdx-evidence.md](ADR-0027-spdx-evidence.md)）：269 个第三方包、**R=D=0、GPL/AGPL/SSPL 命中 0**、弱 copyleft 仅 `r-efi`。**因此本 ADR 的阻塞点已从「拿不到证据」变成「证据显示公告不合格」**——这恰恰是 AR-21 与 ADR-0015 要防的供应链缺口，且它在**依赖进入 workspace 之前**暴露。

## 2. 可选方案

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（本 ADR）** | 立一份 **Proposed** 准入 ADR：候选清单 + 判定档位 + 验收程序都写好，**SPDX 列留空并标注「待 cargo-deny 输出」**；在表填满并把状态改为 Accepted 之前，**任何 crate 不得进入 workspace 依赖** | 采纳 |
| B（否决） | 按「常识」填写 SPDX | **否决**：ADR-0015 P3 明文「不得以『作者说是 MIT』放行」；猜许可证就是把不可事后补救的风险写进产物 |
| C（否决） | 先合入依赖、过后补 ADR | **否决**：AR-21/ADR-0019 的准入纪律正是「ADR 先于合并」 |
| D（否决） | 放弃自绘 GPU，改用 WebView 渲染网格 | **否决**：直接违反 AR-01（字符网格绝不走 WebView），不可协商 |

## 3. 决策（Proposed 状态下的可执行内容）

### D1｜候选清单与链接边界判定档位（SPDX 待填）

| crate | 用途 | 边界判定（ADR-0015） | SPDX（版本；2026-09-23 采证） |
| --- | --- | --- | --- |
| `wgpu` | GPU 网格与外壳绘制（DC-17） | LB-01 / LB-05（运行时 + 静态链接） | **MIT OR Apache-2.0**（30.0.1）→ **A** |
| `winit` | 原生窗口与事件循环（仅 apps/termai-desktop） | LB-01 / LB-05 | **Apache-2.0**（0.30.13）→ **A** |
| `rustybuzz` | 唯一 OpenType shaping 引擎（kernel/03 K-05） | LB-01 / LB-05 | **MIT**（0.20.1）→ **A** |
| `swash` | 唯一栅格化/彩色字形引擎（kernel/03 K-05） | LB-01 / LB-05 | **Apache-2.0 OR MIT**（0.2.10）→ **A** |
| `fontdb` | 字体发现/回退链 | LB-01 | **MIT**（0.24.0）→ **A** |
| 传递依赖（`naga` 等 269 个第三方包） | 由 `cargo metadata` / `cargo tree -e normal` 枚举 | 逐项按 LB-01/LB-05/LB-03 | **见 [ADR-0027-spdx-evidence.md](ADR-0027-spdx-evidence.md)**：运行时闭包 236 + 构建期 33；**R=0、D=0**；黑名单（GPL/AGPL/SSPL）命中 **0**；弱 copyleft 仅 `r-efi`（`MIT OR Apache-2.0 OR LGPL-2.1-or-later`，按 LB-05 择 MIT/Apache 分支） |

> **P3 规则**：任一候选若出现 `NOASSERTION` / `LicenseRef-*` / 无许可证文本 / 来源不明 → **拒绝**，并在本表登记拒绝理由，不得替换为「另一个同名 crate」。
> **第 262 轮的 P3 判定（如实记录）**：采证中 **0 个** `NOASSERTION`/`LicenseRef-*`/无许可证文本。有 **16 个**包把 SPDX 写成 `MIT/Apache-2.0`（crates.io 历史写法，SPDX 未定义 `/`）——**它们不是「许可未知」而是「已知许可用了非标准分隔符」**：证据表同时给出 `strict tier = U` 与 `normalized tier = A` 两列，**归一化这一步不被隐藏**。按此判定它们**不构成 P3 拒绝项**；若 owner 认为 P3 必须按字面拒绝非标准书写，则应把 `/` 形式的包逐项转 `OR` 后重新采证。

### D2｜依赖位置与方向（立项义务，AGENTS §3）

- 新增 **`crates/termai-gpu`**（T1）：GPU 后端探测、T0–T3 降级、draw/present。
- `crates/termai-render` 的 **`→vt` 与 `→gpu` 两条边**在此登记（ADR-0024 D1 暂缓的就是这两条）：`termai-render -> [termai-core, termai-vt, termai-gpu]`。
- **`apps/termai-desktop`（T2）** 承载窗口/事件循环与 IME 宿主；`apps/*` 永不被库依赖。
- 同步义务：K4 允许边、K7 CODEOWNERS、准入表**必须同 PR 更新**。

### D3｜验收程序（把 Proposed 升为 Accepted 的唯一路径）

1. 在可联网环境执行 `cargo deny check licenses advisories bans sources`，把输出与 `deny.toml` 作为证据；**✅ 第 262 轮已执行**（cargo-deny 0.20.2，配置 = 仓库根 `deny.toml`）：`licenses ok / bans ok / sources ok`，**`advisories FAILED`**（两条 unmaintained）。完整输出见 [ADR-0027-spdx-evidence.md](ADR-0027-spdx-evidence.md) 的 cargo-deny 段。
2. 用 `cargo tree -e normal --no-dev` 枚举**发布闭包内**的全部第三方单元，逐项填 SPDX 与 A/R/D；
3. 任何一项为 R/D → 该候选**不得准入**，回到方案选型；
4. **✅ 第 264 轮已完成**：表已填满、cargo-deny 已跑，本 ADR 转 **Accepted**；公告一侧的例外按 ADR-0031 登记（`docs/audit/waivers.json` W-01）。**✅ 第 267 轮：`crates/termai-gpu` 已落地**（WS-03 第一刀：T0–T3 阶梯 + headless wgpu 探测，**只准入 wgpu**；rustybuzz/swash/fontdb/winit 属后续切片，其中三者受 W-01 覆盖）；K4 允许边（`termai-gpu -> [termai-core]`）与 CODEOWNERS 已同步；`kernel/03` 的 K-14 / S8 / S9 本就以 `termai-gpu` 为 T0–T3 单一决策点。`termai-render -> termai-gpu/vt` 两条边在该 crate 真正声明依赖时再加（**不留假依赖**）；
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