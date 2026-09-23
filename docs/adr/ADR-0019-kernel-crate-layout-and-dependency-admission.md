# ADR-0019：内核 crate 布局与依赖准入（termai-core / ipc / vt / pty / session + apps）

- 状态：Accepted
- 日期：2025-01-16
- 决策者：WS-F 工程效能组起草，项目总负责人（Orchestrator）审阅并接受（依据 ADR-0015 的准入判定表与 P0 内核评审结论收敛）
- 关联：AR-03（内核不依赖 AI / 网络 / UI）、AR-18（vte 置于 trait 边界之后）、AR-21（Apache-2.0 OR MIT；GPL / AGPL / SSPL 禁入链接边界）、AR-25（字节保真与三条车道）、AR-28 第 3 条（新依赖入链接边界前必须完成 ADR-0015 白名单登记；portable-pty 明确不默认采用）、AR-29 第 3 条（InputEncoder 单一收口）、DC-15（核心 100% Rust）、DC-16（PtyBackend / ConPTY）、DC-21（Cargo workspace 依赖单向无环）、DC-22（termai-ipc）、DC-23（Session Log 唯一真相）、DC-35（capability 默认拒绝）、DC-40（N-2 兼容窗口）；HARNESS §0.4、§2、§4.3、§8.1-5；AGENTS §3、§5、§6；ADR-0015（LB-01…LB-18、D3 / D4）、ADR-0018（IPC 对外契约追认）、docs/spec/07 §3.1.3 / §3.11、docs/plan/mvp-delivery-plan.md §3 / §6、docs/plan/m0-spec-defects.md（SD-01…SD-05）
- 实现位置：仓库根 `Cargo.toml`（`[workspace] members`、`[workspace.package].license`、`[workspace.dependencies]`）；`crates/termai-{tokens,core,ipc,vt,pty,session}/Cargo.toml`；`apps/{sessiond,termai}/Cargo.toml`；门禁实现 `tools/kernel-gates/check.mjs`（K4 依赖形状 / K5 许可 / K6 规格缺陷登记）与 `.github/workflows/ci.yml` 的 `kernel` job

## 背景与问题

M0 纵向切片新增 5 个库 crate（termai-core / termai-ipc / termai-vt / termai-pty / termai-session）与 2 个 app（sessiond / termai），并引入 6 项第三方依赖。按 AGENTS.md §3「新增 crate/package 必须在 ADR 中说明其依赖位置」与 §5「ADR 触发条件」（新增模块、引入新依赖类别），这些改动在冻结接口与合并之前必须有一份可追溯的准入记录。

两条硬约束没有中间地带：

1. **依赖方向**（HARNESS §4.3 / DC-21 / spec 07 E3）：只允许单向依赖、禁止环；`apps/*` 不得被任何库依赖；termai-core 是叶子契约，AR-03 要求它对 AI / 网络 / UI / 上层内核模块零依赖（编译期兜底）。一旦某个 crate 反向依赖，P0 的「契约冻结」立即失效。
2. **许可与链接边界**（AR-21 / ADR-0015）：核心链接边界内禁止 GPL / AGPL / SSPL；Rust crate 默认静态链接，因此 `[dependencies]` 中的第三方单元按 ADR-0015 LB-01 / LB-05 计入边界内，必须逐项给出 SPDX 与 A / R / D 判定。未知或许可缺失按 P3「未知即拒绝」处理。

失败代价：依赖方向错误会在后续 crate 增删中持续放大（改一处牵动全图），且极易把 `apps/*` 拖成事实上的公共库；许可误判则无法通过事后重新打包补救（下游已获得派生二进制，见 ADR-0015 背景）。因此本 ADR 采取**显式白名单 + CI 机器校验 + 默认拒绝**的方案。

## 可选方案（至少 2 个，含被否决项）

