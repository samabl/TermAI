# TermAI P0 交付计划（P0 · 信任基座）

> **效力**：本文是**交付计划**，不是设计权威。与 [HARNESS.md](../../HARNESS.md) 冲突之处一律以 HARNESS 为准（AGENTS §1）。
> **依据**：HARNESS §7（P0 出口标准）、§8（质量门禁与验收）、§5（预算）、§2（AR-24…AR-31）、§11.2（CR-08…CR-13）；`docs/spec/kernel/00-index.md` §2（P0 阻塞项 B-1…B-11）；ADR-0014（平台矩阵与参考机）、ADR-0018/0019/0020（内核契约与准入）、ADR-0021/0022（CI 产物与视觉基线）。
> **前置**：M0「信任基座纵向切片」已交付（headless：PTY → VT → Grid → Session Log → IPC → capability），证据见 [m0-delivery-report.md](m0-delivery-report.md)。
> **纪律**（HARNESS §0.2 / AGENTS §1）：本文每条工作流必须能指出「落在哪个 Phase / 满足哪条 DC / 受哪条预算与门禁约束」；未运行的门禁一律写**未判定**，不得用「应该没问题」代替。

## 1. P0 出口定义（唯一判定口径）

HARNESS §7 对 P0 的出口标准是四条，本文只做拆解，**不得改写**：

| # | P0 出口（HARNESS §7 原文） | 判定载体 | 当前状态 |
| --- | --- | --- | --- |
| **E-P0-1** | vttest + esctest 全通过 | §8.1-1 **G1**（vttest/esctest/kitty 100%；xterm ≥99%，差异登记在案） | **未判定（已有可执行数字）**：harness 已落地，L0 gating **64/64（R=1.0）**，但语料 **275 / AR-31 的 ≥2000**、**真实语料 0% / ≥20%**——后者**环境阻塞**（需 Xvfb + 钉定 xterm 的 oracle 环境，见 SD-20 段）；esctest 逐 commit 可复现但当前 **110 passed / 414 failed**；**vttest 本机无法构建**（无 C 编译器） |
| **E-P0-2** | 三平台 IME/CJK 矩阵全绿 | §8.2 可访问性 + `kernel/05` IN-AC-04（截图矩阵 + 人工会签，AR-31 第 4 条：不进 §5） | **未实现**：无原生窗口 / GPU / IME 宿主；且渲染依赖（wgpu/rustybuzz/swash/winit）在本环境**无法取得 SPDX 证据**（ADR-0015 P3 未知即拒绝）→ ADR-0024 只落地了**零依赖的镜像切片** |
| **E-P0-3** | 性能门禁进 CI | §8.1-4 **G4**（§5 全部门禁；AR-27 的 G4-PR / G4-REL 双口径）+ ADR-0014（RM-A/B/C，云 runner NON-GATING） | **未判定**：B-10 方法学已落地并自证（`bench:check` 7 PASS / `bench:selftest` 54/54），但 **§5 每条指标的测量实现未做**，且**无 RM-A/B/C 参考机** → 工具每次打印 `gating numbers produced by this run: 0` |
| **E-P0-4** | screen 可恢复 | §8.2 可靠性（UI 崩溃 <2s 重连且屏幕一致；sessiond 重建 P95 ≤2s / P99 ≤5s，AR-26 第 4 条） | **部分**：`recover_session` + **attach 握手（WS-05a）** + **TAIL_REPLAY（WS-05b / ADR-0026）** 已交付；但 **sessiond 重建 P95/P99 的验收未做**、**跨段重放未实现**、且 sessiond 目前**不持有 PTY**（真实生命周期仍在 `apps/termai`） |

**附带不得回退的门禁**（P0 期间任一 PR 都不得使其变红）：§8.1-2 G2 行为回放 ≥99.5%、§8.1-3 G3 视觉回归 ≤0.1%、§8.1-5 G5 依赖与许可、§8.1-6 G6 安全与 fuzz 24h；以及 G7（S1–S10）/ G8（B1–B10）设计门禁。

## 2. 现状基线（本次开工实测，不是回忆）

| 项 | 实测结果 | 证据/命令 |
| --- | --- | --- |
| 内核门禁 K1–K8 | **8 PASS / 0 FAIL** | `node tools/kernel-gates/check.mjs` |
| 设计 token 门禁 | PASS | `npm run tokens:check`（M0 收口记录，本计划不重复声称） |
| 设计静态门禁 S1–S10 | PASS | `npm run design:check:static` |
| M0 垂直链路 | 已交付且端到端 6/6 通过（含原生 ConPTY 修复轮） | `docs/plan/m0-delivery-report.md` §3.1 / §6.2 |
| CI 平台矩阵 | **偏差**：全部作业仍只跑 `windows-latest` | `.github/workflows/ci.yml` 头注释「DEVIATION TO TRACK」 |

> **读法**：K1–K8 全绿只证明「M0 切片没有撒谎」，**不构成 P0 出口**。E-P0-1…E-P0-4 四条中三条为未判定/未实现（见 §1），这是本计划存在的原因。

## 3. 差距分析：从 M0 到 P0

### 3.1 内核阻塞项 B-1…B-11（`kernel/00-index.md` §2）

