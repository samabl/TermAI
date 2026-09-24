# esctest level-1 triage（第 263 轮）：哪些是缺陷、哪些是顺序污染

> **性质**：本文件是**手工判读产物**（不是 `tools/conformance/classify-esctest.mjs` 的生成物，不会被重新生成覆盖）。
> **方法**：把 level-1 的每一条非 color 失败**单独**用 `--include <完整用例名>` 跑一遍（级别 1，ADR-0030；color 族已由 ADR-0029 D-2 静态排除）。
> **机器产物（可复现、不入库）**：`target/conformance/triage-level1.md`；逐条日志在 `target/conformance/triage3/<n>/`。

## 结论

- 基线（level 1）：**103 passed / 378 known-bug / 86 failed**；经 D-2 排除 47 条 color-query 后 **failed_real = 39**。
- **单跑：10 条 PASSES-ALONE（顺序污染）、29 条 FAILS-ALONE（真实失败）、0 条 INVALID。**

### 10 条「单跑通过」= 顺序/状态污染，不是缺陷

`SMTests.test_SM_LNM`、`RISTests.test_RIS_ResetTitleMode`、`DECSETTests.test_DECSET_SaveRestoreCursor`、`DECSETTiteInhibitTests.test_SaveRestoreCursor_{Basic,AltVsMain,MoveToHomeWhenNotSaved}`、`XtermSaveTests.test_XtermSave_{SaveSetState,SaveResetState}`，另 2 条见 `target/conformance/triage-level1.md`。

> **对计划的更正（重要）**：第 262 轮我据此把「LNM / RIS title mode / save-restore cursor 族」列为**待修缺陷**并派了切片——**这个前提是错的**：这批用例**单跑就过**，全集里失败是**顺序污染**。**LNM 的实现本来就是对的**：协议层探针直接验证（`CSI 20 h` + LF → `CSI 2;1R`；去掉 `SM 20` → `CSI 2;5R`；`RM 20` 后 `SM 20` → `3;1R`）。

### 29 条真实失败（单跑仍失败）的分类

| 类别 | 条数 | 性质 |
| --- | --- | --- |
| `XtermWinopsTests.*`（DECSLPP、Iconify、MoveToXY、Push/Pop、ReportIconLabel/WindowLabel、ResizePixels×5） | 约 17 | **设计任务**：窗口/文本区操作，含 `CSI 4/8 t` 动态 resize → 需 resize + reflow 策略（连动 kernel/03） |
| `DECSETTests.test_DECSET_Allow80To132`、`RISTests.test_RIS_ResetDECCOLM` | 2 | **同上**（132 列 = 动态列数） |
| `BSTests.*`（AfterNo/OneWrappedInline、ReverseWrapGoesToBottom、WrapsInWraparoundMode）、`CUBTests.*`（AfterNo/OneWrappedInline） | 6 | **真实网格语义缺陷**：换行边界上的 BS/CUB（`wrap_pending` 语义）——**可直接修** |
| `DECSETTests.test_DECSET_ReverseWraparound_BS` / `_Multi` / `ReverseWraparoundLastCol_BS` | 3 | **真实缺陷**：反绕（DECSET 45）与 BS 的交互 |
| `DECIDTests.test_DECID_Basic` | 1 | **已决定不实现**（8-bit C1；ADR-0030 §6 / OQ-VT-14） |

## 本轮我自己的两次命令错误（记下来，因为两次都产生了「全绿」的假象）

1. **锚定正则**：第一版用 `--include '^Name$'`，esctest **一条都没匹配**，汇总行是 `0 tests passed, 0 known bugs, 0 tests failed`；我的脚本把它读成 `PASSES-ALONE`，于是**39/39 全绿**。**修正**：用未锚定的完整用例名。
2. **单复数**：第二版只匹配复数 `tests passed` / `tests failed`，而 esctest 对**单条**用例写 `1 test passed` 与 `1 TEST FAILED` → 全部落进「无匹配」→ **又一次假全绿**。**修正**：`tests?`。

两次都是**命令错、被测对象没错**，且**只有去读汇总行原文才发现**（当时是 `0 tests passed` / `1 TEST FAILED`）。与计划 §6.3 规则 11 同源：**结论异常时先重跑或换写法，再怀疑被测对象**。

## Reverse-wrap slice spec（第 268 轮补记：9 条的真实性质与精确契约）

9 条真实失败**全部是反绕（reverse wraparound）**，不是网格 bug：

| 项 | 值 |
| --- | --- |
| 常量 | `XTREVWRAP = 45`（xterm 380 起）、`XTREVWRAP2 = 1045`（xterm 383 起的收窄版） |
| esctest 选择 | `esccmd.ReverseWraparound()`：`--xterm-reverse-wrap >= 383` 时返回 1045，否则返回 **45** |
| **我们的调用** | 适配器**不传** `--xterm-reverse-wrap`（默认 **0**）→ esctest 取 **45**，并走各测试 `else` 分支的**旧行为（pre-383）** |
| 前置 | 反绕**同时**需要 `DECAWM`（测试 `test_BS_ReverseWrapRequiresDECAWM` 证明：只 mode 45 不反绕，只 DECAWM 不反绕） |

