# TermAI P0 未闭合清单与技术债登记（debt-p0）

> **效力**：本文件是**登记与索引**，不是设计权威；冲突以 [HARNESS.md](../../HARNESS.md) 为准。
> **依据**：spec 07 §3.9（每版本偿还 ≥1 项 P1 债、性能达标余量入登记）+ AR-19（达标余量必须记账）+ AGENTS §5（未决问题不得用 TODO 代替）。
> **口径**：每条给出 owner / 触发条件 / 依据编号。**未验证的一律写「未验证」**。

## A. 阻塞 P0 出口的四条（E-P0-1…E-P0-4）

| # | 未闭合项 | owner | 触发条件 / 判定 | 依据 |
| --- | --- | --- | --- | --- |
| A1 | **G1 语料仅 275 / ≥2000**；ctlseqs 208/208 有映射，但只有 26 条有门禁期望 | T1 | **进行中**：已派单「+20 条门禁用例」小切片（oracle 必须为 `xterm-ctlseqs` 文档语义，**新增用例若失败不许改弱期望**，须交期望 vs 实际原文）。补 182 条条目的真实期望用例（oracle = xterm ctlseqs 文档 / ECMA-48） | AR-31 第 1 条、kernel/01 §5 V-04。**已核实**：现有语料只断言行/计数器/不变量，**没有一条钉 grid digest**，因此 ADR-0025 的 digest 变更未在语料里留下陷阱；扩充时若想钉 digest 必须显式决定并登记 |
| A2 | **真实语料 0% / ≥20%**，且**环境阻塞**：需 Xvfb + 钉定 xterm + 固定 locale/font 的 oracle 环境与 vim/htop/neovim/fzf/tmux/less/btop 捕捉 | T1 + 平台 | 起 RM-A/RM-B 后才能做；**自钉基线不计入**（`tools/conformance/run.mjs` 已机器强制） | kernel/01 K-03、V-04、ADR-0014 |
| A3 | **vttest 本机无法构建**（无 C 编译器、无 WSL 分发版） | T5 + 平台 | configure 报 `no acceptable cc found in $PATH`；需 RM-A/RM-B + kernel/01 §3.9 driver | §8.1-1、OQ-VT-12 |
| A4 | **esctest **权威口径：218 passed / 41 known-bug / 308 failed / substitutions 0**（第 39 轮实测，**必须带 `-- --expected-terminal xterm --xterm-checksum 336`**）；**最大单簇 = DECRQM oracle 分歧（SD-22，26 条）**；其余失败簇：`CSI … t` 窗口尺寸（SD-19）、颜色族、DECRQM 余项、DECRQSS、DECDSR、DECSET、左右边距/原点模式 **簇级归因（第 36/37 轮实测，负责人自己做）**：① **DECRQM 26 = oracle 分歧**（SD-22，**不是 bug**）；② **XtermWinops 19 = 功能缺口**：`DECSLPP`（`CSI 8;rows;cols t`）必须**真的改变终端尺寸**（实测 `GetScreenSize()` 期望 90x10 实得 80x24），另有 iconify/deiconify 等窗口状态变更——**这需要 grid 支持动态 resize + reflow 语义**，与 kernel/03 的 reflow 设计连动，**不是快修**；③ **DECSET 18 至少含 alt-buffer（mode 47）内容语义差异**（实现在 rect 校验和上与期望不符，`decset.py:422`）。**⚠ 更正（第 38 轮新证据）**：上一轮我写下「已排除 harness 的 DECRQCRA 校验和是共同根因」，**该结论不成立、已降级为未定**。第 38 轮我用探针逐字节复现 `decset.py` 的序列（含把 `CUP(Point(x=1,y=2))` 正确展开为 `CSI 2;1H`），**我们的网格内容与 esctest 期望完全一致**（`ALT = ["", "def", "def"]`、退出后 main 原样恢复），已固化为回归测试 `crates/termai-vt/tests/alt_buffer.rs`。**网格对、比较却失败** → 差异落在**校验和/比对路径**（失败信息里 got 是 `0xffe0` / `0xff9c` 一类的 0xFFxx 编码，而 expected 是原始码位 0x00 / 0x64）。**下一步必须先查 harness 的 `checksum` 实现**，而不是继续改终端。 **簇级归因（第 40 轮，在**正确参数**的 308 条失败运行上重做，旧计数作废）**：① **DECRQM 26** = oracle 分歧（SD-22，不改）；② **颜色三族合计 40**（`ChangeSpecialColor` 14 + `ChangeColor` 13 + `ChangeDynamicColor` 13）＝ **最大的可修簇**：根因是 **`OSC 4 / 10 / 11 / 12` 颜色查询没有任何答复**，适配器直接 `Timeout waiting to read`（`change_color.py:94 → ReadOSC("4")`）——**不是校验和、也不是渲染问题**。**这里有一个必须先做的判断**：答复颜色查询等于**对外声明本终端的调色板**；在终端尚无颜色渲染前，报一套 xterm 默认 256 色是否算诚实的「能力声明」，需要 UX/内核一起定；定了才可派机械切片（实现调色板 + 应答），否则应登记为**差异**。③ `XtermWinops` 19 = 功能缺口（`DECSLPP` 真 resize + 窗口状态，连动 reflow 设计）；④ `DECSET` 16 / `DECSED` 9 / `DECSEL` 6 / `DECCRA` 8 / `BS` 8 / `DL` 7 / `SD` 6 / `DECSERA` 6 等仍待逐个归因；⑤ `DECRQSS` 11 / `DECDSR` 10 属查询类，预计与颜色簇同因（无应答）。 **子集分类框架（第 42 轮，据 kernel/01 §3.5 表；追认前均为初步）**：该表覆盖的是**扩展协议子集**——OSC 133/633/7/0/2/8/9/777/52(仅写)/1337(内联子集)、Sixel、kitty graphics/keyboard；**明确列出「永不」的**是未知 DCS/APC/PM/SOS 与 OSC 52 读。据此初步分类：**(a) 真缺陷（核心 VT，必须修）**：`BS`/`DL`/`SD` 等编辑类与核心 `DECSET/DECRST` 模式——**注意：核心 VT 不在 §3.5 表内，所以「不在表里」绝不能当作偏差的借口**，否则会把基本编辑行为误判为「声明不做」；**(b) 子集外 → 偏差候选**：OSC 4/10/11/12（颜色 40 条）、`DECCRA` 8、`DECSERA` 6、`DECSED` 9 / `DECSEL` 6（选择性擦除）、`DECRQSS` 11 / `DECDSR` 10（查询类）、`XtermWinops` 的 `DECSLPP`（resize 连动 reflow）。**(c) oracle 分歧**：`DECRQM` 26（SD-22）。**(b) 的每一条仍须按 K-04 附最小复现 + 依据 + 豁免期限逐条登记**，不得整类一句「不做」带过——**这正是 SD-23 待追认的内容**。 | T1 | 逐簇修复并在**同一 commit** 重测；接入门禁前须两次自证 | §8.1-1、AR-27 | **[第 39 轮更正]** 此前记录的 117/407 与 110/414 是**调用参数不对**（漏了 `--xterm-checksum 336`，导致 esctest 对每个单元格校验和取反）所致的伪影；它们只在**同参数**下可比（SD-19 的 110→117 仍成立）。**任何 esctest 数字必须连同完整调用命令一起记录。**
| A5 | **无原生窗口 / GPU / IME 宿主**（E-P0-2 完全未实现） | T1 + T2 | 需先过 wgpu/winit/rustybuzz/swash 的依赖准入（ADR-0015） | §7 E-P0-2、DC-17、ADR-0024 D3 |
| A6 | **渲染依赖在本环境无法取得 SPDX 证据**（网络受限、无本地 cargo 缓存）→ 只能落地零依赖切片 | T5 | 在可联网环境跑 `cargo-deny` 输出后补 ADR | AR-21、ADR-0015 P3 |
| A7 | **§5 每条指标的测量实现未做**；**无 RM-A/RM-C** → `tools/bench` 打印 `gating numbers produced: 0` | T1 | 先起 RM-A；再逐指标实现（H1…H19） | B-10、AR-24.3、AR-27、ADR-0014 |
| A8 | **sessiond 不持有 PTY**（生命周期在 `apps/termai`）→「会话关闭 → ≤2s 回收孤儿」在守护进程层**未实现**；且 `close()` **契约上不隐式杀树** | T1 | sessiond 接管 PTY 时必须显式 kill；否则首次多会话即泄漏 | AR-30 第 2 条、§8.2 |
| A9 | **sessiond 重建 P95 ≤2s / P99 ≤5s 的验收未做**（**测量口径已定，实现待做**） | T1 | **口径（先定，避免实现走样）**：① 该指标属 **kernel/06 方法学**，测量落 `tools/bench`（`bench-report` + 机器指纹），**不得写成单元测试里的计时断言**——否则会变成 flaky，且违反 AR-27 的「测量先自证可复现」；② 需 **N 次冷重建**（N 与统计口径由 kernel/06 定）报 p50/p95/p99；③ 在**非 RM-A** 上产出的数字一律 **NON_GATING / INCONCLUSIVE**（ADR-0014），**不得**用它判定 §8.2；④ 断言只能落在 RM-A 上，且同 commit 连测两次 verdict 不得翻转 | AR-26 第 4 条、§8.2、AR-27、kernel/06、ADR-0014 |
| A10 | **跨段重放未实现**（TAIL_REPLAY 只读当前 segment；旋转后只会 BelowWindow） | T1 | 段滚动/归档后必须仍能重放或明确要求全量快照 | ADR-0026 §5 负面 1 |
| A11 | **TailReplay 事件只投影 5 类** → **不能替代 GRID_SNAPSHOT** | T1 | 新增 tag 须先出 ADR | ADR-0026 D5 |
| A12 | **IPC fuzz smoke 已落地**（`c495e07`：确定性 xorshift64*、固定种子、≥20000 次、随机帧头极端 `len` 不分配、CBOR 不 panic、含**正例对照组**）。**仍缺**：① **24h / ≥10⁸ 次**的持续 fuzz（G6 门禁本体，需 CI 排期）；② PTY 与 VT 侧的持续 fuzz；③ 8 MiB 上限端到端实跑与 broker 的 `FRAME_TOO_LARGE` 分支单测 | T1 | AGENTS §6；G6 = 24h 无 crash | §8.1-6、DC-37 |
| A13 | **VRM**：**映射 + 模式切换均已落地**（`1055f0f` 映射；`14bedbb` `VrmState` 持有/切换 + **RP-08 操作面断言**：`Fold→Clip→Fold` 后 `mirror.rev()`、`canonical_bytes()` 不变且 `take_damage()` 为空）。**仍缺**：`ScrollAnchor`、命中测试、a11y 投影、显示行总数、从配置读模式；另有一处已知粗化——`VisualRow.clipped` 是**单一 bool**，无法区分「遮住头/尾/两端」，而 kernel/03 §3.8 的 `VRowKind` 信息更多（按切片边界未做） | T1 | 每一步都不得改变列数与复制字节 | AR-23 §6、kernel/03 K-10、RP-08 |