| 阻塞项 | M0 状态 | P0 需补 |
| --- | --- | --- |
| B-1 字节直喂 L0 的接口 | 已具备 | — |
| B-2 三车道分离 | 已实现（机制） | **判定数字**（依赖 WS-01 语料） |
| B-3 每 hop F0/F1/F2 + conpty-rules | 已实现 | 规则 **owner + expires**（OQ-PTY-01 / AR-31 第 3 条） |
| B-4 trait + golden/replay/repro | 部分（repro 未落盘） | repro 自动落盘（`kernel/01 §3.8`） |
| B-5 九元 PtyBackend | 已实现 | — |
| B-6 GridSnapshot 字段集 | 冻结 v1（临时） | OQ-RND-07 裁决 + **组合字符 side table**（SD-08.2） |
| B-7 Session Log + lease 帧位 | 已实现 | **attach 族 msg_type 分配**（SD-07） |
| B-8 24B 帧 + 握手 + CAP-1…CAP-7 | 已实现 | — |
| B-9 InputEncoder 单收口 | 已实现 | IME commit 真机验证（WS-04） |
| B-10 §5 指标 → kernel/06 测量定义 | **未实现** | WS-08（方法学落地 + 参考机） |
| B-11 `xtask conformance` verb | **未实现** | 已由 AR-31 D3/D4 **降为 P1**，P0 用等价测试入口（不得新增未走 ADR 的 verb） |

### 3.2 四类真实缺口（本计划的靶子）

1. **没有窗口宿主**：无 `apps/termai-desktop`、无 wgpu 网格、无 IME 宿主 → E-P0-2 无法开始，E-P0-3 的帧时/网格对齐/key-to-photon 无法测，G3 无从判定。**这是关键路径的头部。**
2. **没有上游一致性套件**：G1 全部未判定，且 AR-31 第 1 条要求 xterm ≥2000 用例 + ctlseqs 每条目 ≥1 用例 + 真实语料 ≥20%（4–6 人周）→ **这是最长的杆**。
3. **没有测量方法学与参考机**：B-10 未实现、RM-A/B/C 未起 → 任何性能数字都不可比、不可作为门禁（ADR-0014）。
4. **CI 平台矩阵未恢复**：ADR-0014 要求 Win x64 + Linux x64 + macOS arm64，现为 Windows-only → 即使三条都做完，§8.1 也无法在 v1 架构上判定。

## 4. 团队组织（Conway ↔ CODEOWNERS，spec 07 §3.1.2）

P0 只启用三条现有团队线，其余（T3 Agent / T4 生态）在 P2/P3 才入场：

| 团队 | P0 使命 | 拥有路径 | CODEOWNERS | 双签触发 | 本计划负责工作流 |
| --- | --- | --- | --- | --- | --- |
| **T1 Core Kernel** | VT 语义、PTY/Transport、渲染管线、会话真源、性能方法学 | `crates/termai-{vt,pty,render,gpu,session,core,store,ipc}` | `@termai/core-kernel` | IPC 布局 / Log 格式 / L0 渲染契约 | WS-01/02/03/05/08 |
| **T2 Shell & UX** | 桌面壳、原生窗口、IME/候选窗、外壳 chrome、token | `apps/termai-desktop`、`packages/{tokens,webview-shell}` | `@termai/shell-ux` | token schema / L2 桥接接口 | WS-04 |
| **T5 DevEx & Release** | CI 拓扑、门禁工程、发布与供应链 | `crates/termai-xtask`、`.github/`、`tools/` | `@termai/devex` | 门禁阈值 / 签名流程 / CI 拓扑 | WS-06/07 |
| **总负责人（Orchestrator）** | 契约裁决、ADR、跨团队排序、诚实会签 | `docs/`、`HARNESS.md`、`AGENTS.md` | `@samabl` | — | WS-09 |

**RACI 口径**：每条工作流有且只有一个 **A**（Accountable，该团队）；跨边界接口变更必须按 E4 由两侧各一名 reviewer 批准。

**治理缺口（必须在 P0 出口前闭合，否则 E4 是空操作）**：
1. **TSC 未成立**（OQ-19）→ §5/§8 的任何放宽无合法批准人。
2. **CODEOWNERS 双签无法强制执行**：仓库只有一个所有者，同一人不能构成双签；且 GitHub 对未解析的 `@termai/*` 会**静默忽略**该条目，使规则退化为空操作。
3. **分支保护未启用**。

> 处置：WS-09 把上述三项作为 P0 的**出口前置**（与 E-P0-1…E-P0-4 并列），不接受「先做完再补治理」——因为 G1 的差异登记、§5 的余量记账都需要一个能批准的人。

## 5. 工作分解（WBS）

    WS-09 契约与治理（总负责人）──────────────────────────────┐（贯穿）
                                                              │
    WS-06/07 CI · 门禁 · fuzz · 供应链（T5）───────┐          │
                                                  │          │
    WS-08 性能测量方法学 + RM（T1）──┐             │          │
                                     ▼             ▼          ▼
    WS-02 PTY/Transport（T1）──┐   [G4 判定]   [§8.1 全绿]  [契约冻结]
                               │
    WS-01 G1 一致性套件（T1）──┤
                               ▼
    WS-03 渲染管线 wgpu（T1）──▶ WS-04 窗口宿主 + IME/CJK（T2）
                               │
    WS-05 会话可恢复 / attach（T1）

