# TermAI P0 交付计划（P0 · 信任基座）

> **效力**：本文是**交付计划**，不是设计权威。与 [HARNESS.md](../../HARNESS.md) 冲突之处一律以 HARNESS 为准（AGENTS §1）。
> **依据**：HARNESS §7（P0 出口标准）、§8（质量门禁与验收）、§5（预算）、§2（AR-24…AR-31）、§11.2（CR-08…CR-13）；`docs/spec/kernel/00-index.md` §2（P0 阻塞项 B-1…B-11）；ADR-0014（平台矩阵与参考机）、ADR-0018/0019/0020（内核契约与准入）、ADR-0021/0022（CI 产物与视觉基线）。
> **前置**：M0「信任基座纵向切片」已交付（headless：PTY → VT → Grid → Session Log → IPC → capability），证据见 [m0-delivery-report.md](m0-delivery-report.md)。
> **纪律**（HARNESS §0.2 / AGENTS §1）：本文每条工作流必须能指出「落在哪个 Phase / 满足哪条 DC / 受哪条预算与门禁约束」；未运行的门禁一律写**未判定**，不得用「应该没问题」代替。

## 1. P0 出口定义（唯一判定口径）

HARNESS §7 对 P0 的出口标准是四条，本文只做拆解，**不得改写**：

| # | P0 出口（HARNESS §7 原文） | 判定载体 | 当前状态 |
| --- | --- | --- | --- |
| **E-P0-1** | vttest + esctest 全通过 | §8.1-1 **G1**（vttest/esctest/kitty 100%；xterm ≥99%，差异登记在案） | **未判定（已有可执行数字）**：harness 已落地，L0 gating **64/64（R=1.0）**，但语料 **275 / AR-31 的 ≥2000**、**真实语料 0% / ≥20%**——后者**环境阻塞**（需 Xvfb + 钉定 xterm 的 oracle 环境，见 SD-20 段）；esctest 当前 **267 passed / 41 known-bug / 259 failed / substitutions 0**（**必须带 `-- --expected-terminal xterm --xterm-checksum 336`**；缺该参数时 esctest 会对每格校验和取反，得出 110/117 一类的**伪失败**，见 §6.3 规则 8），**已通过 AR-27 两次运行自检**（两次结果与失败集合逐字节相同）；**vttest 本机无法构建**（无 C 编译器）。**⚠ 第 256 轮（ADR-0029 D-1）：G1 的判定域是 kernel/01 §5 的**两个**套件，不是一个**——**V-02（esctest）** 要求 **`R_strict=1 且 X=0`**；**V-04（xterm 语料）** 才是「**≥99% + 差异登记**」。因此这 **259 条失败全部按硬缺口计**，**不得**用差异登记豁免（对 esctest 登记差异会要求 `X>0`，与 K-01 冲突，且构成 §8.1-1 的放宽） |
| **E-P0-2** | 三平台 IME/CJK 矩阵全绿 | §8.2 可访问性 + `kernel/05` IN-AC-04（截图矩阵 + 人工会签，AR-31 第 4 条：不进 §5） | **仍未实现（但渲染侧 CPU 半身已基本建成）**：~~无原生窗口 / GPU / IME 宿主；且渲染依赖（wgpu/rustybuzz/swash/winit）在本环境**无法取得 SPDX 证据**（ADR-0015 P3 未知即拒绝）→ ADR-0024 只落地了**零依赖的镜像切片**~~ → **第 262–272 轮已推翻其中两半**：① **依赖准入已解决**（ADR-0027 Accepted；wgpu 落地第 267 轮，`rustybuzz`/`swash`/`fontdb` 落地第 272 轮；`winit` 已准入但**尚未进入 workspace**）；② **渲染管线已从「零依赖镜像」推进到**：S5 镜像 → **S6 shaping**（`1e2977a`）→ **S7 atlas + 真实栅格化**（`2d895f5`）→ **RP-05 字形摆放与网格对齐测量**（`a08e5f1`，实测四档 DPI 最差 0.5000/0.4375/0.5000/0.5000px）→ **GPU 离屏 draw+readback**（`99cbd26`）。**仍然缺的正是 E-P0-2 的名字**：**原生窗口宿主（winit）与 IME 宿主（TSF / NSTextInputClient / text-input-v3）一行未写**，故 12 组合 CJK 矩阵（IN-AC-04）无法开始；S8 的 draw/present 也还没有消费者（`termai-render → termai-gpu` 边未准入） |
| **E-P0-3** | 性能门禁进 CI | §8.1-4 **G4**（§5 全部门禁；AR-27 的 G4-PR / G4-REL 双口径）+ ADR-0014（RM-A/B/C，云 runner NON-GATING） | **未判定（但首次出现了「被真正判定的 §5 行」）**：B-10 方法学已落地并自证（`bench:check` 8 PASS；带 `--report` 时 **9 PASS**，含 **B8**）/ ~~`bench:selftest` 54/54~~ → **83/83（第 272 轮）**。**第 272 轮新增两件事**：① **H12（输入字节等价）已可判定**——这是**本项目第一条在本机被真正判定的 §5 门禁行**（依据：它是 registry 里唯一 `machine:'none'` 的行）。~~并且判出 FAIL = 93.3333%（70/75），5 处分歧每条都引了 kernel/05 §3.3 的条款（见 A28）~~ → **它先判出 FAIL（93.3333%，5 处分歧各有条款），随后按 K-03 裁决修编码器（`a555a77`）后为 `100%（75/75）`、`[B8] PASS`；语料一字未改。§3.2 与 §3.3 的措辞冲突另立 SD-26（见 A28）**；② **R1/R2（sessiond 重建）机制与生产者已实现**，实测 1.953ms / 2.314ms（都在门禁内）但**仍 INCONCLUSIVE / NON_GATING**（无 RM-A 指纹）。**仍缺**：RM-A/B/C 参考机才能让机器绑定行成为门禁数字；H17/H18/H19 仍 `not_reported`；`gatingNumbersProduced` 仍为 **0**（机器绑定计数器，ADR-0029 D-5）。**「进 CI」的一半早已完成**：`bench:check` 与 `conformance L0` 已接入 Windows/Linux 两个 job（结构性门禁阻断；§5 数字在 INCONCLUSIVE 状态下按 ADR-0014 不阻断）——**缺的仍是参考机上的判定** |
| **E-P0-4** | screen 可恢复 | §8.2 可靠性（UI 崩溃 <2s 重连且屏幕一致；sessiond 重建 P95 ≤2s / P99 ≤5s，AR-26 第 4 条） | **部分**：`recover_session` + **attach 握手（WS-05a）** + **TAIL_REPLAY（WS-05b / ADR-0026）** 已交付；~~但 **sessiond 重建 P95/P99 的验收未做**、**跨段重放未实现**、且 sessiond 目前**不持有 PTY**（真实生命周期仍在 `apps/termai`）~~ → **第 272 轮更正：跨段重放已于第 115–125 轮落地（A10）；sessiond 已在守护进程层持有 PTY（`fd04a11`，A8，关闭 → 树回收 ≤2s 有真实进程验收）**。**仍缺**：**sessiond 重建 P95/P99 的验收（A9：机制已实现、数字在门禁内，但无 RM-A 指纹 → INCONCLUSIVE）**；~~且 `SessionHost::probed()` 尚无调用者（main 仍是 stub）~~ → **第 272 轮已闭合（`6e2ae65`）**：`daemon.rs` 的 serve 循环在生产路径上持有 `SessionHost`，其唯一出口调用 `host.close_all()`（另有 `Drop` 兜底），并有真实的 OS 级端到端运行证据与 5 条真实进程验收。**新增缺口**：A29（线协议无 `SESSION_CREATE`，会话生命周期仍是 argv + 进程生命周期）、A30（链路 EOF 即退出，与 AR-13「不得因无人 attach 而终止」在 M0 传输下冲突，待 P1 supervisor 模型重建） |