**第 44 轮按同法复核 `DL` / `SD`**：两簇的**首个失败用例**分别是 `test_DL_ClearOutLeftRightAndTopBottomScrollRegion` 与 `test_SD_BigScrollLeftRightAndTopBottomScrollRegion`（`dl.py:215`、`sd.py:194`），名字里的 **LeftRight** 指向**左右边距**（`DECSLRM` / 使能模式 69）——**同样不在 §3.5 表内**，`set_private_mode` 无 69、`set_scroll_region` 只处理 `CSI r`。→ 这两个失败用例属 **(b) 子集外偏差候选**，**不是核心 DL/SD 错**。

**但不得整簇搬走**：`DL`/`SD` 各有多条用例，我只举证了**首个失败用例**；其余用例（纯上下滚动区域）**仍可能暴露真实核心缺陷**，必须逐条读前言后再定性。**举证到哪一条，就只能豁免到哪一条**——这是 K-04 逐条登记的意义，也是我上一轮把整簇 BS 搬走时差点犯的错。
**第 45 轮复核 `DECSET` 16 条（按用例名逐个归类，因为该簇是混合的）**：

| 用例 | 归类 | 依据 |
| --- | --- | --- |
| `DECAWM_NoLineWrapOnTabWithLeftRightMargin`、`DECAWM_OffRespectsLeftRightMargin`、`DECAWM_OnRespectsLeftRightMargin`、`DECLRMM` | **(b) 子集外** | 左右边距（`DECSLRM` / 模式 69） |
| `ReverseWraparoundLastCol_BS`、`ReverseWraparound_BS`、`ReverseWraparound_Multi` | **(b) 子集外** | `XTREVWRAP`（模式 45），与第 43 轮 `BS` 同因 |
| `Allow80To132`、`DECCOLM` | **(b) 子集外** | 132 列切换 = **应用请求 resize**，与 `DECSLPP` 同属「动态 resize + reflow」设计问题 |
| `ALTBUF`、`OPT_ALTBUF`、`OPT_ALTBUF_CURSOR` | **(a) 真缺陷候选** | 模式 1047 / 1049（切换时清屏 / 保存光标）——第 38 轮的回归测试只覆盖**模式 47**，1047/1049 未覆盖 |
| `SaveRestoreCursor` | **(a) 真缺陷候选** | `DECSC`/`DECRC` 是核心语义（vttest 也覆盖） |
| `DECOM`、`DECOM_DECRQCRA`、`MoreFix` | **(a) 待定** | `DECOM`（模式 6）我们已实现，需读前言确认是原点模式与边距/校验和的交互 |