| WS | 主题 | A | 出口（可判定） | 依赖 | 依据 |
| --- | --- | --- | --- | --- | --- |
| **WS-01** | G1 一致性套件与语料 | T1 | vttest/esctest/kitty 100%、xterm ≥99%、差异登记；≥2000 用例 + 真实语料 ≥20%；L0 车道判定 | — | §8.1-1、AR-25、AR-31 第 1 条、`kernel/01` §3.1/§3.9 |
| **WS-02** | PTY/Transport 保真与进程树 | T1 | PTY-AC-01…12；孤儿清理 100% 且 ≤2s；PTY-LAT-1 P99 ≤2ms | WS-08（测量） | DC-16、AR-25/28.3/30、`kernel/02` §5 |
| **WS-03** | 原生网格渲染管线 | T1 | RP-01…RP-16；G3 视觉回归；T0–T3 降级矩阵；DPI 切换 ≤1 帧 | WS-02 | AR-01、DC-17、`kernel/03` §5、ADR-0014 |
| **WS-04** | 窗口宿主 + IME/候选窗 + 外壳 | T2 | IN-AC-04 12 组合截图矩阵全绿 + 人工会签；候选窗漂移 ≤2px；preedit 期 PTY 字节 = 0 | WS-03 | `kernel/05` §3.4、AR-29.4、AR-01/22 |
| **WS-05** | 会话可恢复与 attach 全族 | T1 | AC-S1…AC-S6；重建 P95 ≤2s / P99 ≤5s；tail replay / detach notice 可用 | SD-07 裁决 | AR-13/26、`kernel/04` §3.4、SD-07 |
| **WS-06** | CI 平台矩阵与门禁工程 | T5 | v1 架构三平台作业存在且可判定；G4-PR/REL 接入；SBOM/cargo-deny/cargo-audit | — | §8.1、ADR-0014/0021、spec 07 §3.4 |
| **WS-07** | 安全与 fuzz | T5 | VT/PTY/IPC fuzz 24h（≥10⁸ 次）无 crash；崩溃全回归 | WS-01/02 | §8.1-6、DC-37、`kernel/02` PTY-AC-08 |
| **WS-08** | 性能测量方法学 + 参考机 | T1 | §5 19 行（H1…H19）可测；D0/D1 自检；INVALID 语义；RM-A/B/C 起机 | — | B-10、AR-24/27、`kernel/06` §3.9、ADR-0014 |
| **WS-09** | 契约与治理 | 总负责人 | ADR-0023 errata 生效；TSC/分支保护/双签可执行；OQ-19 关闭 | — | §11、AGENTS §3/§5、spec 07 §3.1.2 |

### 5.1 关键路径（决定 P0 最早完成时间）

    WS-09（契约冻结）──▶ WS-03（wgpu 渲染）──▶ WS-04（窗口+IME）──▶ E-P0-2（CJK 矩阵）
                                        └──▶ G3 / RP-* ──▶ E-P0-3（性能门禁）

    WS-01（G1 语料，4–6 人周）────────────────────────────────▶ E-P0-1
    WS-08（方法学 + RM-A）──▶ G4 判定有效 ──▶ E-P0-3

**结论**：P0 的最早出口时间由**两条并行长杆**决定——`WS-01`（语料规模）与 `WS-03→WS-04`（渲染+窗口+IME）。`WS-06/07/08` 是使能件，必须与长杆同时推进，因为它们一旦缺失，长杆的成果无法判定。

## 6. 波次计划与准入

### Wave 1（本次已派单，目标是「让 P0 可判定」）

| 编号 | 工作流 | 团队 | 交付物 | 验收（必须真实运行） |
| --- | --- | --- | --- | --- |
| W1-A | WS-06 平台矩阵 | T5 | Linux x64 作业 + macOS arm64 作业；cfg(unix) clippy 清理；ci.yml 头注释如实更新 | K1–K8 仍 8 PASS；Linux 目标 clippy（若目标可装；否则如实标未验证） |
| W1-B | WS-01 G1 harness | T1 | `tools/conformance/` runner + 确定性 `conformance-report.json` + ctlseqs 派生生成器骨架 + L0 车道收口 | 实测用例数与 L0 通过率；**不得声称 G1 通过** |
| W1-C | WS-08 B-10 | T1 | `tools/bench/`：schema 校验 + fingerprint + §3.1 四态判定 + AR-27 双口径 + OQ-PM-07 分级 + H1…H19 登记 + `--selftest` | `bench:check` / `bench:selftest` 输出；本机数值一律 NON-GATING/INCONCLUSIVE |
| W1-D | WS-09 契约 | 总负责人 | **ADR-0023**（attach 族 msg_type / OSC-DCS 上限 / GridSnapshot clusters side table） | ADR 索引登记；errata 与实现同步（HARNESS §12：ADR 生效 24h 内同步 spec） |


#### Wave 1 回报（截至本轮）

