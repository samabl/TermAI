# tools/bench — B-10 性能测量方法学落地（W1-C）

> 本目录实现 `docs/spec/kernel/06-performance-methodology.md`（简称 kernel/06）的「怎么测、怎么判、
> 测不准怎么办」。它是 **方法学工具**，不是测量工具：它校验 schema、计算指纹、实现判定状态机、
> 登记 §5 映射表，并用 `--selftest` 证明判定逻辑不是恒绿。
>
> **本机没有 RM-A / RM-C 参考机**，所以本工具在任何一次运行中产出 **0 个机器绑定的门禁数字**
> （`gating numbers produced by this run: 0`）。任何真实的*性能*数字必须来自 `cargo xtask bench` 在
> RM-A / RM-C 的 T0 后端上的运行（crates/termai-bench，M0 未实现）。这是 AR-20 诚实原则与 ADR-0014
> 铁律 5 的落地，不是配置项。
>
> **例外：H12（输入字节等价）不是机器绑定行**（registry `machine: 'none'`，逐字节比较与机器无关），
> 因此它可以在本机产出真实值并由 `check.mjs --report` 判定；它仍然**不计入**那个机器绑定的计数器
> （ADR-0029 D-5）。产出器见 `tools/bench/input-bytes.mjs` 与下文「H12 输入字节等价的产出与读取」。

## 命令

```text
node tools/bench/check.mjs              # 运行 B1-B7 + B9（npm run bench:check）
node tools/bench/check.mjs --selftest   # 注入故障，证明判定逻辑不是恒绿（npm run bench:selftest）
node tools/bench/check.mjs --json       # 仅输出机器可读 JSON
node tools/bench/check.mjs --report <p> # 校验 schema（B8）**并读取其中的值**，把每个 metric 绑定到它的 §5 行或可靠性行（R1/R2）
node tools/bench/check.mjs --machine <p># 显式声明本机为已登记的 RM-A / RM-C（默认不声明）
node tools/bench/sessiond-rebuild.mjs   # 【产出】驱动 apps/sessiond 的重建路径 N 次，算 P95/P99，写出 R1/R2 报告（AR-26 第 4 条 / kernel/06 §3.10）
node tools/bench/input-bytes.mjs        # 【产出】把 tools/bench/input-corpus.json 喂给 termai-core 的 InputEncoder，逐字节比对，写出 H12 报告（HARNESS §5 H12 / kernel/05 §5 IN-AC / AR-29 第 3 条）
```

退出码：`0` = PASS，`1` = FAIL。缺失的报告文件是显式 `SKIP`，绝不静默 PASS。

## 文件与职责

| 文件 | 职责 |
| --- | --- |
| `lib.mjs` | schema 引擎 + bench-report / machine-fingerprint schema、稳定 JSON 与 sha256 指纹、§3.1 判定状态机 `evaluate()`、`regression()`（G4-PR / G4-REL）、`gateAndRegression()`、`assertReproducible()`（AR-27 自证）、指标族阈值策略、`classifyMachine()` 诚实边界 |
| `registry.mjs` | HARNESS §5 的机器可读登记：H1…H19（19 条）+ 表外对照 C1；每条含 owner / 测量定义落点 / 门禁载体 / 是否 kernel/06 管辖 / 指标族 / 参考机；`mappingIntegrity()` 做缺号与重复校验 |
| `reliability.mjs` | **HARNESS §8.2 可靠性时序的独立登记**（ADR-0029 D-4）：`R1` = sessiond 重建 P95 ≤2000ms、`R2` = P99 ≤5000ms（ms / 族 frame / 机器 RM-A / owner kernel/06）；每条含测量定义 / 载体 / kernel/06 §3.10 锚点；`reliabilityIntegrity()` 做**独立口径**的完整性校验（恰好两行、无缺号、无重复、每条有 owner / carrier / 测量定义），**不复用** `MAPPING_ROW_COUNT` / `OUT_OF_TABLE_IDS` |
| `fixtures.mjs` | **合成逻辑夹具（不是测量值）**：只用于把状态机推过每个分支，绝不当结果上报、绝不与基线比对 |
| `values.mjs` | **值的读取与呈现**（D-6 第 ②–④ 步）：把报告里的每个 metric **绑定到它声称的 §5 行**（unit / gate 必须等于 registry 的转录）；机器无关行的「未报告」是**显式**的；`gatingNumbersProduced` 从读取到的值计算。**同一文件另设**可靠性行的读取（`reliabilityRow` / `bindReliabilityMetric` / `collectReliabilityValues`）：metric 指名 R1/R2 时 unit / gate / statistic 必须等于 `RELIABILITY_MAPPING`，且**本机无参考机时只允许 `INCONCLUSIVE` / `NON_GATING`**（写成 `PASS` 或声明 `gating:true` 即违规） |
| `sessiond-rebuild.mjs` | **产出（producer）**：驱动 `apps/sessiond` 的重建路径（`sessiond::restore::rebuild_session`）每个 Run N 次，记录**每次重建的墙钟时间**，按 kernel/06 §3.2 frame 行 + K-02 三层口径算 Run 内 P95/P99、报告值取 N≥10 Run 的**中位数**，写出 §3.7 形状的 `bench-report.json`；本机无参考机 ⇒ 两行一律 `INCONCLUSIVE` / `gating:false`，**绝不 PASS** |
| `input-corpus.json` | **H12 的语料（数据文件，不是代码）**：75 条输入事件 + 每条**引用权威**的期望 PTY 字节（`authority` / `authorityKind`），覆盖 keyboard（Legacy / ModifyOtherKeys(2) / Kitty）、paste、ime_commit、mouse、focus、api_inject 六源；**没有权威的期望不写进 `cases`**，一律登记在 `omitted` 里（11 条）并在产出时打印 |
| `input-bytes-driver.rs` | **H12 的驱动（不是第二个编码器）**：用 `rustc --extern termai_core=<rlib>` 编译，只做「读行协议 → 调 `StandardEncoder::encode` → 记录 sink 里的字节与 `EncodeOutcome`」，自身不含任何编码逻辑；`crates/**` 一字未改 |
| `input-bytes.mjs` | **产出（producer）**：`cargo build --release -p termai-core` + `rustc` 编译上面的驱动，把语料按 TSV 行协议喂进去，**逐字节**比较（同时比较 `EncodeOutcome`），写出 §3.7 形状的 `bench-report.json`（metric `H12`，`unit: pct`、`gate: 100`）+ 逐 case 明细；打印**每源覆盖数 / 缺席源 / 刻意省略的期望**；两次驱动运行做 kernel/06 §3.6 D0 自证；退出码 0 = H12 100%、1 = 有用例不符（报告照样写出）、2 = 产出器自身跑不起来 |
| `check.mjs` | 门禁入口 B1–B9 + `--selftest` 83 条注入（本轮为 H12 这一「机器无关门禁行」的判定加 2 注入 + 3 对照；~~78 条（第 258 轮）~~） |