**附带不得回退的门禁**（P0 期间任一 PR 都不得使其变红）：§8.1-2 G2 行为回放 ≥99.5%、§8.1-3 G3 视觉回归 ≤0.1%、§8.1-5 G5 依赖与许可、§8.1-6 G6 安全与 fuzz 24h；以及 G7（S1–S10）/ G8（B1–B10）设计门禁。

## 2. 现状基线（本次开工实测，不是回忆）

| 项 | 实测结果 | 证据/命令 |
| --- | --- | --- |
| 内核门禁 K1–K8 | **8 PASS / 0 FAIL** | `node tools/kernel-gates/check.mjs` |
| 设计 token 门禁 | PASS | `npm run tokens:check`（M0 收口记录，本计划不重复声称） |
| 设计静态门禁 S1–S10 | PASS | `npm run design:check:static` |
| M0 垂直链路 | 已交付且端到端 6/6 通过（含原生 ConPTY 修复轮） | `docs/plan/m0-delivery-report.md` §3.1 / §6.2 |
| CI 平台矩阵 | ~~**偏差**：全部作业仍只跑 `windows-latest`~~ → **已闭合（第 271 轮实测）**：CI 共 **8 个作业**（Windows kernel / linux x64 / macos arm64 / design / tokens / rust / windows build / design-baseline），run **35890840865**（sha `c221807`）**全绿** | `.github/workflows/ci.yml`；GitHub Actions run 35890840865 |

> **读法**：K1–K8 全绿只证明「M0 切片没有撒谎」，**不构成 P0 出口。**四项待决策见 `docs/plan/p0-open-decisions.md`**（D-1 判定域、D-2 颜色能力声明、D-3 设备身份、D-4 §8.2 测量归属）——**它们各自阻塞一批失败或一项验收，且决定权不在实现者****。E-P0-1…E-P0-4 四条中三条为未判定/未实现（见 §1），这是本计划存在的原因。

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
1. **TSC 未成立**（OQ-19）→ §5/§8 的任何放宽无合法批准人。**注意**：ADR-0023 / ADR-0028 / ADR-0029 之所以能由总负责人标记 Accepted，是因为它们**不含任何 §5/§8 放宽**（ADR-0029 明确否决了构成放宽的「读法 B」）；任何放宽仍必须等 TSC。
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
| W1-C | **完成**（T1） | ~~`tools/bench/` 5 文件~~ → **第 255 轮加 `values.mjs` 后为 6 文件** → **第 257 轮加 `reliability.mjs` 后为 7 文件**（schema 引擎 + H1…H19 登记 + **§8.2 可靠性独立登记** + 合成夹具 + **值的读取与 §5 行绑定** + 门禁 B1–B9 + README）；`package.json` 增加 `bench:check` / `bench:selftest`。实跑：**`bench:check` ~~7 PASS~~ → 第 257 轮 8 PASS / 0 FAIL**（§5 19 行逐字一致；bench-report 51 + fingerprint 48 个字段名与 kernel/06 §3.4/§3.7 完全相等；H1…H19 无缺号无重复；阈值每次从 §3.1/§3.6/AR-31 §9 重新推导；指纹单字段变更即哈希变化）；~~**`bench:selftest` 54/54 捕获**~~ → **第 255 轮 63/63、第 256 轮 65/65、第 257 轮 74/74（ADR-0029 D-4 加 B9 的 5 注入 + 3 对照 + 1 基线）**（含 D-6 的注入 + 对照：缺行显式化、H19 误标被 unit+gate 两处不匹配抓住、`gating:true` 使 `B7` FAIL）。**本机产出可引用门禁数字 = 0** | 无 RM-A/RM-C → 全部数值 INCONCLUSIVE / NON-GATING；H1–H19 的**实测**、`cargo xtask bench`、两次 G4 全集真跑（A-PM-01）仍未实现 |
| W1-D | **完成**（总负责人） | ADR-0023 生效；HARNESS **CR-14 / CR-15** 登记；`kernel/07` §3.2（0x05xx）、`kernel/01` §8（上限冻结）、`kernel/03` §8（OQ-RND-07 关闭）、`docs/spec/03` §3.3（命名口径）同步。**D1 落码**：`termai-ipc` 0x0500–0x0503 + `sessiond` 线上用例（`cargo test -p sessiond --test wire` **8/8**；`-p termai-ipc` **34/34**）。**D2 落码**：`termai-vt` 三个上限常量 + 按类型分派（`cargo test -p termai-vt` 全绿，含新增 SOS 上限行为用例） | D3（clusters 侧表）未落码；D1 的 attach 状态机（tail replay / detach notice）仍待 WS-05 |

**W1-B / W1-C 上报的规格歧义已按 M0 先例登记**为 [`docs/plan/p0-spec-defects.md`](p0-spec-defects.md)（**SD-09…SD-12**，不新增 HARNESS §11 的 OQ 编号，因为这些是分册 schema/枚举的表述缺口而非未决设计问题）：**SD-09** spec 07 §3.8.2 的扁平对象与 kernel/06 §3.7 的结构化字段同名冲突 → 以 kernel/06 为权威、扁平形态入加法字段，并登记 **HARNESS CR-16**；**SD-10** kernel/06 §4 的 `FailKind` 缺「绝对门禁越界」成员 → 以 `GATE_BREACH` 扩展项实现；**SD-11** kernel/06 §3.2 缺 H3（PTY-LAT-1）的 Run 定义 → 先标为**读数**，待 owner 确认；**SD-12** 三个机器分类原因码属从 ADR-0014/AR-31 推导的扩展。