| 编号 | 状态 | 已验证证据 | 未验证边界（诚实） |
| --- | --- | --- | --- |
| W1-A | **完成**（T5） | `.github/workflows/ci.yml` 新增 `linux-x64` + `macos-arm64` 作业（jobs 共 8 个，经 YAML 解析）；`crates/termai-pty` 清 Linux clippy 3 处并用 `ptr::addr_of_mut!` 修掉一个真实 macOS 编译错误（E0308）。实跑：`cargo fmt --check` OK；`clippy --workspace -D warnings` OK；**`clippy --target x86_64-unknown-linux-gnu -D warnings` EXIT 0**（修复前 exit 101）；`clippy --target aarch64-apple-darwin` EXIT 0（check-only）；`kernel-gates` **8 PASS / 0 FAIL**；`--selftest` **18/18 捕获** | Linux runner 上的 K3 与 Node 门禁从未运行；Linux 的 B1–B10 未验证（B4 无 linux-x64 基线会显式 SKIP）；**macOS 从未真正编译/运行**，作业已标 `[first introduction, UNVERIFIED]` |
| W1-B | 进行中（T1） | ctlseqs 生成器：**208 条唯一 ctlseqs 条目全部解析成功 → 208 个覆盖用例**；harness 可编译并验证三车道（L0 2/3→fail、L1 3/3→not_applicable、L2 2/3→registered） | 未接入上游 vttest/esctest；用例数距 AR-31 第 1 条的 ≥2000 仍很远；**不得声称 G1 通过** |
| W1-C | **完成**（T1） | `tools/bench/` 5 文件（schema 引擎 + H1…H19 登记 + 合成夹具 + 门禁 B1–B8 + README）；`package.json` 增加 `bench:check` / `bench:selftest`。实跑：**`bench:check` 7 PASS / 0 FAIL**（§5 19 行逐字一致；bench-report 51 + fingerprint 48 个字段名与 kernel/06 §3.4/§3.7 完全相等；H1…H19 无缺号无重复；阈值每次从 §3.1/§3.6/AR-31 §9 重新推导；指纹单字段变更即哈希变化）；**`bench:selftest` 54/54 捕获**。**本机产出可引用门禁数字 = 0** | 无 RM-A/RM-C → 全部数值 INCONCLUSIVE / NON-GATING；H1–H19 的**实测**、`cargo xtask bench`、两次 G4 全集真跑（A-PM-01）仍未实现 |
| W1-D | **完成**（总负责人） | ADR-0023 生效；HARNESS **CR-14 / CR-15** 登记；`kernel/07` §3.2（0x05xx）、`kernel/01` §8（上限冻结）、`kernel/03` §8（OQ-RND-07 关闭）、`docs/spec/03` §3.3（命名口径）同步。**D1 落码**：`termai-ipc` 0x0500–0x0503 + `sessiond` 线上用例（`cargo test -p sessiond --test wire` **8/8**；`-p termai-ipc` **34/34**）。**D2 落码**：`termai-vt` 三个上限常量 + 按类型分派（`cargo test -p termai-vt` 全绿，含新增 SOS 上限行为用例） | D3（clusters 侧表）未落码；D1 的 attach 状态机（tail replay / detach notice）仍待 WS-05 |

**W1-B / W1-C 上报的规格歧义已按 M0 先例登记**为 [`docs/plan/p0-spec-defects.md`](p0-spec-defects.md)（**SD-09…SD-12**，不新增 HARNESS §11 的 OQ 编号，因为这些是分册 schema/枚举的表述缺口而非未决设计问题）：**SD-09** spec 07 §3.8.2 的扁平对象与 kernel/06 §3.7 的结构化字段同名冲突 → 以 kernel/06 为权威、扁平形态入加法字段，并登记 **HARNESS CR-16**；**SD-10** kernel/06 §4 的 `FailKind` 缺「绝对门禁越界」成员 → 以 `GATE_BREACH` 扩展项实现；**SD-11** kernel/06 §3.2 缺 H3（PTY-LAT-1）的 Run 定义 → 先标为**读数**，待 owner 确认；**SD-12** 三个机器分类原因码属从 ADR-0014/AR-31 推导的扩展。

**总负责人对 W1-A 上报事项的裁决**：

1. **两个新作业保持 blocking**（不设 `continue-on-error`）：ADR-0014 要求 v1 门禁架构**可判定**，非阻断作业等于「有作业没牙」，与 A2「兼容性是入场券」冲突。首次 Linux/macOS 运行若红，按真缺陷修复，不靠豁免。
2. **kernel 作业显示名 `K1-K7 → K1-K8` 接受**：`kernel-gates` 早已执行 K8（构建产物门禁，ADR-0021）。分支保护未启用，无 required-check 名称冲突；若将来启用分支保护，需把新名一并登记。
3. **`ci-cost.json` 与 $3,000/月上限（ADR-0014 决策 6）仍未实现**，登记为 WS-06 的 T5 待办。新增 macOS runner 会显著抬高成本，**在该看板落地前不得声称「CI 成本受控」**。
4. **`rust-toolchain.toml` 不新增跨平台 target**：两个新作业在原生平台运行，不应强迫所有开发机下载他平台 std。W1-A 为验证临时安装了 `aarch64-apple-darwin` target，属本机环境变化，不入库。

#### Wave 2 回报（截至本轮）

| 编号 | 状态 | 已验证证据 | 未验证边界（诚实） |
| --- | --- | --- | --- |
| WS-05a | **完成**（T1） | attach 族从「已冻结数值」变成可用握手：`termai-ipc` 的 CBOR `AttachRequest`/`AttachAck` + 冻结 golden 字节向量、POD `DetachNotice`；`sessiond` 的 `ATTACH_REQUEST`/`DETACH_NOTICE` 状态机；版本求交复用同一 `chosen_version`；**Interactive attach 不授予写权**（须显式 `LEASE_ACQUIRE`，AR-03）。实跑：`kernel-gates` 8 PASS；`cargo test -p termai-ipc` 44；`-p sessiond` 25 + 17 | **`TAIL_REPLAY` 未实现**（WS-05b：需要 Log 事件流重放）——它仍回 `UnsupportedMsg` 且有测试锁住，**未伪造空重放**；多端扇出、lease renew/takeover、shm 路径未做 |
| WS-05b | **完成**（T1） | `TAIL_REPLAY (0x0502)` 落地（ADR-0026）：`termai-ipc` 的 §3.6 事件 oneof + golden 向量、`termai-session` 的 `ReplayGap`/`replay_window_check`、`sessiond` 的重放与**窗口拒绝**。**诚实边界是重点**：窗口不可满足（BelowWindow/AheadOfHead/TailUnreadable）一律回 `AttachStateInvalid` + DROP_NOTICE + 可操作 detail，**绝不给短重放**；`from_seq` 已到 head → 空事件集（非错误）；无 floor → 拒绝。实跑：`cargo test --workspace` 全绿（ipc 53 / session 50 / sessiond 34+20）；`kernel-gates` 8 PASS | **跨段重放未实现**（只读当前 segment，旋转后只会 BelowWindow）；8 MiB 上限未端到端实跑；新 IPC 面无 fuzz 语料；事件只投影 5 类 → **tail replay 不能替代 GRID_SNAPSHOT** |
| WS-03 第一刀 | **完成**（T1） | 新增 `crates/termai-render`（ADR-0024）：UI 侧网格镜像，应用 `GridSnapshot`/`GridDelta`、单调 rev、**缺口即失效并要求快照**（不信任流）；应用顺序固定 scroll → payload → cursor；被拒 delta **不部分应用**。依赖**仅** `termai-core`，**零新增第三方依赖**（wgpu/winit/rustybuzz/swash 仍未准入）。实跑：`cargo test -p termai-render` 8/8；K4 允许边 `termai-render -> [termai-core]` | **不产生任何像素**：E-P0-2 / E-P0-3 状态未变；**VRM（软换行/裁剪）未实现**，因 SD-13（core DTO 缺逐行 LineFlags）——**未用宽度近似替代**，以保复制保真 |

