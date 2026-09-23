# 04 · 会话生命周期与持久化规格（Session Lifecycle & Persistence）

> **归属**：内核层规格 `docs/spec/kernel/`。**上位权威**：`HARNESS.md` > `docs/spec/00-glossary.md` > `docs/spec/03-system-architecture.md` > 本文。
> 本文是 03-spec §3.8 的下钻：只补**机制**（数据结构 / 状态机 / 算法 / trait / 迁移 / 判据），不新增结论；与 HARNESS 冲突时以 HARNESS 为准并显式标注（见 §1.2）。
> **术语**：本文「会话 / Session」无前缀时一律指 **PTY Session**（DC-03：PTY Session / Agent Session / Workspace 三「会话」禁止混称）。
> **数值口径（AR-19）**：引用数字一律标注 HARNESS 出处（§5 门禁列 / 目标列、§8.2、OQ-30 建议值）；本文自有的设计常量（TTL / 退避 / 段大小等）可配，凡作为**验收判据**者已在 §8 以【新增】登记，不得与 §5 并列。
> **编号空间（AR-28 第 2 条）**：本分册内部问题编号统一为 **OQ-SES-NN**（如 OQ-SES-01）；其它分册用各自前缀（OQ-VT / OQ-PTY / OQ-RND / OQ-INP / OQ-PM / OQ-ABI）。

## 1. 范围与依据（引用具体 AR/DC 编号）

| # | 本文覆盖 | 依据 |
| --- | --- | --- |
| 1 | Session 状态机、全部迁移、可恢复状态集合 | AR-13、DC-04、DC-18；03-spec §3.6/§3.8 |
| 2 | Session Log 段格式、记录类型、校验和、追加写与 fsync、磁盘上限与滚动 | AR-04、DC-23；AR-13、OQ-04 |
| 3 | checkpoint 与重建算法、AR-13 诚实边界的实现落点 | AR-13、DC-04、DC-23；03-spec AC-13 |
| 4 | 多端 attach 协议、stdin 单写者 lease、抢占与移交 | DC-18、OQ-30；03-spec OQ-A1 |
| 5 | 三级崩溃恢复语义（UI / sessiond / 系统）与耗时预算 | §8.2（UI <2s 重连）、AR-13；03-spec §3.8 |
| 6 | supervisor 监督树、退避、连续失败、用户通知 | 03-spec §3.1；DC-38、DC-37 |
| 7 | 与 termai-agent / plugin-host 的订阅契约、背压与丢弃、AI 关闭零开销 | AR-03、AR-04、AR-07；DC-38 |
| 8 | 24h 无泄漏测量方法与判据 | §5（24h RSS 斜率 <1MB/h）、§8.2（RSS 漂移 <5%） |

### 1.2 与 HARNESS / 03-spec 的差异标注（硬性规则 1）

| # | 差异 | 处置 |
| --- | --- | --- |
| D-1 | 03-spec §3.6 状态机为 `Creating → Running ↔ Detached → Exited / Crashed → Reaped`；本文职责要求 `created / running / detached / exited / recovering / dead` | **细化非冲突**：`Creating→created`、`Running→running`、`Detached→detached`、`Exited→exited`、`Crashed→recovering`（显式可观测瞬态）、`Reaped→dead`（终态）。枚举统一诉求见 **OQ-SES-01（§8）**。 |
| D-2 | 03-spec §3.7 称 Log「真相层」含**原始字节**；AR-13 要求「完整原始输出 scrollback 持久**默认关闭**」 | **消解而非冲突**：Log 内分两类保留——**P0 元数据/事件（默认持久）** 与 **P1 原始字节恢复窗口（默认 volatile ring）**；「完整历史归档」为 opt-in。见 K-05、§3.2。 |
| D-3 | HARNESS OQ-30 的决策阶段为 **P1**；03-spec OQ-A1 写 **P5** | 本文采纳 **P1 冻结帧位与 lease 语义、P5 启用跨端协作写**，冲突上提 **OQ-SES-03（§8）**。 |
| D-4 | HARNESS §8.2 只给 UI 崩溃 <2s 重连，未给 sessiond / 系统崩溃预算 | 本文给【新增】判据（§8），**不**写入 §5 并列；建议 HARNESS §11 追加（§8 OQ-SES-11）。 |

## 2. 关键结论（编号 K-01 起，每条含 结论 / 理由 / 代价）

