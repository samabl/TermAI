# TermAI MVP 交付计划（M0 · 信任基座纵向切片）

> **效力**：本文是**交付计划**，不是设计权威。任何与 [HARNESS.md](../../HARNESS.md) 冲突之处以 HARNESS 为准（AGENTS §1）。
> **依据**：HARNESS §7（Phase P0/P1 出口标准）、§8（门禁）、§5（预算）、§2（AR-24…AR-31）、`docs/spec/kernel/00-index.md` §2（P0 阻塞项 B-1…B-11）。
> **纪律**：本文所有条目必须能指出「落在哪个 Phase / 满足哪条 DC / 受哪条预算与门禁约束」（HARNESS §0.2）。

## 1. MVP 定义（M0）

**MVP = 可运行、可验证、可回放的内核纵向切片（headless）**，即：把 **PTY → VT 解析 → Grid → Session Log → IPC → 结构化上下文** 这条链路端到端打通，并以测试与门禁证明它没有撒谎。

选择 headless（无 GPU 网格渲染）作为 MVP 主体的理由与代价：

| 项 | 内容 |
| --- | --- |
| 理由 | P0 阻塞项 B-1…B-11 中 **8 条**在 headless 链路上即可冻结（字节直喂 L0 车道、三车道分离、F0/F1/F2 标签、Session Log 段格式、IPC 24B 帧与握手、capability 七校验点、InputEncoder 单一收口）；渲染（kernel/03）与 IME（kernel/05 L4）依赖它们先冻结。先冻结契约再自绘 GPU，避免返工。 |
| 代价 | **AR-01 的原生网格像素路径（wgpu）不在本 MVP**；key-to-photon / 帧时 / 网格对齐门禁**无法在本 MVP 判定**，须由后续 M1 承接。 |
| 诚实边界 | 本 MVP **不主张** P0 出口达成（P0 出口含 vttest 全绿 + 三平台 IME/CJK 矩阵 + 性能门禁进 CI，见 HARNESS §7）。本 MVP 只主张：**契约冻结 + 垂直链路可运行 + 已实现部分门禁全绿**。 |

### 1.1 MVP 交付物（Definition of Done）

1. Cargo workspace 构建通过，`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test --workspace` 全绿。
2. `termai` CLI 能：以真实 PTY/ConPTY 启动 shell、按 L0 车道逐字节喂 VT 解析器、输出规范化网格快照（`TERMAI-GRID 1`）、写出 `TMAILOG` 段、退出码与 grid digest 可复现。
3. `sessiond` 能以 24B 帧 + 握手 + 能力协商服务 attach，并把 `GridSnapshot`/`PtyBytes` 以 POD 帧下发。
4. Session Log 崩溃尾截断可恢复（`recover_session` 语义），**恢复路径中不存在 spawn/exec**（可静态断言）。
5. 既有设计门禁（`tokens:check` / `design:check:static`）不回归。
6. 本计划的**可追溯矩阵**中每条已实现项都有对应测试。

### 1.2 明确不在本 MVP（anti-scope）

- 原生 GPU 网格渲染与外壳（kernel/03、DC-17）→ M1。
- IME 宿主与候选窗（kernel/05 §3.4、IN-AC-04 人工矩阵）→ M1。
- AI Agent / 审批 / 审计链 / 回滚（P2）→ M2。
- 插件宿与 WASM ABI（P3）→ M3。
- 云端控制面、同步、协作（P4/P5）。
- SSH/容器/WSL Transport 实现（DC-19）：本 MVP 只冻结 `Transport`/`Channel` trait 与 F0/F1/F2 声明，**不实现 russh**（AR-28.3：新依赖须先过 ADR-0015 白名单登记）。

## 2. 工作分解（WBS）与责任

| WS | 主题 | 责任 | 交付 crate / 目录 | 依赖 | 对应 HARNESS |
| --- | --- | --- | --- | --- | --- |
| WS-A | 契约冻结：IDL / 输入编码 / 风险分级 / capability | 总负责人 | `crates/termai-core`、`crates/termai-ipc` | — | DC-15/DC-22/DC-26/DC-35、AR-29、B-8/B-9 |
| WS-B | VT 语义与一致性 | 内核组 | `crates/termai-vt` | WS-A | DC-17? 否 → kernel/01、AR-25、AR-18、B-1/B-2/B-4 |
| WS-C | PTY / 进程树 / 字节保真 | 内核组 | `crates/termai-pty` | WS-A | DC-16、AR-25/AR-28.3、B-3/B-5 |
| WS-D | 会话真源：状态机 / Log / 恢复 / lease | 会话组 | `crates/termai-session` | WS-A/B/C | DC-18/DC-23、AR-26、OQ-30、B-7 |
| WS-E | 守护进程与 CLI（headless 端到端） | 平台组 | `apps/sessiond`、`apps/termai` | WS-A…D | DC-24、AR-13、B-1 |
| WS-F | 门禁与度量 | 工程效能组 | `.github/workflows/ci.yml`、`tools/` | 全部 | §8.1、AR-27、kernel/06 |