**本轮新增登记**：**SD-15**（快照不带 rev → 快照后 rev 基线无定义）、**SD-16**（`kernel/04` §3.4 首个 Interactive attach 自动授予租约 **与 AR-03 冲突 → 裁决：显式授权优先**）、**SD-17**（`proto_range` vs `proto_min/proto_max` 命名）、**SD-18**（attach 暴露的会话域错误码未登记 → 已在 `kernel/07` §3.8 补登 `NoSuchSession` / `AttachStateInvalid`；`Corrupt` 表达「未 attach」列为 WS-05b 待切换项）。

**新增 ADR**：**ADR-0024**（渲染第一刀：crate 依赖位置 + 第三方依赖**零准入**）。
#### G1 首轮实测（W1-B harness，总负责人独立复跑）

命令：`npm run conformance`（= `node tools/conformance/run.mjs`）。**这是未判定 → 有数字的第一步**，但**不是 G1 通过**。

| 项 | 实测 | 口径 |
| --- | --- | --- |
| 用例总数 | **275**（274 执行 + 1 因能力前置排除） | ctlseqs 派生 + 结构不变量 + spec 用例 |
| L0 门禁子集 | **59/64 → R_strict = R_gate = 0.921875** | 只有 L0 车道出 Pass/Fail；L1/L2 只登记 |
| 仅覆盖（无期望） | 210 | 结构不变量 |
| 三车道纪律 | L1/L2 **从不**产出 PASS/FAIL（`probe_ok=true`） | AR-25.1 / B-2 |
| ctlseqs 覆盖 | **208/208** 条目各有 ≥1 用例；其中 26 条有门禁用例 | AR-31 第 1 条的一部分 |
| 真实语料占比 | **0/275** | AR-31 第 1 条要求 **≥20%** → **未达标** |
| 机器绑定 | `NON_GATING`（非 RM-A/T0） | ADR-0014：云/开发机数值不得作门禁 |
| 判定 | **G1: NOT_JUDGED** | 诚实：用例基数与真实语料都不够 |

**首轮发现 5 个真实 L0 缺陷（这是 harness 的价值所在，不许掩盖）**：

| # | 用例 | 现象 | 初判 |
| --- | --- | --- | --- |
| 1 | `termai-invariants/inv-esc-intermediate-has-no-side-effect` | step 4：光标期望 (0,0) 实得 (4,4) | ESC 中间字节序列**产生了副作用**（不该动光标） |
| 2 | `xterm-ctlseqs-spec/spec-decaln` | 期望整行 `E`，实得空 | **DECALN（`ESC # 8`）未实现** |
| 3 | `xterm-ctlseqs-spec/spec-hpa-basic` | 光标期望 (4,2) 实得 (4,4) | **HPA（`CSI Ps G`）未实现/未生效** |
| 4 | `xterm-ctlseqs-spec/spec-hpr-basic` | 光标期望 (4,6) 实得 (4,4) | **HPR（`CSI Ps a`）未实现/未生效** |
| 5 | `xterm-ctlseqs-spec/spec-rep` | 期望 `aaaa` 实得 `a` | **REP（`CSI Ps b`）未实现** |

> 读法：前三/后两条都属**功能缺失**（未识别序列被忽略），第 1 条若是真缺陷则是**正确性**问题（副作用）。两者都必须先定位再决定：是补实现，还是登记差异。按 HARNESS §8.1-1，xterm 差异必须**登记在案**且受 OQ-VT-04 上限约束（全局 ≤25 项、单 minor 新增 ≤10）——**当前 5 项已用掉 5 个名额**。
> 证据产物：`target/conformance/conformance-report.json`，sha256 `94426e91…84fa73`（`target/` 已忽略，不入库）。

**修复结果（总负责人当轮闭环 —— 修掉，不是登记豁免）**：5 条全部修复。`crates/termai-vt` 三处改动：

1. **`esc_dispatch` 以前忽略 `intermediates`、只按 final byte 派发**，于是 `ESC # 8`（DECALN）被执行成 `ESC 8`（DECRC）——这是**正确性缺陷**，按 K-04 属不可登记（S1 类），必须修。现在 intermediates 参与派发，并实现 DECALN（填 `E`、复位边距、光标归位、清 combining）。
2. 补 **HPA**（`` CSI Ps ` ``）、**HPR**（`CSI Ps a`）、**REP**（`CSI Ps b`）。REP 需要「前一个图形字符」，故新增 `last_graphic` 状态，并以屏幕尺寸为上界（防超大参数）。
3. 同步 `csi_known`：HPA/HPR/REP 不再计 `csi_unknown`；`esc_known` 纳入 `ESC # 8`。
4. 同时修正**语料自身的一处错误前提**：`inv-esc-intermediate-has-no-side-effect` 原先用 `ESC # 8` 表达「未识别序列零副作用」，但 `#8` 是**已定义**的 DECALN；已改为未定义的 `ESC # 9`，保留该不变量的原意（其 `index.jsonl` 的 `documented` 字段本就写着「kernel/01 §3.2 的存在就是为了区分 `ESC # 8` 与 `ESC 8`」）。