| # | 结论 | 理由 | 代价 |
| --- | --- | --- | --- |
| K-01 | **stdin 单写者 lease 是 sessiond 内核不变量**，不是 UI 约定：任何写入路径（UI / Local API `pty.write` / agent / 插件注入队列）都必须持 lease | OQ-30「单写者租约 + 只读多订阅」；只有唯一收口点才能保证审计与「后到先赢」不可发生 | 多端协作写需显式交互（移交/夺取），牺牲「任意端直接打字」的顺滑 |
| K-02 | **checkpoint 是 Log 的加速索引，不是第二真相**；删除全部 checkpoint 后仅靠重放仍可重建 | AR-04 / DC-23：唯一真相 = append-only Log；SQLite 可重建 | 恢复耗时与 CPU 上升（fallback 全量重放）；需维护两条恢复路径的等价性测试 |
| K-03 | **恢复 = 屏幕重建，绝不重放输入**：恢复只把 `PtyOut` 喂给 VT 状态机，`PtyIn` 永不执行 | AR-13 诚实声明 + DC-04「绝不自动重放命令或重启进程」 | 用户以为「会话回来了」但进程已死，必须靠 UI 文案持续纠偏（AR-20 诚实原则） |
| K-04 | **`PtyIn` 默认不落明文字节**，只记 `{sha256, len, source, lease_id, ts}` | 同时满足 DC-04（无需重放故无需明文）与 AR-12 / AGENTS §6（secret 与命令全文不入日志） | 无法事后取证输入明文；需显式开启「输入审计明文」（加密）才能满足部分合规场景 |
| K-05 | **原始输出按「恢复窗口 ring」而非「完整历史」持久**：**raw 段滚动阈值 = 8MiB（单段上限）**；**P1 raw ring 环上限 = 64MiB（由多个 ≤8MiB 段组成）**，恢复窗口 = 环内最近 64MiB 或最近 10 分钟（先到者为准），可选加密 | AR-13 / OQ-04：完整 scrollback 持久默认关闭；但崩溃恢复与屏幕一致（§8.2）需要尾部字节 | 长时间不 attach 的会话无法恢复完整滚动历史；需在 UI 明示缺口 |
| K-06 | **三级崩溃 = 三档 RPO/RTO**，UI 崩溃 RPO=0，sessiond/系统崩溃 RPO ≤ 1 个命令块的元数据 | §8.2 只覆盖 UI；必须为 sessiond 与系统崩溃给出可验收口径，否则「可恢复」不可测 | 达到 RPO ≤1 命令块要求 `CmdEnd` 同步 fdatasync，牺牲少量写吞吐 |
| K-07 | **supervisor 采用指数退避 + 熔断**：250ms 起、×2、上限 30s、±20% 抖动；10 分钟窗口内 5 次崩溃 → 熔断 5 分钟 | 无退避的无限重启会把崩溃放大成 CPU/磁盘风暴并掩盖根因 | 崩溃后恢复变慢（最多 30s + 5 分钟熔断）；需显式「立即重试」入口 |
| K-08 | **事件订阅是版本化、按能力裁剪、带显式丢弃策略的契约**；禁止静默丢弃 | AR-04 惰性事件 + 03-spec §3.4「溢出策略必须显式声明」 | 每个消费者必须声明 credit 与 drop policy；实现与测试面扩大 |
| K-09 | **AI 关闭零开销是三条可断言条件**（无进程 / 无链接 / 零分配），不是「感觉不到」 | AR-03.2（内核不链接 AI SDK、不持网络句柄、零开销）+ 复议条件「P95 劣化 <1%」 | 需在 CI 加二进制依赖与分配计数门禁，增加构建复杂度 |
| K-10 | **attach 采用「snapshot → tail replay → delta」三段一致性协议**，幂等且可断点续传 | 03-spec AC-13：3 客户端并发 attach 后帧哈希一致 | 每次 attach 需要一次快照（可达 MB 级），受 8MB 帧上限约束需分块 |
| K-11 | **`recovering` 必须对 UI 可见**；`dead` 是终态，恢复只能产生新 SessionId | 隐藏瞬态会让用户把「重建中」误判为卡死；终态复用 ID 会破坏审计与「进程不可恢复」语义 | 状态栏与托盘需要新增恢复态展示与文案（i18n） |
| K-12 | **磁盘上限 = min(可用磁盘 5%, 8GiB)/会话，80% 告警、100% 降级**；raw 先停、元数据后停 | 长跑 7×24 必须有硬上限，否则单会话可写满用户磁盘 | 达到硬上限后拒绝新建会话，属用户可见的功能降级 |

## 3. 详细设计（数据结构、状态机、算法、接口签名、时序）

### 3.1 Session 状态机

**状态定义**

| 状态 | 语义 | 持久化 | 允许恢复 |
| --- | --- | --- | --- |
| `created` | Session 记录 + 资源已分配，PTY 尚未成功 spawn | 仅 StateChange 事件 | 否（无输出，退化为 dead） |
| `running` | PTY master 可读；有 Interactive attach 或 headless 驱动 | 是 | **是** |
| `detached` | 无 Interactive attach（lease 空闲），PTY 与子进程继续运行 | 是 | **是** |
| `exited` | 子进程树已 reaped，exit code 已捕获；PTY 句柄待释放 | 是 | 否 |
| `recovering` | sessiond 重启后对该会话做 checkpoint + 尾部重放；**不接受写**，只读订阅受限 | 是 | 可退化为 dead |
| `dead` | 终态：句柄释放，仅保留 Log（可读、只读降级） | 是 | 否 |

**迁移表（唯一权威；实现不得出现表外迁移）**

| from | to | 触发条件 | 守卫 | 动作 | Log 事件 |
| --- | --- | --- | --- | --- | --- |
| created | running | `PtyBackend::spawn` 成功 | Transport 已打开 且 Grid 已初始化 | 启动 reader task、注入 shell 集成脚本（可禁用） | `StateChange{running}` |
| created | dead | spawn 失败 / 5s 超时【新增】 | — | 释放资源、写 `ErrorSummary` | `StateChange{dead, reason=spawn_failed}` |
| running | detached | 最后一个 Interactive attach detach 或 lease 过期 | 子进程仍存活 | 停止 delta 广播；继续读 PTY 并写 Log | `StateChange{detached}` |
| detached | running | 新 Interactive attach 且 lease 授予成功 | 状态为 detached | snapshot → tail replay → delta 追赶 | `StateChange{running}` |
| running / detached | exited | PTY reader EOF 且进程树 reaped | — | 记录 exit code、冻结网格、写 checkpoint | `StateChange{exited, exit_code}` |
| running / detached | recovering | sessiond 崩溃重启后扫描到未关闭会话 | Log 尾部可解析（≥1 有效 record） | 加载 checkpoint + 重放；期间无 lease | `StateChange{recovering}` |
| recovering | detached | 重建成功 | `grid_digest` 校验通过 | 恢复为无 attach 的活会话 | `StateChange{detached, resumed_from=ckpt}` |
| recovering | dead | 重建失败 / checkpoint 与尾部均不可校验 | — | 保留 Log 只读，UI 显式告警 | `StateChange{dead, reason=unrecoverable}` |
| exited | dead | reaper 回收 | 无 attach | 释放 PTY / Job Object | `StateChange{dead, reason=reaped}` |
| 任意 | dead | 用户显式关闭（含 `kill`） | L2 及以上需确认（AR-06） | SIGHUP / JobObject Terminate | `StateChange{dead, reason=closed}` |

