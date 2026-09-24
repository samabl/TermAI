# ADR-0032｜窗口几何与 window-ops：v1 声明能力缺失，而非实现（含 DA1 位 1 的解释）

| 项 | 内容 |
| --- | --- |
| **状态** | **Accepted**（总负责人，依据 kernel/01 K-03 仲裁序与 kernel/03 V-06） |
| **日期** | P0 Wave 3（第 266 轮；由 level-1 esctest 分诊触发，见 docs/audit/esctest-level1-triage.md） |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-01（原生网格）、AR-12、AR-14（像素不进 VT 层）、AR-19、AR-23 §6、AR-25 |
| **关联 DC** | DC-16、DC-17 |
| **关联 HARNESS 章节** | §5（H4 帧时/网格对齐）、§7（E-P0-2）、§8.1-1（G1 / V-02）、§8.2 |
| **关联 ADR** | ADR-0030（VT 级别与 DA 应答）、ADR-0029 D-2（静态能力前置的先例）、ADR-0014（T0–T3）、ADR-0017 |

## 1. 背景与问题

level-1 esctest 分诊（docs/audit/esctest-level1-triage.md）表明：29 条真实失败中，约 17 条是 XtermWinopsTests，另 2 条是 132 列（DECCOLM）。它们需要的能力与 VT 解析无关：

| 测试族 | 需要什么 | 为什么现在不可能 |
| --- | --- | --- |
| IconifyDeiconfiy / MoveToXY / MoveToXY_Defaults | 窗口被图标化 / 被移动，并能被 CSI 11 t / 13 t 如实报告 | 无窗口宿主（E-P0-2 未实现）；我们如实回 11t -> normal、13t -> 3;0;0t，「假装被移动」就是撒谎 |
| Push/Pop icon 与 window title 族 | 标题栈（CSI 22/23 t） | 属纯 VT 状态，可做，但与窗口无关、价值低 |
| ReportIconLabel / ReportWindowLabel | 报告 icon/window label | 同上 |
| DECSLPP / ResizePixels（CSI 4/8 t） | 改行数 / 像素尺寸 | 需要字体度量与窗口几何；AR-14 明确像素不进 VT 层 |
| DECCOLM（CSI ? 3 h）-> 132 列 | 网格列数可变 | cols 目前恒定；可变列数 = resize 语义（kernel/03：cols 只在 resize 时变） |

**而且 reflow 这条路被冻结**：kernel/03 的 V-06 否决「软换行触发 reflow 写回网格」，理由是与 AR-23 §6 / spec 02 C14 的「不改字符内容、网格列数、复制结果」直接冲突。因此不能用 reflow 去「实现」resize。

**另有一处必须先澄清的声称张力**：esctest 的 da.py 注释说 DA1 参数 1 = 「132 columns」，而我们 DA1 回 ?1;2（ADR-0030），却没有 DECCOLM。两者是否矛盾，取决于 1 的权威含义。

## 2. 可选方案

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | v1 声明 xterm-window-ops 能力缺失（窗口几何/图标化/移动/像素 resize/DECSLPP/DECCOLM 一并），经 suites.json 的静态能力前置排除（K-01 的 S_cap，非运行时 skip、非差异登记）；纯 VT 状态的 title push/pop 不在本 ADR 强制范围 | 采纳：没有窗口宿主就如实声明没有，与 D-2 的 color-query 同一形状 |
| **B（否决）** | 现在实现可变行/列与窗口几何 | 否决：属 E-P0-2（WS-03/04）的实现，P0 本波做不到；且像素操作违反 AR-14 |
| **C（否决）** | 把 4/8 t 当 no-op 但回「成功」 | 否决：这是撒谎，直接违反 AR-20；esctest 的下一句断言就会拆穿 |
| **D（否决）** | 用 reflow 实现 resize | 否决：kernel/03 V-06 已冻结否决（与 AR-23 §6 冲突） |

## 3. 决策

### D1｜xterm-window-ops 声明为 v1 静态缺失