**复测**：`npm run conformance` → L0 gating **64/64，R_strict = R_gate = 1.0**，失败 0，报告 sha256 `e4296839…`；`--determinism-check` → **byte-identical**；`cargo test -p termai-vt` 全绿；`kernel-gates` **8 PASS / 0 FAIL**。

> **这仍然不是 G1 通过**：① 用例 275 / AR-31 的 ≥2000；② 真实语料 0% / ≥20%；③ 运行在**非参考机**（`NON_GATING`）。此外 esctest2 全量仍有 325 failed（大量是 `CSI … t` 窗口尺寸查询、颜色族、DECRQM/DECRQSS 未实现——**真实缺口**，不得算作 G1 通过）。

**真实语料 ≥20% 的 oracle 归属（本轮结论，防止把门禁刷成假绿）**：`kernel/01` §5 的 **V-04 明确 `oracle = xterm + Xvfb`**，K-03 固定仲裁顺序（ECMA-48 > xterm ctlseqs 文档 > xterm 实现 > esctest 期望），真实语料的应用集合是 **vim/htop/neovim/fzf/tmux/less/btop**。三点合起来的含义是：

1. **真实捕捉的期望值必须来自外部 oracle**。把「我们自己解析出来的网格」钉成期望值是**循环验证**——它只能算 **G2 回放基线**，不能计入 G1 的 ≥20%。
2. 该 oracle 环境（Xvfb + 钉定 xterm 版本 + 固定 locale/字体 + 上述 TUI 应用）**本机不存在**，因此真实语料路径是**环境阻塞**，不是「还没排期」；它需要 ADR-0014 的 **RM-A/RM-B**。
3. 为此在 `tools/conformance/run.mjs` 加了**机器强制**的区分：`oracle: 'pinned-baseline'` 的用例必须 `real_corpus: true`，但**被排除在 AR-31 的真实语料比例之外**（报告新增 `pinned_baseline_cases` 与 `pinned_baseline_excluded_from_ar31`），并有 fail-closed 校验：真实捕捉**不得**使用 `invariant` oracle。当前实测 `real-world captures (external oracle): 0/275`。
### Wave 2（W1 收口后开，目标是「让 P0 可看见」）

1. **WS-03 起桩**：`crates/termai-render` + `crates/termai-gpu`（新 crate 走 ADR-0019 追认 + K4 依赖白名单 + CODEOWNERS），先做 damage→shaping→atlas→present 的最小闭环与 T0 后端探测。
2. **WS-05 SD-07 落地**：按 ADR-0023 实现 TAIL_REPLAY / DETACH_NOTICE / LEASE_TRANSFER。
3. **WS-01 语料规模化**：ctlseqs 条目 100% 覆盖 + 真实语料接入（≥20%）+ repro 自动落盘（B-4）。

### Wave 3（目标是「让 P0 可判定为达标」）

1. **WS-04**：`apps/termai-desktop` + 原生 IME 宿主 + 12 组合 CJK 矩阵。
2. **WS-08**：RM-A/RM-B/RM-C 起机，D0/D1 自检通过后接入 G4-PR / G4-REL。
3. **WS-07**：24h fuzz 与崩溃回归。

### 波次准入条件（Gate-to-start）

- 任何新 crate 进入工作区前，必须有 ADR 说明依赖位置，并同步 K4 白名单 + K7 CODEOWNERS 覆盖（否则门禁直接红）。
- **命名对齐（Wave 2 前置，已裁决：HARNESS §11.2 CR-15）**：物理 crate 名以 `termai-*` 为唯一真源；HARNESS §4.3 与 spec 03 §3.3 的 `term-render` / `ui-native` / `shell-bridge` / `web-shell` / `plugin-ui-sdk` 是**层级角色名**；`core-dto` 当前由 `termai-core` 承载（ADR-0019 D1 + ADR-0023 D3），拆分需新 ADR。WS-03 的新 crate 立项 ADR 仍须按 AGENTS §3 说明依赖位置并同步 K4 允许边 + K7 CODEOWNERS；命名本身已不再是阻塞项。
- 任何对外契约变更（msg_type / Log 格式 / Grid DTO / capability）必须先有 ADR，且 ADR 落地前只落 tag 表、不落编码（AR-28 第 1 条的操作含义）。
- 任何 §5 / §8 数值的放宽，必须附基准数据 + TSC 批准（AGENTS §5）。**当前 TSC 不存在，故该项在 WS-09 闭合前一律禁止。**

### 6.3 派单纪律（基于前 24 轮的实测教训，非理论）

**实测事实**：两个并行工作流的切片**大于一轮的产出能力**时，它们都退化成「持续读规格、零文件产出」——WS-13 跑到第 8 轮、最后要由总负责人**中断**才落地（中断后我复核其待提交 diff，反而发现并修掉一个真实集成缺口）；WS-VRM 长切片版跑到第 5 轮、**零改动**，同样只能中断。**两次都发生，说明是派单方法的问题，不是工作流的问题。**

**因此 P0 期间的派单必须满足以下五条**：