**不变量**：① `dead` 无出边；② 仅 `running` / `detached` 可进入 `recovering`；③ sessiond **绝不因无 attach 而终止用户进程**（AR-13 核心承诺），闲置回收只作用于 `exited` 会话的资源；④ 每次迁移先写 Log 再广播（先持久后可见）。

    created ──spawn ok──▶ running ⇄ detached ──EOF──▶ exited ──reap──▶ dead
       │ spawn fail           │            │                              ▲
       └──────────────────────┴─sessiond 重启─▶ recovering ───fail───────┘
                                                  │ grid ok
                                                  └──▶ detached

### 3.2 Session Log 段格式

**3.2.1 段文件头（64B 定长，小端）**

| offset | size | 字段 |
| --- | --- | --- |
| 0 | 8 | `magic = "TMAILOG\0"` |
| 8 | 2 | `format_version: u16` |
| 10 | 2 | `min_reader_version: u16` |
| 12 | 4 | `segment_id: u32` |
| 16 | 8 | `created_at_unix_ns: u64` |
| 24 | 8 | `first_seq: u64` |
| 32 | 8 | `prev_segment_hash_prefix: [u8;8]`（BLAKE3 前缀，链式防断层） |
| 40 | 4 | `flags: u32`（bit0 = raw-ring、bit1 = encrypted、bit2 = sealed） |
| 44 | 4 | `header_crc32c` |
| 48 | 16 | reserved |

**3.2.2 记录帧**：`[len:u32][type:u16][flags:u16][seq:u64][ts_ns:u64][payload…][crc32c:u32]`；`len` 仅计 payload（不含头与 crc）。CRC 覆盖 `type..payload`。**只追加，不就地更新**；`sealed` 段不可再写。

**3.2.3 记录类型**

| type | 名称 | payload 要点 | 保留类 | 默认消费者 |
| --- | --- | --- | --- | --- |
| `0x0001` | `PtyOut` | `{pane, bytes}` | **P1 volatile ring** | UI（交互端）、agent（按需报错段）、插件只读镜像 |
| `0x0002` | `PtyIn` | `{pane, sha256, len, source: Human\|Agent\|Plugin, lease_id}`（**默认无明文字节**） | P0 | 审计、lease 追责 |
| `0x0003` | `Resize` | `{pane, cols, rows, px_w, px_h}` | P0 | UI、agent |
| `0x0010` | `CmdStart` | `{pane, cmd_id, prompt_marker, ts}` | P0 | agent、UI |
| `0x0011` | `CmdEnd` | `{pane, cmd_id, exit_code, duration_ms, cwd, confidence}` | P0 | agent、UI |
| `0x0012` | `CwdChange` | `{pane, cwd}` | P0 | agent |
| `0x0013` | `TitleChange` | `{pane, title}` | P1 | UI |
| `0x0020` | `ContextEvent` | `{pane, kind, payload}`（报错摘要等） | P0 | agent |
| `0x0030` | `StateChange` | `{from, to, reason, exit_code?}` | P0 | supervisor、UI、Local API |
| `0x0040` | `CheckpointRef` | `{ckpt_id, segment_id, offset, grid_digest}` | P0 | recovery |
| `0x0050` | `LeaseEvent` | `{session, from, to, action, approver}` | P0 | 审计、UI |
| `0x0060` | `SubscriptionDrop` | `{sub_id, class, dropped_bytes, policy}` | P0 | 审计、UI |
| `0x0070` | `AuditRef` | `{audit_seq, prev_hash, kind}` | P0 | 审计（DC-34 链） |

**3.2.4 保留策略与滚动**：**段上限 = 8MiB**（段滚动阈值，03-spec §3.7；单段一旦达阈值即滚动，即任何 raw/元数据段 ≤8MiB）。P0 段按 `min(可用磁盘 5%, 8GiB)/会话` 上限，超限按「>90 天且未 pin → 删除原字节、保留命令边界摘要」归档；**P1 raw ring 环上限 = 64MiB**（环内可留存多个 ≤8MiB 段；恢复窗口 = 环内最近 64MiB 或最近 10 分钟，先到者为准），溢出丢最旧 raw 段（写 `SubscriptionDrop` 计数事件）。80% 告警、100% 先停 raw、再拒新建会话。

**3.2.5 fsync 时机（决定 K-06 的 RPO）**

| 记录类 | 刷盘策略 | 崩溃 RPO |
| --- | --- | --- |
| `CmdEnd` / `StateChange` / `LeaseEvent` / `CheckpointRef` | 写入即 `fdatasync`，**先刷盘再通知 UI** | ≤ 1 个命令块 |
| 其他 P0 记录 | ≤250ms 或 64KiB 批量 `fdatasync` | ≤64KiB 元数据 |
| `PtyOut`（raw ring） | ≤100ms 或 256KiB 批量；段滚动时 `fdatasync` | 尾部 ≤256KiB 原始字节（只影响屏幕尾部） |

### 3.3 checkpoint 与重建

**Checkpoint 结构**（序列化进 CAS，Log 只存 `CheckpointRef`）：

    struct Checkpoint { ckpt_id: u64, seq: u64, segment_id: u32, offset: u32,
                        grid_digest: [u8;32], modes: [u8;64], cursor: CellPos,
                        scrollback_head: RecId, cwd: PathBuf, created_at_ns: u64 }