| 方案 | 描述 | 采纳 / 否决理由 |
| --- | --- | --- |
| A 只靠代码评审 | 由 maintainer 人工核对依赖方向与许可 | **否决**。不可回归、无强制力；spec 07 §3.1.3 要求 depcheck 反向边检测为合并阻断，人工无法满足 |
| B 只靠 `cargo xtask depcheck` + cargo-deny | 依赖 `cargo metadata` 全图与 cargo-deny 许可扫描 | **部分采纳**（长期目标）。本 M0 尚未落地 `crates/termai-xtask`，且 cargo-deny / cargo-audit 需要网络与额外安装；PR 反馈预算（spec 07 §3.4.2，中位 ≤15min）不允许把许可门禁押在未就绪工具上 |
| C 单文件清单 + 零依赖 Node 校验器（本 ADR） | 用一份 ADR 作为准入真源，由 `tools/kernel-gates/check.mjs` 直接解析 `crates/*/Cargo.toml` 与 `apps/*/Cargo.toml`，校验依赖形状、许可表达式与 GPL denylist | **采纳**。零新增 npm 依赖、本地与 CI 同源、可在 `--selftest` 中注入故障证明不是 always-green |
| D 把所有新 crate 合并成一个大 crate | 用一个 `termai-kernel` 承载全部 | **否决**。与 DC-21 / HARNESS §4.3 的模块边界冲突；破坏 AR-18 的 trait 可替换边界与 AR-25 的车道分离 |
| E 把 session 直接依赖 vt / pty | 让 termai-session 静态链接 termai-vt / termai-pty | **否决**。会把「会话真源」与「解析 / 平台实现」焊死，破坏 AR-18 的 clean-room 替换评估（ADR-0012 / AR-18）与内核模块的可测试性 |

## 决策

### D1 模块与依赖方向（对照 HARNESS §4.3，唯一权威图）

```
termai-tokens   (叶子：生成物，零依赖，DC-09)
termai-core     (叶子契约：core-dto / InputEncoder / risk / capability / SessionId)

termai-ipc      -> termai-core
termai-vt       -> termai-core        (vte 置于 trait 边界之后，AR-18)
termai-pty      -> termai-core
termai-session  -> termai-core, termai-ipc   (不依赖 vt / pty)

apps/sessiond   -> { termai-core, termai-ipc, termai-vt, termai-pty, termai-session }
apps/termai     -> { termai-core, termai-ipc, termai-vt, termai-pty, termai-session }
```

- **termai-core = 叶子契约**：`termai-core` 不依赖任何其它 `termai-*` crate，也不依赖任何 app。这是 AR-03 的编译期兜底：内核核心不会因 AI / 网络 / UI / 上层实现而产生链接依赖。
- **termai-ipc / termai-vt / termai-pty 各自只依赖 termai-core**。**termai-session 依赖 termai-core 与 termai-ipc**（唯一例外，理由见下一条）；库之间的其余组合由上层 app 装配。
- **termai-session 只经 trait 使用 vt / pty**：session 定义 / 消费 trait，具体 `termai-vt` / `termai-pty` 实现由 `apps/sessiond` / `apps/termai` 注入。因此 `termai-session` 的 `[dependencies]` 中**不得出现** `termai-vt` / `termai-pty`。
- **termai-session -> termai-ipc 的正当性（登记时的实现事实）**：kernel/04 的记录头 CRC 与 ADR-0018 D3 的 IPC 帧 / 共享内存槽校验使用**同一 crc32c 语义**；由 termai-ipc 提供唯一实现，可避免第二套 CRC 语义造成「一处校验通过、另一处静默损坏」（ADR-0018 D3 的原始理由）。该边只消费 `crc32c` 等无状态基元，不把 IPC 的进程模型 / 握手职责引入会话层。**注意：本边不在 mvp-delivery-plan §3 的文字清单里逐字列出，但符合其 WBS（WS-D 依赖 WS-A，而 WS-A 含 termai-core 与 termai-ipc）与 ASCII 依赖图中的 `termai-ipc -> termai-session` 汇聚方向。** 如未来 termai-ipc 的公开面扩张到握手 / 进程职责，须走新 ADR 复议。
- **`apps/*` 永不被任何库依赖**：`sessiond` / `termai` 是依赖图的汇点。库 crate 的 `[dependencies]` 中出现 app 名即视为架构事故（AGENTS §3 / spec 07 E3）。
- **无环**：本图是树状（core 为根，四个库为叶向上一层，两个 app 在最外层），且 K4 对任何未登记的 `termai-*` 边 fail-closed，因此不存在环。
- **依赖位置的新增义务**：任何新 crate 必须同时（a）加入根 `Cargo.toml` 的 `[workspace] members`，（b）在本 ADR 或后续 ADR 的允许边表中登记，（c）通过 K4。未登记即合并阻断。

