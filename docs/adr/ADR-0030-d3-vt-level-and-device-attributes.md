# ADR-0030｜D-3 的可实施形态：VT 级别声明、DA1/DA2 应答与 DECID 的 8-bit C1 边界（更正 ADR-0029 D-3）

| 项 | 内容 |
| --- | --- |
| **状态** | **Accepted** |
| **日期** | P0 Wave 3（第 257 轮：D-3 实施前读钉定套件源码时发现的阻塞） |
| **决策者** | 总负责人（Orchestrator），依据**钉定 esctest2 源码**（`2798f12149a19c3295e9b4853ab2da4b2eff1b2b`）与 kernel/01 §5 / OQ-VT-14 |
| **关联 AR** | AR-18、AR-20（诚实原则）、AR-25、AR-31 第 1/2 条 |
| **关联 HARNESS 章节** | §7（P0 出口）、§8.1-1、§11.2 |
| **取代/关联 ADR** | **更正 ADR-0029 D-3 的「与钉定 oracle 逐字节一致」**；承接 kernel/01 **V-02**（esctest 100% / `X=0`）与 **OQ-VT-14**（8-bit C1 策略） |
| **治理提示** | 与 ADR-0029 同：不含任何 §5/§8 放宽，故由 Orchestrator 定案；治理缺口（TSC 未成立 / C1）不变 |

## 1. 背景与问题

ADR-0029 D-3 决定「`DA` / `DA2` / `DECID` 必须实现，且与钉定 oracle **逐字节一致**」。实施前把钉定套件克隆到 `target/conformance/esctest2` 并读源码，发现该要求**不可直接实施**，有三处硬事实：

1. **esctest 的 DA 断言不是字节相等，而是「集合包含 + 范围」**（`esctest/tests/da.py`）：它要求 `params[0] == expected[0]` 且 `expected` 的**每个**参数都出现在响应里，而 `expected` 由 `--max-vt-level`（**默认 5**）决定。级别 5 的 `expected = [65,1,2,6,9,15,16,17,18,21,22,28,29]`——**满足它必须声称 selective erase / locator / state reports / user windows / horizontal scrolling / color / rectangular editing / ANSI text locator**，而本实现**没有这些能力**。声称它们同时违反 ADR-0029 D-3 自己的「声称集 ⊆ 实现集」与 AR-20。
2. **`da2.py` 断言的是范围，不是某个 xterm 版本号**：`314 ≤ params[1] ≤ 999`、`len(params) == 3`（`params[0]` 在 level 1 下不被断言）。「逐字节一致」比测试实际要求更强。
3. **`DECID` 是 8-bit C1 的 `0x9A`**（`escio.DECID() → CmdChar(0x9a)`），而 kernel/01 **OQ-VT-14** 规定 **UTF-8 模式下不识别 8-bit C1**。因此该用例的可通过性取决于解析器的模式声明，而不是一个固定字节串。

**本机实测（同一检出、同一命令，只换 `--max-vt-level`）**：

| level | passed | known-bug | failed | eligible = passed+failed |
| --- | --- | --- | --- | --- |
| 1 | 99 | 378 | **90** | 189 |
| 2 | 105 | 369 | 93 | 198 |
| 3 | 111 | 334 | 122 | 233 |
| 4 | 266 | 43 | 258 | 524 |
| **5（默认，现口径）** | 267 | 41 | **259** | 526 |

**读数**：`--max-vt-level` 是套件提供的**静态能力声明**（被终端声明的 VT 等级）；降低它会把该级别之上的用例记为 known-bug（不进入 eligible）。**它既是 K-01 允许的 `S_cap` 形态，也正是本仓库最容易被用来美化数字的杠杆**——所以本 ADR 用**规则**而不是用失败数决定级别。

## 2. 可选方案

| # | 事项 | 采纳 | 被否决项与理由 |
| --- | --- | --- | --- |
| 2.1 | **VT 级别** | **A：按规则取「xterm DA1 在该级别下的 expected 集合全部为已实现能力」的最高级别——当前 = 1** | **B 取失败最少的级别**：**否决**（结果同为 1，但理由错——规则必须独立于结果）。**C 维持 5 并声称未实现的能力**：**否决**（AR-20 + 违反 D-3 自己的声称集约束）。**D 维持 5 并把 DA 记为差异**：**否决**（V-02 要求 `X=0`，esctest 无差异通道） |
| 2.2 | **DA 应答** | **A：级别 1 报 `CSI ? 1 ; 2 c`（VT100 + AVO）；DECID 同值** | **B 报自有身份**：**否决**（level 1 的 `expected=[1,2]` 不满足则恒红）。**C 不应答**：**否决**（DA1 是基础能力，且应答是 V-02 的必需项） |
| 2.3 | **DECID 的 0x9A** | **A：依 OQ-VT-14——仅在非 UTF-8 模式识别并应答；UTF-8 模式下按既有规则计数不识别，该用例按模式前置处理** | **B 在 UTF-8 模式也识别**：**否决**（违反已冻结的 OQ-VT-14，且与真实 xterm 的 UTF-8 行为不符） |