## 门禁 B1–B9

| # | 门禁 | 校验什么 | 依据 |
| --- | --- | --- | --- |
| B1 | HARNESS §5 → registry 逐字对照 | 解析 `HARNESS.md` §5 表格的 19 行，逐字符比对指标名 / 发布门禁单元格 / 目标单元格 | HARNESS §5「映射要求」 |
| B2 | kernel/06 §3.4 / §3.7 → schema 逐字对照 | 解析正文 JSON 示例块，比对字段名集合是否**完全相等**；核对 9 个 legacy 字段 | kernel/06 §3.4、§3.7 |
| B3 | H1…H19 映射表完整性 | 无缺号、无重复、恰好 19；与 kernel/06 §3.9 表格逐行交叉核对（含「属 kernel/06 管辖」列） | kernel/06 §3.9、AR-24 第 3 条 |
| B4 | 阈值转写对照 | 解析 kernel/06 §3.6 的 `eps_self` 表与 §3.1 代码块，核对本工具中的每个常量 | kernel/06 §3.1/§3.6、AR-31 第 9 条 |
| B5 | 指纹确定性 | 同输入同哈希；41 个叶子字段**逐个**改动都改变哈希；键序无关 | kernel/06 §3.4、§3.7 |
| B6 | §3.1 状态机分支覆盖 | 6 条对照分支 + 21 条故障分支全部产出文档规定的裁决 | kernel/06 §3.1 |
| B7 | 机器绑定诚实边界 | 无参考机时必须 NON_GATING / INCONCLUSIVE 且产出 0 个门禁数字 | ADR-0014 铁律 5、kernel/06 §6、AR-31 第 8 条 | **⚠ 第 202 轮注**：**「产出 0 个门禁数字」这一条目前是**恒真**的——`gatingNumbersProduced` 是 `check.mjs:672` 的**字面常量 0**，没有任何代码从结果计算它（第 188/189 轮核实）。** `B7` 的另外两条判据是活的（机器分类、无指纹情形）。**因此本行描述的是**要求**，不是**当下被强制的事实**；把计数器做成计算值是 `docs/plan/p0-open-decisions.md` D-6 的第 ④ 步。** **✅ 第 255 轮：该步已完成**——`gatingNumbersProduced` 现由 `values.mjs` 读取到的 metric 计算（声明 `gating:true` 者计入），注入一个产出 gating 数字的行会被 `B7` 判 FAIL；该注入与对照已进 `bench:selftest`（63/63）。**
| B8 | 报告 schema 校验 **+ §5 行绑定 + 机器无关门禁行判定 + 可靠性行绑定**（`--report`，或树中存在报告时） | ① 对 bench-report.json 做 §3.7 校验；② 每个 metric 若指名某个 §5 行，其 `unit` / `gate` 必须等于 registry 的转录（否则 FAIL）；③ 机器无关行的「未报告」显式列出；文件不存在则 SKIP；④ 每个 metric 若指名 R1/R2，其 `unit` / `gate` / `statistic` 必须等于 `RELIABILITY_MAPPING`（B9 每次从 kernel/06 §3.10 正文重新推导），且**本机无参考机时该行只允许 `INCONCLUSIVE` / `NON_GATING`**——写成 `PASS` 或声明 `gating:true` 即 FAIL；⑤ **机器无关的 §5 门禁行**（registry `machine: none` 且 `governed != 'no'`，当前为 `H12`）要**按它自己申报的 verdict 判定**：`verdict: FAIL` → B8 FAIL（读了一个门禁行却不判定它，等于让失败的载体从读它的门禁下溜过去；`governed: 'no'` 的 H17/H18/H19 不是 §5 门禁，不受此条约束） | kernel/06 §3.7、spec 07 §3.8.2、HARNESS §5、kernel/06 §3.10、ADR-0029 D-4 / D-5、AR-20 |
| B9 | 可靠性登记完整性（HARNESS §8.2 / kernel/06 §3.10） | 每次从 kernel/06 §3.10 正文**重新推导** P95 / P99 门禁数（2000 / 5000 ms）；核对 `RELIABILITY_MAPPING` 与正文（指标名 / 门禁 / 族 / 机器绑定）；`reliabilityIntegrity()` 报错、正文不再含该契约、或登记与正文不一致 → FAIL；并断言 `SECTION5_MAPPING` 仍恰好 19 行、B1/B3 仍 PASS，且 `reliability.mjs` 的可执行代码（去注释后）未引用 §5 的登记口径 | ADR-0029 D-4、kernel/06 §3.10、AR-26 第 4 条 |