在 tools/conformance/suites.json 的 esctest2 下新增能力 xterm-window-ops，前缀覆盖该族（XtermWinopsTests.），依据 = 本 ADR + AR-14 + E-P0-2 未实现。这些用例**移出判定集**（excluded_by_capability），**不是**被修好，也不是差异登记。**当且仅当窗口宿主落地（E-P0-2）后撤销该排除。**

### D2｜DECCOLM / 132 列的未实现如实登记

CSI ? 3 h（80/132 列）在 v1 **不实现**；RIS 也就不需要复位它。与 D1 同批排除的两条（DECSETTests.test_DECSET_Allow80To132、RISTests.test_RIS_ResetDECCOLM）按 deccolm-132 能力缺失排除。**不因此收窄 DA1**——见 D3。

### D3｜DA1 的 1 按 DEC/ECMA 含义解释（K-03 仲裁序）

DA1 参数 1 的权威含义是 **「VT100 with Advanced Video Option」（DEC STD 070 / vttest 传统）**，**不是** esctest 注释里的「132 columns」。依据 kernel/01 K-03：**ECMA-48 / DEC 标准 > xterm ctlseqs 文档 > xterm 实现 > esctest 期望**——esctest 的注释处于最低档。**因此 DA1 回 ?1;2 不构成对 132 列（DECCOLM）的实现承诺**，ADR-0030 的「声称集 ⊆ 实现集」不被违反。

> **代价（如实写明）**：这是一次解释性裁决。若将来取到 DEC STD 070 原文、或 xterm 实现证明 1 必须蕴含 132 列支持，则 D3 需复议，届时选择「实现 DECCOLM」或「降级 DA1 声称并接受 DA 用例失败」。

### D4｜报告纪律

G1 报告必须把该族列入 excluded_by_capability（按能力名与前缀分列），并**同时**给出 level、raw eligible 与 gate-eligible（ADR-0030 D-1 已强制）。**不得**把这 19 条叙述成「已实现」或「已知缺陷」。

## 4. 理由

1. **AR-20 诚实原则**：没有窗口宿主就不声称窗口能力；与 D-2 的 color-query 处理完全同形。
2. **AR-14 边界**：像素级几何归 UI 层；把 4/8 t / DECSLPP 塞进 VT 层会破坏「VT 层不持有字体度量」这一既有不变式。
3. **K-03 可裁决**：DA1 位 1 的争议有明文仲裁序可依，无需实现 132 列即可保持声称自洽。
4. **可撤销**：能力排除是声明，随 E-P0-2 落地即可撤销，不留技术债。

## 5. 后果

- **正面**：这 19+2 条从「说不清的失败」变成「有依据的排除」，failed_real 只反映真正要修的缺陷（BS/CUB 6、反绕 3 等）。
- **负面（必须接受）**：**v1 不支持 132 列与窗口几何**；依赖它们的应用（罕见）会退化。**E-P0-2 落地前，本 ADR 的排除必须保持可见**，不得据此声称「esctest 已全通过」。

## 6. 反方记录与复议条件

- **反方（兼容派）**：窗口操作是 xterm 的常见扩展，声明缺失会降低兼容度。
  **驳回**：我们本来就没有窗口——现在的「兼容」只是回常量；声明缺失比回假值更诚实，且 11t/13t 的如实回答不变。
  **复议条件**：E-P0-2 的窗口宿主落地后，**必须**撤销 xterm-window-ops 与 deccolm-132 排除并重跑该族。
- **反方（DA1 派）**：1 就该蕴含 132 列，ADR-0030 的声称应随之收窄。
  **复议条件**：取到 DEC STD 070 原文或 xterm 行为证据时重开 D3。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| 能力声明 | tools/conformance/suites.json（新增 xterm-window-ops、deccolm-132；check-suites.mjs 校验） |
| 报告 | tools/conformance/esctest-report.mjs（按能力/前缀分列；无需改代码，读 registry 即可） |
| DA1 解释 | ADR-0030 D-2/D-3 的注释 + kernel/01 §3.5 的 DA 行 |
| 撤销触发 | E-P0-2（apps/termai-desktop + 窗口宿主）落地 |