## 3. 决策

### D-1｜级别的确定规则（本 ADR 的核心）

1. 声明的 VT 级别 = **使 xterm DA1 在 `--max-vt-level N` 下的 expected 集合全部为已实现能力的最高 N**。按当前能力集合，**N = 1**（`expected=[1,2]` = VT100 + Advanced Video Option）。
2. **禁止**按失败数选择级别；**任何级别变化都必须由能力实现驱动**，并在新 ADR/勘误中记录。
3. esctest 的调用**必须显式传 `--max-vt-level <claimed>`**（禁用默认 5 的隐式口径），且 **G1 报告必须同时给出「级别」与「eligible 分母」**：**只有失败数、没有级别的数字不得被引用**（对齐 plan §6.3 规则 8：证据必须含产生它的确切命令）。
4. 低级别下 known-bug 中混有大量**「因级别不足未运行」**的用例——报告必须把这一类**单列**为 `excluded_by_vt_level`，不得叙述成「已知缺陷」。
5. E-P0-1 的口径由此**改写为**：「level 1 eligible 189 条，其中失败 90 条；另有 378 条因级别不足未运行」。**两个数字必须同时出现。**
   > **勘误（第 261 轮，D-3 落地后实测；只改数字，不改语义）**：level 1 = **103 passed / 378 known-bug / 86 failed**（raw eligible 仍 **189** = passed+failed）；应用 D-2 的 `color-query` 静态排除后，**gate-eligible = 189 − 47 = 142，failed_real = 39**。该换算由 `tools/conformance/esctest-report.mjs` 机械化（7/7 注入 + 对照），**不得手算**；`excluded_by_vt_level` 报为 `unknown`（log 无逐用例级别归属）。上表为 D-3 之前的快照，保留以记录尺子的变化。
   > **勘误 2（第 270 轮，只加口径、不改语义）**：**调用还必须显式声明反绕 flag `--xterm-reverse-wrap 383`**。理由：不传时 esctest 默认 0，于是 `ReverseWraparound()` 返回 **mode 45** 并按 **2023 年以前**的 xterm 语义评判我们——**尺子换了而数字没换**。声明 383 后 esctest 返回 **1045**（广义），而**直接设置 mode 45 的用例仍测收窄语义**，与 xterm `cursor.c` 的 `CursorBack` 一致（45 要求目标行 `LineTstWrapped`，1045 无条件）。**ed 后本机 level 1 = 124 passed / 376 known-bug / 67 failed；经 `esctest-report` 判定 `excluded_by_capability 67`、`failed_real 0`。** **教训与 §6.3 规则 8 同源：数字必须连它的命令一起报。**

### D-2｜级别 1 的可执行应答

| 请求 | 识别形态 | 应答 |
| --- | --- | --- |
| **DA1** | `CSI c` / `CSI 0 c`（无 intermediate） | `CSI ? 1 ; 2 c`（VT100 + AVO） |
| **DA2** | `CSI > c` / `CSI > 0 c`（intermediate `>`） | `CSI > 0 ; 314 ; 0 c`（Pp=0=VT100；Pv=**314**，本产品**自报版本**，取 esctest 接受区间 314–999 的下界以避免冒充 xterm 版本；Pc=0） |
| **DECID** | 8-bit C1 `0x9A` | 非 UTF-8 模式：与 DA1 同值；UTF-8 模式：不识别（OQ-VT-14） |

**对 ADR-0029 D-3 的更正（逐字记下）**：「与钉定 oracle 逐字节一致」**改为**「**满足钉定套件在该级别的断言**（DA1 集合包含、DA2 范围）」。理由：逐字节一致要求声称未实现的能力（§1 事实 1）。ADR-0029 D-3 的诚实义务（声称集 ⊆ 实现集）**保留并加强**。

### D-3｜验证与一致性

1. 新增 conformance 用例：① DA1 应答的参数集 ⊆ 已实现能力集；② DA2 恰好三段且 `Pv ∈ [314,999]`；③ DECID 在声明模式下的应答与 DA1 一致。
2. `vte_adapter::csi_known` 与 `grid::csi_dispatch` 对 `c` 的**计数与行为必须一致**：当前 `csi_known` 把 `'c'` 计为已知（`vte_adapter.rs:606`）而 `csi_dispatch` 没有 `b'c'` 分支（`grid.rs:1765` 起）——**这是既有的计数/行为不一致，本 ADR 一并修正**。

## 4. 理由