## 3. 交付物落位与依赖方向（DC-21 / AGENTS §3）

    tokens(叶子) ── core(叶子契约: IDL/input/risk/capability)
                         │
              ┌──────────┼───────────┐
              ▼          ▼           ▼
          termai-vt   termai-pty   termai-ipc
              └──────────┼───────────┘
                         ▼
                  termai-session ──▶ apps/sessiond
                         └──────────▶ apps/termai

- 只允许单向依赖；`apps/*` 不得被任何库依赖；`termai-core` 不得依赖 vt/pty/ipc/session（AR-03 的编译期兜底）。
- **实际依赖边**（ADR-0019 D1 已登记）：termai-ipc → termai-core；termai-vt → termai-core；termai-pty → termai-core；termai-session → {termai-core, termai-ipc}（kernel/04 §4 明确 session 从 IPC 取得帧格式/帧上限/msg_type 号段）。**session 不直接依赖 vt/pty**：VT 与 PTY 由 apps 层通过 GridReplay / ByteTransport 两个 trait 注入，因此内核可脱离真实 PTY 做单元测试，依赖方向也不会成环。
- **新增 crate 的 ADR 义务**（AGENTS §3）：全部 5 个新 crate 与 2 个 app 的位置需一份 ADR 追认。本 MVP 以 **ADR-0019** 登记（见第 6 节）。

## 4. 可追溯矩阵（已实现项 → 依据 → 证据）

| MVP 项 | 依据（Phase / AR / DC / B-xx） | 证据（测试 / 产物） | 状态 |
| --- | --- | --- | --- |
| core-dto Grid 字段集 v1（`GridSnapshot`/`GridDelta`/`ScrollOp`） | P0/P1 · B-6 · OQ-RND-07 建议值 | `crates/termai-core/tests` canonical 序列化 | 计划 |
| InputEncoder 单一收口（键盘 / 粘贴 / IME commit / API 注入） | P0 · AR-29.3 · DC-? 否 → kernel/05 K-01 · B-9 | 键位编码套件（IN-AC-01） | 计划 |
| 风险分级 L0/L1/L2/L3/U | P2 前置 · AR-06 · DC-26 | `risk::classify_argv` 用例表 | 计划 |
| `capability::require` 纯函数（CAP-1/CAP-2 的服务端闸门） | P2 前置 · DC-35 · AR-06 | deny/allow/dry-run 决策表 | 计划 |
| 24B 帧 + CRC32C + reserved 拒帧 | P0 · DC-22 · B-8 | 帧层 fuzz smoke + 边界用例 | 计划 |
| 握手状态机 + 能力严格求交 + DEGRADED | P0 · DC-22 | 握手矩阵（IPC-AC-02） | 计划 |
| `VtBackend`/`EscapeSink` trait + vte 适配器（不外泄 vte 类型） | P0 · AR-18 · B-4 | trait 差分 + API 泄漏编译用例（V-11） | 计划 |
| L0/L1/L2 三车道分离；100%/≥99% 只在 L0 判定 | P0 · AR-25.1 · B-2 | 车道标签断言 | 计划 |
| 未识别序列四态契约 + 计数键全 14 项 | P0 · kernel/01 §3.4 | `unterminated` 语料属性测试（V-07） | 计划 |
| OSC 133/633 状态机 + OSC 7 解码 | P1（AR-04 上下文前提） | 命令边界用例 | 计划 |
| golden `TERMAI-GRID 1` 快照 + 哈希 | P0/P1 · kernel/01 §3.7 | 往返解析 + 哈希稳定性 | 计划 |
| 九元 `PtyBackend` + ConPTY / forkpty | P0 · DC-16 · ADR-0018 · B-5 | 接口不变量属性测试（PTY-AC-07） | 计划 |
| F0 字节恒等（PTY↔VT） | P0 · AR-25.2 · B-3 | 逐字节差分，禁止采样（PTY-AC-01） | 计划 |
| Session Log `TMAILOG` 段 + 13 类 record | P1 · DC-23 · AR-26 · B-7 | 段读写 / CRC / 尾截断 | 计划 |
| `recover_session` 崩溃恢复（无 spawn） | P1 · AR-13/AR-26 | 静态断言 + 恢复一致性（AC-S3/S4） | 计划 |
| stdin 单写者 lease（TTL 30s / renew 10s） | P1 冻结帧位 · OQ-30 · B-7 | 万次争抢双写 = 0（AC-S7） | 计划 |
| `sessiond` + `termai` CLI headless 端到端 | P0/P1 · AR-13 · DC-24 | 端到端 smoke + 网格哈希 | 计划 |
| CI 六件套中可自动化的部分接入 | §8.1 · AR-27（G4-PR/REL 双口径） | `ci.yml` 作业清单 | 计划 |