## 可靠性时序登记与门禁 B9（ADR-0029 D-4 / kernel/06 §3.10）

HARNESS §8.2 的「sessiond 重建 P95 ≤2s / P99 ≤5s」（AR-26 第 4 条）**不属于 §5 的十九行**，因此**不得**进入 `SECTION5_MAPPING`——那张表是 HARNESS §5 的逐字转录，B1/B3 每次从文档重新解析比对，加一行即是假转录（ADR-0029 D-4）。`reliability.mjs` 对这份契约做**容器同一、登记分离**：

| 事实 | 落点 |
| --- | --- |
| 两行指标 | `R1` = `reliability.sessiond_rebuild.p95`（gate ≤2000ms）、`R2` = `reliability.sessiond_rebuild.p99`（gate ≤5000ms）；`unit: ms`、`statistic: p95/p99`、`family: frame`、`machine: RM-A`、`owner: kernel/06` |
| 契约来源 | kernel/06 §3.10（门禁行 2000 / 5000、指标族 frame、机器绑定 RM-A、Run 定义、自检 §3.6 D0/D1 + `assertReproducible()`、报告走 §3.7 `bench-report.json` 且 `metric` 以 `reliability.` 前缀区分） |
| 完整性 | `reliabilityIntegrity()`：恰好两行、无缺号（R1/R2）、无重复、统计量集合恰为 p95/p99、每条有 owner / carrier / 测量定义 / §3.10 锚点 |
| 独立性 | **不 import、不复用** `MAPPING_ROW_COUNT` / `OUT_OF_TABLE_IDS` / `SECTION5_MAPPING`；B9 扫描 `reliability.mjs` 的可执行代码（去注释后）确认未出现这三个标识符与 `registry.mjs` |
| 门禁 B9 | 每次从正文重新推导两个门禁数；正文不再含该契约、登记与正文不一致、或 `reliabilityIntegrity()` 报错 → FAIL；并断言 §5 仍恰好 19 行、B1/B3 仍 PASS（**B1/B3 的行为不变**） |
| 本机状态 | 无 RM-A → 本节任何数值一律 `INCONCLUSIVE` / `NON_GATING`；判定人待 TSC（kernel/06 §3.10「本机状态」） |

第 257 轮 B9 的自证：`--selftest` 为它加了 5 条注入（重复行、缺 R2、缺 owner、门禁数与正文不符、正文不再含 §3.10）+ 3 条对照，另加 1 条真实树基线——`bench:selftest` 由 **65/65** 变为 **74/74**。

## sessiond 重建的产出与读取（AR-26 第 4 条 / kernel/06 §3.10）

HARNESS §8.2 的「sessiond 重建 P95 ≤2s / P99 ≤5s」此前只有**登记**（`reliability.mjs` + B9），没有产出：`docs/audit/debt-p0.md` A9 记的就是这一步。本轮把它补齐，且**只补在允许的路径内**（`apps/sessiond/**`、`tools/bench/**`），`SECTION5_MAPPING` 与 B1/B3 一字未动。

