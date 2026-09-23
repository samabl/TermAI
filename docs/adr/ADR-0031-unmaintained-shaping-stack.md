# ADR-0031｜渲染字体栈的 unmaintained 阻塞：`rustybuzz` / `ttf-parser`（**Proposed**）

| 项 | 内容 |
| --- | --- |
| **状态** | **Proposed**——「接受 unmaintained 依赖」是**安全默认值的放宽**，按 AGENTS §7.4 与 §5 需 owner/TSC 追认，编排者不单方定案 |
| **日期** | P0 Wave 3（第 262 轮，ADR-0027 采证时发现） |
| **决策者** | 总负责人（Orchestrator）提出；**决定权在 owner/TSC** |
| **关联 AR** | AR-18（VT 引擎与依赖策略）、AR-21（供应链 / GPL 禁入）、AR-01/AR-03 |
| **关联 DC** | **DC-17**（wgpu + rustybuzz/swash 指定）、DC-21 |
| **关联 HARNESS 章节** | §4.3、§7（E-P0-2）、§8.1-5 **G5**（依赖与许可 / SBOM） |
| **关联 ADR** | **ADR-0027**（依赖准入，因本阻塞保持 Proposed）、ADR-0015（链接边界 / 「未知即拒绝」）、ADR-0014（平台矩阵）、ADR-0024 |

## 1. 背景与问题

第 262 轮按 ADR-0027 D3 在**仓库外**探针上跑 `cargo deny check licenses advisories bans sources`（cargo-deny 0.20.2，配置 = 仓库根 `deny.toml`）：

| 检查 | 结果 |
| --- | --- |
| `licenses` | **ok**（270 包；**R=D=0**，GPL/AGPL/SSPL 命中 **0**） |
| `bans` / `sources` | **ok** |
| **`advisories`** | **FAILED** |

两条公告：

| 条目 | ID | 内容 |
| --- | --- | --- |
| **`rustybuzz` 0.20.1** | **RUSTSEC-2026-0206** | **unmaintained**；“No safe upgrade is available”（`cargo info` 最新即 0.20.1） |
| **`ttf-parser` 0.25.1** | **RUSTSEC-2026-0192** | **unmaintained**；公告建议替代 `skrifa`（fontations，MIT OR Apache-2.0） |

而 **DC-17 / kernel/03 K-05 把 `rustybuzz` 定为唯一 shaping 引擎、`swash` 定为唯一栅格化引擎**；`ttf-parser` 既是 rustybuzz 的解析依赖，也经 `ab_glyph → winit` 进入。

**因此 E-P0-2 的依赖准入现在卡在「公告」，不是「许可证」**——而这条在依赖进入 workspace **之前**被采证抓到，正是 ADR-0027 D3 存在的意义。

## 2. 可选方案

| 方案 | 内容 | 代价 / 结论 |
| --- | --- | --- |
| **A** | **时间盒例外**：允许这两个 unmaintained 单元进入，但必须 ① 无已知 CVE/RUSTSEC 漏洞（仅 unmaintained）；② 登记**到期**（≤2 minor 且 ≤6 个月，同 K-04 风格）；③ 把「替换评估」列为 **P0 出口前置**；④ 报告与 SBOM 中**显式标注** | **可解锁 P0**；代价是**放宽安全默认值**，须 owner/TSC 追认。**不构成「供应链已清」** |
| **B** | **立刻替换**：解析层 `ttf-parser → skrifa`；shaping 引擎评估 `swash::shape` 或 `harfbuzz_rs` | **正确但大**：**改的是 DC-17 / kernel/03 K-05**（唯一引擎的指定），须同时出修订 ADR；`harfbuzz_rs` 还引入 C 依赖与新的链接边界判定 |
| **C** | 自维护 fork / vendor `rustybuzz` | 长期维护成本与安全响应责任落到本仓库；对 P0 是净负担 |
| **D** | **阻塞 E-P0-2** 直到 B 完成 | **最保守**；代价是 P0 关键路径整体停摆，且 E-P0-3 的帧时门禁也无从判定 |

## 3. 决策（Proposed：需 owner/TSC 追认）

**本 ADR 不自行采纳任何方案。** 编排者给出建议与「若采纳必须同时满足的条件」：

- **建议路径：A（时间盒例外）解锁 P0，B 作为目标态**，理由是：unmaintained ≠ 已知漏洞；P0 的渲染/IME 关键路径无法承受 D 的长期停摆；而 B 需要一次 DC-17/K-05 的架构修订，工期与风险都远超本轮。
- **若 owner 采纳 A，必须同时满足以下四条（缺一不可）**：
  1. 登记条目含 **RUSTSEC ID + cargo-deny 命令 + 探针证据**，并写入 `docs/audit/debt-p0.md`；
  2. **到期条件**（≤2 minor 且 ≤6 个月，先到者为准）与**替换触发条件**（`rustybuzz`/ttf-parser 出现任何 RUSTSEC 漏洞公告，或 `skrifa`/替代 shaper 达到可用即触发）；
  3. **不解除 ADR-0027**：许可证一侧已 ok，但 `advisories` 这条必须在 ADR-0027 里**保持可见**，并在 SBOM/发布说明中标注；
  4. **到期即红**：与 K-04 的差异登记同样机制，过期未替换则 G5 判失败。
- **若 owner 采纳 B/D**：本 ADR 随之为其服务（B → 新架构 ADR；D → 记为 E-P0-2 的阻塞项）。

## 4. 理由

1. **A1/A2**：终端是信任基础设施；**unmaintained 是可观测的供应链风险**，但风险等级与「已知漏洞」不同，应被**登记并设到期**，而不是被**静默接受**或**无限期停摆**。
2. **可审计**：无论 A 还是 B，**证据已经存在**（`ADR-0027-spdx-evidence.md` + `deny.toml`），决定因此是可复核的。
3. **保守默认**：本 ADR 不由编排者自裁，正是 AGENTS §7.4「不确定时选更保守的安全默认值」在**流程**上的执行。

## 5. 后果

- **正面**：供应链阻塞被提前暴露并留证；无论选哪条路，ADR-0027 的许可证证据都可复用。
- **负面（如实记录）**：**ADR-0027 保持 Proposed，E-P0-2 的代码路径仍不能开始**；若 owner 选 D，P0 关键路径停摆；若选 A，项目将**明确承担**一段时间的 unmaintained 风险。

## 6. 反方记录与复议条件

- **反方（安全优先）**：unmaintained 就该拒绝，选 B。
  **回应**：这正是把决定权交回 owner 的原因；本 ADR 记录 A 与 B 的代价，不预设结论。**复议条件**：`skrifa` + 某个 maintained shaper 的组合被证明能满足 DC-17 的功能集且不引入 C 依赖时，B 的成本大幅下降，应优先 B。
- **反方（进度优先）**：直接接受，不必设到期。
  **驳回**：无到期的例外就是第 61 轮 K-04 批评过的「橡皮章」；**必须有到期与触发条件**。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| 证据 | `docs/adr/ADR-0027-spdx-evidence.md` + 仓库根 `deny.toml` |
| 勾选/到期登记 | `docs/audit/debt-p0.md`（若采纳 A） |
| 架构替换（若选 B） | 新 ADR 修订 **DC-17 / kernel/03 K-05**；同步 spec 03 §3.3 与 K4 允许边 |
| 门禁 | **G5**（cargo-deny + SBOM）；本 ADR 生效后应把 `cargo deny check` 接进 CI |