### 4.1 签收状态（M0 收口时填写）

> 权威状态与度量见 [docs/plan/m0-delivery-report.md](m0-delivery-report.md) 第 2/3/6 节；本表只给一行结论。

| 分组 | 状态 | 说明 |
| --- | --- | --- |
| 契约冻结（Grid DTO v1、InputEncoder、风险分级、capability、IPC 帧与握手、Session Log 段格式） | **已交付** | 有实现 + 单元测试；SD-01…SD-05 已由 ADR-0020 处置 |
| capability 闸门 CAP-1/CAP-2（租约与写权） | **已交付** | sessiond 落地；非写者写 stdin 被 CAP_DENIED 拒绝并有审计标志 |
| 三车道分离（100%/≥99% 只在 L0 判定） | **已交付（机制）**、**未判定（数字）** | 机制与判定函数已实现；缺少上游语料，故不给通过率 |
| 未识别序列四态契约 + 14 计数键 | **已交付** | 计数键与契约测试在 termai-vt |
| golden 快照与 `.trec` 回放 | **已交付** | 往返不动点 + 哈希稳定性（termai-vt） |
| 九元 PtyBackend + ConPTY / forkpty | **已交付** | 接口不变量 + 真实 spawn 冒烟（termai-pty） |
| F0 字节恒等（PTY master → VT parser） | **已交付** | `termai run` 报告 `read_bytes == fed_to_vt`；Log 保存同一批字节 |
| Session Log + 崩溃恢复（无 spawn） | **已交付** | 尾截断诚实上报；恢复路径静态断言无 spawn/exec |
| stdin 单写者租约 | **已交付** | 万次争抢仅 1 次授予（AC-S7） |
| sessiond + CLI headless 端到端 | **已交付** | 线上协议测试 + `termai run` 端到端验收测试 |
| CI 六件套可自动化部分 | **部分** | K1–K6 + tokens + design 静态层；G4/G5 完整/G6 24h 未接入 |
| §5 性能预算 / §8 性能门禁 | **未判定** | 无参考机与测量实现（B-10），不产出任何性能数字 |


## 5. 本 MVP 实际执行 / 不执行的门禁

| 门禁 | 本 MVP | 说明 |
| --- | --- | --- |
| G1 VT 兼容（vttest/esctest/kitty 100%、xterm ≥99%） | **部分** | 只跑自建 L0 语料与未识别序列契约；**未接入 vttest/esctest/kitty 上游套件**（OQ-VT-03 的 ≥2000 用例语料是 4–6 人周投入）→ 必须诚实标注为**未判定** |
| G2 行为回放 ≥99.5% | **部分** | 自建 `.trec` 子集可跑；未达生产语料规模 |
| G3 视觉回归 ≤0.1% | **不适用** | 本 MVP 无 GPU 渲染；既有 `design:check` 浏览器层继续对 prototype 生效 |
| G4 性能门禁 | **不执行** | AR-27 要求测量先过可复现性自检（D0/D1），本 MVP 无参考机（ADR-0014 RM-A/B/C）→ 未判定 |
| G5 依赖与许可 | **执行（静态）** | 手工登记依赖清单；链接边界 GPL/AGPL = 0 |
| G6 安全与 fuzz | **部分** | 帧层 / VT 解析 fuzz smoke（非 24h） |
| G7/G8 设计门禁 | **保持** | 既有 `tokens:check` / `design:check:static` 不回归 |

## 6. 变更与登记义务

1. **ADR-0019 已落地（Accepted）**：登记 5 个新 crate + 2 个 app 的依赖位置与依赖准入（vte / unicode-width / blake3 / sha2 / libc / windows-sys；portable-pty 拒绝、russh 未准入）。
   **ADR-0020 已落地（Accepted）**：实现期线格式与契约 errata（SD-01…SD-05），并已在 HARNESS §11.2 登记 CR-12；SD-06（门禁编号口径）登记为 CR-13。
2. **OQ-RND-07**：本 MVP 冻结 Grid 字段集 **v1**（`GRID_DTO_MAJOR.MINOR = 0.1`）。这是**临时冻结**，OQ-RND-07 在 P1 由 IPC owner 联合裁定时若变更，按 capability 协商 + 兼容 ≥2 minor 处理（AR-04/DC-40）。
3. **新依赖**：`vte`（AR-18 明确指定）、`unicode-width`、`windows-sys`、`libc`、`blake3`、`sha2`。均须过 ADR-0015 判定表并在 ADR-0019 登记；**PORTABLE-PTY 明确不采用**（AR-28.3）。
4. **不得静默放宽**：任何 §5 预算或 §8 门禁的放宽须附基准数据 + TSC 批准（AGENTS §5）。

## 7. 变更记录

| 日期 | 变更 | 依据 |
| --- | --- | --- |
| M0 立项 | 建立 M0 范围、WBS、可追溯矩阵、门禁执行口径 | HARNESS §7 P0/P1；kernel/00-index §2 |