| 环节 | 落点 |
| --- | --- |
| **机制**（必须先成立） | `apps/sessiond/src/restore.rs` 的 `rebuild_session()`：Log 读取 → checkpoint 窗口 → VT 尾回放到**全新引擎**；`apps/sessiond/tests/session_rebuild.rs` 断言**重建后的屏幕与重建前逐字节相等**（`GridSnapshot` 整体相等 + `canonical_bytes()` 相等 + digest 相等 + 逐行 `row_text` 相等），**断言等值而不是阈值**——这一半不允许 flaky |
| **计时** | `apps/sessiond/examples/rebuild_bench.rs`：对同一个确定性 Log 夹具做 N 次重建，每次记录 `rebuild_session` + `GRID_SNAPSHOT` 快照 + digest 的墙钟时间；等值比较在**计时窗口之外**做，夹具的偏置不进读数 |
| **统计量** | kernel/06 §3.10 + §3.2 frame 行 + K-02 三层：样本（每次重建）→ Run 内 `p95`/`p99` → 报告值 = **N≥10 个 Run 统计量的中位数**；报告里的 `runStat: "median"`、`runs`、`samples` 三层同出。**插值规则**文档未规定，本工具用最近秩（`⌈p·n⌉` 个最小值）并在 `method` 里明说，不把选择留成隐含约定 |
| **Run 定义** | kernel/06 §3.10 本行标 `origin:'reading'`：一次 sessiond 重建 = 客户端重连 / attach 起 → 可用的 `GRID_SNAPSHOT`（含 digest）为止的**服务端可观测段** |
| **样本下限** | §3.10 要求 **Run 内 ≥1e4 样本**才报 P99；产出默认 `--samples 10000`，低于下限时 `method` 与 stdout 都会写明「在参考机上该 Run 会是 `INVALID(INSUFFICIENT_SAMPLES)`」，不静默降格 |
| **机器绑定** | 本机无 RM-A 指纹 ⇒ `INCONCLUSIVE(REFERENCE_MACHINE_UNAVAILABLE)` + `gating:false`（kernel/06 §3.10「本机状态」+ ADR-0014 铁律 5）；§3.1 第 1 步同样会拒绝（`INVALID(FP_MISSING)`），两条路都到不了 PASS |
| **报告落点** | 默认写 `target/bench-reports/sessiond-rebuild-report.json`（**gitignore 区**，且不被 `discoverReports()` 扫描），因此 `bench:check` 的默认输出仍是 `summary: 8 PASS`；要看这些行必须显式 `--report <path>` |
| **门禁读取** | `check.mjs --report <p>` 会把 R1/R2 连同值、门禁、统计量、样本数、Run 数、MAD/中位数一起打印；本机非门禁时逐行标 `INCONCLUSIVE`。**新增判定**：R1/R2 在无参考机的本机写成 `PASS` 或声明 `gating:true` ⇒ B8 / B7 FAIL |
| **夹具** | `sessiond::restore::fixture::write_session_log()`：用**真实 sessiond 代码路径**（`Registry::feed_pty_out`）写出确定性 Log（SGR + CJK + 组合符 + 换行折行 + CUP 覆盖 + OSC 标题），时间戳与段头全为 0，因此同一场景跨 Run 字节相同（D0 场景冻结：`log_sha256` + `state_digest` 逐 Run 相同才判 PASS） |

**已知边界（AR-20，必须如实说）**：

1. 可测的是**无 checkpoint 的全量重放**路径。`CheckpointRef` 那条分支需要 CAS 内容存储，而 `apps/sessiond/src/registry.rs` 的 `EngineReplay::restore` 目前是 M0 适配器（报告一次还原尝试，并不从 CAS 载入字节）——所以带 checkpoint 的 Log **尚不可忠实重建**；夹具断言 `checkpoints == 0`，驱动遇到 `resumed_from.is_some()` 直接拒绝对外发布读数。
2. `recover_session` 只回放 `PtyOut`，**不回放 `Resize`**，因此跨终端尺寸的 Log 也不可忠实重建；夹具只用一个尺寸。
3. 本机**不是 RM-A**：这两行永远是 `INCONCLUSIVE` / `NON_GATING`。要成为门禁数字，必须在 RM-A（T0、独占、指纹已登记）上跑同一命令并给出指纹。
4. 场景是**参数化的**（默认 Log 500 行 / ~45KB）；换场景即换数字，报告 `metrics[].method` 与 `runner` 里都写了场景参数与机器状态。

## H12 输入字节等价的产出与读取（HARNESS §5 H12 / kernel/05 §5 IN-AC / AR-29 第 3 条）

HARNESS §5 的 H12「输入字节等价（键盘 / 粘贴 / IME commit 全语料回放，逐字节比对）= 100%」在 registry 里是 **`machine: 'none'`**：逐字节比较不需要参考机，也不需要 T0 后端。它是 §5 十九行中**唯一能在普通机器上给出真实数字**的行，所以本轮把它从「只有 `InputEncoder` 单测」推到「有语料 + 有产出 + 有读取 + 有判定」。

