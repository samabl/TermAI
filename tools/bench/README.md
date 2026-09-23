# tools/bench — B-10 性能测量方法学落地（W1-C）

> 本目录实现 `docs/spec/kernel/06-performance-methodology.md`（简称 kernel/06）的「怎么测、怎么判、
> 测不准怎么办」。它是 **方法学工具**，不是测量工具：它校验 schema、计算指纹、实现判定状态机、
> 登记 §5 映射表，并用 `--selftest` 证明判定逻辑不是恒绿。
>
> **本机没有 RM-A / RM-C 参考机**，所以本工具在任何一次运行中产出 **0 个门禁数字**。
> 任何真实性能数字必须来自 `cargo xtask bench` 在 RM-A / RM-C 的 T0 后端上的运行
> （crates/termai-bench，M0 未实现）。这是 AR-20 诚实原则与 ADR-0014 铁律 5 的落地，不是配置项。

## 命令

```text
node tools/bench/check.mjs              # 运行 B1-B7（npm run bench:check）
node tools/bench/check.mjs --selftest   # 注入故障，证明判定逻辑不是恒绿（npm run bench:selftest）
node tools/bench/check.mjs --json       # 仅输出机器可读 JSON
node tools/bench/check.mjs --report <p> # 额外对一个真实 bench-report.json 做 schema 校验（B8）
node tools/bench/check.mjs --machine <p># 显式声明本机为已登记的 RM-A / RM-C（默认不声明）
```

退出码：`0` = PASS，`1` = FAIL。缺失的报告文件是显式 `SKIP`，绝不静默 PASS。

## 文件与职责

| 文件 | 职责 |
| --- | --- |
| `lib.mjs` | schema 引擎 + bench-report / machine-fingerprint schema、稳定 JSON 与 sha256 指纹、§3.1 判定状态机 `evaluate()`、`regression()`（G4-PR / G4-REL）、`gateAndRegression()`、`assertReproducible()`（AR-27 自证）、指标族阈值策略、`classifyMachine()` 诚实边界 |
| `registry.mjs` | HARNESS §5 的机器可读登记：H1…H19（19 条）+ 表外对照 C1；每条含 owner / 测量定义落点 / 门禁载体 / 是否 kernel/06 管辖 / 指标族 / 参考机；`mappingIntegrity()` 做缺号与重复校验 |
| `fixtures.mjs` | **合成逻辑夹具（不是测量值）**：只用于把状态机推过每个分支，绝不当结果上报、绝不与基线比对 |
| `check.mjs` | 门禁入口 B1–B8 + `--selftest` 54 条注入 |

## 门禁 B1–B8

| # | 门禁 | 校验什么 | 依据 |
| --- | --- | --- | --- |
| B1 | HARNESS §5 → registry 逐字对照 | 解析 `HARNESS.md` §5 表格的 19 行，逐字符比对指标名 / 发布门禁单元格 / 目标单元格 | HARNESS §5「映射要求」 |
| B2 | kernel/06 §3.4 / §3.7 → schema 逐字对照 | 解析正文 JSON 示例块，比对字段名集合是否**完全相等**；核对 9 个 legacy 字段 | kernel/06 §3.4、§3.7 |
| B3 | H1…H19 映射表完整性 | 无缺号、无重复、恰好 19；与 kernel/06 §3.9 表格逐行交叉核对（含「属 kernel/06 管辖」列） | kernel/06 §3.9、AR-24 第 3 条 |
| B4 | 阈值转写对照 | 解析 kernel/06 §3.6 的 `eps_self` 表与 §3.1 代码块，核对本工具中的每个常量 | kernel/06 §3.1/§3.6、AR-31 第 9 条 |
| B5 | 指纹确定性 | 同输入同哈希；41 个叶子字段**逐个**改动都改变哈希；键序无关 | kernel/06 §3.4、§3.7 |
| B6 | §3.1 状态机分支覆盖 | 6 条对照分支 + 21 条故障分支全部产出文档规定的裁决 | kernel/06 §3.1 |
| B7 | 机器绑定诚实边界 | 无参考机时必须 NON_GATING / INCONCLUSIVE 且产出 0 个门禁数字 | ADR-0014 铁律 5、kernel/06 §6、AR-31 第 8 条 |
| B8 | 外部报告 schema 校验（`--report`） | 对指定 bench-report.json 做 §3.7 校验；文件不存在则 SKIP | kernel/06 §3.7、spec 07 §3.8.2 |

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

- **产出门禁数字**：需要 RM-A（计算类：H1、H5、H6、H7、H8、H9、H10、H11）与 RM-C
  （显示 / 延迟类：H2、H3、H4），并且内核侧 `cargo xtask bench` 落地（M0 未实现）。
  在此之前，本工具的输出一律 `INCONCLUSIVE(REFERENCE_MACHINE_UNAVAILABLE)`。
- **H1–H19 的实测**：本工具只校验「测量定义已登记且与正文一致」，不执行任何一项测量。
- **`bench-repro-report.json`（A-PM-01）**：`assertReproducible()` 已实现判定，但真正跑两次 G4 全集
  需要参考机与 xtask。
- **D0 场景冻结**：`scene.frozen` 由调用方（xtask / scene 工具）提供；本工具只消费与判定。
- **D_max / 相机偏置（K-14 / §3.3）**：属探针与人工标定，本工具未实现其数值采集。