1. **A1 / AR-20**：级别 1 的 `1;2` 是本实现**真实具备**的能力（VT100 + AVO）；不声称 6/9/15/16/17/18/21/22/28/29。
2. **A2（兼容性是入场券）**：不应答 DA1 会让应用把我们当哑终端；应答一个**真的**最小集合既兼容又不撒谎。
3. **可判定性**：级别由规则确定、由 conformance 用例锁定；报告强制带级别与分母，使「数字变小」这件事本身可审计。

## 5. 后果

### 正面
- D-3 从「不可实施」变为可实施：4 条 DA 用例（DA1×2、DA2×2）有确定目标。
- 「级别即声明」变成一条可检查规则，而不是一次性的数字选择。

### 负面（必须接受的代价）
1. **口径变化会被误读**：E-P0-1 从「259 失败」变成「90 失败 + 378 级别排除」。**这不是改善，是换了一把尺子**——两个数字必须并列出现。
2. **DECID 在 UTF-8 模式下仍是不通过的用例**（1 条），需要模式前置或非 UTF-8 模式的解析器支持。
3. **本机仍达不到 E-P0-1**：level 1 的 90 条失败全部是硬缺口。
4. **DA2 的 Pv 是自报版本**，不得据此声称 xterm 兼容版本号。

## 6. 反方记录与复议条件

- **反方（怀疑派）**：把级别降到 1 是缩小分母、隐藏真实缺陷。
  **驳回**：级别由**能力集合规则**决定，不由失败数决定；且 **V-04（ctlseqs 语料）不按 VT 级别门控**，扩展能力继续被它独立覆盖；报告强制并列两个数字。
  **复议条件**：若某个**已实现**能力只被 level ≥ 2 的用例覆盖，**必须升级级别并跑该级别**；届时若失败数上升，那是真实缺口暴露，不是回归。

- **反方（兼容派）**：DA1 只报 `1;2` 会让应用低估我们。
  **驳回**：低估不是撒谎；升级的唯一路径是**实现能力**，而这正是复议条件要求的。

- **反方（反对排除 DECID）**：排除 DECID 是规避。
  **驳回**：OQ-VT-14 是**既有冻结决定**（UTF-8 模式不识别 8-bit C1）；在 UTF-8 模式识别 `0x9A` 与真实 xterm 行为不符。
  **复议条件**：若解析器提供并默认非 UTF-8 模式，DECID 立即恢复为必修（本 ADR 的 D-2 已给出其应答值）。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| 级别规则与报告口径 | `docs/plan/p0-verification-runbook.md`（esctest 命令必须带 `--max-vt-level` 与级别说明）；`tools/conformance` 的报告口径 |
| DA1 / DA2 实现 | `crates/termai-vt/src/grid.rs` 的 `csi_dispatch`（`b'c'` 分支，按 `intermediates` 分派）+ `vte_adapter.rs` 的 `csi_known` 一致性 |
| DECID | 8-bit C1 路径（非 UTF-8 模式）；UTF-8 模式继续按 OQ-VT-14 计数 |
| 验证用例 | `tools/conformance` 的 DA 用例（声称集 ⊆ 实现集；DA2 区间） |
| HARNESS 登记 | §11.2 追加 **CR-18** |
| 决策简报 | `docs/plan/p0-open-decisions.md` D-3 追加本 ADR 的更正 |

**同步义务**：① `docs/adr/README.md` 索引；② HARNESS §11.2 CR-18；③ `docs/plan/p0-open-decisions.md` D-3；④ `docs/plan/p0-verification-runbook.md` §2（esctest 命令的 `--max-vt-level` 与级别口径）；⑤ 实现轮完成后更新 `kernel/01` §3.5 的 DA 行（把「与 oracle 逐字节一致」改为「满足该级别断言」）。

## 附录｜DECID 现状的实现证据（第 257 轮实现轮追加，不改本文语义）

实现者核实的**事实**（不是推断）：预扫描器不在 `terminal.rs`，而是 `vte_adapter.rs` 的私有 `PreState`；`PreState::ground_step` 只识别 7-bit ESC 引导的序列（`[ ] P X ^ _` 与 ESC），**没有 0x80–0x9F 分支**。唯一的 8-bit C1 处理在 `VteAdapter::execute`：`eight_bit_c1 == true` 时把字节转发给 `sink.execute(byte)`，否则计 `c1_8bit_in_utf8` + `invalid_utf8` 并上屏 U+FFFD（OQ-VT-14）。而 `Grid::execute` 只匹配 0x07/0x08/0x09/0x0A–0x0C/0x0D/0x0E/0x0F，其余 `_ => {}`——**因此即使在非 UTF-8 路径上，0x9A 被转发也从不会被派发成 DECID**。`eight_bit_c1` 默认为 false。

**结论**：DECID 目前在本仓库**任何模式下都未实现**；开启它需要新增一个控制处理器（`Grid::execute` 的 0x9A 分支 + 模式前置），**超出本轮切片，故未实现**。这与 §3 D-2 的处置一致：**应答值已冻结，实现留给独立切片**；届时按 §6 的复议条件恢复为必修。