**总负责人对 W1-A 上报事项的裁决**：

1. **两个新作业保持 blocking**（不设 `continue-on-error`）：ADR-0014 要求 v1 门禁架构**可判定**，非阻断作业等于「有作业没牙」，与 A2「兼容性是入场券」冲突。首次 Linux/macOS 运行若红，按真缺陷修复，不靠豁免。
2. **kernel 作业显示名 `K1-K7 → K1-K8` 接受**：`kernel-gates` 早已执行 K8（**工作流策略门禁**：产物留存 ADR-0021 ＋ **每个 check 步骤必须配对 selftest**——第 141 轮加入，见登记表 K8 段）。分支保护未启用，无 required-check 名称冲突；若将来启用分支保护，需把新名一并登记。
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
#### Wave 3 回报（第 272 轮：三条切片，子代理执行 + 总负责人独立复核后提交）

**入口纪律（本轮新增一条，写在这里以便下一个会话读到）**：本会话的三条切片都遵守了「一轮可验证 + 显式非目标 + 限定阅读面 + 原件证据 + 止损」五条，**三条全部落地**；其中两条是**判断/管道类**切片（vttest 可行性、sessiond 重建路径），它们过去是「跑几轮零产出」的重灾区——本轮能落地的原因不是模型变好，而是**派单前把管道搭好了**：vttest 那条明确给了「四步顺序 + 25 分钟可行性预算 + 组装工具链的候选路径」，sessiond 那条明确给了「已核实的事实清单，不许重新推导」。**这一条是 §6.3 规则 7/9 的正例**。

| 编号 | 工作流 | 状态 | 已验证证据（总负责人亲自实跑） | 未验证边界（诚实） |
| --- | --- | --- | --- | --- |
| **W3-A** | A8：sessiond 持有 PTY + 会话关闭显式杀树（E-P0-4 / AR-30 第 2 条） | **完成**（`fd04a11`） | `apps/sessiond/src/host.rs`（`SessionHost`/`PtySession`）：`kill(Force)` → 有界 `wait`（`REAP_BUDGET = 1.5s`）→ 恰好一次释放 pty；reap 失败报 `NotReaped` 而非成功。`apps/sessiond/tests/pty_lifecycle.rs`：关闭后**子进程与孙进程** 2s 内消失（自 close 起轮询），对照 1「未关闭的会话同时刻仍有活树」、对照 2「close 幂等」。**非空真性**：临时桩掉 kill+wait → `left: 2 / right: 0`。总负责人实跑：`cargo test --workspace` exit 0（sessiond lib 42 / pty_lifecycle 3 / wire 20）、`kernel-gates` **8 PASS**、`conformance` **68/68**、`bench:check` **8 PASS** | **E-P0-4 仍为「部分」**：① `SessionHost::probed()` **没有任何 main 循环调用它**（`apps/sessiond/src/main.rs` 仍是 stub），`broker.rs` 也无 session-closed 拆除点（只有 `GO_AWAY → Draining`）——即这条回收路径在当前生产路径上**还没有触发者**；② A9 的重建时延验收（AR-26 第 4 条）未做；③ Unix 侧的树枚举只列组长（`crates/termai-pty/src/unix/pty.rs:418-436`），故「孙进程」断言只在 Windows/ConPTY 上成立 |
| **W3-B** | kernel/03 **S6 shaping + S7 atlas**（E-P0-2 关键路径第一刀） | **完成**（`6f3a590`） | 根 `Cargo.toml` 准入 `rustybuzz 0.20.1` / `swash 0.2.10` / `fontdb 0.24.0`（ADR-0027 D1 tier A；unmaintained 公告走 ADR-0031 的 W-01）。`shape.rs`：cluster→cell 跨度 + `fit_squeezed` 计数 + `canonical_bytes` 字节级确定性，`ColumnMismatch`/`SpanWidthMismatch` **可触发**；`atlas.rs`：`AtlasKey` 货架分配 + 页 LRU + miss/hit/eviction/generation。真实系统字体（`fontdb` → `C:\Windows\Fonts\BIZ-UDGothicB.ttc`），无字体即**响亮失败**。总负责人实跑：`cargo test --workspace` exit 0、`clippy -D warnings` exit 0、`fmt --check` exit 0、`kernel-gates` **8 PASS**（K4 允许边未变） | **本切片不产像素**，故**它自身不移动 E-P0-2 也不移动 E-P0-3**；RP-01/02/05/10/11/14 与 §5 每一行仍未判定。**K-04 缺口（新登记）**：`kernel/03:187` 要求的 `termai-vt::width::measure` **不存在公开 API**（`crates/termai-vt/src/lib.rs:17-29`；唯一 wcwidth 用法在私有 `grid.rs:19/911`），因此本切片用注入端口 `CellWidthSource` 保住了 K-04 的实质（`termai-render` 无第二张宽度表），但那条规格行**目前仍是纸面**；把它变成真的是下一刀（公开宽度 API + K4 登记 ADR-0027 D2 的 `termai-render -> termai-vt` 边） |
| **W3-C** | A3：vttest 可构建性（E-P0-1 / V-01） | **完成（结论：仍不可构建，但原因被更正）**（`c3c66c8`） | 四路探针的原始输出（`tools/conformance/vttest/README.md`）：MSVC `cl` 19.51 与 w64devkit `gcc` 14.1 + `make` 4.4.1 **都可用**；A3 原引的 `no acceptable cc found in $PATH` **可复现但根因是 autoconf 在 `;` 分隔 PATH 上按 `:` 切分**（同一 shell 里 `gcc --version` 正常）；真正的墙是 **`vttest.h:49-58` 只要 termios.h/termio.h/sgtty.h，而 mingw-w64 三者皆无** → `vttest.h:57: #error please fix me`。脚本**有意不打补丁**（桩掉 termios 的 vttest 不再是 V-01 需要的那把上游尺子） | **未产出一行 vttest 输出、没有 `probe.mjs`、不主张任何 G1 进展**。即便按已验证的解法装上 MSYS2 `msys` 工具链（10–20 分钟，T5 系统变更），V-01 **仍不可判定**：① 期望网格需钉定 **xterm + Xvfb** oracle（本机无 X11）；② 需 `expect` 式**双向** driver 回答 DA/DECRQM/DSR；③ `unix_io.c:260-266` 的 `readnl()` 在管道 EOF 时 `read()` 返回 **0 而非 −1** → 有限 stdin 脚本**死循环而非退出** |