| 环节 | 落点 |
| --- | --- |
| **被测量的东西** | `termai_core::input::StandardEncoder`（AR-29 第 3 条 / kernel/05 K-01 的 S3 唯一编码点）。**不新增第二个编码器**：`tools/bench/input-bytes-driver.rs` 只做「行协议 → 调 `encode()` → 记录 `InputSink` 里的字节与 `EncodeOutcome`」，`crates/**` 一字未改 |
| **怎么驱动** | producer 先 `cargo build --release -p termai-core`，再用 `rustc --edition 2021 --extern termai_core=target/release/libtermai_core.rlib` 编译该驱动（`termai-core` 没有自己的依赖，所以这一行就够）。没有 cargo / rustc / rlib 时 producer **退出码 2**，绝不用 JS 手写编码器顶替 |
| **语料** | `tools/bench/input-corpus.json`：75 条 case，覆盖 keyboard（Legacy 35 / ModifyOtherKeys(2) 8 / Kitty 10）、paste 10、ime_commit 3、mouse 5、focus 2、api_inject 2；每条带 `expectBytes`（hex）+ `expectText`（可读字面量，producer 每次交叉校验两者一致）+ `authority` / `authorityKind`（kernel/05 条款号，或 xterm ctlseqs 规则） |
| **判据** | 每个 case：实际字节**逐字节**等于权威给出的期望，**且** `EncodeOutcome` 等于申报值（丢弃 / 吞掉 / 需确认 都是「0 字节」的合法期望，必须能与「什么都没做」区分）。H12 = 命中 / 已执行 × 100，`unit: pct`、`gate: 100`、`statistic: exact`（registry 转录，B1/B3 每次重新推导） |
| **没有权威的期望** | **不写进语料**：`input-corpus.json.omitted` 登记 11 条（`NamedKey::Space`、F13–F24、滚轮 button 编号、单独 `meta` 位、Kitty 下的 IN-04、`report_text` / `report_associated_text` 语义、preedit 0 字节、>1MiB 粘贴、OSC 52 读、DECSET 鼠标状态机、死键合成），producer 逐条打印原因——**语料之外的覆盖面不会被这个数字冒充** |
| **D0 自证** | 同一语料跑驱动两次，结果必须逐字节相同（kernel/06 §3.6 D0）；编码器源码（`crates/termai-core/src/**` + `Cargo.toml`）与语料各取 sha256，写进 `selfcheck.scene.hash` |
| **报告落点** | 默认 `target/bench-reports/input-bytes-report.json` + `input-bytes-report.cases.json`（逐 case 明细）；`target/` 是 gitignore 区且不被 `discoverReports()` 扫描，因此 `bench:check` 的默认输出仍是 `summary: 8 PASS` |
| **门禁读取** | `check.mjs --report <p>` 把 H12 作为「机器无关的 §5 门禁行」打印（值 / 门禁 / verdict 一起）；`verdict: FAIL` ⇒ **B8 FAIL**（上表第 ⑤ 条判定）。`gating: true` **合法**（它就是 §5 发布门禁），但 ADR-0029 D-5 把 `gating numbers produced by this run` 限定为**机器绑定**（RM-A / RM-C）的数字，所以 H12 不计入该计数器（与 H13 同形），B7 保持 PASS |
| **自证** | `--selftest` 为这条判定加了 2 注入（H12 `verdict=FAIL` 必须让 B8 FAIL；H12 带外来 unit 必须先判绑定违规）+ 3 对照（100%/PASS 报告绑定并被呈现、`gating:true` 不计入计数器且 B7 绿、`governed: no` 的 H18 即便 `verdict=FAIL` 也不受第 ⑤ 条约束），计数由 **78/78** 变为 **83/83** |

**本机实测（第 259 轮）：`H12 = 93.3333%`（70/75）= FAIL**。5 条与 kernel/05 §3.3 表逐字冲突：

| case | 模式 | 期望（引用） | 实际 |
| --- | --- | --- | --- |
| `M03` | ModifyOtherKeys(2) | `CSI 27;6;65~`（kernel/05 §3.3 表「Ctrl+Shift+A」MOK(2) 列） | `0x01`（Shift 被吞） |
| `M04` | ModifyOtherKeys(2) | `ESC x`（§3.3 表「Alt+x」MOK(2) 列） | `CSI 27;3;120~` |
| `M05` | ModifyOtherKeys(2) | `CSI 27;1;27~`（§3.3 表「Esc」MOK(2) 列） | `0x1B` |
| `K05` | Kitty(disambiguate+report_all) | `CR`（§3.3 表「Enter」Kitty 列） | `CSI 13u` |
| `K06` | Kitty(disambiguate+report_all) | `CSI 1;1D`（§3.3 表「Left」Kitty 列） | `CSI 57354u` |

即：`KeyboardMode::ModifyOtherKeys(2)` 目前**只在「Ctrl + 无 C0 映射的可打印字符」这一格**符合 §3.3 表（`M02` 通过），其余三格退回 Legacy；Kitty 模式对 `Enter` 与方向键走 PUA 码点 `CSI {code}u` 路线，与 §3.3 表的 `CR` / `CSI 1;1D` 不一致。**这两组要么改表、要么改 encoder，必须由 kernel/05 的 owner 裁决**；本工具只负责把「实现字节 ≠ 被引用条款的字节」如实报出，并且**不**把 H12 报成通过（`check.mjs --report` 因此判 B8 FAIL，退出码 1）。