→ **结论**：`DECSET` 不能整簇定性——**约一半是子集外（边距 / 反向回绕 / 132 列），另一半是待查的真缺陷候选**。下一步优先查 **(a) 候选里的 `SaveRestoreCursor`**（核心、最小、vttest 也覆盖），其次 `ALTBUF` 1047/1049 家族。
**第 46 轮复核 `SaveRestoreCursor`（我在第 45 轮把它列为「(a) 真缺陷候选，因为 DECSC/DECRC 是核心」）—— 又是错的**：`decset.py:565-573` 用的是 **`DECSET(SaveRestoreCursor)`**，即 **DEC 私有模式 1048**（`CSI ? 1048 h/l` 保存/恢复光标），而 `set_private_mode` 只处理 1/6/7/25/47/1047/1049/2004 → 光标未恢复（got `cursor.x()=5` vs expected 2）。**核心 `DECSC`/`DECRC`（`ESC 7`/`ESC 8`）我们本来就实现**，失败源于**未实现的扩展模式 1048** → 归 **(b) 子集外**。

**方法教训已经重复三次**（`BS`→模式 45、`DL`/`SD`→左右边距、`SaveRestoreCursor`→模式 1048）：**逐个读用例是错的粒度**。正确做法是**机械地全量分类**——对每条失败用例，抽取其函数体里用到的 `DECSET/DECRESET/DECRQM` 模式常量（经 `esccmd.py` 解析成数值），与「我们已实现的模式集合」比对，一次性给出 `uses-unimplemented-mode` / `no-extension-mode`。**这项分析已派单**（见 `docs/audit/esctest-triage.md`），**在它完成前，不再逐条读用例**——避免继续用低效且易错的方式产出结论。
> **A4 的权威分类来源（第 55 轮起）**：**机器产物 `docs/audit/esctest-classification.md`**（由 `tools/conformance/classify-esctest.mjs` 从 `docs/audit/esctest-failing-index.md` 纯变换生成）。**下面的逐簇/逐用例人工结论一律视为「历史归因记录」**——它们保留是为了留下证据链与三次自我更正的教训，**但不再作为当前事实**；引用当前分类请用那张表。 **✅ 流水线首次端到端验证（第 56 轮）**：CHT（`CSI Ps I`）/ CBT（`CSI Ps Z`）已实现（`f40d308`）。**实测（同一正确参数）**：passed **218 → 224（+6）**、failed **308 → 302（−6）**、CHT/CBT 失败 **6 → 1**、substitutions 仍为 0。**预测是「若归因正确则 +6」，实测正好 +6** —— 索引（308/308 定位）→ 分类（81/227）→ 定位 → 修复 → 可测效果，整条链条第一次闭合。**仍余 1 条**：`CHTTests.test_CHT_IgnoresScrollingRegion`（CHT 必须忽略滚动区域）——**未声称已修**，是下一个要读的用例。 **第 57 轮：那剩余的 1 条 CHT 已定性 = 范围问题，不是缺陷**。`cht.py:30-44` 先 `DECSET(DECLRMM)` 再 `DECSLRM(5, 30)`，期望 CHT 被**右边距**夹在 30（got 33）→ 属**左右边距族**（模式 69），按第 53 轮判断为 **(b) 子集外**（`kernel/01` 未登记 DECSLRM）。→ **CHT/CBT 的结论：不依赖左右边距的用例已全部修好；唯一剩余者是需要未登记能力的用例**。**另一处交叉验证**：分类器当初就把这条判成 `uses-unimplemented-mode`（模式 69），与人工复核一致——这是分类表准确性的一次独立确认。 **第 57 轮附带发现（供将来若决定做 DECLRMM 时用）**：`CSI Pl;Pr s` 当前会走进 `csi_dispatch` 的 `b's' if intermediates.is_empty() => save_cursor()`，**即左右边距设置被当成「保存光标」**。现在无害（我们不实现边距），但**一旦决定实现 DECLRMM/DECSLRM，必须先用参数个数区分二者**（DECSLRM 带 2 个参数，DECSC 的 `CSI s` 无参数）——**这是一处真实的序列冲突，不能留给实现者自己发现**。另：工作流**独立**用 `--include "test_(CHT|CBT)_"` 得到 6 passed / 1 failed，与我的全量结论一致——**两条独立方法互证**。 **⚠ 第 61 轮：第四个被推翻的假设（`CUD`/`CUU` 边距钳制）**。我核实了现状（`cursor_up`/`cursor_down` 的区间钳制**只在 `origin()` 开启时生效**），据此把条件改成「光标起始行落在 `[scroll_top, scroll_bottom]` 内」，**并实跑 esctest 验收**：结果是 **224 passed / 302 failed —— 与修复前逐字段相同**，两条用例**照旧失败**（`cud.py:62` 期望 `y=4`、`cuu.py:60` 期望 `y=2`，断言在**行**上，不在钳制上）。**改动已回滚**（不保留一个无法用数字证成、却会改变 CNL/CPL/VPR 行为的行为变更）。**结论：这两条失败的真实原因不是区间钳制**；需要读 `cud.py`/`cuu.py` 的函数体（可能涉及 origin mode 下的 CUP 语义或 DECSTBM 参数），**尚未定位**。**教训与第 38/39/43 轮同源**：**核实了一个机制，不等于核实了它与失败的因果**——「代码看起来该那样」与「改成那样能修好」是两件事，只有后者才值得改。 **第 62 轮：拿到了决定性数据，并确认第 61 轮的回滚是对的**。两条用例的**实际值**：`test_CUD_StopsAtBottomMarginInScrollRegion` **expected y=4，got y=24**；`test_CUU_StopsAtTopMarginInScrollRegion` **expected y=2，got y=1**。**两者都被钳到屏幕边缘（24 / 1），说明我的 `inside` 谓词评估为 false** —— 即**在 `CUD(99)`/`CUU(99)` 执行时，光标并不在我以为的滚动区间内**。而用例正文（`cud.py:53-59` / `cuu.py:51-57`）是 `DECSTBM(2,4)` → `CUP(Point(1,3))` → `CUD/CUU(99)`，**按理光标应在区内**。→ **真正未定位的是「`DECSTBM` + `CUP` 之后，我们的 `scroll_top/scroll_bottom` 与 `cursor_row` 到底是什么」**；下一步应当**直接打印这三个值**（而不是再从语义推理）。**方法教训（第 5 次）**：第 61 轮我先核实机制、再改，但**从未核实「失败时那三个变量的实际取值」**；本轮一拿到实际值（24 / 1）就能立刻判断谓词为假——**下一次遇到这类问题，第一件事是取实际值，不是读语义**。
>
> **仍需人判断的三件事（机器无法给出）**：① DECRQM 的 oracle 分歧（**SD-22**）；② 「esctest 全通过」的判定域是子集还是全集（**SD-23**，左右边距 72 条、颜色 40 条都挂在这上面）；③ 在**尚未渲染颜色**时报不报一套 256 色调色板才算诚实的能力声明。
### A4 附：簇级归因的方法教训（第 43 轮更正）