**发起人裁决（第 272 轮，逐条记录以便审计）**：

1. **oracle/参考机环境（Xvfb + 钉定 xterm、RM-A/B/C）：本轮不做** → **E-P0-1 的真实语料 ≥20% 与 vttest golden 保持「未判定」**，不做替代方案、不用自钉基线冒充。
2. **治理前置 C1–C3（TSC 未成立 / CODEOWNERS 双签不可执行 / 分支保护未启用）：暂不处理，继续作为出口前置登记** → 期间 **§5/§8 的任何放宽一律禁止**（AGENTS §5）。
3. **提交策略：本地按切片用显式路径提交 + 推送到远端当前分支以触发真实 CI**（`0f699b9..6f3a590` 已推送；CI 的 `on: push` 覆盖全部作业）。

**本轮新增的登记**：**A20**（`esctest-report.mjs` 打印的「可复现命令」漏 `--xterm-reverse-wrap`，**复现不了它自己印的数字**；同处 `kernel/01:279` 的 `suites.toml` 与实现的 `suites.json` 命名漂移）；**A3 原因更正**；**A6 关闭**；**A4/首屏口径更正**；**A8 在守护进程层闭合**。详见 [debt-p0.md](../audit/debt-p0.md)。

#### Wave 4 回报（第 272 轮后半：三刀 + 一次 CI 事故的完整闭环）

| 编号 | 工作流 | 状态 | 已验证证据（总负责人亲自实跑） | 未验证边界（诚实） |
| --- | --- | --- | --- | --- |
| **W4-A** | K-04 单一列宽权威 + S6/S7 测试可移植（E-P0-2 前置） | **完成**（`a9f2a55`） | `crates/termai-vt/src/width.rs` 新增公开 `measure_scalar`/`measure`，`grid.rs:914` 改为调用它（`unicode-width` 只剩一处可达）；`termai-render` 声明 `termai-vt` 并把**生产**绑定换成 `VtWidthSource`，dev 依赖替身删除；K4 允许边加 `termai-render -> termai-vt`（ADR-0027 D2 已登记）并带**注入 + 对照**，`--selftest` **30/30 → 32/32**；新测试证明「渲染端口与网格自身花的列数在 ASCII/CJK/组合/emoji-ZWJ/VS16/tab/ESC 上一致」。总负责人实跑：`kernel-gates` 8 PASS、`conformance` **68/68** | 统一到**逐标量**规则后，「👩‍💻 = 4 列」这类 ZWJ/VS16 **序列宽度语义未变**（改它属 VT 语义变更）→ 登记为 **A25**；不产像素，故不移动 E-P0-2/E-P0-3 |
| **W4-B** | GPU 离屏像素路径 + 网格对齐（H19/RP-05）测量机制 | **完成**（`99cbd26`） | `termai-gpu::{offscreen,align}`：离屏目标 + cell-quad 管线（WGSL 精确灰度覆盖）+ 回读 + 从像素重建边缘的测量；本机 **T0（DX12 硬件）** 实测四档 DPI 最差偏差 **0.0103 / 0.0096 / 0.0090 / 0.0077 px**（小数格子 7.8×16.25…15.6×32.5），注入 0.75px 位移能报 **EXCEEDED**；GPU vs CPU 覆盖镜像 ≤1/255，并**抓到真实合成缺陷**（放大 1px 的四边形在 `blend: None` 下会擦掉邻格已写像素） | **不是 H19 门禁数字**：`gate_eligible=false`（无钉定驱动指纹）→ 即便 T0 也 NON_GATING；图案是矩形而非字形位图 → 测的是几何半边（见 **A23**） |
| **W4-C** | AR-26 第 4 条（sessiond 重建 P95/P99）的机制与测量实现 | **完成**（`ddc627a`） | `restore.rs` 重建路径 + `session_rebuild.rs` 屏幕**逐字段相等**验收；`tools/bench/sessiond-rebuild.mjs` 按 kernel/06 §3.10 出 R1/R2；新增门禁 **B8** 与注入 + 对照，`bench:selftest` **74/74 → 78/78**。**总负责人独立复跑** `bench:check --report …` → **[B8] PASS、9 PASS**；**R1 = 1.953ms（≤2000）、R2 = 2.314ms（≤5000）、100,020 次重建全部相等** | 两行仍 **INCONCLUSIVE / NON_GATING**（无 RM-A 指纹）；检查点保真缺口（无 CAS、`recover_session` 只重放 `PtyOut`）登记为 **A24**；E-P0-4 仍「部分」（见 A9） |
| **W4-D** | CI 全红 → 定位 → 修复 → 全绿（本轮的**方法论价值最高**的一段） | **完成**（`6911c9e`、`eabbd92`） | **三段式**：① 我先把 CI 的**可观测性缺口**修掉（`6911c9e`：两个 unix 作业的测试步骤在失败时把用例名转成 `::error::` 注解，而注解**匿名可读**）——**没有这一步，后面两步都做不到**；② 注解立刻给出唯一失败用例 `sessiond::host::tests::the_probed_backend_can_open_and_close_a_session`（macOS 与 Linux 各一条）；③ 读码确认真因是 **Unix `spawn_forkpty` 无就绪握手**：父进程可在子进程 `setsid()` 前返回，`kill(-pgid)` 以 `ESRCH` 失败**且结果被丢弃**，子进程存活 → 这是**真实的生产孤儿泄漏**，AR-30 第 2 条在 Unix 上被证伪。修复 `eabbd92`（PTY-READY-1 握手 + PTY-KILL-1 组/pid 兜底 + 三条 Unix 验收测试） | **CI 已全绿**：run **35948532691**（PR）/ **35948528067**（push），sha `eabbd92`，**7 成功 + 1 按设计跳过**。**残留**：注解不替代日志（Linux 侧只有 2000 字符 tail）；`design:selftest` 的间歇红灯（A21）本轮通过但**未定性** |

**这一段的方法论收获（写给下一个会话）**：本轮我在同一个问题上**连续两次给出了错误的根因**——先是 `pty_lifecycle` 的 `TREE_PROCESSES`（被证据推翻：该文件从一开始就按平台 cfg 门控），再是 shaping 的字体依赖（部分为真：它确实红了 macOS，但修完仍红）。**两次都不是「没查」，而是「只查了一半就下结论」**（§6.3 规则 11 的原始教训在这里第二次应验）。**真正终结猜测的不是更聪明的推理，而是先把「失败用例名可达」这件事做成基础设施**（W4-D ①），然后让证据自己说话。**推论**：当诊断能力缺失时，正确的第一动作是**补诊断能力**，而不是继续加深推理。