## bench-report 字段对照（kernel/06 §3.7 逐字）

顶层字段（示例块中出现即视为契约字段，全部必填）：

| kernel/06 §3.7 字段 | schema 类型 | 说明 |
| --- | --- | --- |
| `schemaVersion` | string，必须是 `1.0.0` | 版本不符 → `SCHEMA_VERSION` |
| `reportId` / `generatedAt` | string | |
| `commit` | object `{sha, dirty}` | **9 个 legacy 字段之一（顶层）** |
| `toolchain` | object `{rustc, channel}` | |
| `runner` | object `{class, machineId, role, backend}` | |
| `fingerprintSha256` | string，64 位小写 hex | 缺失/不合规 → `SCHEMA_MISSING_FIELD` / `SCHEMA_PATTERN` |
| `environment` | object `{powerPlan, exclusive, warmed, valid}` | |
| `selfcheck.scene` | object `{verdict, frames, hash}` | D0 |
| `selfcheck.doubleRun` | object `{verdict, deltaPct}` | D1 |
| `selfcheck.verdict` | enum（四态） | |
| `corpus` | object `{manifestSha256, shardsOk, bytes}` | |
| `metrics` | array，至少 1 项 | |
| `verdict` | enum `PASS / FAIL / INCONCLUSIVE / INVALID` | |
| `cost` | object `{minutes, runnerClass, estUsd}` | |

`metrics[]` 条目字段：`metric`、`value`、`unit`、`samples`、`runner`、
`commit`、`toolchain`、`ts`（这 8 个即 **legacy 字段**）、`statistic`、`runs`、
`runStat`、`madOverMedian`、`gate`、`target`（可 null）、`gating`、
`verdict`（六态超集，见下）、`method`、`dCalibrationMs`（可 null）、`artifacts`。

**9 个 legacy 字段** = metric 级的 8 个（`metric / value / unit / samples / runner / commit / toolchain / ts`）
+ 顶层的 `commit`（kernel/06 §3.7 原文：「以上 9 个 legacy 字段（metric / value / unit / samples /
runner / commit / toolchain / ts + 顶层 commit）逐字保留」）。任一缺失 → `SCHEMA_MISSING_LEGACY_FIELD`。

## machine-fingerprint 字段对照（kernel/06 §3.4 逐字）

顶层：`id`、`role`、`cpu`、`mem`、`gpu`、`display`、`storage`、
`os`、`power`、`clocks`、`fonts`、`input`、`hypervisor`、
`updated_at`、`superseded_by`。

嵌套：
- `cpu`：`model`、`microcode`、`cores`、`threads`、`r23_single`、`r23_multi`
- `mem`：`size_gb`、`modules`、`speed`、`timings`
- `gpu`：`model`、`driver`、`api`、`fl`
- `display`：`model`、`edid_sha256`、`mode`、`vrr`、`hdr`、`dpi_scale`、`panel_gtg_ms`
- `storage`：`model`、`fw`、`seq_read_mbps`
- `os`：`name`、`build`、`arch`
- `power`：`plan`、`governor`、`app_nap`
- `clocks`：`source`、`qpc_freq_hz`
- `fonts`：`set_sha256`、`render_backend`
- `input`：`inject`、`keyboard_report_hz`

共 48 个字段名，`computeFingerprintSha256()` 对**全部叶子字段**（41 个）做稳定 JSON 序列化后取 sha256：
同输入同哈希，任一字段变化即哈希变化（因此任一变更触发 ADR-0014 铁律 2 的基线重设）。

## 值的读取与 §5 行绑定（D-6 第 ②–④ 步，第 255 轮落地）

`--report <p>` 不再只是 schema 校验：`values.mjs` 读取报告中的 `metrics[]`，并把每个 metric **绑定到它指名的 §5 行**。

| 读什么 | 规则 |
| --- | --- |
| **行集合** | 由 registry 推导「不需要参考机即可给出值」的行：`machine === 'none'` 且 `governed === 'no'` 且 `family !== 'external'` → **H17 / H18 / H19**（推导，不硬编码） |
| **绑定** | metric 指名某 H 行时，其 `unit` 与 `gate` **必须逐字等于 registry 的转录**（registry 由 B1/B3 每次从文档重新推导）——不等即 `B8` FAIL |
| **呈现** | 每行给出值 + registry 门禁 + owner + 来源；**报告没带的行打印为 `NOT REPORTED` 并给出 owner / carrier，绝不静默省略** |
| **表外对照** | `C1`（双主题对比度）作为**表外对照**呈现，**不计入 19 行** |
| **计数器** | `gatingNumbersProduced` = 读取到的、声明 `gating: true` 的 metric 数；非参考机上非 0 即 `B7` FAIL |

**第 255 轮的两处更正（本工具现在会抓出来）**：

