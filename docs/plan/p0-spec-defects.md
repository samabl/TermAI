# TermAI P0 交付期规格缺陷登记（SD-09 起）

> **性质**：本文件是实现期**发现**，不是设计权威。M0 期的登记见 [m0-spec-defects.md](m0-spec-defects.md)（SD-01…SD-08）。
> **纪律**（AGENTS §5）：修正既有结论必须走 ADR；本文件只做**登记**，处置落在代码注释与本表，需要的追认动作单列。
> **依据**：HARNESS §11.2（CR-14 / CR-15 / CR-16）、§5、§8.1；**AR-24 第 3 条**（kernel/06 是测量口径的唯一权威）；ADR-0014（平台矩阵与参考机）、ADR-0023（P0 契约 errata）。

## SD-09｜spec 07 §3.8.2 的扁平 legacy 对象与 kernel/06 §3.7 的结构化字段同名冲突

- **证据**：spec 07 §3.8.2 期望一个扁平对象 `{metric,value,unit,samples,runner,commit,toolchain,ts}`；而 kernel/06 §3.7 的 bench-report 顶层 `commit` / `toolchain` / `runner` **都是对象**。把扁平字段并入顶层会与**同一节**的 schema 直接冲突。
- **本 P0 处置**：以 **kernel/06 §3.7 为唯一 schema 权威**（AR-24.3）；9 个 legacy 字段按 §3.7 原文落在 **metric 级 8 个 + 顶层 `commit`**；spec 07 §3.8.2 描述的扁平形态作为**加法字段 `flatProjection`** 承载（§3.7 允许加法字段向后兼容 1 minor），并由门禁 B2 每次运行打印该歧义。
- **需要的动作**：spec 07 §3.8.2 补注「扁平对象是 legacy 摘要形态，权威 schema 见 kernel/06 §3.7」；已登记 **HARNESS CR-16**。

## SD-10｜kernel/06 §4 的 FailKind 缺「绝对门禁越界」成员

- **证据**：§3.1 的 `FAIL(v, gate)` 需要表达「未超回归阈值但**越过绝对门禁**」（例如空闲 RSS >120MB、帧时 ≥8.3ms），而 §4 的 `FailKind` 枚举没有对应成员。
- **本 P0 处置**：`tools/bench` 以扩展项 **`GATE_BREACH`** 实现，标 `origin:'extension'`，**不冒充 spec 成员**。
- **需要的动作**：kernel/06 §4 增列该成员（分册修订，不改 AR/DC）。

## SD-11｜kernel/06 §3.2 未给 H3（PTY-LAT-1）的 Run 定义

- **证据**：AR-30 第 1 条把 **PTY-LAT-1 升为 §5 门禁**（P99 ≤2ms），但 kernel/06 §3.2 的测量定义只写到 H2；H3 的 Run / 样本量口径缺失。
- **本 P0 处置**：`tools/bench` 按「与 H2 共用注入流」读作 `isTail` / ≥1e5，标 `origin:'reading'`，并在 registry 与 README 显式登记为**读数而非引用**。
- **需要的动作**：kernel/06 owner 确认或改判；若改判，H3 的判定实现必须同步（否则 H3 会以错误口径进入门禁）。

## SD-12｜非 spec 原文的机器分类原因码

- **证据**：`CLOUD_RUNNER` / `FLOOR_MACHINE_ONLY` / `NOT_MACHINE_GATE` 三个原因码不在 kernel/06 正文，是由 **ADR-0014 决策 1 / 铁律 5** 与 **AR-31 第 8 条**推导出来的。
- **本 P0 处置**：标 `origin:'extension'` 并在 `tools/bench/README.md` 给出推导链。
- **需要的动作**：若 kernel/06 owner 认可，并入 §6 / §3.4 的枚举登记；否则应改为 spec 已有的码。

## SD-13｜GridSnapshot / RowPayload 缺逐行 LineFlags（WRAPPED），逻辑行无法重建