#### Wave 5 回报（第 272 轮第三段：四条切片，全部由子代理执行 + 总负责人独立复跑后提交）

| 编号 | 工作流 | 状态 | 已验证证据（总负责人亲自实跑） | 未验证边界（诚实） |
| --- | --- | --- | --- | --- |
| **W5-A** | kernel/03 **S7 栅格化**（E-P0-2 关键路径） | **完成**（`2d895f5`） | atlas 现在产出**真实 8 位单通道 alpha 覆盖位图**并写进真实 `R8Unorm` 页，可读回墨迹框与 bearing；AR-14 配置（`hint(false)` + `Format::Alpha` + 零偏移）**只有一处决定**，并被一条独立重驱 swash 的测试逐字节钉住；彩色字形（COLR/CBDT）**显式拒绝**而非近似。实跑：`cargo test -p termai-render` 17/16/10/4/3 全 0 failed；`'0'@16px` = 8×14、112 字节、digest `0xc3151f9179320e30`（跨进程稳定）；`kernel-gates` 8 PASS | **不产像素**：无 GPU 上传/纹理/帧/damage 循环，**无 cell-box 摆放**（故仍**没有 H19 数字**）、key 里无 DPI/scale、无双帧页固定、无彩色页；`Sharp`/`Soft` 目前同一条灰度曲线。四条注入对照（开 hinting、两键共用位图、`Format::Subpixel`、强制字体降级）各自被对应测试抓住——**其中强制降级那条暴露了我方「不透明像素 > 0」是字体属性而非 S7 属性**，已改为峰值 + 覆盖质量断言 |
| **W5-B** | sessiond **成为真正的守护进程**（E-P0-4 的生产触发者） | **完成**（`6e2ae65`） | `daemon.rs`：M0 stdio 链路驱动**既有** Broker（不改协议、不新增 msg_type）、持有 `SessionHost`、每会话一个 pump 线程把 pty 输出喂进 `Registry::feed_pty_out` 并把引擎的 DSR/CPR **回写 pty**（禁用回写则 5 条测试全红——注入对照）；**唯一出口**（EOF / `GO_AWAY` / 握手被拒 / 帧错误 / 传输错误）调用 `host.close_all()`，另有 `Drop` 兜底。**真实二进制 OS 级验证**：CRC32C 校验过的 PING 得到应答（msg_type 0x5、corr_id 7、CRC 相符）、链路开启时有活子进程、EOF 后 `not_reaped=0 clean=true` exit 0 | `run()` 每进程**只服务一个会话**（M0 由 argv 命名）；**线协议无 `SESSION_CREATE`**（见 A29，需 ADR）；无信号处理器（保 `forbid(unsafe_code)`）；**EOF 即退出与 AR-13 冲突**（见 A30）；多会话关闭顺序执行、pump 每 16KiB 持整 broker 锁 |
| **W5-C** | §5 **H12 输入字节等价**测量（E-P0-3） | **完成，先判 FAIL、修后 PASS**（`1e2977a` → `a555a77`） | 75 例语料**每条期望都带依据**（kernel/05 条款或 xterm ctlseqs 规则），11 条无可引依据者**显式列为 `omitted`**；driver 链**真实** `termai_core` rlib 且**不含编码逻辑**；`bench:check --report` 首测 **[B8] FAIL、`H12 = 93.3333 pct`**，按 K-03 裁决（kernel/05 §3.3 表胜）修编码器后 **[B8] PASS、`H12 = 100 pct (75/75)`、`summary: 9 PASS / 0 FAIL`**，**语料未改**。`bench:selftest` **78/78 → 83/83**；注入（改坏期望）→ 92%/FAIL，对照（编码器满足的 70 例）→ 100%/PASS | 未覆盖：原生 IME 宿主、死键组合、FocusRouter/IN-04、OSC 52 读、DECSET 鼠标状态机、11 条无可引依据的期望；`gatingNumbersProduced` **仍为 0**（H12 是 `machine:'none'`，ADR-0029 D-5）——**是「§5 门禁行被判定」，不是「机器绑定门禁数字」**；§3.2/§3.3 措辞冲突登记为 SD-26 |
| **W5-D** | A20（可复现命令）＋ SD-25（命名漂移） | **完成**（`fb0b544`） | 重构命令现在与运行手册 §2 **逐字节一致**（含 `--xterm-reverse-wrap 383`）；未声明时打印 `NOT-DECLARED` + 显式占位符说明；`esctest-report --selftest` **7/7 → 10/10**，变异实验（还原修复前）得 **7/10**；SD-25 按 SD-17 先例登记并**额外诚实指出** `suites.json` 根本没有 SHA-256 字段（只有 40 位 `pinned_revision`）；K6 上界 → `SD-09..SD-25`，`kernel-gates --selftest` **32/32 → 34/34** | `excluded_by_vt_level` 仍 UNKNOWN；报告仍不解析 `substitutions`/`feeds`（SD-20 剩余动作）；命令按当前 manifest 重构 → 无法证明旧日志的 provenance；`check-suites.mjs` 仍不校验该字段 |

**本轮最重要的产物不是「更多功能」，而是「第一条被判定的 §5 门禁行」**：H12 一落地就报 FAIL，并给出 5 条可引用条款的分歧——**这正是「真实门禁数字」该有的样子**（若它一上来就是 100%，反而应当先怀疑 producer 是不是在自我循环）。

#### Wave 6 回报（第 272 轮第四段：四条切片 + 一次并行协作事故）