1. **WCAG 对比度不是 H19**。H19 是**网格对齐误差 ≤0.5px**（`unit: px`、`gate: 0.5`、判据 kernel/03 RP-05）；**双主题对比度是表外对照 `C1`**（HARNESS §5 无此行，kernel/06 §3.9 记为 C1）。第 254 轮的示例报告把 5.17 标成 `H19`（`unit: ratio`、`gate: 4.5`），本工具现在以 **unit + gate 两处不匹配**判 `B8` FAIL。
2. **H18 的值是 53 而不是 26**。`tokens:check` 第 5 关对**三个** HTML 文件给数（netcatty 27 / prototype 26 / terminal-first 0），只取其中一个不是 §5 门禁「= 0」所指的量。

**本机可得性（环境问题，不是排期问题）**：**H18** 的值在本机可得（`tokens:check` 第 5 关，warn-only）；**H17** 需 `design:check` 的浏览器层（本机无 `CHROME_PATH`）；**H19** 需 RM-C 的 golden 像素渲染（本机无渲染管线）。**因此本机可产出的 §5 行目前只有 H18，另加表外对照 C1**——其余行在报告里缺席是如实的 `NOT REPORTED`，不是失败。

**H13 / B7（ADR-0029 D-5）**：`gatingNumbersProduced` 只计**机器绑定**的门禁数字——`H13`（安装包，`governed: yes` 且 `machine: none`）可以声明 `gating: true` 而不触发 `B7`；`H18`（`governed: no`）声明 `gating: true` 是**假的**，由 `B8` 的行绑定判为违规。

**SD-24（已登记）**：kernel/06 §3.4 把「网格对齐 / 视觉 golden」的**测量**绑到 **RM-C**，而 registry 对 H17 / H19 写 `machine: 'none'`（因为它们的**门禁判定**归 `tools/design-gates`）。本工具跟随 registry；这条「测量机器 vs 判定机器」的分歧见 `docs/plan/p0-spec-defects.md` SD-24。

## 判定状态机落点（kernel/06 §3.1）

`lib.mjs` 的 `evaluate(input)` **严格保持 §3.1 的步骤顺序**：

```text
1  !fingerprint.recorded              -> INVALID(FP_MISSING)
2  fingerprint != machine.now()       -> INVALID(FP_MISMATCH)
3  backend != T0 / cloud / RM-B       -> NON_GATING(BACKEND_DEGRADED | CLOUD_RUNNER | FLOOR_MACHINE_ONLY)
4  !scene_frozen()                    -> INVALID(SCENE_NOT_FROZEN)          # D0
5  !double_run_consistent(eps_self)   -> INVALID(MEASUREMENT_UNSTABLE)      # D1
6  runs.valid < 10                    -> INVALID(INSUFFICIENT_RUNS)
7  metric.is_tail && run.n < 1e5      -> INVALID(INSUFFICIENT_SAMPLES)
8  mad_over_median(runs) > 族阈值     -> INCONCLUSIVE(NOISE_FLOOR)
9  v = median(run.stat) + calibration; v <= gate ? PASS(margin) : FAIL(GATE_BREACH)
```

- 六个裁决状态（`PASS / FAIL / INCONCLUSIVE / INVALID / SKIP / NON_GATING`）由 `evaluate()` 与
  `tolerant()` 产出；报告顶层 `verdict` 只允许 §3.7 写明的四态（`REPORT_VERDICTS`）。
- **自检先于比对**：第 4/5 步在门禁与基线比较之前；`gateAndRegression()` 保证 INVALID / INCONCLUSIVE /
  SKIP / NON_GATING **不进入基线比对**（§3.5）。
- `assertReproducible(runA, runB)` 实现 AR-27 / §3.6 尾段：同 commit、同指纹、同语料、同场景哈希下，
  逐指标 verdict **不得翻转**，且 `abs(Δmedian) <= eps_self`；否则 `INVALID(MEASUREMENT_UNSTABLE)`。

## AR-27 双口径落点

`lib.mjs` 的 `regression(input)`（kernel/06 §3.1 第二段逐行）：

| 条件 | 结果 |
| --- | --- |
| `d = (median_now − baseline.median) / baseline.median <= 0.05` | `PASS`（记录 margin） |
| `context == 'PR'` 且 `d > 0.05` | **G4-PR → `FAIL(REGRESSION_PR)`**（单次即阻断合并） |
| `nights.consecutive >= 2` 且第二夜仍 `> 0.05` | **G4-REL → `FAIL(REGRESSION_REL)`** |
| `context == 'RC_FULL'` 且 `d > 0.05` | **G4-REL → `FAIL(REGRESSION_REL)`**（发版前全套一次即确认） |
| 其余（夜间单次超标） | `INCONCLUSIVE(SUSPECT_SINGLE_NIGHT)` |

前置：`evaluate()` 必须先产出 PASS/FAIL，否则不得与基线比对；基线缺失 → `SKIP(NO_BASELINE)`
并打印重建命令提示。

## OQ-PM-07 分级阈值落点

`lib.mjs` 的 `METRIC_FAMILY_POLICY`（AR-31 第 9 条 / OQ-PM-07）：