**触发条件**：① 每 256 个 `CmdEnd`【新增】；② 连续 30s 无 `PtyOut` 且 damage 为空；③ resize/reflow 完成后；④ detach 时；⑤ `exited` 时；⑥ 收到 SIGTERM 时尽力。**不触发**于连续输出滚动中（避免写放大与吞吐耦合，保护 §5「解析+渲染吞吐 ≥500MB/s」）。

**重建算法**（伪代码；唯一入口 `recover_session`）：

    fn recover_session(id) -> RecoverOutcome {
        let ckpt = latest_valid_checkpoint(id)?;          // 校验 ckpt CRC + grid_digest 格式
        let mut grid = Grid::from_checkpoint(ckpt)?;      // 仅网格/模式/光标/scrollback 指针
        let mut meta = MetaIndex::from_checkpoint(ckpt);
        for seg in segments_after(ckpt.segment_id) {
            for rec in seg.iter() {
                if !rec.crc32c_valid() {                  // 尾部损坏：截断，绝不静默丢
                    return Ok(RecoverOutcome { grid, meta, tail_truncated: true,
                                               last_valid_seq: rec.prev_seq() });
                }
                match rec.ty {
                    PtyOut => grid.feed_raw(rec.payload),      // 纯 VT 回放
                    PtyIn  => { /* 仅入审计索引，永不执行 */ }
                    _      => meta.apply(rec),
                }
            }
        }
        Ok(RecoverOutcome { grid, meta, tail_truncated: false, last_valid_seq: seg.head_seq })
    }

**AR-13 诚实边界的实现落点**：① 恢复后 `SessionState = detached` 且 `resumed_from = ckpt.ckpt_id`，UI 状态栏与 Sessions 分区必须显示「屏幕已恢复（checkpoint + 尾部 N 段）· 进程未恢复 · 退出码：未知/已知」；② 文案契约遵循 02-spec C4：允许「恢复布局 / 恢复工作目录 / 恢复滚动历史 / 重新连接屏幕」，**禁止**「恢复进程 / 重启会话 / 恢复运行中的任务」；③ `tail_truncated=true` 时额外显示缺口标记与「缺失区间」条数；④ 恢复流程中不存在任何 `spawn` / `exec` 调用（可静态断言，见 §5 AC-S4）。

### 3.4 多端 attach 与 stdin 单写者 lease（OQ-30 / 03-spec OQ-A1）

**协议消息**（termai-ipc `msg_type`，P1 冻结帧位）

| 消息 | 方向 | 字段 |
| --- | --- | --- |
| `ATTACH_REQ` | C→S | `{proto_range, client_kind, session_id, mode: ReadOnly\|Interactive, resume_from?: u64, capabilities[]}` |
| `ATTACH_ACK` | S→C | `{chosen_ver, state, snapshot_ref{segment_id, seq, grid_digest}, lease?: LeaseInfo, limits{max_frame=8MiB, credits}}` |
| `GRID_SNAPSHOT` | S→C | `{seq, grid_digest, chunk_idx, chunk_total, payload}`（≤1MiB/块） |
| `TAIL_REPLAY` | S→C | `{from_seq, events[]}`（仅 P0 事件） |
| `GRID_DELTA` | S→C | `{seq, damage[], payload}` |
| `LEASE_GRANT / RENEW / PREEMPT / REVOKE / DENIED` | 双向 | `{lease_id, holder, since, ttl_s, reason}` |
| `DETACH_NOTICE` | C→S | `{lease_release: bool}` |

**时序**：`ATTACH_REQ` → `ATTACH_ACK` → `GRID_SNAPSHOT`（seq₀）→ `TAIL_REPLAY`(seq₀..head) → `GRID_DELTA`(head→)。幂等：`resume_from = last_applied_seq`，服务端只发 `> resume_from`；重复 attach 返回同一订阅句柄。帧上限 **8MB**（HARNESS OQ-30 建议值 / 03-spec OQ-A4），超限分块，禁止拆帧乱序。

**lease 状态机**：`Free → Held{client, acquired_at, expires_at} → Free`；`Held → Held{new holder}` 仅经 `transfer`（显式移交）。默认 **TTL 30s / renew 10s**【新增】；连续 2 次 renew 丢失即释放。

| 规则 | 内容 |
| --- | --- |
| 授予 | 首个 `Interactive` attach 且 `Free` → 自动授予（写 `LeaseEvent{grant}`） |
| 抢占 | **默认禁止后到先赢**；仅 ① 过期自动回收；② 用户在目标端显式「夺取输入权」且持有者已 `detached` 或 15s 无响应 → 二次确认 + `LeaseEvent{takeover, approver}`；企业策略可整体禁用 takeover |
| 移交 | `transfer`：接收端 ACK 后生效；双方写入 `LeaseEvent{transfer}`，审计记录 operator/approver |
| 只读端行为 | 输入行置灰；悬停提示「输入权在 <client/host>」；`⌥T` 发起移交请求；被拒 → 非模态 toast + Sessions 分区 attach 列表高亮 |
| Headless / Local API | 同一 lease 契约；`pty.write` 必须带 capability 令牌 + `lease_id`，否则 `CapabilityUnavailable`（默认拒绝，DC-35） |
| 冲突可见性 | 任意拒绝/抢占/过期都必须产生用户可见事件（toast 或分区角标），并在 Audit Log 可回看；**无静默失败** |

### 3.5 崩溃恢复三级语义