| 编号 | 工作流 | 状态 | 已验证证据（总负责人亲自复跑） | 未验证边界（诚实） |
| --- | --- | --- | --- | --- |
| **W6-A** | **H12 判 FAIL → 按 K-03 裁决修编码器 → PASS**（E-P0-3） | **完成**（`a555a77`） | 5 处分歧（MOK(2) 的 `Ctrl+Shift+A`/`Alt+x`/`Esc`、Kitty 的 `Enter`/`Left`）全部消失；修法是**规则化**的（`mok_form` / `mok_replaces_legacy` / `functional_named`，每条分支引用 kernel/05 §3.3 的对应行），**语料一字未改**。我复跑：`input-bytes.mjs` → **`H12 = 100 pct (75/75)`、`verdict: PASS`**；`bench:check --report` → **`[B8] PASS`、`summary: 9 PASS / 0 FAIL`**；`cargo test -p termai-core` 49 passed | **PASS 只对本语料覆盖的 S3 编码阶段成立**（75 例：keyboard 53 / paste 10 / ime_commit 3 / mouse 5 / focus 2 / api_inject 2；11 条无可引依据者列入 `omitted`）；未覆盖原生 IME 宿主、死键组合、FocusRouter/IN-04、OSC 52 读、DECSET 鼠标状态机；**不是机器绑定门禁数字**（`machine:'none'`，ADR-0029 D-5）→ E-P0-3 仍「未判定」；§3.2 与 §3.3 的措辞冲突登记为 **SD-26** |
| **W6-B** | **A24 item 2：`Resize` 按 Log 位置回放**（E-P0-4） | **完成**（`db838a5`） | `recover_session` 现在按记录顺序应用 `Record::Resize`；重建后的屏幕与活会话在**四条轴**上相等（GridSnapshot 含几何 / canonical_bytes / digest / 逐行文本），覆盖「Log 中段 resize」「末尾 resize」「无 resize 对照」。**两次注入都是决定性的**：忽略 `Resize` → `left: (120,40) right: (80,24)`；**只套用最后一条** → 几何对了但四条轴仍然抓住（scrollback 1059 vs 2143）——证明它测的是屏幕而不是尺寸 | **CheckpointRef/CAS 保真仍未闭合**（无 CAS 存储；夹具仍断言 `checkpoints == 0`，计时驱动仍拒绝对检查点续接发布数字）；spawn 几何不是 Log 记录；**计时夹具仍只用一个尺寸**，故 R1/R2 不覆盖 resize；`EngineReplay` 仍靠 trait 默认丢弃 `Resize`（建议 3 行 override + 删掉 `ResizeAwareReplay`）。AR-26「screen 可恢复」**只对「有 resize、无检查点」的会话**成立 |
| **W6-C** | **RP-05 字形摆放 + 网格对齐测量**（E-P0-2/E-P0-3 关键路径） | **完成**（`a08e5f1`） | `place.rs` 把**真实栅格化位图**放进 cell box 并测 `abs(glyph_bitmap_origin − cell_box_origin)`：四档 DPI **0.5000 / 0.4375 / 0.5000 / 0.5000 px**（各 40 采样，refusals 0，格子度量在四档全是小数）。**该探针不可假绿**：零字形 → `NoDrawnGlyphs` 拒绝；`floor()` 会得 0.8750–0.9688px（测试断言了这一点）；临时 +1px 注入 → 1.5000px 且合约测试红，还原后 `place.rs` 的 SHA256 逐字节相同；另有两条常驻注入断言（错格 → 6.6250–12.7500px，全部 EXCEEDED） | **是 CPU 几何数字，不是渲染帧**：无设备/交换链/上屏时间证据 → 离 RM-C 一律 NON-GATING；`place.rs` **尚无 S8 消费者**（`termai-render → termai-gpu` 边未准入）、`tools/bench` 未接线；只判 Sharp（Soft 的边缘质心未实现）；因 `AtlasKey.px_size` 是 u16，栅格尺寸取整到整设备像素（K-12 的 `size_q6` 会修）；**三档恰好等于 0.5px 上界 → 余量为零**（见 A23） |
| **W6-D** | 登记与门禁簿记（SD-26 / K6 / kernel-04 伪码 / A24 行） | **完成**（`7cd47d4`、`5a7f006`） | SD-26 按 SD-25 形状登记（**要改的是 spec 文本**：§3.2「功能键」行需补「指向 §3.3 + 范围声明」）；K6 上界 `SD-09..SD-26` 且带**注入 7f + 判别注入 7g + 对照 7h**，`kernel-gates --selftest` **34/34 → 37/37**，还原上界的独立运行 **36/37、exit 1**；kernel/04 §3.3 的恢复伪码同步为「`Resize` 在它所在位置生效」；A24 行按约定划删除线更正 | 三处「H12 = FAIL」的旧断言由我发现并**在同一轮内**改正（A28、计划 E-P0-3 行、计划 W5-C 行）——**这是规则 12/17 的又一次实例：改了数字必须同时改总表** |