1. **一轮可验证**：切片必须能在**一轮内**达到「编译通过 + 测试通过 + 可提交」。做不到就再切小。
2. **显式非目标**：必须写明「**本切片不做**」的清单（例：不做 `ScrollAnchor`、不做命中测试、不做 a11y 映射）。否则模型会顺手把范围铺开。
3. **限定阅读面**：只指定**必读的少数几处**（文件 + 小节），并说明「其余不要读」。无限阅读是无限规划的燃料。
4. **证据格式前置**：要求贴**关键断言原文**与实跑输出行，**不接受摘要**——摘要是语义漂移最容易藏身的地方（第 9 轮我已经吃过一次：一个错误记录值 201 被当成基线用了两轮）。
5. **止损条款**：写明「若 N 分钟内不能编译通过，就停下并回报**报错原文**，不要扩大范围」。
6. **被证伪的回滚理由必须显式撤销并重做**：若某次回滚/否决的**登记理由**后来被证据推翻，必须① 把理由更正写回原登记；② 显式重新排期该改动，而不是让它停在「曾被否决」的状态。**先例**：SD-19 的窗口查询实现，当时以「201 → 110 是回归」为由回滚；SD-20 二分后证明 201 是**错误记录**（该提交实测 103），该改动其实是 **pass-neutral 且把 688 次适配器代答清零**。**能重开的前提是当时留下了完整实验数字**（替换数 / 通过数 / feeds）——所以第 4 条（证据格式）同时也是这条的基础。
7. **「机械切片」与「判断切片」要分派方式不同**：实测三类小切片**一次落地**（VRM 映射、`WrapMode` 归属方、IPC fuzz smoke），它们的共同点是**产出物可由既有类型/规则机械推出**；而**要求从第三方文档「推导新期望」**的切片（G1 语料 +20：从 ctlseqs 文档判断 20 个条目的行为）**跑了 3 轮零产出**，尽管任务书已含 20 分钟止损条款与「宁可 8 条确定的不凑 20 条」的退路。**结论**：判断切片必须在派单**之前**由负责人把判断做完——例如先产出「条目 → 文档行为」的对照表，再让工作流把表**机械转写**成用例；否则它会一直在读文档、无法收敛。

**负责人一侧的对称义务**：中断一个工作流**不是浪费**——中断后必须**亲自读它待提交的 diff**再决定取舍（WS-13 的 case 证明这是捡回真实缺陷的机会，而不是收尾动作）。
## 7. 门禁与度量计划

| 门禁 | P0 目标 | 当前 | 负责 | 接入方式 |
| --- | --- | --- | --- | --- |
| G1 VT 兼容 | vttest/esctest/kitty 100%；xterm ≥99% + 差异登记 | 未判定 | T1 (WS-01) | PR-S3 子集 + nightly 全量（spec 07 §3.4.2） |
| G2 行为回放 | ≥99.5% | 部分 | T1 (WS-01) | `.trec` 语料回放 |
| G3 视觉回归 | ≤0.1% | 不适用（无渲染） | T2 (WS-03/04) | B4 + ADR-0022 的 runner 基线 |
| G4 性能 | §5 19 行；>5% 回归阻断 | 未判定 | T1 (WS-08) + T5 | G4-PR（每 PR）/ G4-REL（连续 2 夜或发版前），前置 D0/D1 |
| G5 依赖与许可 | GPL/AGPL/SSPL = 0；SBOM | 部分 | T5 (WS-06) | cargo-deny + cargo-audit + npm audit + CycloneDX |
| G6 安全与 fuzz | 24h / ≥10⁸ 次无 crash | 部分（smoke） | T5+T1 (WS-07) | nightly fuzz 靶：VT/PTY/IPC |
| G7/G8 设计门禁 | S1–S10 / B1–B10 | PASS | T2 (WS-04) | `design:check` |

**度量诚实条款**：
1. 非 T0 GPU 后端上的性能数值一律 **NON-GATING**（ADR-0014 / spec 07 §3.8.1）。
2. 云 runner 上的性能数值一律 NON-GATING，参考机不可用时该门禁项标 **INCONCLUSIVE**，不得用云 runner 顶替。
3. 测量本身未通过 D0/D1 可复现性自检前，判 **INVALID** 且不得与基线比对（AR-27）。
4. 达标余量记入技术债登记，不得因为「高于门禁」而放弃记账（AR-19）。

## 8. 可追溯矩阵（P0 出口 → 工作流 → 证据）