| 崩溃对象 | 恢复范围 | 耗时预算 | RPO | 用户可见后果 |
| --- | --- | --- | --- | --- |
| **UI 进程** | UI 重连；sessiond / PTY / 子进程不受影响 | **<2s 重连且屏幕一致**（§8.2 门禁原文）；本文以 P95 度量、目标 P95 ≤1s【新增】 | **0**（真相在 sessiond） | 窗口闪回后屏幕一致，attach 端数与 lease 不变 |
| **sessiond** | checkpoint + 尾部段重建屏幕与结构化元数据；**进程不可恢复**（PTY master 关闭 → 子进程多数终止） | 冷重建 + 重连 **P95 ≤2s / P99 ≤5s**【已决：AR-26 第 4 条，已生效门禁】 | 元数据 ≤1 命令块；raw 尾部 ≤256KiB；**进程状态不可恢复** | 状态栏「会话已重建 · 屏幕可恢复，进程不可恢复」+ 缺口标记 |
| **系统崩溃 / 断电** | 同 sessiond，且含 OS 重启后 sessiond 冷启 | 冷启到首个会话可 attach **P95 ≤5s**【新增】 | 同 sessiond（依赖 §3.2.5 fdatasync） | 同上 + 一次性「上次未正常退出」通知（可禁用展示） |

### 3.6 supervisor 监督树与重启策略

    OS init/systemd(用户级) → termai-supervisor(每用户单实例)
                                ├─ sessiond        (常驻 7×24, 崩溃自动重启)
                                ├─ termai-agent    (按需/常驻)
                                ├─ termai-tools    (随工具调用)
                                └─ plugin-host     (按需启动, 空闲退出)

`RestartPolicy { base: 250ms, factor: 2.0, max: 30s, jitter: ±20%, window: 10min, max_restarts: 5, circuit_open: 5min }`【新增】。区分 **crash**（非零退出/信号/心跳超时）与 **intentional exit**（exit 0、idle-timeout、`--no-plugins`）：退避只作用于 crash。窗口内 5 次 crash → 打开熔断：sessiond 停止接受新会话创建（保诊断能力），UI 顶部横幅 + 托盘通知 + 「诊断导出」+ 手动重启；agent → AI safe mode（04-spec §3.12）；plugin-host → 插件 safe mode（06-spec §3.7）。**禁止静默重启**（AR-20：能力边界必须明示）。重启 sessiond 后自动进入 `recovering`，不触发用户进程重启。

### 3.7 订阅契约、背压与 AI 关闭零开销

| 事件类 | 代表事件 | 允许订阅方 | 默认保留 | 溢出策略（必须显式声明） |
| --- | --- | --- | --- | --- |
| C0 控制 | `StateChange`、`LeaseEvent`、`Resize` | UI、agent、plugin-host、Local API | P0 | `Disconnect`（不允许丢） |
| C1 元数据 | `CmdEnd`、`CwdChange`、`TitleChange`、`ContextEvent` | UI、agent、plugin-host（按清单 `osc_events`） | P0 | `DropOldest`（环形 1MiB/订阅） |
| C2 原始字节 | `PtyOut` 增量 | UI（交互端）、agent（按需报错段）、插件只读镜像（限速） | P1 volatile | `Coalesce(16ms)` → 再 `DropOldest` |
| C3 大对象 | CAS 引用（图片/大 diff/长日志） | agent、UI | 引用 P0 / 内容按 CAS GC | 引用不丢，内容按 CAS 策略 |

**背压**：每订阅 per-channel credit（默认 256KiB，上限 1MiB）；超限按上表策略处置，且**每次丢弃必须计 `SubscriptionDrop` 并可在 UI `subscription.drops` 回看**（禁止静默丢弃）。慢订阅者不得阻塞写路径：sessiond 对 C2 采用「热路径只追加到环形缓冲、由独立 publisher 线程投递」，保证 key-to-photon P99 ≤16ms（§5 门禁）不因订阅者劣化。

**AI 关闭零开销（可断言）**：`EventBus::publish` 在订阅引用计数为 0 时走 `ZeroOverhead` 分支（不分配、不序列化、不取锁）；再叠加编译期不启用 `agent` feature。三条断言见 AC-S9。

### 3.8 24h 无泄漏测量

`cargo xtask bench --soak --duration 24h --profile release` 输出 `soak-report.json`。**负载语料**：4 会话 ×（1GB 语料循环 `cat` + 每 30s 一条 OSC 133 命令块 + 每 5min resize + 每 10min attach/detach），含 8h 空闲段，并注入 1 次 sessiond SIGKILL + 1 次 UI SIGKILL。**采样**：RSS（`/proc/<pid>/status:VmRSS` / `GetProcessMemoryInfo`）每 60s；同时采 fd/handle 数、GPU 资源句柄、Log 段数、订阅队列字节。**判据**：RSS 线性回归斜率 **<1MB/h**（§5 门禁）；端到端漂移 **<5%**（§8.2）；fd/handle 斜率 = 0【新增】；Log 段数落在预期滚动数 ±1。**失败定位**：每个采样点带归因标签（alloc site / 队列 / fd 类型），失败自动 dump 60s 前差分快照；`MAD/中位数 >2%` 判 `INCONCLUSIVE`（对齐 07-spec §3.8.3），不判失败。