**并行协作事故（必须记录，因为它改变派单方式）**：一段切片的子代理为还原**自己的**临时注入，执行了 `git checkout -- apps/sessiond/src/registry.rs apps/sessiond/src/restore.rs apps/sessiond/tests/session_rebuild.rs crates/termai-session/src/checkpoint.rs`，把**另一条切片刚实现并通过测试**的三个文件整体回退；**因从未 `git add`，git 里没有对象可恢复**，只能由对方凭自己的补丁副本（`%TEMP%\a24.patch`）与转录重建。**处置**：① 立即向**所有在跑切片**发送禁令（checkout/restore/stash/reset/clean 一律禁止，包括对自己改过的文件）并要求回报执行过的 git 命令；② A24 重建后**逐字节复核**（它给出了四个文件的 SHA256）并通过；③ 把这条写进 runbook §0 第 6 条与计划 §6.3 规则 21，并在派单模板里加入「禁止破坏性 git 操作 + 回报 git 命令」两项。**代价可量化**：一轮重做 + 一次协作沟通；**收益**：这是本会话唯一一次「他人的改动被删掉」，且它被**发现并修复在同一轮内**。

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
4. **WS-08 的 D-6 收口（第 255 轮已完成）**：`tools/bench/values.mjs` + `B8` 扩展（schema 校验 → **+ §5 行绑定**）+ 逐行呈现（缺席显式 `NOT REPORTED`）+ 计数器改为计算值；~~`bench:selftest` 65/65（第 256 轮 ADR-0029 D-5 再 +2）~~ → **第 257 轮 ADR-0029 D-4 后为 74/74**。**本机可得 H18 = 53 与表外对照 C1 = 5.17**，其余行如实缺席。

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
8. **测量必须连完整调用命令一起登记**：第 39 轮证明，同一个 esctest 检出、同一份代码，只因为漏了 `-- --expected-terminal xterm --xterm-checksum 336`，结果就从 **218 passed / 308 failed** 变成 **117 / 407**（esctest 会对每格校验和取反）。**不同调用参数下的数字不可比较**；因此 SD-20 当时「201 不可复现」的结论也是错的——它是参数差异，不是错误记录。**这条与第 4 条（证据格式）是同一件事的两个面**：证据不仅要含断言原文，还要含**产生它的确切命令**。
9. **第 7 条的修正（第 48 轮）**：把切片分成「机械/判断」**还不够**。实测第四类失败：**「需要跨多个文件定位并归一化数据」的切片同样不落地**——`WS esctest triage` 要求「按类名找到 `tests/<模块>.py` → 定位函数体 → 解析 `esccmd.py` 常量 → 比对模式集合」，跑了 2 轮**零文件产出**，与判断切片同样失败。**共同点是「工作流必须自己搭建数据管道」**。因此派单前负责人应把**管道搭好**（例如先产出一份「失败用例 → 源文件 → 函数体行号区间」的清单），让工作流只做**纯变换**。**本条的代价**：负责人先做一步数据准备，换取切片真正落地；**在负责人上下文耗尽时，这类任务应当明确留给下一个会话**，而不是反复派发让它空转。
10. **每个门禁必须有「注入 + 对照」**（第 132–141、160–163 轮）：**一个「通过」的门禁不等于一个「能失败」的门禁**。本会话三次撞上同一偏差——`bench` 自检没接进 CI、`ci-cost` 只测算术分支、**K4 的 AR-03 检查是假绿（违规分支抛 TDZ 错误、永不生效）**。**做法**：新增任何门禁，同一次改动里给出①**注入**（制造该门禁应报的缺陷）②**对照**（同一输入去掉缺陷后必须通过，以排除「它只是拒绝一切」）。**实例**：`kernel-gates --selftest` ~~23/23~~ **~~24/24（第 255 轮加 SD-24 注入）~~ → ~~26/26（第 258 轮加两条 K8 注入）~~ ~~28/28（第 259 轮加 conformance-suites 注入 + 对照）~~ **30/30（第 265 轮加 waivers 注入 + 对照）**、`bench:selftest` ~~—~~ **~~65/65（第 256 轮）~~ → 74/74（第 257 轮 ADR-0029 D-4 加 B9）**、`conformance selftest`、`ci-cost --selftest`。
11. **先怀疑自己的命令，再怀疑被测对象**（第 81、116、130、154、168 轮，共 7 次）：**空输出、0 命中、字段读成不存在**——多半是自己的写法或范围问题。**做法**：结论异常时，**重跑那条命令或换一种写法得到同一结论**，再下判断。**反例价值**：第 116 轮正因如此，从一个本来要被当作死代码删掉的正确函数里找回了调用者。
12. **改数字必须同时改总表；更正时给旧结论加删除线**（第 98、128、147、148、151 轮）：**本会话七处「文档与现实不一致」没有一处是写错，全是「只改了一半」**——追加更正会把旧断言留在行首。**做法**：**旧结论加删除线并注明轮次，新结论随后**；**并由一条可执行的检查兜底（第 220 轮）**：**扫全文找「含更正类词（更正／已推翻／已被／回滚／作废）却**不含 `~~`**」的行**——**第 220 轮实测在 36 行表格中得到 5 条候选，其中 3 条是真违规（A15、A16、以及同族的行），2 条是判断问题**（**数字被整体替换、旧文本已不存在**的；**缺陷登记表的标题是原始发现、结论列承载更正**的）。**它有意做成「候选清单」而不是门禁**——**因为误报率约 2/5**，**而第 140 轮的教训是：按模式推断「什么算违规」会造出假警报**。；摘要与细节在同一次改动里一起改。
13. **更正时的锚点必须重新推导，不能用行号或「上一次那一行」**（第 152–168 轮）：用漂移的锚点追加，会把新内容插进它不该在的地方（**入口段因此被埋两次、笔记被层层套上引号**）。**做法**：**按内容定位，并验证锚点唯一**。
14. **一次改名/扩责的代价 = 引用它含义的地方，不是提到它名字的地方**（第 139 轮）：`K8` 在 6 个文件里被提及 21 次，**但只有 2 处引用了会变化的标题文本**；若只数 21，就会把 2 处的改动误估为 21 处。
15. **扩充语料/指标，必须带来新的判定能力，而不是新的行数**（第 129 轮）：`AR-31` 的「≥2000 用例」若靠生成参数变体凑数，**runner 的判定仍是 `NOT_JUDGED`**——**造出「看起来更好、却不决定任何事」的数字是本项目的反面典型**。
16. **注册/归属未定的东西，先定归属再写代码**（第 126、142、143 轮）：`A9`（§8.2 测量无归属）与 §5 测量主场（**ADR-0014 声明的 `crates/termai-xtask` 不存在**）**都不是实现问题，而是「记在哪里、谁判定」的问题**——**写作前先问「这个东西的归属写在哪一份文档里」**。
17. **改任何数字的前后各搜一次它在哪些文档里被引用**（第 171 轮）：**规则 12 说「改数字要同时改总表」，但「知道该同步」不足以避免遗漏**——第 170 轮我合并出 16 条纪律，第 171 轮入口段仍写着「九条」，而写下规则 12 的人正是我。**做法**：改动数字后执行一次全文搜索（例如 grep 该数字与相邻词），逐个确认；**第 172 轮照此把入口段与交付计划的全部数字声明核对了一遍，结论是均已同步**——**这正是该动作应当产生的结论，而它此前从未被执行过**。
18. **任何限制结果数的检索都必须确认「还有没有更多」**（第 157、211 轮）：**`Select-Object -First N`、`head`、分页——把「前 N 条命中」当成「全部命中」，会让检索产生**假结论**。**第 211 轮我查「回放|replay」时用了 `-First 8`，**两个文件的命中按路径排序、登记表在前**，于是**交付计划追踪表里的「G2 行为回放」整行被截断**——**我据此宣布「这条门禁未被登记」，而它一直在**（A15 已于第 211 轮更正）。**第 157 轮同族**：**不区分大小写的 `HACK` 匹配了 `AttachAck`，造出 9 条不存在的 TODO 债务。** **做法（两个动作）**：**① 看它有没有顶到上限**——**输出条数等于你设的限制，就是被截断了**；**小于限制，才是全集**（**第 209 轮查 `docs/roles` 用 `-First 10` 只输出 1 条——1 < 10，故「只有一次提交」成立；第 210 轮用 `-First 8` 输出正好 8 条——顶到上限，于是计划里的 G2 整行被截掉**）。**② 结论为「某处没有 X」时，必须再打印一次总数**（`| Measure-Object`）。**「没找到」与「不存在」之间隔着一次截断，而截断的信号是「条数正好等于你允许的条数」。**
19. **要做成门禁的检查，先问它的答案里有没有判断成分**（第 140、222、234 轮）：**有判断成分的检查只能是候选清单；没有判断成分的才能是门禁；两者都做不到的，就不做。** **三次实例**：① **第 140 轮**按步骤名推断「什么算门禁」→ **13 条假警报**（后改为显式清单）；② **第 222 轮**删除线检查 → **约 2/5 是判断问题** → **做成候选清单（exit 0）**；③ **第 234 轮**范围声明检查 → **3/3 是「引用旧事实」而非「旧断言」** → **回滚，不做**。**判据是**：**一个机械模式若分不清「断言」与「引用」，它就会在**记录过自己错误的仓库**里频繁误报**——**而总在喊的检查会教人忽略它**。** **第 237 轮按本条把现有 CI 集审计了一遍**：**两个 job 各十道门禁（第 258 轮加 `conformance verify`、第 259 轮加 `conformance suites`、第 265 轮加 `waivers check`；第 237 轮时为七道）**——`tokens:check`、`design:check`、`kernel:check`、`bench:check`、`conformance L0`、`ci-cost check`、**`audit-claims check`**、**`conformance verify`**、**`conformance suites`**、**`waivers check`**——**逐一问「它的答案里有没有判断成分」，结论是十道都没有**（**`tokens` 的 hex debt 是**门禁内**的 warn-only，本就声明为不阻断；`design` 的 `maxDiffPct` 默认为 0；`bench` 的 §5 数字为 NON_GATING**）。**而唯一含判断的那个检查（`tools/audit/lint-notes.mjs`，约 2/5 是判断问题）**没有接进 CI**——**这正是本条要求的形状。**
20. **凡「N 个 X」这类计数声明，定期打开原文逐行核对**（第 243、244 轮）：**第 19 条说的是「什么能被机械化成门禁」；本条说的是它的补集**——**能被机械化的只有**措辞封闭**的那部分**（如「§6.3 有 N 条」「各 N 道门禁」），**而「N 个 check 与 M 个 selftest」「N 条环境缺口」这类开放句式封闭不了**。**两轮实测**：**第 243 轮**入口段称「六个 check 与四个 selftest」，**实际七对**（第 232 轮加 `audit-claims` 后未同步）；**第 244 轮**称运行手册有「六条环境缺口」，**实际 5 行**。**两处都不在工具覆盖内，都是靠**打开原文数一遍**查到的。** **做法**：**数的时候只数**声明所指的那些行**——**两次出错都是数进了声明之外的东西**（第 238 轮数到了另一个数组，第 244 轮数进了表头）。** **⚠ 而「找出有哪些计数声明」这一步同样会出错（第 248 轮补）**：**用正则把「数字 + 量词」揪出来时，会把**彼此无关的两个短语拼在一起**，造出并不存在的声明**。**三次实例**：**第 238 轮**把另一个数组的数算进来、**第 244 轮**把表头算成一行、**第 248 轮**把「新增第三个 `SOS_PM_LEN_LIMIT_DEFAULT`」误读成「有三个 `SOS_PM_LEN_*` 常量」**（实际只有 1 个，**而「第三个」指的是长度上限的序列：OSC／DCS·APC／SOS·PM**）。**因此本条的完整做法是两步**：**① 先**逐行读原文**确认那句话是否真的在声称一个数**；**② 再只数它所指的那些行**。**只做②不做①，会凭空造出要核的声明**。