**上一轮我把 `BS` 的 8 条失败列进「(a) 真缺陷（核心 VT，必须修）」，这是错的。** 读 `bs.py:173-189` 的**测试前言**可见：该组用例先 `DECSET(DECAWM)`，再 **`DECSET(XTREVWRAP)`**（xterm 扩展模式 **45**），然后期望 BS 能**反向跨行**回退；而 `set_private_mode` **没有 45 这一支**，光标被夹在第 1 列（实测 got `Point(1,5)`，expected `Point(5,3)`）。**失败由未实现的扩展模式引起，不是核心 BS 语义错** → `BS` 更正为 **(b) 子集外偏差候选**（依据：XTREVWRAP 不在 `kernel/01` §3.5 表内）。

**方法教训（比这条更正更重要）**：esctest 的**簇名按特性分组，不等于根因在核心语义**。判定必须读**用例前言里启用了哪些模式/扩展**，不能从簇名推断。`DL` 7 / `SD` 6 / `DECSET` 16 等簇**必须按同法逐个复核后**才能定性。

**与上一轮注意事项并存（两者都成立）**：「不在 §3.5 表里」**本身**不构成偏差借口（第 42 轮的注意事项），但**当用例前言明确启用了我们未实现的扩展模式时，就有正面证据**指向子集外——这两条不矛盾：前者禁止**推断**豁免，后者要求**举证**豁免。
> **A4 的可读性维护（第 47 轮自省）**：A4 行经第 36–46 轮已累积大量逐簇更正与三处自我更正，**正在变成一坨难以阅读的补丁堆**——而「缺口被弄丢」正是本登记表要防的事，**表本身变难读就是同一个失败模式的另一种形式**。 **管道现状（第 49–51 轮）**：① 第 1 段已由负责人完成并入库——`tools/conformance/failing-index.mjs` → `docs/audit/esctest-failing-index.md`（**308 条失败用例，308 条定位成功，0 未解析**）；② 第 2 段（**纯变换**：按 `file:line` 读函数体、抽取 `DECSET/DECRESET/DECRQM` 模式、与已实现集合比对并分类）已派单，产物为 `docs/audit/esctest-classification.md`（含三类计数与 **top10 未实现模式号**）。**在那张表落地前，A4 不再新增逐用例人工结论**（前 46 轮已因此撤回三次）。 **分类表已落地（第 52 轮，机器产物）**：`docs/audit/esctest-classification.md`（由 `tools/conformance/classify-esctest.mjs` 生成，纯变换）。**308 条失败分类为：81 条 `uses-unimplemented-mode`（子集外候选）+ 227 条 `no-extension-mode`（核心缺陷候选）**。**最高杠杆单点：模式 69（DECLRMM）一项就占 81 条里的 72 条**；其后是 40×5、45×4、3×4、95×2、41×1、1048×1。**重要限制**：分类器只能看见 `DECSET/DECRESET/DECRQM` 调用，**看不见直接发出的序列**（如 `DECSLRM` 是 `CSI Pl;Pr s`，不是 DECSET 调用），所以 **227 是核心缺陷候选的「上界」，不是定论**——仍需按 K-04 逐条核。 **范围判断：模式 69 / DECSLRM（第 53 轮）**：`kernel/01` **全文 0 次提及** `DECSLRM` / `DECLRMM` / 左右边距（仓库内命中仅来自我方的分析文档与 esctest 用例名）。→ **左右边距未在任何处登记**，既不在 §3.5 扩展子集，也不在核心控制序列清单里。**结论：分类表里那 72 条属 (b) 子集外偏差候选，须按 K-04 登记（依据：未登记）；实现 DECLRMM/DECSLRM 是「超出冻结范围的能力新增」，需要 owner/ADR 决定，不能因为「能让数字变好」而顺手做掉。** **这条恰好是本节最该记住的一句：台账里最大的数字，不等于最大的义务。** 真正的靶子仍是那 **227 条核心缺陷候选（其中 218 条完全不涉及 DECSET）**。 **第 54 轮：从「核心缺陷候选」里挖到第一组真缺陷，同时发现分类器第二处盲区**。① **真缺陷（ECMA-48 标准、oracle 第 1 档）：`CHT`（`CSI Ps I`）与 `CBT`（`CSI Ps Z`）两簇共 6 条** —— `csi_dispatch` 里**既没有 `b'I'` 也没有 `b'Z'`**（`csi_known` 同样未列），所以它们被当成未知序列忽略。这是本项目 G1 台账里**第一条被定位的核心语义缺陷**（此前 SD-19 与 DECALN/HPA/HPR/REP 分别是窗口查询与遗漏序列）。② **分类器盲区（更正第 52 轮的 227）**：它只看 `DECSET/DECRESET/DECRQM`，**看不见 OSC 与直接 CSI** —— 因此 **40 条颜色用例（OSC 4/10/11/12 驱动）与 `BS_ReverseWrap*`（模式 45 驱动、但函数体里只看得见已实现的模式 7）都被错误归入「核心缺陷候选」**。→ **227 这个数应读作「未由 DECSET 调用解释的失败」，其中至少 40 条颜色 + 4 条反向回绕应从核心缺陷中剔除**。
>
> **处置**：等 `docs/audit/esctest-triage.md`（机器产物的全量分类）落地后，A4 的**逐用例/逐簇人工结论应被替换为**：
> 1. **一个指针**指向 triage 表（可复核的机器产物）；
> 2. **仅保留三类机器无法判定的开放问题**：**(i) SD-22 的 DECRQM oracle 分歧**；**(ii) SD-23 的「esctest 全通过」判定域**（子集 vs 全集）；**(iii) 颜色查询的能力声明决定**（无渲染时报不报 256 色）；
> 3. 历史更正**不删除**，但**移入单独小节**（保留证据链），不再与当前结论混排。
>
> **在此之前不执行这次重整**：triage 表未落地就想清理，会把唯一可信的分类依据提前删掉。
### A4 附：第 63 轮——两条测量路径互相矛盾（未解，已排除三种解释）