### D2 第三方依赖准入登记（ADR-0015 判定）

判定口径以 ADR-0015 为唯一权威：Rust `[dependencies]` 非 proc-macro 走 **LB-01**；Rust 默认静态链接走 **LB-05**，二者均在**链接边界内**。SPDX 求值走 ADR-0015 D4；`A OR B` 只要一支在白名单即允许，但必须记录 **elected 分支**。

| 依赖 | 版本 | SPDX | ADR-0015 判定 | 边界 / 位置 | 用途与依据 |
| --- | --- | --- | --- | --- | --- |
| `vte` | 0.15（锁定） | `MIT OR Apache-2.0` | **A 允许**（Tier A 白名单；`MIT` / `Apache-2.0`） | LB-01 / LB-05 边界内；静态链接进 `termai-vt`，且置于 trait 边界之后 | AR-18：P0-P2 基础 ESC 状态机；不 fork，锁定版本并记录上游差异；类型不外泄（trait 差分包住） |
| `unicode-width` | 0.2 | `MIT OR Apache-2.0` | **A 允许** | LB-01 / LB-05 边界内；**仅 `termai-vt`** 列宽计算（`termai-core` 是零依赖叶子，不得引入依赖） | 宽字符 / grapheme 列宽（AR-14 网格对齐、AR-25 字节保真前提） |
| `blake3` | 1.x | `CC0-1.0 OR Apache-2.0` | **A 允许**（OR 求值：`CC0-1.0` 与 `Apache-2.0` 均为 Tier A；**elected 分支 = Apache-2.0**，构建未行使被禁分支） | LB-01 / LB-05 边界内 | 内容寻址 / 摘要（DC-23 CAS、审计哈希链 DC-34 的候选实现） |
| `libc` | 0.2 | `MIT OR Apache-2.0` | **A 允许** | LB-01 / LB-05 边界内；仅 Unix 目标 | `forkpty` / `termios` / 平台原语（DC-16 的 Unix 侧 PtyBackend） |
| `windows-sys` | 0.59（feature 白名单见根 manifest） | `MIT OR Apache-2.0` | **A 允许** | LB-01 / LB-05 边界内；仅 `cfg(windows)` | ConPTY、Job Object 进程树、Console / IO / Threading（DC-16 / ADR-0018 九元面） |
| `sha2` | 0.10 | `MIT OR Apache-2.0` | **A 允许** | LB-01 / LB-05 边界内；`termai-session` | 会话 Log 的 PtyIn 记录摘要（kernel/04 §3.2.3）；与 blake3 分域使用，不互相替代 |

**明确拒绝 / 未准入**

- **`portable-pty`：REFUSED（拒绝，不采用）。** 依据 **AR-28 第 3 条**。理由：Windows 唯一生产路径是 ConPTY + Job Object（DC-16 / ADR-0018 的九元 `PtyBackend`，含 `process_tree` / `signal` / `capabilities`），`portable-pty` 的能力面与进程树语义不满足该契约；若引入将成为不可替换的黑盒，直接冲突 AR-25 的字节保真门禁。该条目不得进入任何 `[dependencies]`；K4 的 denylist 与「未登记依赖」检查会 fail-closed。
- **`russh`：NOT admitted until it is registered（未准入）。** 依据 AR-28 第 3 条与 ADR-0018 结尾：SSH 栈属于新的依赖类别（网络 / 加密 / 平台原生），进入链接边界前必须先完成 ADR-0015 的白名单登记并新增 ADR。**M0 只冻结 `Transport` / `Channel` trait 与 F0/F1/F2 声明，不实现 russh**（mvp-delivery-plan §1.2 / §6 第 3 条）。在登记完成前，任何 crate 引入 russh 视为 D 档（未知），合并阻断。
- 其余未在本表登记的第三方依赖：一律按 ADR-0015 P3「未知即拒绝」处理，必须先补 ADR-0015 判定，再更新本 ADR 或新增 ADR。`Cargo.lock` 只记录解析结果，**不构成准入**。