## 4. 接口与依赖（我需要谁给什么、我向谁承诺什么）

    // crates/termai-session/src/lifecycle.rs
    pub enum SessionState { Created, Running, Detached, Exited, Recovering, Dead }
    pub enum AttachMode { ReadOnly, Interactive }
    pub struct SessionId(u128);            // ULID，排序友好
    pub struct LeaseId(u64);

    pub trait SessionLifecycle: Send + Sync {
        fn create(&self, spec: SessionSpec) -> Result<SessionId, SessionError>;
        fn attach(&self, id: SessionId, c: ClientId, m: AttachMode)
            -> Result<AttachTicket, SessionError>;
        fn state(&self, id: SessionId) -> SessionState;
        fn close(&self, id: SessionId, r: CloseReason) -> Result<(), SessionError>;
    }
    pub trait StdinLease: Send + Sync {
        fn acquire(&self, id: SessionId, c: ClientId, ttl: Duration) -> Result<LeaseId, LeaseError>;
        fn renew(&self, l: LeaseId) -> Result<(), LeaseError>;
        fn transfer(&self, l: LeaseId, to: ClientId, approver: Actor) -> Result<LeaseId, LeaseError>;
        fn revoke(&self, l: LeaseId, by: Actor) -> Result<(), LeaseError>;   // 仅显式路径
    }
    pub trait LogWriter: Send {
        fn append(&mut self, rec: &Record) -> Result<RecId, LogError>;
        fn flush(&mut self, mode: FlushMode) -> Result<(), LogError>;   // Buffered|FsyncData|FsyncFull
        fn rotate(&mut self, p: &RotationPolicy) -> Result<SegmentId, LogError>;
    }
    pub trait EventBus: Send + Sync {
        fn subscribe(&self, spec: SubscribeSpec) -> Result<SubscriptionHandle, SubscribeError>;
        fn publish(&self, ev: &ContextEvent) -> PublishOutcome; // ZeroOverhead|Delivered{n}|Dropped{n}
    }

| 对方 | 我需要得到 | 我承诺给出 |
| --- | --- | --- |
| termai-core / IDL（AR-04） | 事件 schema（OSC 133/633、exit code、cwd、duration）、Grid snapshot/delta 类型、Checkpoint 序列化 | 状态与迁移事件稳定 schema；破坏性变更走 capability 协商，兼容 ≥2 minor |
| termai-ipc（DC-22） | 帧格式、**8MB 帧上限**、共享内存环形缓冲、msg_type 号段 | attach/lease 消息定义与 fuzz 语料（DC-37） |
| termai-store（DC-23） | 段文件读写、CAS、SQLite 派生索引、迁移器（兼容 ≥2 大版本） | Log 段格式 + 版本 + CRC 截断恢复语义；索引可从 Log 完整重建（03-spec AC-06） |
| Local API（DC-24） | `session.*` / `pty.*` / `log.*` 命名空间映射、capability 令牌、审计写入 | lease 校验与审计；写操作默认拒绝（DC-35） |
| termai-supervisor | 拉起 / 重启 / 退出码上报 / 心跳；熔断状态查询 | 区分 crash 与 intentional exit 的退出码契约 |
| UI（AR-13 / 02-spec C4） | lease 交互（移交/夺取/拒绝提示）、恢复文案与缺口标记、attach 列表 | 恢复状态机与 `resumed_from` / `tail_truncated` 字段（供文案与审计） |
| termai-agent / plugin-host（AR-03 / AR-07） | 订阅声明（事件类 + credit + drop policy） | 只读订阅 API、惰性事件、插件限速只读镜像；AI 关闭零开销三断言 |
| 07-spec L8 / RM-A（ADR-0014） | soak 承载机与 `bench-report.json` schema | `soak-report.json` 与 24h 无泄漏门禁数据 |

## 5. 可验证验收（每条含测量方法、语料或工具、判据、CI 落点）

| AC | 测量方法 | 语料 / 工具 | 判据 | CI 落点 |
| --- | --- | --- | --- | --- |
| AC-S1 状态机穷尽且封闭 | 属性测试遍历全部迁移；断言表外迁移 0 通过 | `proptest` + 状态机模型 | 非法迁移拦截率 100% | PR（L1） |
| AC-S2 记录 CRC 与尾部截断 | 对录制 Log 随机注入单字节损坏（段头/crc/中间记录） | `tests/replay/corrupt/` + `cargo xtask replay --corrupt-tail` | 恢复到最后有效 record；不 panic；不静默丢；`tail_truncated` 正确置位 | PR + nightly（L3） |
| AC-S3 重建一致性 | 在线网格 digest vs `checkpoint+tail` 重建 digest；3 客户端并发 attach 帧哈希 | `cargo xtask replay` + `termai-headless` | 三者一致（03-spec AC-13） | PR（L3） |
| AC-S4 无命令重放 | 重建期间 `execve`/`spawn` 调用计数；静态断言 `recover_session` 无 spawn 路径 | 调用探针 + `cargo xtask depcheck` 规则 | 调用数 = 0 | PR（L1/L2） |
| AC-S5 UI 崩溃重连 <2s | 三档负载（空闲 / 中等 / 500MB/s cat）注入 UI SIGKILL，测重连 P95 与屏幕 digest | RM-C 自托管 + 时序探针 | 重连 **<2s**（§8.2 门禁原文）；度量口径 P95【新增】+ digest 一致 | nightly（L8） |
| AC-S6 sessiond 崩溃恢复 | SIGKILL sessiond → supervisor 重启 → 比对屏幕与元数据；统计缺口 | 故障注入脚本 + soak | 元数据缺口 ≤1 命令块；P95 ≤2s / P99 ≤5s（AR-26 第 4 条，已生效门禁）；无命令重放 | nightly（L8） |
| AC-S7 lease 唯一性 | 3 客户端并发争抢 stdin ≥10⁵ 轮；审计断言同时持有者 ≤1 | race/loom 测试 + fuzz | 双写事件 = 0；争用审计覆盖率 100% | PR + nightly（L5） |
| AC-S8 磁盘上限与滚动 | 24h 满速输出；监控占用、raw 丢弃、P0 完整性 | `cargo xtask bench --soak --disk-cap` | 占用 ≤cap；raw 丢最旧；P0 记录零丢失；80% 告警触发 | 周度（L8） |
| AC-S9 AI 关闭零开销 | ① 进程表无 termai-agent；② `cargo tree` 断言 sessiond 不链接任何 AI SDK/网络库；③ 分配计数探针在 publish 路径 = 0；④ 对照基准 P95 劣化 | `cargo xtask bench --metric ai-off` + 分配探针 | ①②③ 恒真；④ 劣化 **<1%**（AR-03 复议条件）/ ≤5%（04-spec N3） | PR（静态）+ nightly（L4） |
| AC-S10 24h 无泄漏 | §3.8 soak；RSS 回归斜率 + fd 斜率 + 段数 | RM-A 自托管 | RSS **<1MB/h**（§5 门禁）、漂移 **<5%**（§8.2）、fd 斜率 = 0【新增】 | 周度 + 发版前（L8） |
| AC-S11 退避与熔断 | 连续注入 5 次崩溃，记录重启间隔与通知 | 故障注入 | 序列 ≈250ms/500ms/1s/2s/4s（±20%）；第 5 次后熔断 5min；通知 100% | nightly（L2） |
| AC-S12 背压不阻塞且丢弃可见 | 慢订阅者停读 30s；同时测热路径延迟与丢弃计数 | `termai-bench` + 慢消费者夹具 | key-to-photon P99 ≤16ms（§5 门禁）不劣化；丢弃计数可见；无静默丢 | nightly（L4） |