| 指标族 | MAD/中位数上限 | eps_self（§3.6） | 覆盖的 H 行 |
| --- | --- | --- | --- |
| `frame`（帧时 / P99 尾部） | 5% | 5% | H2、H3、H4 |
| `startup`（启动 P95） | 3% | 3% | H1 |
| `throughput`（吞吐中位数） | 2% | 2% | H5 |
| `rss`（RSS 中位数） | 1% | 1% | H6、H7、H10、H11 |
| `exact`（字节相等 / =0 结构断言） | 0% | 0% | H9、H12、H13、H17、H18、H19 |
| `default`（OQ-PM-07 未枚举者） | 2%（§3.1 基线） | 2% | H8 |
| `external`（非机器门禁） | — | — | H14、H15、H16 |

`madLimitFor(family)` 用于判 INCONCLUSIVE，`epsSelfFor(family)` 用于 D1 与 AR-27 自证；
`epsSelfFromFormula(noiseFloor, madLimit)` 暴露 §3.6 的 `max(噪声地板, 2 × MAD 上限)` 公式供调用方使用。

## 已标记的规格歧义（`check.mjs` B2 每次运行都会打印）

1. **扁平投影与结构化顶层字段同名**：spec 07 §3.8.2 的 legacy 扁平对象是
   `{metric, value, unit, samples, runner, commit, toolchain, ts}`，但 kernel/06 §3.7 的顶层
   `commit` / `toolchain` / `runner` 都是**对象**。把扁平投影直接并入顶层会与同一节的 schema 冲突。
   本工具的做法：将其承载在**加法字段 `flatProjection`** 中（§3.7 允许加法字段向后兼容 1 个 minor），
   并把它登记为「已声明的加法字段」，从而 B2 仍可要求**契约字段集**与正文逐字相等。
   需要 kernel/06 与 spec 07 的 owner 追认（或给出替代口径）。
2. **`FailKind` 缺 `GATE_BREACH`**：kernel/06 §4 的 `FailKind` 只有
   `RegressionPr / RegressionRel / BackendDegraded / CorpusDrift / SchemaViolation`，
   而 §3.1 的 `FAIL(v, gate)`（绝对门禁越界）没有对应成员。本工具以 `GATE_BREACH` 扩展并表示它是扩展项。
3. **H3 的 Run 定义**：kernel/06 §3.2 没有为 PTY-LAT-1 单列 Run 定义。本工具按「与 H2 共用同一注入流」
   读作尾部指标（`isTail`，每 Run ≥1e5 样本）。这是**读数**而不是正文引用。
4. **`NON_GATING` / `SKIP` 的原因码**：§3.1 只给出 `BACKEND_DEGRADED`；`CLOUD_RUNNER`、
   `FLOOR_MACHINE_ONLY`、`NOT_MACHINE_GATE` 由 ADR-0014 决策 1 / 铁律 5 与 AR-31 推导而来，
   在本工具中标记为 `origin: 'extension'`。

## 未完成项 / 下一步

- **产出门禁数字**：需要 RM-A（计算类：H1、H5、H6、H7、H8、H9、H10、H11，以及 §8.2 的 R1/R2 重建行）与 RM-C
  （显示 / 延迟类：H2、H3、H4），并且内核侧 `cargo xtask bench` 落地（M0 未实现）。
  在此之前，本工具的输出一律 `INCONCLUSIVE(REFERENCE_MACHINE_UNAVAILABLE)`。
- **sessiond 重建行（R1/R2）**：机制断言与测量产出都已落地（见上一节）；**仍缺**参考机指纹与 T0 独占，
  以及 `CheckpointRef` / `Resize` 两条分支的忠实重放（需要 CAS 内容存储与「按 Resize 分段重放」）。
  因此 E-P0-4 的「≤2s / ≤5s」目前**不能说已完成**：本机数字不是门禁级证据。
- **§5 机器无关行的值**：读取路径已落地（D-6 第 ②–④ 步）；本机可产出者现在是 **H18**、**H12**（见上一节，
  `node tools/bench/input-bytes.mjs`）与**表外对照 C1**，其余行如实 `NOT REPORTED`（H17 需浏览器层、
  H19 需 RM-C 像素渲染）。**H12 是本机唯一能给出「门禁级」结论的行**：它 `machine: none`，所以
  §5 的「= 100%」在本机即可判定（第 259 轮判定为 **FAIL**，5 处与 kernel/05 §3.3 表冲突，见上一节）。
- **H1–H19 的实测**：本工具只校验「测量定义已登记且与正文一致」，不执行任何一项测量
  （机器无关行的值由 `--report` **读入**，不是本工具测出来的）。
- **`bench-repro-report.json`（A-PM-01）**：`assertReproducible()` 已实现判定，但真正跑两次 G4 全集
  需要参考机与 xtask。
- **D0 场景冻结**：`scene.frozen` 由调用方（xtask / scene 工具）提供；本工具只消费与判定。
- **D_max / 相机偏置（K-14 / §3.3）**：属探针与人工标定，本工具未实现其数值采集。