**需要实现的语义（旧行为 = 我们当前被断言的那一支）**：

1. 新增 `MODE_REVERSE_WRAP`（private mode **45**；`DECRQM` 也要能报它）。
2. **BS（0x08）**：`wrap_pending` 为真时**只取消它、光标不动**（`test_DECSET_ReverseWraparoundLastCol_BS`：末列写 `b` 后 BS，x 仍是 width）；否则 x>1 时 x−1；x==1 且 mode45+DECAWM 时**回上一行末列**；若已在**上边距**（scroll_top）则绕到**下边距**（scroll_bottom）末列（`test_BS_ReverseWrapGoesToBottom`：`DECSTBM(2,5)`、CUP(1,2)、BS → **(80,5)**）。
3. **CUB（CSI Ps D）**：同样的反绕，但要按次数逐格移动；`test_CUB_AfterNoWrappedInlines`/`AfterOneWrappedInline` 的 `else` 分支给出确切落点（**80 列下分别是 (5,3) 与 (9,3)**）。
4. 边距/左右边距交互（`@vtLevel(4)` 的那几条当前**不在** level-1 判定集内，但实现时不要把它们弄坏）。

**为什么我（编排者）本轮没直接改**：旧行为的精确落点依赖逐格移动 + 上下边距绕行 + `wrap_pending` 取消三者的组合，而 `test_BS_WrapsInWraparoundMode`（空行从 (1,3) 反绕到 (80,2)）与 `test_BS_AfterNoWrappedInlines`（同样 mode 45 却不越硬换行）**只有把完整测试体读全才不矛盾**；盲改会把「顺序污染」与「真实语义」再搅在一起。**下一轮的验收判据**：逐条单跑 9 条全绿 + `esctest-report` 的 `failed_real` 从 **18** 继续下降，且**不得**改动已被 triage 判为污染的那 10 条。


## 第 268 轮结果：反绕实现已落地（`01b1ead`），9 条目标全绿

**同一调用、同一级别（level 1）、新构建的 harness**：

| 指标 | 第 263 轮（旧） | 第 268 轮（新） |
| --- | --- | --- |
| passed | 103 | **123** |
| failed_raw | 86 | **68** |
| excluded_by_capability | 68 | **66** |
| **failed_real** | **39 → 18** | **2** |

- **9 条目标用例逐条单跑全部通过**（BS ×4、CUB ×2、DECSET 反绕 ×3）——`failed_real` 从 18 降到 2，其余下降来自那 10 条顺序污染在修复后也一并通过。
- **但代价必须写明**：`BSTests.test_BS_InitialReverseWraparound` 之前**是通过的**，本轮**转为失败**。它断言「CUP(1,1) → NEL → BS 之后光标不动（不跨到未软换行的上一行）」，而当前实现在列 0 **无条件**反绕。

### 剩余 2 条的真实性质

| 用例 | 性质 |
| --- | --- |
| `DECIDTests.test_DECID_Basic` | **已决定不实现**（8-bit C1，OQ-VT-14 / ADR-0030 §6） |
| `BSTests.test_BS_InitialReverseWraparound` | **真实缺口 1 条**：mode 45 的**收窄语义**（只在软换行处反绕）与本实现的无条件反绕冲突。**注意**：esctest 注释说明 xterm 在 2023 年把 **45 收窄**、另立 **1045 保留旧广义行为**；而 `test_BS_WrapsInWraparoundMode`（空行也从 (1,3) 反绕到 (80,2)）与 `test_BS_InitialReverseWraparound`（NEL 后不反绕）在**同一次调用（默认 `--xterm-reverse-wrap 0`，即 45）下看似互斥**。**因此这 1 条不能靠猜**：需要 xterm 的 `CursorBack` 源码（或 ctlseqs 条款）才能裁定两条断言的分界；在此之前保留失败，不通过改断言或扩大排除来「通过」。 |

### 本轮我自己的流程违规（记下来）

**反绕实现是被 `git add -A` 卷进 `01b1ead`（`feat(gpu)`）的**，提交信息里**没有**它，而我当时以为该提交只含 GPU 切片。成因：我中断了仍在写文件的 subagent 后，**它仍在步进边界写入了 `grid.rs`**，随后我为 GPU 切片执行 `git add -A` 时把它一起扫走。**判据（已写入 runbook 第 5 条铁律）**：提交必须按**显式路径**；在 subagent 可能仍在写文件时**不得**用 `git add -A`；提交前后都要能逐条说出每个文件的来路。


## 第 269 轮：最后 1 条缺口的权威依据（xterm 源码，已取到）