补充口径：

1. 根 `Cargo.toml` 的 `[workspace.dependencies]` 是**唯一准入面**：所有第三方依赖版本在此集中声明，crate 通过 `{ workspace = true }` 继承，避免版本漂移与「绕过登记直接写死版本」。
2. `libc` / `windows-sys` 必须按目标平台条件引入（`[target.'cfg(...)'.dependencies]` 或 crate 内 `cfg` 门控），不得在非目标平台强制链接。
3. 本次准入不改变任何 AR / DC 结论，也不放宽 §5 预算或 §8 门禁；它只把 AR-21 与 ADR-0015 的判定落到具体依赖与具体版本。

### D3 规格缺陷（SD-01…SD-05）与 errata ADR 的关系

`docs/plan/m0-spec-defects.md` 登记了 M0 实现期发现的规格缺陷，当前共 **5 条（SD-01…SD-05）**：

| 编号 | 分册 | 类型 | 本 M0 处置 | 需要的追认 |
| --- | --- | --- | --- | --- |
| SD-01 | kernel/07 §3.1 | 线格式 | 显式小端 24B 帧头（不用 `repr(C)`） | 新 ADR 修正 `repr(C)` 表述 |
| SD-02 | kernel/07 §3.3 | 契约缺字段 | Hello 追加 `required: Vec<CapId>`，回传 `unknown_optional` | ADR 并入 kernel/07 §3.3 |
| SD-03 | kernel/04 §3.2.2 | 线格式 | 显式小端 24B 记录头，CRC 覆盖 `[4 .. 24+len]` | 与 SD-01 合并写入同一份 ADR |
| SD-04 | kernel/05 §3.1 | 签名笔误 | `&mut dyn InputSink`；`InputEncoder` 不加 `Send` 超 trait | errata 记录 |
| SD-05 | kernel/04 §4 与 kernel/07 §3.3 | 类型宽度不一致 | `SessionId = termai_core::SessionId(pub u128)`（ULID），IPC `session_claim` 使用该类型 | kernel/07 §3.3 补注类型来源 |

> 说明：本文件初始登记为 SD-01…SD-04；SD-05（SessionId 宽度）为避免 IPC attach 帧与 Log 会话标识不可互操作而随后追加。以文件当前内容为准。

**本 ADR 不修正任何 spec。** SD-01…SD-05 的规格修正（kernel/04 / kernel/05 / kernel/07 的正文）由**后续独立的 errata ADR** 承接；本 ADR 只做两件事：

1. 记录这些缺陷是"M0 实现先于 spec 修正"的已知偏差，并将其与依赖 / 模块准入解耦（避免把线格式 errata 混进依赖 ADR，导致复议条件互相污染）。
2. 由门禁 **K6** 断言 `docs/plan/m0-spec-defects.md` 存在且包含 SD-01…SD-05，防止缺陷登记在并发改动中被静默删除或降级。

**在 errata ADR 生效之前，以 `docs/plan/m0-spec-defects.md` 的「本 M0 处置」列为准**（该文件自述：追认前以本处置为准）。

### D4 门禁映射（机器可执行）

本 ADR 的约束由 `tools/kernel-gates/check.mjs` 与 CI `kernel` job 强制：

| 门禁 | 内容 | 依据 |
| --- | --- | --- |
| K1 | `cargo fmt --all -- --check` | AGENTS §6（rustfmt） |
| K2 | `cargo clippy --workspace --all-targets -- -D warnings` | AGENTS §6 |
| K3 | `cargo test --workspace` | spec 07 §3.3 L1 / L2；MVP DoD 1 |
| K4 | 解析 `crates/*/Cargo.toml` + `apps/*/Cargo.toml`：D1 的允许边（含 `termai-session -> termai-ipc`）、apps 反向边 = 0、GPL / AGPL / SSPL denylist、portable-pty 拒绝 | DC-21 / E3 / AR-21 / AR-28 第 3 条 |
| K5 | 每个 `package.license` 为 `Apache-2.0 OR MIT`（或 `license.workspace = true` 继承），且 `[workspace.package].license` 精确等于该 SPDX 表达式 | AR-21 / spec 07 §3.11 |
| K6 | `docs/plan/m0-spec-defects.md` 存在且含 SD-01…SD-05 | AGENTS §5；本 ADR D3 |