- **证据**：`kernel/03` §3.8 定义「逻辑行 = 由 `LineFlags::WRAPPED` 串起来的网格行链」，§3.3 的 `LineRecord` 亦带 `LineFlags`；而 core DTO v1（`crates/termai-core/src/grid.rs`）的 `GridSnapshot` 只有全局 `wrap_pending`，`RowPayload` 只有 `row` + `cells`，**没有任何逐行标志**。
- **影响**：软换行/裁剪（VisualRowMap，AR-23 §6 / kernel/03 K-10 / **RP-08**）无法实现——无法把网格行链成逻辑行；UX-G17「软换行开关下复制逐字节相同」因此不可判定。这是**对外契约级**缺口。
- **本 P0 处置**：不实现近似替代（不许按列宽猜折行，那会破坏复制保真）；`termai-render` 先落地镜像切片，VRM 等字段补充后落地。归属见 **ADR-0024 D2**。
- **需要的动作**：按 **ADR-0023 D3**（字段集以 `kernel/03` §3.3 为准）为 `GridSnapshot` / `RowPayload` 增加逐行 `flags`（至少 WRAPPED 位），并同步 `canonical_bytes` / golden / digest 的版本处理与 `kernel/01` 的 golden 规则。属**实现对齐已冻结设计**（minor 字段新增），须与 golden 哈希兼容性一并验证。 **已裁决：ADR-0025**——D1 定义字段与 `LINE_WRAPPED`；D2 把逐行 flags 纳入 `canonical_bytes` 并把 `GRID_DTO_MINOR` 升到 2（golden 保持 `TERMAI-GRID 1` 可解析、缺字段视为 0）；D3 由 `termai-vt` 产生、`termai-render` 消费。**实现待 W1-B 的 G1 语料收口后执行**，避免两边同时改 golden。

## SD-14｜GridDelta 的 scroll 字段重复承载

- **证据**：`crates/termai-core/src/grid.rs` 的 `GridDelta` 同时有 `scroll: Option<ScrollOp>` 与 `damage: Damage`（后者也带 `scroll`）；`kernel/03` §3.3 的伪代码同样两处并存。
- **影响**：应用顺序与「哪个是真源」无定义，两个实现者会做出不同选择，且可能双应用或漏应用滚动。
- **本 P0 处置**：`termai-render` 取 `delta.scroll.or(delta.damage.scroll)` 的**单一优先级**并加测试锁定，代码注释引用本条。
- **需要的动作**：kernel/03 owner 二选一并删除另一处，或显式写明「两处必须一致，否则以 `GridDelta.scroll` 为准」。

## SD-15｜GridSnapshot 不带 rev，快照替换后的 rev 基线未定义

- **证据**：`kernel/03` §3.3 用单调 `rev` 检测缺口，但 core DTO 的 `GridSnapshot` **没有 rev 字段**；替换镜像后客户端的下一个 delta 落在哪个 rev 上无定义。
- **影响**：镜像实现者会各自发明（拒绝第一个 delta / 无条件接受 / 从 0 起算），跨端 attach 与崩溃恢复的 rev 语义随之分叉。
- **本 P0 处置**：`termai-render` 的镜像规定「快照后**接受下一个 delta 并将其 rev 作为新基线**」，加测试锁定，代码注释引用本条。
- **需要的动作**：kernel/03 / 07 owner 决定是否给 `GridSnapshot` 增 `rev`（minor），或显式写明快照后的复位规则。

## SD-16｜kernel/04 §3.4 的「首个 Interactive attach 自动授予租约」与 AR-03 的显式授权冲突

- **证据**：`kernel/04` §3.4 的租约表规定首个 `Interactive` attach 在租约空闲时**自动授予**并写 `LeaseEvent{grant}`；而 **AR-03 契约层**规定「写 stdin **必须显式授权**」，AR-06 要求破坏性/写操作逐条批准。
- **影响**：自动授予等于「attach 即获得写权」，与 AR-03 的显式授权直接冲突；也把「订阅」与「写入」两种意图混在一次握手里。
- **裁决（总负责人）**：**以 AR-03 为准**。attach 只建立订阅；写权必须由显式 `LEASE_ACQUIRE` 获取。WS-05a 已按此实现：`Interactive` attach 的 `ATTACH_ACK.lease = null`，写 stdin 在显式取租约前仍 `CAP_DENIED`，且 CAP 闸门未被削弱。
- **需要的动作**：kernel/04 §3.4 的租约表按本条修订（自动授予改为「必须显式 acquire」），或新增 ADR 追认；在修订前**以本条为准**。