**证据**：① **直接探针**（在 `cargo test` 里 feed `\x1b[2;4r` → `\x1b[3;1H` → `\x1b[99B`）显示 `region=(1,3)`、光标 `(2,0)` **在区内**；**带上钳制改动后 `CUD(99)` → `(3,0)`，正是期望的 y=4**。② **同一次 esctest 全量**仍报 `expected 4 got 24` / `expected 2 got 1` —— 而**这两个值恰好等于「区间=全屏 0..23」时该改动所产生的输出**。

**已排除**：**(a) 二进制陈旧**（`exe` 的 LastWriteTime 比 `grid.rs` 新，且 `cargo build` 无 `Compiling`）；**(b) CUP 编码不同**（`esccmd.CUP(Point)` → `params=[point.y(), point.x()]` → `CSI 3;1H`，与探针一致）；**(c) 断言读的位置**（两条都断言在 `y()` 上）。

**未解之谜**：**esctest 路径下 `DECSTBM(2,4)` 之后 `scroll_top/scroll_bottom` 到底是什么？** 若为全屏，则两个 got 值同时成立。

**下一步（明确可执行）**：**在真实路径里取数**——让 harness 在 `CUD`/`CUU` 上输出一次性诊断（或让探针走同一条 `--server` 路径），直接打印 esctest 路径下的 region 与光标；**不要在另一个测试里复现序列再跨路径比较**。改动已回滚、探针已删除，证据留在本条。
### A4 附：第 64 轮——真实路径测量法（本会话最有用的工具）与 CUD/CUU 的半解