## 6. 风险与降级（触发条件、降级行为、用户可见后果）

| # | 触发条件 | 降级行为 | 用户可见后果 |
| --- | --- | --- | --- |
| R-S1 | Log 尾部 CRC 损坏 / checkpoint 不可校验 | 截断到最后有效 record；`tail_truncated=true`，缺口条数入 UI | 屏幕存在缺口标记；元数据仍可用；不静默丢 |
| R-S2 | `format_version` 超出本版本可读范围 | 只读降级：可查看 Log，禁止 attach 写与新建 | 明确告警「该会话由更新版本创建，请升级」；不静默丢数据（DC-23） |
| R-S3 | sessiond 崩溃循环（窗口内 ≥5 次） | 熔断 5min，停止新建会话，保诊断导出 | 顶部横幅 + 托盘通知 + 手动重启；终端核心仍可用（AR-03） |
| R-S4 | 磁盘达 80% / 100% 上限 | 80% 告警 → 100% 先停 raw ring、再拒新建会话 | 会话内提示「原始输出不再保留」；新建入口禁用并给清理指引 |
| R-S5 | lease 争用 / 多端同时输入 | 拒绝第二写者，提供移交与夺取入口 | 非写端输入置灰 + 提示持有者；被拒有 toast；审计可回看 |
| R-S6 | 慢订阅者（agent 停读 / 插件限速） | 按声明的 drop policy 丢弃，`SubscriptionDrop` 计数 | agent/插件面板显示「事件已裁剪」角标；核心终端不受影响 |
| R-S7 | 恢复窗口内 raw 字节不足（长时间 detached + cap 滚动） | 恢复结构化元数据 + 已知屏幕；缺口明示 | 「滚动历史不完整」提示；不伪造内容 |
| R-S8 | 插件 / agent 崩溃 | 各自 safe mode；**不**重启 sessiond（03-spec OQ-A6 口径） | 插件/AI 功能降级提示；会话零影响 |

## 7. 被否决的方案与反方意见（保留 tradeoff）

| 被否决方案 | 反方最强理由 | 否决理由 | 复议条件 |
| --- | --- | --- | --- |
| A 每会话一 sessiond 进程 | 隔离最好，崩溃只影响单会话 | 与 ADR-0009「每用户单守护 + 每会话子任务」冲突；fd/内存放大，7×24 成本上升 | 子任务隔离实测仍导致跨会话串扰（进程内 panic 传播）时重议 |
| B 以 tmux 作为持久化真源 | 久经考验、零开发、生态兼容 | ADR-0009 已否决：无法承载结构化上下文；Windows 无 tmux | 仅在 sessiond 不可达时作为兼容客户端 coexist，不作真源 |
| C 每次 damage 后 checkpoint | 恢复更快、重放更短 | 写放大与 CPU 开销直接侵蚀 §5 吞吐门禁与 §5 空闲 RSS | 若重建 P95 >2s（OQ-SES-02）连续两版不达标，改自适应触发 |
| D 完整原始输出默认持久 | 用户最想要「历史永不丢」 | 违反 AR-13 / OQ-04 默认值；隐私与磁盘成本 | 用户显式开启（可加密）后即为合法路径 |
| E 无 lease，后到先赢 / 广播所有输入 | 多端协作最顺滑，实现最简单 | OQ-30 明确「单写者租约 + 只读多订阅」；不可审计且破坏屏幕一致性 | P5 跨端协作若实测中断率高，可加「协作会话」显式模式，仍不放弃 lease |
| F sessiond 重启后重放输入日志以恢复进程 | 用户最期待「进程也回来了」 | 违反 DC-04，且副作用不可逆（A1 信任基础设施） | **不可复议**（诚实声明不可放宽） |
| G checkpoint 存入 SQLite 作真相 | 查询快、实现现成 | ADR-0003 否决：索引损坏即历史丢失；违背「可重建」 | **不可复议** |
| H supervisor 无限重启、无退避 | 恢复最快、实现最少 | 崩溃风暴放大故障、掩盖根因 | 熔断期内提供显式「立即重试」按钮补偿 |
| I 每次写都 fsync 以求系统崩溃零丢失 | RPO = 0 最安全 | 写放大使吞吐无法达 §5 ≥500MB/s；且 raw 尾部丢失不影响 K-06 承诺 | 若出现强合规场景，以「合规模式」显式开启并接受吞吐降级 |