K1-K3 若缺少工具（cargo）则显式 SKIP 并打印原因，绝不静默通过；K4-K6 为纯 Node 零依赖，任何环境都必须执行。

## 理由

1. **在 AR / DC 框架内落地**：D1 是 HARNESS §4.3 / DC-21 的直接细化，D2 是 AR-21 / AR-28.3 / ADR-0015 的直接应用，没有引入新的设计结论。
2. **同时满足 A1 / A2 / A3**：叶子契约（A1 信任基座、A2 正确性）不被上层实现污染；trait 边界保住 vte 可替换（A2）；core-dto / InputEncoder 单一收口为 A3 的结构化上下文提供稳定契约。
3. **可回归、可在 15s 内给出结论**：K4-K6 纯 Node，无需网络；PR-S1 就能拦住反向依赖与许可越界，符合 spec 07 §3.4.2 的反馈预算。
4. **默认拒绝**：未登记依赖 / 未知 crate 边一律 FAIL，对应 AGENTS §7.4「不确定时优先更保守的安全默认值」。

## 后果（正面 / 负面 / 需要接受的代价）

**正面**

- 5 个新 crate + 2 个 app 的依赖位置有了唯一可追溯记录，DC-21「CI 强制」从口号变为可执行检查。
- 6 项第三方依赖逐项有 SPDX 与 ADR-0015 判定，`portable-pty` 的拒绝与 `russh` 的未准入被显式记录，后续 PR 无法"顺手引入"。
- SD-01…SD-05 与 spec 修正解耦，不会因 errata 排期拖住 M0 合并，也不会因 M0 合并而让 spec 偏差失忆。

**负面 / 必须接受的代价**

- K4 的允许边表是硬编码在门禁里的，新增 crate 需要同时改 ADR + 门禁，存在"流程摩擦"；这是有意的（AGENTS §3 要求 ADR 先于合并）。
- K4 基于轻量 TOML 行解析而非完整 TOML 语义，无法覆盖 workspace 继承后的**实际**解析版本（例如 `vte = "0.15"` 到底解析到哪个 patch）。它只校验**声明面**；实际解析版本仍应由 cargo-deny / cargo-audit / SBOM（spec 07 §3.11）在 PR-S4 覆盖。
- K1-K3 直接跑全 workspace，PR 时间上升；当其它 workstream 正在并发改 crate 时会看到"非本 PR 引起"的红灯（本 ADR 要求 CI 与本地报告都如实标出该情形，不得隐藏）。
- 本 ADR 未落地 `cargo xtask depcheck` / cargo-deny；长期方案（方案 B）仍是目标，本 ADR 的清单届时作为白名单文件迁移。

## 复议条件与代价（AGENTS.md §5.3）

> 说明：AGENTS.md §5 为不分小节的项目符号列表，不存在字面的 5.3；本节按 §5 第 3 条（ADR 触发条件）与 §7.3（写清代价与复议条件）的意图编写。

1. **依赖方向复议**：若某 crate 被证明必须新增一条库对库依赖（例如 termai-session 需要直接复用 termai-vt 的类型），必须附**耦合度与可替换性证据**（例如 vte 替换评估结果），走 RFC → 新 ADR，并在旧 ADR 上标注取代关系。不得以"少写一层 trait"为由直接改 crate 依赖。
2. **依赖准入复议**：新依赖或版本升级必须先更新 ADR-0015 判定（SPDX + 边界档位），再更新本 ADR / 新 ADR。`portable-pty` 的拒绝复议需证明其满足 ADR-0018 的九元 `PtyBackend` + Job Object 语义与 AR-25 字节保真门禁；`russh` 的复议需先完成 ADR-0015 白名单登记与安全 / 法务会签（spec 07 §3.10）。
3. **门禁强度复议**：K4 / K5 / K6 的任何放宽（例如把未登记 crate 从 FAIL 降为 WARN）都属 §8 门禁变更，必须附基准数据 + TSC 批准（AGENTS §5），本 ADR 不放宽任何既有门禁。
4. **规格修正归口**：SD-01…SD-05 的 spec 正文修正不在本 ADR 内；若 errata ADR 的结论与本 M0 处置不同，以 errata ADR 为准，并在 `m0-spec-defects.md` 标注取代关系。
5. **不可放宽项**：AR-21 的 GPL / AGPL / SSPL 边界内禁止、ADR-0015 P3「未知即拒绝」、`apps/*` 不被库依赖、`termai-core` 叶子性，均不可通过本 ADR 复议。