**方法**：不必靠 esctest 全量去判断一个改动是否生效，可直接驱动 harness 本体——它的行协议是 `FEED <hex>`：

```powershell
$s = "FEED 1b5b323b34721b5b333b31481b5b3939421b5b366e`nQUIT`n"   # DECSTBM(2,4); CUP(3;1); CUD(99); DSR
$s | & .\target\debug\termai-vt-conformance.exe --server
```

**实测**：改动前 `RESP 1b5b32343b3152`（`\x1b[24;1R`，第 24 行）；改动后 `RESP 1b5b343b3152`（`\x1b[4;1R`，**第 4 行 = 规范要求**）。**已提交**（`f4fe7fe`）。

**同时解释了第 61/63 轮的「无效果」**：那两次 `cargo build` 输出 `Finished ... in 0.04s` 且**没有 `Compiling` 行**——**量的是未含改动的旧二进制**。**教训：`cargo build` 必须看到 `Compiling termai-vt` 才算重建；否则测的是旧产物。**

**仍未解**：同一份 exe 直接 `FEED` 得到第 4 行，但**经适配器跑 esctest 仍是 `got 24`**（passed 仍 224 / failed 302）。**差异在适配器发送的内容或其建立的状态，不在此改动**。**因此那两条用例不声称已修好**；下一步：对单条用例 dump 适配器实际 FEED 的字节。
### A4 附：第 65 轮——单条用例的 3 秒复现回路（下一轮直接用）

**复现一条失败用例，不必跑 60 秒全量**（注意：`--include` 与 `--v` 都是 **esctest 参数，必须放在 `--` 之后**；适配器自己不认识它们）：

```powershell
python tools/conformance/upstream/esctest_adapter.py --esctest C:\Users\z5075\AppData\Local\Temp\termai-conformance-upstream\esctest2 `
  --out target/conformance/dbgcud -- --expected-terminal xterm --xterm-checksum 336 `
  --include test_CUD_StopsAtBottomMarginInScrollRegion --v 2
```

**已确认**：单条运行**照样复现**（`0 passed / 1 failed`，`got 24`），所以这个回路可用于快速迭代。

**但 `esctest.log` 里没有线缆字节**（`--v 2` 不含发送序列），所以下一步必须在**适配器边界**取数，二选一：① 临时在 `esctest_adapter.py` 的 `link.feed` 处打印 `data.hex()`（改一行，用完撤）；② 用适配器的 `--harness` 参数指向一个**会 tee stdin 的包装脚本**，再把包的 stdin 落盘。**取到真实 FEED 字节后，与第 64 轮手工 FEED 的 `1b5b323b3472 1b5b333b3148 1b5b393942` 逐字节比对**——差异就是答案。
### A4 附：第 66 轮——找到了 CUD/CUU 的真因，但修它使总数变差（已回滚，记录权衡）

**真因（已用字节级证据确证）**：适配器的 `feed()` **在每次 FEED 之后都发一条 `RESIZE <cols> <rows>`**（同一尺寸）。而本仓库的 `Grid::resize` **即使尺寸不变也会重建状态、清掉滚动区域**。实测：

