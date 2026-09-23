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
| A4 | **esctest 当前 **117 passed / 407 failed / substitutions 0**（SD-19 落地后两次实测一致）；**最大单簇 = DECRQM oracle 分歧（SD-22，26 条）**；其余失败簇：`CSI … t` 窗口尺寸（SD-19）、颜色族、DECRQM 余项、DECRQSS、DECDSR、DECSET、左右边距/原点模式 **簇级归因（第 36/37 轮实测，负责人自己做）**：① **DECRQM 26 = oracle 分歧**（SD-22，**不是 bug**）；② **XtermWinops 19 = 功能缺口**：`DECSLPP`（`CSI 8;rows;cols t`）必须**真的改变终端尺寸**（实测 `GetScreenSize()` 期望 90x10 实得 80x24），另有 iconify/deiconify 等窗口状态变更——**这需要 grid 支持动态 resize + reflow 语义**，与 kernel/03 的 reflow 设计连动，**不是快修**；③ **DECSET 18 至少含 alt-buffer（mode 47）内容语义差异**（实现在 rect 校验和上与期望不符，`decset.py:422`）。**已排除的假设**：曾怀疑「harness 的 DECRQCRA 全校验和实现错误导致大面积失败」，但 117 条通过里包含屏幕内容比对（如 `XtermWinops_Fullscreen` Passed），**故校验和机制不是共同根因**——不得用「harness 坏了」解释这些失败。 | T1 | 逐簇修复并在**同一 commit** 重测；接入门禁前须两次自证 | §8.1-1、AR-27 |
| A5 | **无原生窗口 / GPU / IME 宿主**（E-P0-2 完全未实现） | T1 + T2 | 需先过 wgpu/winit/rustybuzz/swash 的依赖准入（ADR-0015） | §7 E-P0-2、DC-17、ADR-0024 D3 |
| A6 | **渲染依赖在本环境无法取得 SPDX 证据**（网络受限、无本地 cargo 缓存）→ 只能落地零依赖切片 | T5 | 在可联网环境跑 `cargo-deny` 输出后补 ADR | AR-21、ADR-0015 P3 |
| A7 | **§5 每条指标的测量实现未做**；**无 RM-A/RM-C** → `tools/bench` 打印 `gating numbers produced: 0` | T1 | 先起 RM-A；再逐指标实现（H1…H19） | B-10、AR-24.3、AR-27、ADR-0014 |
| A8 | **sessiond 不持有 PTY**（生命周期在 `apps/termai`）→「会话关闭 → ≤2s 回收孤儿」在守护进程层**未实现**；且 `close()` **契约上不隐式杀树** | T1 | sessiond 接管 PTY 时必须显式 kill；否则首次多会话即泄漏 | AR-30 第 2 条、§8.2 |
| A9 | **sessiond 重建 P95 ≤2s / P99 ≤5s 的验收未做**（**测量口径已定，实现待做**） | T1 | **口径（先定，避免实现走样）**：① 该指标属 **kernel/06 方法学**，测量落 `tools/bench`（`bench-report` + 机器指纹），**不得写成单元测试里的计时断言**——否则会变成 flaky，且违反 AR-27 的「测量先自证可复现」；② 需 **N 次冷重建**（N 与统计口径由 kernel/06 定）报 p50/p95/p99；③ 在**非 RM-A** 上产出的数字一律 **NON_GATING / INCONCLUSIVE**（ADR-0014），**不得**用它判定 §8.2；④ 断言只能落在 RM-A 上，且同 commit 连测两次 verdict 不得翻转 | AR-26 第 4 条、§8.2、AR-27、kernel/06、ADR-0014 |
| A10 | **跨段重放未实现**（TAIL_REPLAY 只读当前 segment；旋转后只会 BelowWindow） | T1 | 段滚动/归档后必须仍能重放或明确要求全量快照 | ADR-0026 §5 负面 1 |
| A11 | **TailReplay 事件只投影 5 类** → **不能替代 GRID_SNAPSHOT** | T1 | 新增 tag 须先出 ADR | ADR-0026 D5 |
| A12 | **IPC fuzz smoke 已落地**（`c495e07`：确定性 xorshift64*、固定种子、≥20000 次、随机帧头极端 `len` 不分配、CBOR 不 panic、含**正例对照组**）。**仍缺**：① **24h / ≥10⁸ 次**的持续 fuzz（G6 门禁本体，需 CI 排期）；② PTY 与 VT 侧的持续 fuzz；③ 8 MiB 上限端到端实跑与 broker 的 `FRAME_TOO_LARGE` 分支单测 | T1 | AGENTS §6；G6 = 24h 无 crash | §8.1-6、DC-37 |
| A13 | **VRM**：**映射 + 模式切换均已落地**（`1055f0f` 映射；`14bedbb` `VrmState` 持有/切换 + **RP-08 操作面断言**：`Fold→Clip→Fold` 后 `mirror.rev()`、`canonical_bytes()` 不变且 `take_damage()` 为空）。**仍缺**：`ScrollAnchor`、命中测试、a11y 投影、显示行总数、从配置读模式；另有一处已知粗化——`VisualRow.clipped` 是**单一 bool**，无法区分「遮住头/尾/两端」，而 kernel/03 §3.8 的 `VRowKind` 信息更多（按切片边界未做） | T1 | 每一步都不得改变列数与复制字节 | AR-23 §6、kernel/03 K-10、RP-08 |

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
| SD-20 | esctest 记录值 201 不可复现 | **已二分定位 = 记录错误**；跨 commit 必须重测 |
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