## 反方记录与复议条件

- **反方（工程效率）**：单文件 ADR + 硬编码允许边维护成本高，且 K4 的行解析器不如 cargo metadata 完整，可能出现"声明合规但实际解析版本越界"。
  - **接受该缺口，否决"因此不做"**：K4 的定位是 PR-S1 快速闸门，动态解析版本由 PR-S4 的 cargo-deny / SBOM 兜底；两层的分工写在 D4。**复议条件（可观测）**：若 12 个月内出现 ≥1 次"K4 通过但 SBOM 发现越界版本"的事故，则把 K4 升级为消费 `cargo metadata` 的门禁并新增 ADR。
- **反方（内核）**：`termai-session` 不依赖 vt / pty 会强制 app 层写更多装配代码。
  - **否决理由**：AR-18 的可替换性与 AR-25 的车道分离依赖这条边界；装配成本是一次性的，替换成本是持续的。**复议条件**：若 clean-room 解析器评估（AR-18）结论为"保留 vte"，仍不得改为库对库依赖——替换性评估的结论不是耦合的通行证。
- **反方（合规）**：`blake3` 的 `CC0-1.0` 分支虽在白名单，但 CC0 用于软件存在专利与担保条款的争议。
  - **接受其提示**：elected 分支已记录为 `Apache-2.0`，构建不行使 CC0 分支；**复议条件**：若法务判定 CC0-1.0 不得用于核心，则要求 `blake3` 以 `Apache-2.0` 单独 elected，或替换为纯 `MIT OR Apache-2.0` 的摘要实现（需新 ADR）。

## 关联决策（DC-xx）与实现位置

| 决策 / 条目 | 内容 | 实现位置 |
| --- | --- | --- |
| DC-21 / HARNESS §4.3 | 单向无环依赖、apps 不被库依赖、core 为叶子 | 根 `Cargo.toml` members；`tools/kernel-gates/check.mjs` K4 |
| AR-21 / spec 07 §3.11 | Apache-2.0 OR MIT；GPL / AGPL / SSPL 边界内 = 0 | `[workspace.package].license`；K4 denylist + K5 |
| AR-18 | vte 置于 trait 边界后，不自研 ESC 解析器，锁定版本 | `crates/termai-vt`；K4 允许边 `termai-vt -> termai-core` |
| AR-28 第 3 条 | 新依赖须先过 ADR-0015；portable-pty 拒绝 | D2；K4 |
| ADR-0015 | LB-01 / LB-05 边界内判定、D4 SPDX 白名单与 OR 求值 | D2 准入表 |
| ADR-0018 | 九元 `PtyBackend` / IPC 对外契约 | `crates/termai-pty`、`crates/termai-ipc`；K4 |
| SD-01…SD-05 | M0 规格缺陷登记，触发后续 errata ADR | `docs/plan/m0-spec-defects.md`；K6 |
| §8.1-5 / spec 07 G5 | 依赖与许可门禁 | CI `kernel` job（K4 / K5）；长期迁移到 cargo-deny + SBOM |

- **不取代任何 ADR**：本 ADR 扩展 ADR-0015 的准入表到 M0 具体依赖，并为 AR-28 第 3 条提供落地记录。
- **受影响文档**：`docs/adr/README.md` §6 索引（由 Orchestrator 追加 ADR-0019 行；本 ADR 作者无权限修改）、`docs/plan/mvp-delivery-plan.md` §6（ADR-0019 由"待写"转"已写"）。
- **双签**：新增依赖类别与对外契约（termai-ipc）需 CODEOWNERS 双签（AGENTS §3 / spec 07 E4）。