| P0 出口 / 验收 | 依据（AR/DC/§） | 工作流 | 证据形态 | 状态 |
| --- | --- | --- | --- | --- |
| vttest + esctest 全通过 | §7 E-P0-1、§8.1-1、AR-25、AR-31.1 | WS-01 | G1 报告 + 差异登记 + repro 目录 | 未判定（W1-B 起桩） |
| xterm ≥99% | 同上 | WS-01 | ≥2000 用例报告（含真实语料占比） | 未判定 |
| 三平台 IME/CJK 矩阵全绿 | §7 E-P0-2、`kernel/05` IN-AC-04、AR-29.4/AR-31.4 | WS-04 | 12 组合截图矩阵 + 人工会签记录 | 未实现 |
| 性能门禁进 CI | §7 E-P0-3、§8.1-4、AR-27、ADR-0014 | WS-06/08/03 | G4-PR/REL 报告 + 机器指纹 | 未判定（W1-C 起桩） |
| screen 可恢复 | §7 E-P0-4、§8.2、AR-13/AR-26 | WS-05 | 重建 P95/P99 报告 + AC-S1…S6 | 部分（恢复器已交付；attach 全族未实现） |
| 孤儿进程清理 = 100% 且会话关闭 ≤2s 回收 | AR-30 第 2 条、§8.2、`kernel/02` §3.3 | WS-02 + WS-05 | PTY-ORPHAN-1 用例（native + pipe） | **部分**：native ConPTY 路径已补覆盖（根 + **后代**，`kill(Force)` 后 2s 内清空，commit `89b491d`）；但 **M0 的 sessiond 不持有 PTY**（registry 只走 `TerminalEngine` trait，真实 PTY 生命周期在 `apps/termai` CLI），故「**会话关闭 → 回收**」在守护进程层**尚未实现**，当前仅靠 CLI 进程退出时关闭 Job 句柄兜底；且 `close()` 的契约是「**不隐式杀树**」，长期存活的 sessiond 必须在会话关闭时**显式 kill**，否则首次多会话就会泄漏 |
| 行为回放 ≥99.5% | §8.1-2 | WS-01 | `.trec` 回放报告 | 部分 |
| 视觉回归 ≤0.1% | §8.1-3 | WS-03/04 | B4 diff | 不适用（无渲染） |
| 依赖与许可 | §8.1-5 | WS-06 | cargo-deny/audit/SBOM | 部分（K4/K5 声明清单） |
| 安全与 fuzz 24h | §8.1-6 | WS-07 | fuzz 报告 | 部分（smoke） |
| 三平台 CI 可判定 | ADR-0014 | WS-06 | ci.yml 作业 + runner 证据 | 偏差（Windows-only，**W1-A 修复**） |
| §5 每条指标有测量定义 | B-10、AR-24.3 | WS-08 | H1…H19 登记 + 判定实现 | 未实现（**W1-C 起桩**） |
| attach 族数值契约 | SD-07 | WS-05/09 | ADR-0023 + 线上协议测试 | **D1 数值已落地**（`termai-ipc` 0x0500–0x0503 + `sessiond` 线上用例 8/8）；attach 状态机（tail replay / detach notice）仍待 WS-05 |
| 字符串态上限唯一数值 | SD-08.1 / ADR-0023 D2 | WS-01 + WS-09 | `kernel/01` §8 + `termai-vt` 常量 + `BackendCaps` | **已落地**（OSC 1 MiB / DCS·APC 16 MiB / SOS·PM 1 MiB）：新增第三个 `SOS_PM_LEN_LIMIT_DEFAULT`，上限选择按字符串类型分派（M0 的 bug 是 SOS/PM 复用了 DCS 上限）；`terminal_api.rs` 常量断言与 `strings.rs` 的 SOS 行为用例已更新。SOS/PM 溢出仍计入既有 `DcsOverflow` 键——14 个计数键由 `kernel/01` §3.4 冻结，不新增键 |
| 组合字符复制保真 | SD-08.2 / `kernel/01` K-08、V-10 | WS-03/09 | GridSnapshot clusters 侧表 + golden | 未裁决（**W1-D**） |

## 9. 风险登记（P0 执行专属，补充 HARNESS §9）

| # | 风险 | 触发条件 | 缓解 | 责任 |
| --- | --- | --- | --- | --- |
| P0-R1 | 渲染+窗口+IME 三件套是「从零到一」，时长不可压缩 | WS-03/04 连续两波无可见产物 | 先定死 damage/GridDelta 契约与 T0 探测，再做外观；不追求 v1 视觉一次到位 | T1/T2 |
| P0-R2 | G1 语料建设变成无底洞 | 用例数增长但差异表不收敛 | ctlseqs 条目映射覆盖率作为中间指标；差异登记设豁免上限 | T1 |
| P0-R3 | 性能门禁在无 RM 期间被「临时放宽」 | 有人提议用云 runner 数值替代 | 纪律：非参考机数值 = NON-GATING，INCONCLUSIVE 不等价于通过（ADR-0014） | T1/T5 |
| P0-R4 | 契约变更绕过 ADR 直接改码 | 出现「先实现再补 ADR」 | 波次准入条件：ADR 先于编码；CI 侧由 K6/K7/K8 兜底 | 总负责人 |
| P0-R5 | 治理缺口使 E4 长期空转 | TSC/分支保护未建 | 列为 P0 出口前置（§4） | 总负责人 |
| P0-R6 | 平台矩阵恢复引入 Linux/macOS 特有缺陷 | Linux 作业首次全量 K3 | 先在本地跨目标 clippy/check 收敛，再上 runner | T5 |

## 10. 诚实边界（本计划不主张什么）

> **未闭合项的唯一索引**：[docs/audit/debt-p0.md](../audit/debt-p0.md)。本文给「怎么组织、按什么顺序做」，该文件给「现在到底还差什么、谁负责、什么条件才能关」。两者冲突时，以该文件的**逐条依据编号**为准。

1. 本计划**不主张任何 P0 出口已达成**：E-P0-1/E-P0-3 为未判定，E-P0-2 为未实现，E-P0-4 为部分。
2. 本计划**不产出任何可用于门禁的性能数字**：本机不是 RM-A/RM-C，且 kernel/06 的测量自检尚未实现。
3. 本计划**不主张 G1 通过**：Wave 1 只建立 harness 与可复现报告；AR-31 第 1 条的 ≥2000 用例 + 真实语料 ≥20% 是后续波次的工作量，需要 4–6 人周。
4. 本计划**不修改任何历史文档**：M0 报告与 `docs/roles/*` 保持冻结；新增结论一律以 AR/ADR/CR 追加。

## 11. 变更记录

| 日期 | 变更 | 依据 |
| --- | --- | --- |
| P0 立项 | 建立 P0 出口拆解、差距分析、团队组织、WBS、波次、门禁与可追溯矩阵；Wave 1 派单（W1-A/B/C/D） | HARNESS §7/§8/§5/§2、kernel/00-index §2、m0-delivery-report |
