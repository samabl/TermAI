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