## SD-17｜AttachRequest 字段命名与 spec 不一致（proto_range vs proto_min/proto_max）

- **证据**：`kernel/04` §3.4 写 `proto_range`；实现（WS-05a）用 `proto_min` / `proto_max`，理由是复用 `Hello` 的同一对字段与同一求交函数（`handshake::chosen_version`），避免第二套版本区间表示。
- **影响**：纯命名，但属对外契约措辞；不统一会让跨端实现各自猜测字段名。
- **本 P0 处置**：接受实现命名（与 `Hello` 同源是更强的一致性论据），登记本条。
- **需要的动作**：kernel/04 §3.4 与 kernel/07 §3.3 把 `proto_range` 标注为「即 `proto_min` / `proto_max``」。另：`ATTACH_ACK` 无 `sub_id` 字段（订阅句柄仅存在于服务端，经 `Broker::attach_subscription()` 暴露），与 kernel/04 §3.4 一致，无需动作。

## SD-18｜attach 族对外错误码在 kernel/07 §3.8 未登记即暴露

- **证据**：kernel/07 §3.8 的登记规则要求「对外暴露前**先补入本表并冻结**，未登记即暴露视为契约事故」。M0 的 `on_snapshot_request` 与 WS-05a 的 attach 路径暴露了会话域字符串码 `NoSuchSession`；「未 attach / 状态非法」当前映射到已登记的 `IpcError::Corrupt`。
- **影响**：`NoSuchSession` 属未登记即暴露；用 `Corrupt`（语义为帧/载荷损坏）表达「状态非法」是语义借用，会让 CLI/UI 的错误分支不可靠。
- **本 P0 处置**：已在 **kernel/07 §3.8 补登** `NoSuchSession` 与 `AttachStateInvalid`（字符串码即契约，新增走 minor）；版本拒绝复用既有 `VerUnsupported`，不新增码。
- **需要的动作**：WS-05b 把「未 attach / 状态非法」从 `Corrupt` 切到 `AttachStateInvalid`（或在 kernel/07 §3.8 明确写成 `Corrupt` 的合法用法并给出理由）。在此切换完成前，不得声称 attach 的错误分支已冻结。

## SD-19｜`CSI Ps t`（XTWINOPS）窗口/文本区查询超出 kernel/01 §3.5 的扩展子集

- **证据**：esctest2 的 `reset()` 在每个用例前查询 `CSI 11/13/18/19 t` 并**阻塞等待答复**；`kernel/01` §3.5 的扩展协议子集表**未登记 XTWINOPS**，M0 的实现是 `b't' => {}`（静默忽略），迫使 esctest 做 **687 次传输替换**（逐条公示于 `tools/conformance/upstream/README.md`）。
- **影响**：不答复 → 真实应用（shell / 编辑器查询终端尺寸）会阻塞或退化；esctest 的通过率也无法在**无替换**前提下读取。
- **本 P0 处置（已实现并测过，当前处于回滚状态）**：曾实现 `11 t` → `CSI 1 t`（normal）、`13 t` → `CSI 3 ; 0 ; 0 t`、`18 t` → `CSI 8 ; rows ; cols t`、`19 t` → `CSI 9 ; rows ; cols t`，并同步让适配器**不再代答**这四项。实测（esctest2 全量 567 条）：**替换次数 688 → 0**，**通过数 110 → 110（与替换无关，pass-neutral）**。
- **回滚理由（已更正）**：我最初把「201 → 110」归因于这次改动，**该比较不成立**——同一条命令在当前工作区连跑两次都得到 **110**（见 **SD-20**），说明 201 是**另一个树状态**下的数字。因此这次改动**没有造成回归**（等通过、替换清零）；回滚只发生在我基于错误基线做保守判断的那一刻。
- **需要的动作**：① `kernel/01` §3.5 扩展子集表补登 XTWINOPS 已实现子集与「像素类不答复」的边界；② 在 `termai-vt` 实现 11/13/18/19 t（terminal 侧已写过一次，可直接重做）；③ `esctest_adapter.py` 与 harness **只能有一端答复**（适配器当前既转发又注入，是重复答复的来源）；④ **先解决 SD-20**，再以「替换 = 0 且通过数不低于当时基线」为判据重跑。像素类（14/15/16 t）继续由适配器按固定 window model 代答并公示。

## SD-20｜esctest-over-adapter：记录值 201 不可复现（**已二分定位 → 记录错误**）

- **证据（同一 esctest2 检出、同一适配器、同一命令；只换 harness 二进制）**：

  | harness | 通过 | known-bug | 失败 | feeds | substitutions |
  | --- | --- | --- | --- | --- | --- |
  | 提交 @@7fc89dd@@（W1-B 收口时的树，独立 worktree 重建） | **103** | 43 | **421** | 34022 | 688 |
  | 当前树（含 VT 修复 @@e141127@@）第 1 次 | **110** | 43 | **414** | 34024 | 688 |
  | 当前树第 2 次 | **110** | 43 | **414** | 34024 | 688 |

- **结论（三条）**：① **W1-B 记录的 201 在它自己的提交上也不可复现**（该提交实测 103）——`201` 判为**错误记录**，任何报告不得再引用；② 其后的 VT 修复（DECALN/HPA/HPR/REP + intermediates 语义）使通过 **+7**（103 → 110）、失败 **−7**（421 → 414），是**改进而非回归**；③ **测量在每个提交内是确定性的**（同一状态连跑两次逐字段相同）。
- **对 SD-19 的影响**：SD-19 的 window-op 实验是 **pass-neutral**（110 → 110）且把替换从 **688 清零**，回滚仅因我基于错误基线（201）做了保守判断；更正已写回 SD-19。
- **本 P0 处置**：SD-20 从「测量不可信」改判为「一次记录错误 + 方法教训」：**跨提交比较 esctest 数字必须重测，禁止复用旧值**。
- **剩余动作**：把 esctest 接入 CI（或任何门禁）前，必须① 固定 harness 二进制与 @@substitutions@@ 口径；② 在同一 commit 上跑两次做可复现性自证（AR-27）；③ 报告里同时给出 @@substitutions@@ 数与是否为零。

## SD-21｜Context 事件的 `confidence` 单位未定义（线上 f32 与 Log u8 之间无映射规定）

- **证据**：kernel/07 §3.6 把 `CommandBoundary.confidence` 定为 `f32`；kernel/04 §3.2.3 的 `CmdEnd` 记录只列字段名、**未写类型与单位**，而仓库实现（termai-session 的 Log 记录、termai-vt `shell.rs`）用的是 `u8`。
- **影响**：线上与 Log 之间没有定义的换算（百分比 0–100？0–1？0–255？）。投影时若猜错，该字段会**静默失真**；而 AI 侧的提示符启发式置信度（kernel/01 §3.6 的 `confidence = Low` 语义）直接依赖它。
- **本 P0 处置**：**线上保持 `f32`**（ADR-0026 D6；§3.6 是 Context 事件 schema owner）；`u8 → f32` 的换算必须在投影点**显式写出并注释**，禁止隐式 `as` 转换；单位确认前**不得声称该字段已冻结**。
- **需要的动作**：kernel/04 owner 在 §3.2.3 写明 `CmdEnd.confidence` 的类型、单位与取值范围；若确为百分比，换算固定为 `f32 = u8 as f32 / 100.0`，并同步 kernel/07 §3.6 的注释。

## SD-22｜DECRQM 对「未实现的 ANSI 模式」应回 Pm=0 还是 Pm=4（oracle 分歧，26 条 esctest 失败）

- **证据**：`esctest2` 的 `DECRQMTests`（本次实测 **26 条失败，是单一最大簇**）用 `doPermanentlyResetAnsiTest` 断言 `${T}requestAnsiMode(mode) == [mode, 4]${T}`，即「**永久复位**」；涉及 EBM/FEAM/FETM 等 ECMA-48 ANSI 模式。本仓库按 xterm ctlseqs 的 DECRQM 语义，对**未跟踪**的模式统一回 **Pm=0（未识别）**。
- **为什么不能直接改成 4**：Pm=4 的含义是「本终端**永久复位**该模式」。对一批我们既未实现、也未打算实现为可设置的模式回 4，等于**对外声明一种能力状态**；而回 0 是「我不认识」。在拿到 xterm 实现或 ctlseqs 条款之前，**改哪一边都是猜**。
- **本 P0 处置**：**保持 Pm=0**，并把 26 条失败登记为**oracle 分歧**而不是就地改绿。依据 K-03 的仲裁顺序（ECMA-48 > ctlseqs 文档 > xterm 实现 > esctest 期望），**esctest 是优先级最低的一档**，不足以单独推翻我们对 DECRQM 语义的读法。
- **需要的动作**：取到 xterm 实现（或 ctlseqs 中 DECRQM 的原文条款）后判定：① 若 xterm 对未实现 ANSI 模式确实回 4，则按 K-03「实现行为」档**修改我方实现**并回归；② 若 xterm 回 0，则按 K-04 把这 26 条**登记为差异**（附最小复现 + 条款/行为引用 + 豁免期限），并在报告里显式扣减。**在此之前不得声称这 26 条已解决。**

## SD-23｜HARNESS §7 的「esctest 全通过」与 kernel/01 §3.5 的「扩展子集」互相冲突（决定 40+ 条失败是缺陷还是偏差）

- **背景**：HARNESS §7 的 P0 出口写「**vttest + esctest 全通过**」；而 `kernel/01` §3.5 只声明了一个**扩展协议子集**，登记的是 OSC **133 / 633 / 7 / 0 / 2 / 8 / 9 / 777 / 52（仅写）**。第 40 轮的簇归因显示，`esctest` 里最大的可修簇 **40 条（颜色三族）** 测的是 **`OSC 4 / 10 / 11 / 12` 颜色查询**——**这四个根本不在子集里**，我们一律不答复，于是适配器超时。
- **为什么这不可能是「缺陷」**：`kernel/01` §3.5 明文写 **OSC 52 读取「永不实现」**（与 AR-29.5 一致），若把「esctest 全通过」按字面理解为「每一条 xterm 序列都通过」，则**我方自己冻结的子集就已经让 P0 在构造上不可能达成**。因此两种读法必有一错。
- **两种读法及其后果**：
  - **读法 A（字面）**：esctest 每条都必须通过 → 必须实现 OSC 4/10/11/12（以及 esctest 覆盖的其余全部 xterm 扩展），**P0 范围显著扩大**，且与 §3.5「永不实现 OSC 52 读」直接矛盾。
  - **读法 B（子集 + 差异）**：**「全通过」判定在已声明子集上**；子集外的序列按 **K-04** 进**差异登记表**（附最小复现 + 依据 + 豁免期限），**不计入**通过率的分母之外，也不假装通过。
- **本 P0 处置**：**采读法 B 作为工作口径**（否则 P0 自相矛盾），但把它登记为**待 owner 追认**的冲突，而不是我单方面改判出口标准——**E-P0-1 的对外状态在追认前保持「未判定」**。
- **需要的动作**：① `kernel/01` owner 明确「esctest 全通过」的判定域（是全集还是子集），并在 §5 V-04 写清；② 若是子集，则必须给出**子集外序列的差异登记流程**（K-04 已有双签与期限机制）与**通过率的分母口径**；③ 在第 41 轮之后的账里，把 40 条颜色失败**按颜色族单独列账**，标注「子集外，待读法 B 追认」，**不得混入产品缺陷数**。

## 处置总表

| 编号 | 落点 | 类型 | 本 P0 处置 | 需要动作 |
| --- | --- | --- | --- | --- |
| SD-09 | spec 07 §3.8.2 与 kernel/06 §3.7 | schema 冲突 | kernel/06 §3.7 为权威；扁平形态入加法字段 `flatProjection` | spec 07 §3.8.2 补注；**CR-16 已登记** |
| SD-10 | kernel/06 §4 | 枚举缺成员 | `GATE_BREACH` 扩展项 + `origin:'extension'` | kernel/06 §4 增列 |
| SD-11 | kernel/06 §3.2 | 定义缺失 | 按与 H2 共用注入流读作 `isTail`/≥1e5，标为**读数** | kernel/06 owner 确认或改判 |
| SD-12 | kernel/06 §3.4/§6 | 原因码越界 | 标 `origin:'extension'` 并写明推导链 | owner 认可后并入枚举 |

## 与前序登记的关系

- SD-01…SD-08 的处置见 [m0-spec-defects.md](m0-spec-defects.md)；本文件**不重复**其内容。
- SD-07 / SD-08.1 / SD-08.2 已由 **ADR-0023** 处置（见该 ADR 与 m0-spec-defects 处置总表）。