```powershell
# 手工喂入：DECSTBM(2,4) ; CUP(3;1) ; CUD(99) ; DSR
FEED 1b5b323b3472 / RESIZE 80 24 / FEED 1b5b333b3148 / RESIZE 80 24 / FEED 1b5b393942 / RESIZE 80 24 / FEED 1b5b366e
# 结果：RESP 1b5b32343b3152  → \x1b[24;1R（第 24 行）＝ esctest 观测到的失败值
# 去掉中间的 RESIZE：RESP 1b5b343b3152  → \x1b[4;1R（第 4 行）＝ 规范要求
```

并且**适配器实际发送的字节与手工喂入逐字节相同**（`1b5b323b3472` / `1b5b333b3148` / `1b5b393942` / `1b5b366e`），**唯一差别就是那三条穿插的 `RESIZE`**。→ **滚动区域在测试中途被同尺寸 RESIZE 抹掉**，这就是那两条用例失败的原因，也解释了为什么直接 FEED 是对的、经适配器却错。

**修法与其代价（关键）**：给 `Grid::resize` 加「尺寸未变则直接返回」的守卫——**这在真实路径上确实修好了那两条**（`RESP 1b5b343b3152`），**但全量 esctest 从 224 passed / 302 failed 掉到 205 / 321（−19）**，且新失败的包括 `CUD_ExplicitParam`、`CUU_DefaultParam` 这类**基础用例**。

**解释（假说，需下一轮验证）**：适配器那条「每次 FEED 后 RESIZE」实际上**在充当测试之间的状态重置**；它把滚动区域/保存状态清掉，从而**掩盖了本终端在测试之间不重置状态的泄漏**。一旦同尺寸 RESIZE 变成真 no-op，泄漏暴露，测试互相污染。

**处置**：**守卫已回滚**（`git checkout`）；`kernel-gates` 8 PASS、`cargo test -p termai-vt` 0 失败。**正确的修法不在终端一侧单独完成**：要么① 适配器**只在尺寸变化时**发 RESIZE，并**显式**在用例间重置状态；要么② 终端提供/修正**用例间的复位**（`RIS`/`DECSTR` 是否完整重置滚动区域与保存状态——这本身可能就是缺陷）。**下一轮应从②开始查**：`esctest` 的 `reset()` 发什么、我们是否完整响应。
### A4 附：第 67 轮——状态泄漏的真因是 **DECSTR 未实现**（已派单）

**证据**：`grid.rs` 只有 RIS（`b'c' => self.reset()`，见 996 行），**没有任何处理 `CSI ! p`（DECSTR）的分支**；而第 66 轮捕获的 esctest 前导字节里**确实有 `1b5b2170` = `\x1b[!p`**。→ **esctest 的用例间复位指令被我们完全忽略**，滚动区域/保存状态因此跨用例泄漏；适配器那条「每次 FEED 后 RESIZE」**一直在替我们做复位**，所以第 66 轮加上 resize no-op 守卫后暴露泄漏、passed 从 224 掉到 205。

**因此正确顺序是**：**先实现 DECSTR**，再让 resize 的 no-op 守卫变得安全。**已按此顺序派单**，并要求它实测**三个数字**（只 DECSTR / 再加守卫 / 与基线 224-302 对比），且**若加守卫后仍低于基线，就只保留 DECSTR**——**不为「实现完整」保留一个让总数变差的改动**。

**这同时是一个独立的核心缺陷**（标准 xterm 控制序列缺失），与 CHT/CBT 同类：**由 esctest 抓到、被分类表归入核心候选、根因明确、可一轮实现**。
### A4 附：第 68 轮——用真实路径独立确证 DECSTR 是 no-op（并顺带确证第 64 轮的修复生效）

```powershell
# 控制组：DECSTBM(2,4) ; CUP(3;1) ; CUD(99) ; DSR
$s  = "FEED 1b5b323b34721b5b333b31481b5b3939421b5b366e`nQUIT`n"
# 实验组：在 DECSTBM 与 CUP 之间插入 DECSTR（1b5b2170）
$s2 = "FEED 1b5b323b34721b5b21701b5b333b31481b5b3939421b5b366e`nQUIT`n"
$s  | & .\target\debug\termai-vt-conformance.exe --server   # RESP 1b5b343b3152  → \x1b[4;1R
$s2 | & .\target\debug\termai-vt-conformance.exe --server   # RESP 1b5b343b3152  → \x1b[4;1R
```

**两个判读**：① **控制组给出第 4 行**（区域钳制生效）→ **第 64 轮的 `f4fe7fe` 在真实路径上确实有效**，再次确证；② **插入 `DECSTR` 后结果不变**→ **DECSTR 当前确实是 no-op**，与第 67 轮从代码得出的结论一致（**两条独立方法互证**）。

**一次失败的实验也记下**：最初我把控制组写成「`DECSTBM; CUD(99)`（**漏了 CUP**）」，得到第 24 行——那**不是** DECSTR 的证据，而是**因为 `DECSTBM` 把光标归位到区域外（第 0 行），按第 64 轮的语义「起始在区域外则按屏幕边界」**。**教训：判别性实验必须让两组只差一个变量，否则结论无从谈起**（我第一版两组都差了两个变量）。
## B. 尚未闭合的契约 / 规格登记（SD 系列）