21. **并行切片绝不执行破坏性 git 命令（第 272 轮事故）**：`git checkout -- <path>` / `git restore` / `git stash` / `git reset` / `git clean` **一律禁止，包括对自己改过的文件**。本会话同一 checkout 内同时有 2–3 条切片在写；一条切片为还原**自己的**临时注入执行了路径级 checkout，把另一条切片**刚实现并通过测试**的三个文件（`apps/sessiond/src/restore.rs`、`crates/termai-session/src/checkpoint.rs`、`apps/sessiond/tests/session_rebuild.rs`）整体回退，对方只能从 `%TEMP%` 的补丁副本重建。**做法**：反向编辑或从 `$env:TEMP` 拷回；提交按显式路径（规则 5 同类）；派单时**把这条写进任务书**并要求回报列出执行过的 git 命令；不确定归属先问。**这条与 runbook §0 第 6 条是同一件事的两处落点。**

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
| 孤儿进程清理 = 100% 且会话关闭 ≤2s 回收 | AR-30 第 2 条、§8.2、`kernel/02` §3.3 | WS-02 + WS-05 | PTY-ORPHAN-1 用例（native + pipe） | ~~**部分**：native ConPTY 路径已补覆盖（根 + **后代**，`kill(Force)` 后 2s 内清空，commit `89b491d`）；但 **M0 的 sessiond 不持有 PTY**（registry 只走 `TerminalEngine` trait，真实 PTY 生命周期在 `apps/termai` CLI），故「**会话关闭 → 回收**」在守护进程层**尚未实现**~~ → **第 272 轮已闭合（守护进程层，`fd04a11`）**：`SessionHost::close()` = `kill(Force)` → 有界 `wait`（1.5s）→ 释放 pty，`apps/sessiond/tests/pty_lifecycle.rs` 用真实进程断言**子进程与孙进程** 2s 内消失，带「未关闭会话仍活」与「close 幂等」两条对照，并以桩掉 kill+wait 的方式证明断言非空真。**仍余**：`SessionHost::probed()` **无调用者**（`main.rs` 仍是 stub），故该路径在生产上尚无触发者 |
| 行为回放 ≥99.5% | §8.1-2 | WS-01 | `.trec` 回放报告 | 部分 |
| 视觉回归 ≤0.1% | §8.1-3 | WS-03/04 | B4 diff | 不适用（无渲染） |
| 依赖与许可 | §8.1-5 | WS-06 | cargo-deny/audit/SBOM | 部分（K4/K5 声明清单） |
| 安全与 fuzz 24h | §8.1-6 | WS-07 | fuzz 报告 | 部分（smoke） |
| 三平台 CI 可判定 | ADR-0014 | WS-06 | ci.yml 作业 + runner 证据 | 偏差（Windows-only，**W1-A 修复**） |
| §5 每条指标有测量定义 | B-10、AR-24.3 | WS-08 | H1…H19 登记 + 判定实现 | **部分**：登记与判定已落地（W1-C）；**机器无关行的读取 / 绑定路径第 255 轮落地**（本机实得 **H18 = 53**、表外对照 **C1 = 5.17**；H17 需浏览器层、H19 需 RM-C 像素渲染），其余行需参考机 |
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
| 第 272 轮 | **Wave 3 三条切片落地并推送**（A8 sessiond 持有 PTY + 会话关闭杀树 `fd04a11`；kernel/03 S6 shaping + S7 atlas + 字体栈准入 `6f3a590`；A3 vttest 可行性四路探针与原因更正 `c3c66c8`）；更正债务表四处旧断言并新增 A20（esctest-report 可复现命令缺口）；发起人裁决：oracle/RM 环境本轮不做、治理 C1–C3 暂不处理、按切片提交并推送触发真实 CI | AR-20/AR-26/AR-30、ADR-0027 D1、ADR-0030、ADR-0031、§6.3 规则 7/8/9/10/12 |