把 xterm 源检出到 `target/xterm-src`（`ThomasDickey/xterm-snapshots`，gitignored），读 **`cursor.c:106–205` 的 `CursorBack`**——这是 `BS` 与 `CUB` 的实现，也是本条的**权威条款**（K-03 仲裁序里「实现」档）：

```c
#define WRAP_MASK  (REVERSEWRAP  | WRAPAROUND)   /* mode 45  + DECAWM */
#define WRAP_MASK2 (REVERSEWRAP2 | WRAPAROUND)   /* mode 1045 + DECAWM */
rev  = ((flags & WRAP_MASK)  == WRAP_MASK);
rev2 = ((flags & WRAP_MASK2) == WRAP_MASK2);
...
if ((rev || rev2) && do_wrap) --count;   /* 待折行 absorbs one step */
else                          --col;
for (;;) {
  if (col < left) {
    if (rev2) { col = right; if (row == top) row = bottom + 1; }   /* 广义 */
    else if (!rev) { col = left; break; }
    --row;                                                          /* 试反绕 */
  }
  if (row != cur_row) {
    if (!rev2 && !LineTstWrapped(ld)) { if (row < bottom) ++row; col = left; break; }  /* mode 45：上一行非软换行 → 反绕失败 */
    col = right;
  }
  ...
}
```

**结论（三条）**：

1. **mode 45（`rev`）= 收窄语义**：跨行反绕**要求目标行 `LineTstWrapped`**；若上一行不是软换行，则**反绕失败**（`row` 还原、`col = left`）。
2. **mode 1045（`rev2`）= 广义语义**：无条件 `col = right`，且在上边距时 `row = bottom + 1`（绕到屏幕底部）。
3. **我们现在的实现把 45 与 1045 当同一条广义路径**，所以 8 条用 `ReverseWraparound()` 的用例过了，而**直接用 `XTREVWRAP`(45) 的 `test_BS_InitialReverseWraparound` 失败**——它要的正是第 1 条的「反绕失败」。

### 最后 1 条的两条出路（需一次口径决定，属下一轮）

| 出路 | 内容 | 代价 |
| --- | --- | --- |
| **A（正确表示现代终端）** | 实现 `rev`(45)=收窄、`rev2`(1045)=广义两条路径；并在 `suites.json` 的 esctest2 `invocation` 里声明 `xterm_reverse_wrap >= 383`（esctest 据此让 `ReverseWraparound()` 返回 **1045**） | **会改变 esctest 的整套期望值**（多条用例有 `>=383` 分支），即**重设一次口径**；须按 ADR-0030 的口径纪律记录「级别 + flag + 分母」 |
| **B（保持 flag 0）** | 不改调用；承认 `test_BS_InitialReverseWraparound` 是**旧 xterm 语义下的 oracle 不一致**并保留失败 | 代价是**永久留 1 条失败**，且不能声称 esctest 100% |

**我方判断（待 owner/评审确认）**：**A**。理由：我们要声称的是「与钉定 xterm 一致」，而钉定版本是现代的（≥383）；用 flag 0 等于**故意让 esctest 按 2023 年以前的 xterm 评判我们**，这正是 ADR-0030 D-1「数字必须连级别一起报」要防的那类口径漂移。**A 的代价是重跑并重记一整套数字**，不是放宽门禁。


## 第 270 轮结案：level 1 `failed_real = 0`

- 反绕按 xterm `cursor.c` 的 `CursorBack` **忠实移植**：mode 45（收窄，要求目标行 `LINE_WRAPPED`，失败则行还原、列落左边距）与 mode 1045（广义，上边距时落 `bottom + 1`）分成两条路径；`wrap_pending` 吸收一步。
- 调用口径补齐：`suites.json` 的 `invocation.xterm_reverse_wrap = 383`（现代钉定终端），命令随之加 `--xterm-reverse-wrap 383`。
- **实测（level 1 + flag 383）**：passed **124** / known-bug 376 / failed_raw **67**；`excluded_by_capability` **67**（47 color-query + 17 xterm-window-ops + 2 deccolm-132 + 1 c1-8bit-controls）；**`failed_real` 0**；eligible 124。
- 最后一条 `DECIDTests.test_DECID_Basic` 以 **`c1-8bit-controls`** 能力声明排除（OQ-VT-14：UTF-8 模式不识别 8-bit C1），**不是**被"修好"。
- 门禁：kernel 8 PASS、`--selftest` 30/30、conformance L0 68/68 R=1.0、waivers PASS、check-claims 10 pairs、fmt/clippy 干净。

> **不得据此声称 G1 通过**：这只是 **level 1** 的判定集，且其中 **67 条是声明的能力缺失**（可撤销的排除，不是通过），另有 **376 条因级别不足未运行**（`excluded_by_vt_level` 仍是 unknown）。V-02 的「100%」只在 `E − S_cap` 上说；E-P0-1 的对外状态仍是 **未判定**。