| 编号 | 内容 | 状态 |
| --- | --- | --- |
| SD-13 | 逐行 LineFlags（ADR-0025） | **已实现并提交**（`67ee7f2`；后续 `bf90bba` 修掉一个真实集成缺口：**flag 单独变化也会 damage 该行**，否则跟随 `GridDelta` 的镜像永远看不到折行链——只有全量快照才会带上它）。**总负责人独立复核**（读码，非复述）：裸 `line_feed()` 只能经两个带注释的包装到达——`line_feed_explicit`（先 `clear_line_flags`）与 `line_feed_wrapped`（先 `mark_line_wrapped`），显式换行/IND/NEL 分别落在 899/942/945；四个行搬移算子（`scroll_up`/`scroll_down`/`insert_lines`/`delete_lines`）全部调用 `rotate_row_flags`；擦除路径（`erase_display`/`erase_line` 等）调用 `clear_line_flags`；`reset` 与 alt-screen 进出调用 `clear_all_line_flags`，alt 用 `saved_row_flags` 保存/恢复。**擦除分支复核（已完成）**：ED/EL 全部经 `erase_cells`，而它在**擦除触及右边缘时**才清 flag（`to >= cols-1`），并写明理由「越过右边缘会移除 wrap 链接本要延续的内容；保持右边缘的部分擦除**有意**不动 flag」。该规则与 ADR-0025 D1 的语义（flag 在「继续到下一行」的那一行）一致，且边界情形有注释——**未发现缺口** |；**待定的小问题**：镜像侧 `apply_snapshot` 对**长度不足的 `row_flags` 采取静默补零**（而非拒绝）——对可丢弃的镜像这是稳妥的，但**ipc 解码器**收到短数组时应当拒绝还是补零，需要一个 owner 明确（契约字段的容错方向不应由两处各自决定）
| SD-14 | GridDelta 的 scroll 双承载 | 未处置（render 侧已取单一优先级） |
| SD-15 | GridSnapshot 无 rev → 快照后基线未定义 | 未处置（镜像取「下一个 delta 的 rev 为基线」） |
| SD-16 | kernel/04 §3.4 首个 Interactive attach 自动授予租约与 AR-03 冲突 | **已裁决**（显式授权优先）；分册待修订 |
| SD-17 | `proto_range` vs `proto_min/proto_max` | 已接受实现命名；分册待补注 |
| SD-18 | attach 错误码未登记即暴露 | 已补登；`Corrupt`→`AttachStateInvalid` 切换已完成 |
| SD-19 | XTWINOPS（`CSI Ps t`）超出 kernel/01 §3.5 子集 | **已重做并落地**（`b772f1e`）：`termai-vt` 回答 `11/13/18/19 t`，`14/15/16 t`（像素类）**故意不答**并写明理由（AR-14）；适配器只代答像素类。**实测**：substitutions **688 → 0**、passed **110 → 117**、failed 414 → 407，两次运行逐字段相同 |
| SD-20 | esctest 记录值 201 不可复现 | **结论已更正（第 39 轮）**：201 **是可复现的**，当时的「不可复现」来自**调用参数差异**——我的比较运行漏了 `--xterm-checksum 336`，esctest 于是对每格校验和取反，制造出大量伪失败（110/117 对 201/218）。**教训**：测量必须连同**完整调用命令**登记，否则跨次比较无效（见 计划 §6.3 规则 8） |
| SD-21 | Context 事件 `confidence` 单位未定义（线上 f32 / Log u8） | 未处置；kernel/04 owner 待确认 |
| SD-01…SD-12 | 见 [m0-spec-defects](../plan/m0-spec-defects.md) / [p0-spec-defects](../plan/p0-spec-defects.md) | M0 部分已由 ADR-0020/0023 处置 |

## C. 流程与治理（不修则 P0 出口缺合法批准人）

| # | 未闭合项 | owner | 说明 |
| --- | --- | --- | --- |
| C1 | **TSC 未成立**（OQ-19） | 发起人 | §5/§8 的任何放宽目前**没有合法批准人** |
| C2 | **CODEOWNERS 双签无法执行**：仓库只有一个所有者；且 GitHub 对未解析的 `@termai/*` **静默忽略** → 规则可能退化为空操作 | 发起人 | E4 目前**不可强制** |
| C3 | **分支保护未启用** | 发起人 | required checks 无强制力 |
| C4 | **`ci-cost.json` 与 $3,000/月上限未实现**；新增 macOS runner 抬高成本 | T5 | ADR-0014 决策 6；落地前不得声称「CI 成本受控」 |
| C5 | **macOS arm64 从未真正编译/运行**；**Linux runner 的 K3 与 Node 门禁从未运行** | T5 | 两个新作业已标 UNVERIFIED |
| C6 | **季度依赖图 / 半年度技术栈体检** | T5 | spec 07 §3.9；本文件只覆盖 P0 阶段 |

## D. 已按纪律关闭（不再作为未决项）

- **OQ-RND-07**（Grid 字段集归属）：由 ADR-0023 D3 正式冻结；ADR-0025 增字段。
- **L-12 / L-15**：按「不修改历史」与「设计上不做」关闭（AR-31 补充第 2/3 条）。
- **G1 的 `R=1.0`**：仅是**当前 275 条语料**的实测，**不是 G1 通过**；任何报告引用它时都必须与 A1/A2/A3 同时出现，否则视为过度声称。