## 8. Open Questions（含影响面、建议值、决策阶段）

> **【新增】指标登记（硬性规则 2）**：下列数值来自本文设计，尚未进入 HARNESS §5，不得与 §5 并列；需 TSC 决定是否升为门禁。

| ID | 问题 / 【新增】指标 | 影响面 | 建议值 | 决策阶段 |
| --- | --- | --- | --- | --- |
| OQ-SES-01 | 状态枚举与 03-spec §3.6（`Creating/Crashed/Reaped`）不一致（**已决：同步已完成**） | IDL 枚举、审计、UI 文案、回放断言 | **已决：采纳本文 §3.1 六态（`created/running/detached/exited/recovering/dead`）为唯一枚举；`docs/spec/03` §3.6 已补「状态枚举口径（以 kernel/04 为准）」一行并给出旧名→新名映射（`Creating→created`、`Running→running`、`Detached→detached`、`Exited→exited`、`Crashed→recovering`、`Reaped→dead`）；kernel/07 §3.6 的 `SessionLifecycle` tag 1 同口径** | **已决（03-spec §3.6 已同步）** |
| OQ-SES-02 | sessiond 崩溃重建耗时口径（**已决：AR-26 第 4 条**） | 可靠性门禁、supervisor 策略、checkpoint 频率 | **冷重建 + 重连 P95 ≤2s / P99 ≤5s**（与 §8.2 UI <2s 同源；已生效门禁） | **已决（AR-26）** |
| OQ-SES-03 | lease 的决策阶段冲突（HARNESS OQ-30 = P1 vs 03-spec OQ-A1 = P5）（**已决：AR-26 第 5 条**） | IPC 帧位冻结、协作写开放节奏、审计 schema | **P1 冻结帧位与 lease 语义；P5 启用跨端协作写** | **已决（AR-26）** |
| OQ-SES-04 | P1 raw ring 默认窗口与加密密钥管理（OQ-04）（**部分已决：AR-26**） | 隐私、恢复完整度、磁盘占用 | **已决（AR-26 第 1–3 条）：P1 raw ring 默认 volatile、不落长期存储**；raw 段上限 8MiB、环上限 64MiB，恢复窗口 = 环内最近 64MiB 或最近 10 分钟取先到者（口径同 §3.2.4 / K-05）。**仍待决**：加密密钥走 OS keychain、超期轮换的实现细节 | **部分已决（AR-26）/ 密钥管理 P1** |
| OQ-SES-05 | 磁盘硬上限与用户「无限历史」期望冲突 | 磁盘、用户体验、支持成本 | 【新增】默认 min(可用磁盘 5%, 8GiB)/会话，可配但上限不得取消 | P1 |
| OQ-SES-06 | checkpoint 频率（256 命令块 / 30s 空闲）对 CPU 与吞吐的影响 | §5 吞吐门禁、写放大 | 先按建议值实现并以 AC-S6 校准，超标则改自适应 | P1 |
| OQ-SES-07 | plugin-host / agent 与 sessiond 的监督层级归属（03-spec OQ-A6） | 可靠性、崩溃预算、safe mode 语义 | supervisor 统一看护；plugin-host / agent 崩溃**不**重启 sessiond | P3 |
| OQ-SES-08 | 系统崩溃后是否把历史元数据注入新建 shell（非命令重放） | DC-04 边界、用户预期 | 不注入；只在恢复面板展示，避免污染新 shell 环境 | P1 |
| OQ-SES-09 | 【新增】验收判据集合：sessiond 冷重建 P95 ≤2s、系统崩溃冷启 ≤5s、默认负载订阅丢弃率 = 0、lease 争用审计覆盖率 100%、24h fd 斜率 = 0 | 门禁与预算章节 | 先列入 §5 之外的「可靠性门禁候选」，观测两版后升门禁 | P1 |
| OQ-SES-10 | 【新增】本文其余设计常量与可测阈值：spawn 超时 5s、lease TTL 30s / renew 10s、supervisor 退避 250ms→30s（±20%、10min 内 5 次熔断 5min）、订阅 credit 256KiB（上限 1MiB）、P1 raw ring 64MiB、checkpoint 每 256 命令块或 30s 空闲 | 超时/配额语义、可靠性门禁、内存与磁盘上界 | 作为**实现常量**冻结并纳入 AC-S1/S7/S8/S11 验证；仅观测到违约时升为门禁 | P1 |
| OQ-SES-11 | **【已决：AR-26】** §8.2 仅规定 UI 崩溃 <2s，缺 sessiond / 系统崩溃口径；AR-13「原始输出默认不持久」与 03-spec §3.7「Log 含原始字节」表述张力；OQ-30 与 OQ-A1 阶段冲突 | 可靠性门禁完整性、存储语义、协作开放节奏 | **已决（AR-26）：① §8.2 增 sessiond 重建预算（P95 ≤2s / P99 ≤5s）；② 明确「P0 元数据 / P1 恢复窗口 / opt-in 历史归档」三层保留；③ OQ-30 阶段 = P1 帧位 + P5 启用**。HARNESS 文本回填由 Orchestrator 执行 | **已决（AR-26）** |

**仲裁代价说明（硬性规则 1）**：若不追加 ①，sessiond 崩溃会被当作「非门禁事件」，而它恰好是 §8.2「屏幕可恢复、进程不可恢复」承诺的核心路径；若不追加 ②，实现者会在「Log 是否落原始字节」上做两种互相矛盾的实现；若不追加 ③，IPC 帧位可能在 P1 被误冻结为不可扩展。三项均为文档级澄清，不改变任何 AR 结论，代价是 24h 内同步 03-spec 与 §11 各一行。
