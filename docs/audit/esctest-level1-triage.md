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
