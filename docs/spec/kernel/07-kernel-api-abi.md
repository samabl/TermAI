# kernel/07 · 内核 API / ABI 与能力协议规格（Kernel API / ABI & Capability Protocol）

> **归属**：内核 API / ABI 与能力协议评审负责人 ｜ **Phase**：P0（帧协议与握手 + 能力校验点）、P1（Context 事件面 + 共享内存传帧 + Local API 契约测试）
> **权威顺序**：HARNESS.md > docs/spec/00-glossary.md > docs/spec/03-system-architecture.md > 本文件。
> **定位**：本文只把 03-spec §3.4（termai-ipc）、§3.5（Local API）、§3.8（多端 attach/单写者）与 §4（契约表）展开为可施工、可验证的一层，**不新增架构结论**；与 HARNESS 冲突处以 HARNESS 为准并显式标注【差异】。
> **预算口径**：本文一切既有数字引用 HARNESS §5「发布门禁 / 目标」双列（AR-19）与 §8 门禁；任何新增可测指标一律标注【新增】并归入 §8.1 待裁决。

## 1. 范围与依据（引用具体 AR/DC 编号）

| # | 范围内 | 依据 |
| --- | --- | --- |
| 1 | termai-ipc 二进制帧格式（长度前缀、消息类型、字段布局）与握手（版本协商、能力位图、失败与降级） | AR-04、DC-22、HARNESS §4.1 |
| 2 | 接口边界：UI 进程 / termai-agent / plugin-host / Local API 客户端各自的读、写与授权前提 | AR-03、AR-06、AR-07、DC-24、DC-35 |
| 3 | Context 事件 schema 版本化（事件类型、字段与类型、时间戳语义、兼容窗口 ≥2 minor） | AR-04、DC-03、DC-23、DC-40 |
| 4 | capability 令牌在内核侧的强制校验点、失败返回码、审计事件产生 | AR-06、DC-34、DC-35、05-spec §3.2；ADR-0015（签名门限与审计保留） |
| 5 | 共享内存传帧：环形布局、生产者/消费者同步、未读丢弃、崩溃与逃逸边界 | AR-04、DC-37、OQ-A4 |
| 6 | 稳定性与弃用：破坏性变更定义、弃用流程、客户端优雅降级 | DC-40、AR-04、docs/spec/07 §3.10 |
| 7 | 「内核不依赖 AI / 网络 / UI」在接口层面的体现（trait 不出现相关类型） | AR-03、AR-21、DC-15、DC-21、ADR-0002 |

**范围外**：kernel 系列其他分册（VT 序列语义；Grid/Scrollback 数据结构；Session Log 文件格式与迁移器，DC-23）；Tool ABI 字段语义（04-spec §3.2）；脱敏规则库（05-spec §3.4）；Local API 具体 method 清单（06-spec §3.9）。其中 **GridSnapshot / GridDelta / ScrollOp 的载荷字段集归 kernel/03（其 OQ-RND-07）**，本文只定其帧封装、通道与丢弃自愈语义。
**硬边界复述（不可协商，AR-03 / §0.4）**：AI 不进 PTY→像素热路径；内核不链接 AI SDK、不持有网络句柄、不依赖 UI；热路径禁用 JSON/gRPC（DC-22）。

## 2. 关键结论（K-01 起，每条含 结论 / 理由 / 代价）

| # | 结论 | 理由 | 代价 |
| --- | --- | --- | --- |
| K-01 | 线格式定为**固定 24 字节帧头 + payload**，payload 分两档：`ENC=POD`（`#[repr(C)]` IDL 生成，仅 ≤4KiB 热路径消息）与 `ENC=CBOR`（冷路径可变结构） | 热路径零解析、零分配、可 `memcpy` 直读；CBOR 与 05-spec §3.2.1 的 capability 令牌编码同源，全栈只维护一种二进制自描述编码 | 需 IDL 支持双后端代码生成；【差异-02】03-spec §3.4 只列 Cap'n Proto / MessagePack，本文取 CBOR，**已由 ADR-0018（D2）追认**（§8.2 OQ-ABI-01 已决） |
| K-02 | `crc32c` 由 03-spec 的「可选」升为**强制**（帧头第 21–24 字节） | IPC 是 §8.1-6 的 fuzz 靶面；shm 槽位与 UDS 帧共用同一校验和语义，避免「校验和是否存在」成为分支 | 每帧 +4B、每帧一次 CRC（硬件 `crc32c` 指令 ≈1 cycle/8B）；【差异-01】对 03-spec 的**收紧**，非放宽，**已由 ADR-0018（D3）追认** |
| K-03 | 每个 `(client, direction)` 独立一对 **SPSC 环**，拒绝多生产者共享环；丢弃策略统一为 **DropNewest + Resync**，拒绝 DropOldest | SPSC 无 CAS、无 ABA；DropOldest 需要生产者推进消费者指针，破坏单生产者单消费者不变量，是竞态温床 | 客户端数 N 时环数为 2N，需按 `limits.shm_clients` 上限做准入（默认 4）；大 N 场景退化为控制通道分块 |
| K-04 | capability 校验是**纯函数** `capability::require()`，禁止在其中做 I/O、分配、取时钟；`now` 由调用方注入 | 可单测、可 fuzz、可在热路径调用；校验点只有 7 个（§3.4），全部经此函数，CI 断言无第二条实现（对齐 05-spec §3.4「唯一收口」范式） | 调用方必须提供单调时钟与 TokenStore 视图，接口面稍宽 |
| K-05 | 人类键盘输入**不逐键做 capability 校验与审计**，而是「租约持有即授权」；租约的获取/转移/吊销走能力校验 + 审计 | §5 门禁 key-to-photon P99 ≤16ms（目标 ≤8ms）不允许每键一次签名校验与审计 fsync；AR-03 只要求「写 stdin 必须显式授权」，租约就是该授权的载体 | 人类输入无法逐键回放审计（本就不采集内容）；非人类主体（agent/plugin/CLI）逐次或按 plan_hash 批量校验 + 审计 |
| K-06 | 共享内存**永不按名打开**：所有权者以随机名 + `O_EXCL|0600` 创建，客户端经已鉴权的 UDS 用 `SCM_RIGHTS`/`DuplicateHandle` 接收句柄；只读消费者（插件镜像）映射 `PROT_READ` / `PAGE_READONLY` | 消除「同 uid 恶意进程按名 attach」与「shm 成为逃逸通道」两类攻击面；映射权限即能力，不依赖命名空间隐晦 | Windows 需显式 DACL + 完整性级别；跨平台抽象层增加一层「句柄传递」实现 |
| K-07 | Context schema 版本 = `major.minor`，**minor 只增字段**、tag 永不复用、reader 必须忽略未知 tag 并计数告警；兼容窗口 **N-2 minor**（= AR-04 的 ≥2 minor），破坏性 = 新 `msg_type` + 新 capability + 双版本并行 ≥2 minor 且 6 个月弃用期 | 与 DC-40 同口径，避免 IPC、Local API、WIT 三套窗口互相掩护（AR-19 的教训） | 永久保留 reserved tag 表；每个 minor 冻结 golden vectors，CI 存储与维护成本上升 |
| K-08 | `termai-core / termai-session / termai-ipc / termai-vt / termai-pty` 的**公开 API 类型集合**不得出现 AI/网络/UI 类型，CI 用 `xtask arch-check`（cargo metadata 传递闭包 + rustdoc JSON 公共项）强制 | AR-03 是「可度量」而非「靠承诺」；类型层面不出现即不可能被误用，比注释与 code review 强 | 新增任何 HTTP/UI 类依赖都会阻断 PR，跨边界改动摩擦上升；纯逻辑复用需下沉到 core-dto |
| K-09 | 内核侧审计写入对 L1+ 与非人类主体 **fail-closed**（审计队满 → deny）；审计不在热路径同步 fsync | DC-34 要求不可抵赖，AR-06 要求「未审批执行 = 0」；把审计失败降级为「仅告警」会直接击穿护栏指标 | 审计后端故障时可用性下降（人类输入不受影响，因走租约语义 K-05）；需有界队列 + 背压显式化 |

## 3. 详细设计（数据结构、状态机、算法、接口签名、时序）

### 3.1 termai-ipc 帧格式（DC-22）

```rust
// crates/termai-ipc/src/frame.rs
pub const FRAME_HEADER_LEN: usize = 24;
pub const MAX_FRAME_LEN: usize   = 8 << 20;   // 8 MiB，对齐 OQ-30「帧上限 8MB」
pub const POD_MAX_LEN: usize      = 4096;      // ENC=POD 上界，超出必须走 CBOR 或分块

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FrameHeader {
    pub len:      u32,   // payload 字节数（不含帧头与 crc 字段自身）
    pub ver:      u16,   // 0xMMmm：高 8 位 major，低 8 位 minor
    pub msg_type: u16,   // §3.2 消息类型表
    pub flags:    u16,   // §3.1.2 位域
    pub rsv:      u16,   // 必须为 0；非 0 即拒帧（前向兼容哨兵）
    pub corr_id:  u64,   // 请求/响应配对；事件流用 0
    pub crc32c:   u32,   // 覆盖 header[0..20] ‖ payload
} // 字段顺序与 03-spec §3.4 一致；rsv 用于补齐 8 字节对齐
```

| 位 | 名称 | 语义 |
| --- | --- | --- |
| 0 | `ENC` | 0 = POD（`repr(C)` 固定布局），1 = CBOR |
| 2:1 | `CHAN` | 0 控制 / 1 数据 / 2 Context 事件 / 3 审计 |
| 3 | `RESPONSE` | 该帧是对 `corr_id` 的应答 |
| 4 | `ERROR` | payload 为 `ErrorBody` 结构 |
| 5 | `MORE_CHUNK` | 分块未结束 |
| 6 | `LAST_CHUNK` | 分块最后一帧 |
| 7 | `REPLAY` | 幂等重放（携带原 `corr_id`） |
| 8 | `AUDIT_REQUIRED` | 该帧触发的动作必须产生审计记录（缺记录即 deny） |
| 9 | `DROP_NOTICE` | 生产侧发生过丢弃，消费侧必须发起 Resync |
| 10 | `SEALED` | payload 内引用 sealed memfd/handle（`cap_ref`），不含明文令牌 |
| 11 | `DEGRADED` | 本帧由降级能力集发出（§3.6） |
| 12–15 | rsv | 必须为 0 |

**解析算法（伪代码，热路径无分配）**

```text
fn decode(buf) -> Result<Frame, IpcError>:
  if buf.len < 24: return NeedMore
  h := read_pod::<FrameHeader>(buf)
  if h.rsv != 0:            return Err(ReservedNonZero)     // 拒帧 + 审计 ipc.reserved_nonzero
  if h.len as usize > MAX_FRAME_LEN: return Err(FrameTooLarge)  // 拒帧，不分配
  if buf.len < 24 + h.len:  return NeedMore
  if cfg.verify_crc and crc32c(buf[0..20] ‖ buf[24..24+h.len]) != h.crc32c: return Err(CrcMismatch)
  if h.flags.enc == POD and h.len > POD_MAX_LEN: return Err(EncodingMismatch)
  return Frame{ h, payload: &buf[24..24+h.len] }   // 借用，不拷贝
```

### 3.2 消息类型表（数值即契约，永不复用）

| 区段 | 类型 | 值 | 通道 | 编码 |
| --- | --- | --- | --- | --- |
| 0x00xx 控制 | Hello / HelloAck / HelloNack | 0x0001/0x0002/0x0003 | 控制 | CBOR |
| | Ping / Pong / GoAway / Error | 0x0004…0x0007 | 控制 | POD(**Error** 为 CBOR) |
| | CreditUpdate / ResyncRequest / ResyncBegin | 0x0008/0x0009/0x000A | 控制 | POD |
| 0x01xx 数据 | Input / Paste / Resize / Signal | 0x0100…0x0103 | 数据 | POD |
| | GridSnapshot / GridDelta / PtyBytes / ScrollbackChunk | 0x0180…0x0183 | 数据/共享内存 | POD |
| 0x02xx Context | ContextSubscribe / ContextUnsubscribe | 0x0200/0x0201 | 控制 | CBOR |
| | ContextEvent / ContextDropNotice | 0x0202/0x0203 | 事件 | CBOR |
| 0x03xx 能力 | CapRefPresent / CapRefSealed / CapDenied / LeaseAcquire / LeaseGrant / LeaseRevoke / LeaseTransfer | 0x0300…0x0302、0x0304…0x0307 | 控制 | POD |
| 0x04xx 审计 | AuditRecord / AuditBackpressure | 0x0400/0x0401 | 审计 | CBOR |
| 0x05xx 会话接入 | AttachRequest / AttachAck / TailReplay / DetachNotice | 0x0500…0x0503（0x0504–0x05FF 保留） | 控制/数据 | CBOR（**DetachNotice** 为 POD） |
| 0xF0xx | Experimental（默认关闭，需 capability `ipc.experimental`） | 0xF000… | 任意 | 任意 |

> **0x05xx 会话接入段由 ADR-0023 D1 分配并冻结**：0x0500 AttachRequest / 0x0501 AttachAck / 0x0502 TailReplay / 0x0503 DetachNotice；0x0504–0x05FF 保留，新分配须新 ADR。

**拒绝规则**：未知 `msg_type` 落在**已知区段**内 → 回 `Error{code: UnsupportedMsg}` 并继续（不断连）；落在保留区段 → 回 `Error{code: ReservedMsgType}`；仅帧层错误（`rsv` 非 0、CRC 失配、超长）才断连。

**输入消息命名与数值收口（与 kernel/05 §4.3 统一）**：输入消息名与数值**以输入域分册 `kernel/05` §4.3 为准**——`0x0100 = Input`（其 `kind` 字段区分键盘 / 鼠标 / IME commit / API 注入五源）、`0x0101 = Paste`（大载荷分块 + PasteGate 语义独立成帧）；本文只定帧封装、通道与丢弃自愈语义，**不再使用 `InputKey` / `InputPaste` 旧名**。理由：kernel/05 是输入语义 owner（K-01 单一编码收口），且 Paste 的分块与审计语义与普通 Input 帧不同，独立数值便于能力协商。

### 3.3 握手：版本协商 + 能力位图（DC-22）

```rust
pub struct Hello {
    pub proto_min: u16, pub proto_max: u16,     // 客户端可接受区间
    pub client_kind: ClientKind,                // Ui | Agent | PluginHost | Cli | Ide | Ci
    pub caps: Vec<CapId>,                       // 排序去重；CapId = u32 稳定枚举
    pub session_claim: SessionId,
    pub feature_bits: u128,                     // 前向兼容的可选特性位
    pub nonce: [u8; 16],                        // 抗重放，与 peer 凭据绑定
}
pub struct HelloAck {
    pub chosen_ver: u16, pub caps_inter: Vec<CapId>,
    pub limits: Limits, pub auth_state: AuthState, pub server_feature_bits: u128,
}
pub struct Limits { pub max_frame: u32, pub credits: u32, pub max_chunks: u16,
                    pub shm: Option<ShmOffer>, pub audit_queue: u32 }
```

**状态机**

```text
        ┌────────┐  connect   ┌────────────┐  HelloAck   ┌──────────┐
        │ Closed │───────────▶│ HelloSent  │────────────▶│ Negotiated│
        └────────┘            └─────┬──────┘             └────┬─────┘
              ▲                     │ HelloNack               │ shm attach + CapRefPresent
              │                     ▼                         ▼
        ┌─────┴─────┐         ┌──────────┐              ┌─────────┐
        │  Failed   │◀────────│ Refused  │              │  Ready  │
        └───────────┘         └──────────┘              └────┬────┘
              ▲                                              │ GoAway / 帧层错误 / 审计背压
              └──────────────────────────────────────────────▼
                                                      ┌──────────┐   drain ≤2s   ┌────────┐
                                                      │ Draining │──────────────▶│ Closed │
                                                      └──────────┘               └────────┘
```

**协商算法**：`lo = max(c.proto_min, s.proto_min)`，`hi = min(c.proto_max, s.proto_max)`；`lo > hi` → `Refused(VerUnsupported)`；否则 `chosen = hi`（取高，避免无谓降级）。能力 = 严格求交；**客户端声明但服务端未知的 required 能力必须显式拒绝**（`Refused(CapUnknown)`），未知 optional 能力静默移除并置 `DEGRADED`。

| 失败路径 | reason 码 | 可重试 | 客户端动作 | CLI 退出码 |
| --- | --- | --- | --- | --- |
| 区间无交集 | `VerUnsupported` | 否 | 提示升级，进入 `Refused`，**禁止**自行降到 `proto_min` 以下重试 | 4（版本不兼容） |
| required 能力未识别 | `CapUnknown` | 否 | 中止；把缺失能力名写入结构化错误 | 4 |
| peer 凭据不匹配（uid/SID） | `PeerDenied` | 否 | 中止 + 审计 | 3（权限拒绝） |
| nonce 重放 | `HandshakeReplay` | 否 | 中止 + 审计（疑似窃取句柄） | 3 |
| 握手超时（默认 2000ms） | `HandshakeTimeout` | 是（≤3 次，指数退避） | 重试后仍失败 → 中止 | 2 |
| 服务端能力不足但版本相交 | 无（Ack + `DEGRADED`） | — | 按 §3.6 降级表选择模式 | 0 |

**时序**：`connect → SO_PEERCRED/GetNamedPipeClientProcessId 校验 → Hello → HelloAck →（可选）ShmAttach + CapRefPresent → Ready`；全过程 P99 ≤2ms【新增 N-1】。

### 3.4 capability 令牌的内核侧强制校验点（AR-06 / DC-35 / ADR-0015）

```rust
// crates/termai-core/src/capability.rs —— 纯函数，无 I/O、无分配、无时钟取用
pub fn require(req: &CapRequest<'_>, tok: &CapToken, now: MonoTime, pol: &EffectivePolicy)
    -> Result<Grant, AuthzError>;

pub enum AuthzError { Missing, Expired, Revoked{ since: MonoTime }, Scope, Taint, Approval, Policy, Malformed }
```

> **编号命名空间（避免与全局 CI 门禁撞号）**：本表校验点自 v1 修订起统一为 **CAP-1…CAP-7**，**不再使用 G1…G7**——`G1–G8` 是 HARNESS §8.1 / spec 07 的全局 CI 门禁编号（见 `docs/spec/07` §3.4.1）。

| # | 唯一入口（函数） | 所在 crate | 受控动作 | 要求 capability（`act`） | 失败返回 |
| --- | --- | --- | --- | --- | --- |
| CAP-1 | `IpcPtyEntry::on_frame` | termai-session | 非租约持有者写 stdin / signal / resize | `stdin.write` / `pty.signal` | `MsgType::CapDenied` + 审计 |
| CAP-2 | `LeaseManager::acquire` / `transfer` / `revoke` | termai-session | 单写者租约授予与转移（OQ-A1） | `stdin.write` + 显式审批凭据 | `LeaseDenied` + 审计 |
| CAP-3 | `LogQuery::open` / `export` | termai-store | 读 Context / Session Log / 审计导出 | `session.read` / `audit.read` | `AuthzError` + 审计 |
| CAP-4 | `HostFnGate::call` | termai-plugin-host | 插件宿主 API（fs / net / exec / secrets / ui.declare） | 每 method 声明的 capability（06-spec §3.9） | `CapabilityUnavailable`（禁止 panic，DC-40） |
| CAP-5 | `LocalApiBroker::dispatch` | termai-session(local-api) | JSON-RPC method | per-method capability | JSON-RPC `-32030` + 结构化 `code` + 审计 |
| CAP-6 | `ConfigStore::apply_patch` | termai-core | 配置写 | `config.write` | `AuthzError` + 审计 |
| CAP-7 | `PluginRegistry::grant` / `revoke` | termai-session | 能力授予、吊销 | 仅桌面 UI / CLI 持用户确认；插件令牌无此权限 | `AuthzError` + 审计 |

**令牌传递（对齐 05-spec §3.2.2「不跨进程暴露明文」）**：帧只携带 `cap_ref: u32`（接收方 TokenStore 槽位）+ `seal_hash: [u8;16]`；令牌本体经 sealed memfd（Linux `memfd_create` + `F_SEAL_SEAL`）或匿名管道 `DuplicateHandle`（Windows）传递，`flags.SEALED=1`。进程退出即失效；不落盘、不进日志。

**失败码与审计一一对应**：`AuthzError` → code `TERMAI-E-AUTHZ-{MISSING|EXPIRED|REVOKED|SCOPE|TAINT|APPROVAL|POLICY|MALFORMED}`；每次 CAP-1–CAP-7 的决策（allow / deny / dry_run）**恰好产生一条** `AuditRecord`（字段集取 05-spec §3.7：`seq/ts_utc/mono/actor/session_id/action{verb,args_digest}/risk_tier/taint/decision/policy_id/approver/receipt_hash/result`）。**fail-closed（K-09）**：审计队列满或链写入失败 → `deny`，不降级为告警。人类键盘输入不逐键审计（K-05），审计粒度 = 租约生命周期。

### 3.5 共享内存传帧规格（OQ-A4 落地）

**布局**：一个环 = 1 个控制页（4096B，页对齐）+ `2^slot_count_log2` 个等长槽（默认 256 槽 × 64KiB = 16MiB，可降至 4MiB）。

```rust
#[repr(C)]
pub struct RingHeader {                 // 控制页，偏移 0
    pub magic: u32,                     // b"TRAI"
    pub layout_ver: u16, pub slot_shift: u8, pub slot_count_shift: u8,
    pub owner_pid: u32, pub boot_id: [u8; 16], pub crash_epoch: u32,
    pub producer_seq: AtomicU64,        // 已提交（publish）到的槽序号
    pub consumer_seq: AtomicU64,        // 已消费完毕的槽序号
    pub credits: AtomicU32,             // 消费端授予的额度
    pub dropped: AtomicU64,             // DropNewest 计数（必须上报，不得静默）
    pub header_crc: AtomicU32,
}
#[repr(C)]
pub struct SlotHeader {                 // 每槽首部，24B
    pub seq: u64, pub msg_type: u16, pub flags: u16,
    pub producer: u32, pub len: u32, pub crc32c: u32, pub _pad: u32,
} // payload 紧随其后，槽内剩余空间以 0 填充（避免残留信息泄露）
```

**同步协议（SPSC，Acquire/Release）**

```text
producer.reserve():
  p = producer_seq.load(Relaxed)
  if p - consumer_seq.load(Acquire) >= slot_count:      # 环满
     policy == DropNewest: dropped += 1; flags |= DROP_NOTICE; return Dropped
     policy == Block:      futex_wait(consumer_seq, timeout); 超时 → Disconnect
     policy == Disconnect: 控制通道置 GoAway; return Fatal
  return slot[p & mask]
producer.publish(): 写 SlotHeader（含 crc32c）→ fence(Release) → producer_seq.store(p+1, Release)
consumer.consume():
  c = consumer_seq.load(Relaxed); p = producer_seq.load(Acquire)
  if c == p: return Empty
  if p < c: return Corrupt                    # 不变量破坏 → detach + 重握手
  off = c & mask; if slot.size != expected: return Corrupt
  sh := read_slot_header(off); if sh.seq != c: return Corrupt   # 撕裂/覆盖检测
  if crc32c(slot_bytes) != sh.crc32c: dropped_corrupt += 1; advance(c); 上报审计
  copy_out(off)                               # 拷贝出环，绝不持有环指针
  fence(Release); consumer_seq.store(c+1, Release); credits += 1
```

| 通道 | 溢出策略 | 未读数据丢弃后的自愈 |
| --- | --- | --- |
| 网格 delta / 装饰提交 | `DropNewest` | `DROP_NOTICE` → 消费端发 `ResyncRequest`（控制通道）→ 生产端回 `GridSnapshot`（分块） |
| 输入 / 控制 | `Block`（默认 50ms） | 超时 → 控制通道 `GoAway(Backpressure)` 并记录审计，绝不解码半帧 |
| 审计 / 日志流 | `Disconnect` + fail-closed | 断连后由 CAP-1–CAP-7 触发 deny，保证不出现「无审计的执行」 |
| Scrollback 大块 | 分块（`MORE_CHUNK`/`LAST_CHUNK`，单帧 ≤8MiB） | 任一块 CRC 失败 → 整段丢弃并重取，不部分应用 |

**崩溃安全**：attach 时校验 `magic/layout_ver/header_crc/boot_id`；`owner_pid` 死亡或 `crash_epoch` 变化 → 丢弃整环并重新握手（永不复活旧环，防 ABA）。owner 每次启动 `crash_epoch += 1`。

**「共享内存不得成为逃逸通道」的 8 条约束**：① 布局内**只有平凡数据**——无指针、无函数指针、无偏移当地址、无句柄；② 消费侧把槽内容一律当**不可信输入**（fuzz 靶面），长度上限 `slot_size - sizeof(SlotHeader)`；③ 插件镜像映射 `PROT_READ`/`PAGE_READONLY`，物理上不可能写入；④ **绝不允许 `PROT_EXEC`**；⑤ 脱敏（RED-0…RED-6）先于写环，槽内绑定脱敏收据哈希，无收据的槽被消费侧拒收；⑥ secret 只写 `secret://handle` 标识，值永不入环；⑦ 环名随机 + 不以名打开 + 容量上界 + 懒分配，抗猜测与抗 DoS；⑧ 槽内容**永不**直接充当 argv/路径/URL（与 §6.2 规则 5 污点追踪一致）。

### 3.6 Context 事件 schema 与其版本化（AR-04 / DC-40）

**事件类型清单**（`ContextEvent.payload` 的 `oneof`，tag 即契约）

| tag | 事件 | 关键字段（类型） | since |
| --- | --- | --- | --- |
| 1 | `SessionLifecycle` | `state: created\|running\|detached\|exited\|recovering\|dead`（**六态以 kernel/04 §3.1 为准**；旧名→新名映射一行：`creating→created`、`crashed→recovering`、`reaped→dead`，`running` / `detached` / `exited` 不变）、`reason: Option<String>` | 1.0 |
| 2 | `CommandBoundary` | `cmd_id: u64`、`phase: prompt_start\|cmd_start\|cmd_end`、`osc: 133\|633`、`confidence: f32` | 1.0 |
| 3 | `ExitStatus` | `cmd_id: u64`、`code: Option<i32>`、`signal: Option<u8>` | 1.0 |
| 4 | `CwdChanged` | `cwd: PathRef`（长度前缀 UTF-8，含 `lossy: bool`） | 1.0 |
| 5 | `TitleChanged` | `title: String`、`scope: icon\|window` | 1.0 |
| 6 | `Resize` | `rows: u16`、`cols: u16`、`px: Option<(u32,u32)>` | 1.0 |
| 7 | `ErrorFragment` | `cmd_id: Option<u64>`、`redacted: String`、`rule_ids: Vec<u16>`、`trust: untrusted` | 1.0 |
| 8 | `ModeChanged` | `modes: bitflags u64`（alt-screen / bracketed-paste / kitty-keyboard / mouse…） | 1.0 |
| 9 | `TransportState` | `kind: local\|ssh\|container\|wsl`、`state: connecting\|ready\|lost` | 1.0 |
| 10 | `TruncationNotice` | `dropped_bytes: u64`、`reason: enum` | 1.0（【新增】事件，见 §8.1） |

**时间戳语义（三时钟，禁止混用）**：`seq: u64`（每会话单调递增，**唯一排序与去重依据**，跳号即表示丢失）；`ts_mono_ns: u64`（同进程单调时钟，**唯一时长计算依据**，不跨重启可比）；`ts_wall_ns: i64`（UTC epoch 纳秒，仅用于跨设备/回放展示，允许回拨，回拨超阈值时置 `clock_skew: true`）。规则：排序只用 `seq`，时长只用 `mono`，展示用 `wall`；任何人不得用 `wall` 计算超时。

**兼容窗口 ≥2 minor 的具体做法**

| 规则 | 内容 | CI 断言 |
| --- | --- | --- |
| V1 | minor **只允许追加 optional 字段**；禁改类型、禁改语义、禁删除 tag | IDL diff 检查：`removed_tags == 0 && changed_types == 0` |
| V2 | tag 永不复用；删除 = 标记 `@deprecated(since)` 并移入 reserved 表（永久保留） | `reserved.txt` 与代码生成一致 |
| V3 | reader 必须**忽略未知 tag** 并计数；首个未知字段打一次 warn + 审计 `context.unknown_field`（限速 1/min），**绝不 panic、绝不断连** | 注入未知 tag 的向量必须 decode 成功 |
| V4 | writer 在 minor M 仅把 ≤ M−2 的字段标为 required；新字段对 N−2 reader 必须是 optional/default | N−2 reader 读 M writer 产物成功率 ≥95%【新增 N-7，口径引 DC-40】 |
| V5 | 破坏性 = 新 `msg_type` / 新 capability / 删改 tag 语义 → 双版本并行 ≥2 minor 且 6 个月弃用（DC-40） | RFC + ADR 记录存在性检查 |
| V6 | 每个 minor 冻结 golden vectors：每事件 ≥3 样本（最小 / 典型 / 边界），存 `crates/core-dto/vectors/v{M}.{m}/` | `xtask ctx-compat --matrix` 全通过 |

### 3.7 稳定性、破坏性变更定义与弃用流程（DC-40）

| 变更类别 | 破坏性 | 判据 | 处置 |
| --- | --- | --- | --- |
| 新增 optional 字段 / 新 tag / 已知区段新 msg_type | 否 | 旧 reader 忽略后行为不变 | minor 递增 + 冻结 golden vector |
| 改既有字段类型 / 语义 / 单位 / 时钟语义 | **是** | 旧 reader 解析结果或语义改变 | major；双版本并行 ≥2 minor 且 6 个月 |
| 删除或复用 msg_type / act / tag 数值 | **是** | 旧客户端误解释 | **永不做**（数值永久 reserved） |
| 收紧长度上限 / 关闭既有 capability / 改丢弃策略 | **是** | 旧客户端被静默降级 | major；须 capability 协商 |
| 改错误码数值或 reason 语义 | **是** | 客户端分支错乱 | major |
| 新增 capability 词表条目 | 否 | 默认拒绝，未授权即无影响 | minor + 词表 ADR |

**弃用状态机**：`Active → Deprecated(N) → Deprecated(N+1) → Removed(N+2)`。`Deprecated` 期间新旧并存，旧路径**每连接输出一次**结构化 `Error{code: Deprecated}`（不阻断），并在 `termai --version --json` 暴露 `deprecations[]`；公告期与 minor 窗口取**更晚**者（**6 个月 + N-2 minor**，DC-40），移除只能随 major，且必须同版本提供 codemod（`termai migrate --from <ver>`）。**客户端优雅降级**：缺 capability → 进入 §6 降级模式而非报错退出；收到 `Deprecated` → 记日志并继续；收到未知 `msg_type` → 跳过并计数（§3.2）。

### 3.8 跨分册错误码登记表（索引，单一真源）

> 由本文件承载的**跨分册错误码索引**；各码的**定义**仍在各自分册，本表只做收敛登记，冲突时**以定义分册为准**。数值即契约：已发布数值**永不复用**，改语义 = major（§3.7）。本表登记 01 / 02 / 04 / 07 四个内核分册各自的错误码与诊断码。

| 来源分册 | 命名空间 / 类型 | 码 / 取值 | 语义 | 契约处置 |
| --- | --- | --- | --- | --- |
| kernel/01 | `ParseError` / `ParseErrorKind`（含 `offset` / `state` / `final_byte`） | 未识别 / 畸形序列分类枚举（诊断面，非数值码） | VT 层诊断；**不回显、不落载荷内容**（kernel/01 §3.4） | 新增 kind 走 minor；不改既有语义 |
| kernel/01 | `VtCounters` 计数键 | `csi_unknown` / `csi_malformed` / `esc_unknown` / `osc_unknown{num}` / `osc_aborted` / `osc_overflow` / `dcs_unknown` / `dcs_overflow` / `dcs_bel_in_data` / `string_cancelled` / `st_stray` / `c1_8bit_in_utf8` / `c1_8bit_used` / `invalid_utf8` | 未识别序列计数（UI 提示与回放断言的可见面） | 计数键为契约；新增键走 minor |
| kernel/02 | `PtyError` | `Spawn{errno,stage}` / `Io` / `NoSuchPty` / `JobAttachDenied` / `SignalUnsupported` / `Timeout{pid,after}` / `TreeNotEmpty{live}` / `Unsupported(&str)` | PTY 启动 / 进程树 / 信号错误 | 内核内部契约；跨 IPC 时映射为 `ErrorBody` |
| kernel/02 | `SignalOutcome` / `ResizeEffect` | `Delivered` / `ByteFallback(u8)` / `Unsupported`；`Applied` / `AppliedLossy` / `LocalOnly` | 信号与 resize 的降级结果 | 与 `TransitCaps` 声明的一致性由 PTY-AC-06 断言 |
| kernel/04 | `SessionError` / `LeaseError` / `LogError` / `SubscribeError` | trait 签名错误类型（未逐值冻结） | 会话 / 租约 / 日志 / 订阅 | 对外暴露的新变体走 major；内部新增走 minor |
| kernel/04 | `StateChange.reason` | `spawn_failed` / `unrecoverable` / `reaped` / `closed` | 状态迁移原因 | Log 事件字段；新增原因走 minor |
| kernel/07 | `AuthzError` → `code` | `TERMAI-E-AUTHZ-{MISSING\|EXPIRED\|REVOKED\|SCOPE\|TAINT\|APPROVAL\|POLICY\|MALFORMED}` | capability 校验失败 | 对外契约；每次决策恰好一条 `AuditRecord` |
| kernel/07 | `IpcError` | `ReservedNonZero` / `FrameTooLarge` / `CrcMismatch` / `EncodingMismatch` / `Corrupt` / `NeedMore` / `UnsupportedMsg` / `ReservedMsgType` | 帧层与消息层错误 | 帧层错误断连，消息层错误不断连（§3.2） |
| kernel/07 | 握手 `reason` | `VerUnsupported` / `CapUnknown` / `PeerDenied` / `HandshakeReplay` / `HandshakeTimeout` | 握手失败分类（CLI 退出码见 §3.3） | 数值即契约；新增走 minor |
| kernel/07 | JSON-RPC 错误码 | `-32030` | Local API capability 拒绝 | 对外契约（`code` 字段结构化 + 可操作提示） |

**缺口说明（kernel/03 / 05 / 06：显式「暂不登记」+ 理由 + 触发条件，L-18）**

登记规则：任一分册的错误枚举 / 诊断码一旦经 IPC 或 Local API 对外暴露（或需要跨进程传输、序列化、UI 分支判定），必须**先补入本表并冻结数值与语义，再实现暴露**；未登记即暴露视为契约事故（§3.7「数值即契约」，已发布数值永不复用）。

| 来源分册 | 现状（正文可见的错误面） | 处置 | 暂不登记的理由 | 触发条件（满足任一即先登记再冻结） |
| --- | --- | --- | --- | --- |
| kernel/03 | 正文未定义独立错误枚举；失败面以 FrameState 降级（Idle / Occluded / Suspended）、DeviceLost 重建与 RP-13 / RP-14 断言承载（03 §3.6 / §5） | **暂不登记** | 无字段级数值码，也不跨进程回传；渲染失败以「降级模式 + 断言」表达，登记为错误码会造出无消费者的契约 | ① GPU DeviceLost / 降级原因需经 IPC 回传 UI 并触发分支提示时；② RP-14 的降级等级需要结构化错误载体时 |
| kernel/05 | `InputError`（`InputSink::write`，05:74）；`ClipboardError`（`ClipboardBroker::read` / `write`，05:292–293）；`EncodeOutcome::Dropped(DropReason)`（05:75） | **暂不登记** | 均为 crate 内部 trait 返回类型，未定义稳定数值、未做跨进程序列化；其对外入口 `clipboard.get` / `clipboard.set` / `input.inject` 在 05 §4.3 仍是「提案，需 ADR」，批准前不对外 | ① `clipboard.*` / `input.inject` 经 Local API 或 IPC 暴露时；② `InputError` / `ClipboardError` 需跨进程传输或进 UI 结构化提示时 |
| kernel/06 | `BenchError`（06 §4，termai-bench 仪器库）；`Verdict` / `FailKind`（06 §4） | **暂不登记** | termai-bench 是 test-only 叶子库，**禁止被任何出货 crate 依赖**（06 §4、DC-21），其返回值永不进入 IPC / Local API，不存在跨进程契约 | 基准结果需经 Local API / IPC 暴露给 CLI / IDE / CI 客户端（如 `bench.report`）时，`BenchError` 与 `Verdict` 编码先登记 |

## 4. 接口与依赖

### 4.1 接口边界矩阵（谁 × 资源 × 权限）

图例：`R` 只读 ｜ `W†` 写且必须持 capability 令牌 + 审计 ｜ `R*` 只读且经裁剪/脱敏 ｜ `L†` 以租约授权 ｜ `–` 禁止。

| 资源 \ 主体 | termai-desktop (UI) | termai-agent | plugin-host（含 WASM 插件） | Local API 外部客户端（CLI/CI/IDE） |
| --- | --- | --- | --- | --- |
| PTY 字节流（读） | R（本窗格） | R*（订阅裁剪） | –（只给 scrollback 镜像，非字节流） | R*（`log.query`，脱敏） |
| PTY stdin（写） | W†/L†（持租约的人类输入） | W†（`stdin.write`，AR-03 显式授权） | –（AR-07 永禁） | W†（`pty.write`，无 TTY 下 L2/L3 拒绝） |
| Grid snapshot / delta | R（+W† 仅布局/selection，不经 IPC 写网格） | R* | R*（只读镜像 + 显式注入队列） | R*（`grid.snapshot`，带 epoch） |
| Scrollback 全文 | R（本地渲染） | R*（按订阅裁剪） | R*（镜像，脱敏） | R*（`log.export`，默认脱敏） |
| Session Log / Context 事件 | R | R（只读订阅，AR-03） | R（仅 Tier1 扩展点） | R*（`context.*`） |
| cwd / env / git 元数据 | R | R*（脱敏后） | – | R* |
| 命令边界与退出码 | R | R | R（OSC 133/633 只读订阅） | R |
| Config | R（`config.get`）/ W† | –（不持配置真相） | – | R* / W†（`config.write`，走 patch 校验） |
| secret（keychain） | –（只见 handle UI） | –（只见 `secret://handle`，AR-12） | –（Untrusted 禁 secrets，OQ-12） | – |
| 出站网络 | –（不出站） | W†（`net.connect:host`，过分类闸门） | W†（经宿主代理，逐目的地） | – |
| Tool invoke（shell/fs） | – | W†（Policy Engine 二次校验，DC-27/35） | W†（`ai.tool.register` + 用户审批） | W†（`ai.tool.invoke`） |
| Plugin 能力 grant/revoke | W†（用户确认后） | –（无 grant 权） | –（不可自我扩权） | W†（`plugin.grant`，持确认） |
| Audit log | R（`audit.tail`） | R*（写自己的记录） | –（写宿主代传） | R†（`audit.export`，未脱敏需管理员二次确认） |
| stdin 单写者租约 | W†（获取/转移） | W†（须显式授权） | – | W†（须显式授权） |

### 4.2 授权前提（写操作的准入条件）

| 写操作 | 前提 | 依据 |
| --- | --- | --- |
| 人类键盘写 stdin | 持有效 lease（generation 匹配）；lease 获取/转移需 capability + 审计 | AR-03、OQ-A1 |
| agent 写 stdin | `stdin.write` 令牌 + 会话范围匹配 + 审计；跨会话写风险上调一档（L1→L2、L2→L3）且不得被 rule 固化 | AR-03、AR-06、AR-22 §5 |
| plugin 写任何终端输出 | **永不开放**（Tier3 永禁） | AR-07、DC-39 |
| 任何 L2/L3 动作 | 每次 dry-run + 确认；L3 追加二次输入目标名 + 不可回滚徽章；不可由配置关闭 | AR-06 |
| 配置写 | `config.write` + patch 校验；安全策略不适用 CLI/env 覆盖（取严） | AR-09、DC-35 |
| 能力 grant | 仅 UI/CLI 持用户确认；插件令牌无此权限 | 06-spec §3.9 |

### 4.3 对内模块契约与依赖红线

| 提供方 | 消费方 | 契约 | 变更规则 |
| --- | --- | --- | --- |
| `termai-ipc` | UI / sessiond / agent / plugin-host / headless | 帧格式 + 握手 + 能力集 + shm 布局 | 版本治理 + 24h fuzz（DC-37）；破坏性走 capability 协商 |
| `core-dto`（IDL） | 全部模块 | Context 事件 schema + 双端类型 | 兼容 ≥2 minor；Rust/TS 双生成 |
| `termai-core::capability` | sessiond / plugin-host / local-api / agent（消费端） | `require()` 纯函数 + `EffectivePolicy` | 字段集冻结；新增 `act` 走 capability 词表 ADR |
| `termai-session` | UI / agent / plugin-host | `IpcPtyEntry` / `LeaseManager` / `LocalApiBroker` | 唯一入口，禁止旁路（CI 断言） |

依赖红线：`core ← session ← { agent, plugin-host }`（DC-21）单向无环；`termai-ipc` 不得依赖 session/agent/UI；`apps/*` 不得被任何库依赖。

## 5. 可验证验收（测量方法、语料或工具、判据、CI 落点）

| AC | 测量方法 | 语料 / 工具 | 判据 | CI 落点 |
| --- | --- | --- | --- | --- |
| IPC-AC-01 帧层鲁棒 | 对 `decode()` 持续 fuzz（长度、rsv、CRC、超长、分块乱序） | `cargo-fuzz` 目标 `ipc_decode` + `ipc_ring_consume`，种子取自真实流量 | 24h 无 crash、无 OOM、无越界（miri 通过） | §8.1-6（与 VT/PTY 同门禁） |
| IPC-AC-02 握手矩阵 | 枚举 `{proto_min,proto_max} × caps × client_kind` 全组合属性测试 | proptest + 预置 6 个失败路径样本 | 每个失败路径返回**规定 reason 码**且不断连策略正确；反向断言：unknown required cap 100% 拒绝 | PR（`termai-ipc` 契约测试） |
| IPC-AC-03 能力校验不可绕过 | 对 CAP-1…CAP-7 共 7 个强制检查点各写越权用例；静态搜索第二实现 | 单元 + `cargo xtask arch-check --authz-single-entry` | 越权 100% deny + 100% 有审计记录；`require()` 之外无令牌判定 | §8.1-6、§8.2（安全） |
| IPC-AC-04 审计链与 fail-closed | 单字节篡改检测 + 审计队列灌满测试 | 篡改用例集 + 队列注入 | 单字节篡改 100% 可检出；队列满时 L1+ 动作 100% deny 且不产生「无审计的执行」 | §8.2（可靠性/安全） |
| IPC-AC-05 共享内存安全 | 对抗性槽内容 fuzz + 只读映射权限断言 + 崩溃 epoch 用例 | `fuzz_ring_consume`、`/proc/self/maps` 断言禁 `PROT_EXEC`、插件侧写入必失败 | 攻击性槽 100% 被拒或安全忽略；插件写入尝试 100% 失败；旧环永不复活 | §8.1-6 |
| IPC-AC-06 Context schema 兼容 | 跨版本向量矩阵（N、N-1、N-2 双向） | `crates/core-dto/vectors/v*/` golden vectors + `xtask ctx-compat --matrix` | N-2 reader 读 N writer 成功率 ≥95%；未知 tag 注入 decode 成功且有限速审计 | PR + 发版门禁（DC-40 口径） |
| IPC-AC-07 未知消息处理 | 构造已知区段未知类型 / 保留区段 / 帧层错误三类 | 协议一致性用例 | 前两类回 `Error` 且不断连；仅帧层错误断连 | PR |
| IPC-AC-08 内核不依赖 AI/网络/UI | `cargo metadata` 传递闭包 + rustdoc JSON 公共项扫描 | `cargo xtask arch-check --kernel-purity` | 禁用集 ∩ 内核 crate 闭包 = ∅；`--no-default-features` 可构建 | PR（`-D warnings` 同级） |
| IPC-AC-09 单写者租约 | 3 客户端并发抢写 stdin + 转移/超时/吊销用例 | 并发集成测试 | 任一时刻写者唯一；转移必须显式且留审计；无「后到先赢」 | §8.2、OQ-A1 关闭前置 |
| IPC-AC-10 降级路径 | 屏蔽 shm / 移除能力 / 旧版本客户端 | 环境矩阵 | 按 §3.6 表进入对应模式，且 L0/L1/L3 功能与 §5 性能门禁不劣化 | 发版门禁 |
| 【新增】N-1…N-8 | 见 §8.1，**在 TSC 确认前不作为门禁** | 时序探针 / `termai-bench` | 同 §8.1 判据 | 待定 |

## 6. 风险与降级

| # | 风险 | 触发条件 | 缓解 | 降级 |
| --- | --- | --- | --- | --- |
| I1 | 帧协议复杂度与 fuzz 面扩大（03-spec A5） | 协商失败或畸形帧导致崩溃 | 单一帧格式 + rsv 哨兵 + 严格长度上限 + 24h fuzz | 断开该客户端，sessiond 不受影响 |
| I2 | 审计 fail-closed 伤可用性 | 审计后端 I/O 失败或队满 | 有界队列 + 背压显式化 + 人类输入走租约语义（K-05） | L1+ 动作 deny 并 UI 明示；人类输入继续可用 |
| I3 | 共享内存被当作逃逸通道 | 插件/恶意进程尝试写环或按名 attach | §3.5 八条约束 + 只读映射 + 句柄传递 + 权限测试 | 映射失败即拒绝该客户端，不降级为「用文件代替」 |
| I4 | Context schema 演进致消费方崩溃 | 新 minor 引入未知 tag | V1–V6 + reader 忽略未知 + golden vectors | 消费方跳过未知事件并告警，会话继续 |
| I5 | capability 校验点被旁路（03-spec A10） | 新增代码直接操作 PTY/FS | newtype 私有构造 + 单入口静态检查 | PR 阻断；运行时审计异常告警 |
| I6 | 多端 attach 环数膨胀超内存预算 | 客户端数 N 增大 | `limits.shm_clients` 默认 4 + 超出退化控制通道分块 | 新客户端进入 `NoShm` 模式（§3.6） |
| I7 | 人类输入延迟被审计/校验侵蚀 | 有人把审计移入热路径 | IPC-AC-03 + 【新增 N-2】监控 | 回退到租约语义并追责变更 |

**降级模式表（§3.6 引用）**：`Full`（控制 + 数据 + shm + Context + 能力面）→ `NoShm`（控制 + 数据分块，性能门禁须仍达标）→ `ReadOnly`（无 stdin/lease，只读订阅 + UI 渲染）→ `Refused`（版本区间无交集，拒绝连接并给结构化升级提示）。

## 7. 被否决的方案与反方意见

| 被否决方案 | 反方最强理由 | 我方反驳 | 复议条件 |
| --- | --- | --- | --- |
| 热路径用 MessagePack / JSON | 一套编码走全链路，生态工具最多，调试友好 | JSON 违反 DC-22；MessagePack 无标签类型，schema 演进靠位置，N-2 兼容做不干净 | 若 IDL 双后端被证无法维护，改单一 CBOR（仍拒绝 JSON） |
| 热路径用 Cap'n Proto | 零拷贝、schema evolution 一等公民、有 RPC 支持 | 需 schema 编译期固化 + 引入第二套 token 编码（05-spec 已定 CBOR），与「只维护一套二进制编码」冲突 | 若 `ENC=POD` 手写布局在 3 平台出现对齐事故，可局部引入 |
| 单一 MPSC 共享环承载所有客户端 | 环数少、内存省 | 多生产者需 CAS + ABA 防护，竞态与 fuzz 面远超收益 | 若 `shm_clients` 上限实测成为瓶颈，按 per-producer 子环扩展（仍是 SPSC） |
| DropOldest 丢未读数据 | 保证最新状态可见，语义等价于「只关心最新」 | 生产者推进消费者指针破坏 SPSC 不变量；DropNewest + Resync 语义等价且无竞态 | 无（在 SPSC 前提下不复议） |
| 逐键审计人类输入 | 审计最完整，取证无损 | 击穿 key-to-photon P99 ≤16ms 门禁，且违反「不采集内容」；租约即授权载体 | 合规出现逐键要求时：加「逐键计数 + 周期链摘要」，仍不记内容 |
| 令牌明文走帧 payload | 实现最简单，不需要 `SCM_RIGHTS` | 违反 05-spec §3.2.2「不跨进程暴露明文」；UDS 抓包/日志即泄露 | 无（安全红线） |
| 共享内存按名 open（`/dev/shm/termai-*`） | 跨平台实现一致、调试方便 | 同 uid 任意进程可 attach 并注入数据，等于把 shm 做成逃逸通道 | 无（安全红线） |
| 用 gRPC/HTTP2 做跨进程 | 有成熟流控、拦截器、多语言 | 违反 DC-22「热路径禁用 gRPC」；尾延迟不可控 | 仅可用于云控制面（AR-11），不得进入本地 IPC |
| Context 事件用「位置式」二进制（field order） | 编解码零开销、体积最小 | 无法安全增删字段，N-2 兼容不可能；tag 式才有未知字段跳过语义 | 无 |

## 8. Open Questions

### 8.1 新增可测指标（均标注【新增】；在 TSC 确认前不作为发布门禁）

| # | 指标 | 建议值（门禁 / 目标） | 测量 | 建议落点 |
| --- | --- | --- | --- | --- |
| N-1 | IPC 握手（本地 UDS）端到端 | P99 ≤2ms【新增】 | 时序探针 + 1000 次连接样本 | §5 新增一行 |
| N-2 | 输入帧 gate 附加延迟（租约校验路径） | P99 ≤20µs / P50 ≤5µs【新增】 | 埋点直方图（`termai-bench`），与无 gate 基线对比 | §5 并入 key-to-photon 归因 |
| N-3 | shm 单帧（64KiB）端到端投递 | P99 ≤100µs【新增】 | 环压测工具 | §5 新增一行 |
| N-4 | 大快照分块吞吐（≥1MiB 快照） | ≥1GB/s【新增】 | `termai-bench` | §5 新增一行 |
| N-5 | `capability::require()` 纯函数 | P99 ≤2µs【新增】 | criterion | §5 新增一行 |
| N-6 | 未知 tag/未知事件类型跳过 | 100%，零 panic【新增】 | 注入向量集 | 并入 IPC-AC-06/07 |
| N-7 | N-1/N-2 IPC 客户端对最新 sessiond 通过率 | ≥95%【新增，口径引 DC-40】 | 冻结客户端二进制矩阵 | 发版门禁 |
| N-8 | 共享内存驻留量（计入内存口径） | 单客户端 ≤16MiB，默认 4MiB；空闲时 `madvise(MADV_DONTNEED)` 后不计入【新增】 | `/proc/self/smaps` shmem 行 + RSS 采样 | §5 内存口径补充说明 |

### 8.2 协议与治理待决

| # | 问题 | 影响面 | 建议 | 必须决策的 Phase |
| --- | --- | --- | --- | --- |
| OQ-ABI-01 | 【差异-02】payload 编码取 CBOR + POD 双档（**已决：ADR-0018 D2**） | 对外契约（DC-22）、双端代码生成、fuzz 语料 | **已由 ADR-0018 追认：POD（固定布局、≤4KiB）用于热路径消息，CBOR 用于低频 / 演进型消息；ADR 已落地，IDL 可落编码** | **已决（ADR-0018）** |
| OQ-ABI-02 | 【差异-01】`crc32c` 由「可选」升为强制（**已决：ADR-0018 D3**） | 帧开销、fuzz 面、shm 一致性 | **已由 ADR-0018 追认：IPC 帧头校验 + 共享内存槽校验同语义、均为强制**；§3.1 帧头第 21–24 字节不变 | **已决（ADR-0018）** |
| OQ-ABI-03 | 共享内存驻留量是否计入 §5「空闲 RSS（1 万行）≤120MB」口径 | 性能门禁解释权、多端 attach 内存上界 | 采纳 N-8：空闲时必须 `MADV_DONTNEED`，计入上限单列 16MiB/客户端 | P1 |
| OQ-ABI-04 | 人类输入租约模式下「逐键审计」的合规底线（K-05 的代价） | 审计完整性、合规、性能门禁 | 租约级审计 + 逐键计数与周期链摘要；若合规要求逐键，则走 OQ-S9 式提案 | P1 |
| OQ-ABI-05 | `stdin` 单写者租约的抢占/超时/转移细节（OQ-A1 未决，本文只留帧位 0x0304–0x0307） | 协作安全、审计 schema | 显式 lease + 显式转移 + 审计事件，禁止后到先赢 | P5（P1 预留帧位） |
| OQ-ABI-06 | Local API 的 peer 鉴权信任边界（OQ-A2 未决）：同机多用户 / 容器 / CI runner 的 UDS 权限模型 | 安全、企业私有部署、headless CI | `0600` + peer credential 校验 + per-client capability 令牌 + 全量审计 | P1 |
| OQ-ABI-07 | capability 令牌跨设备/跨会话复用（OQ-S8）：远程 attach 时 token 的签发主体与句柄传递 | 远程 IPC 加密与鉴权边界（OQ-18） | 默认不可复用；远程新设限定主体；v1 只预留帧位与 `SEALED` 标志 | P5（P1 预留） |
| OQ-ABI-08 | 插件 AI tool 的 risk 分类裁决者（OQ-06-06）在 IPC 层的落点：`CapRefPresent` 是否携带作者声明的 risk | 「L2/L3 未审批执行 = 0」门禁、越权判定 | 作者声明 + 宿主静态校验，冲突取更高风险，作者不可下调；IPC 只传裁决结果 | P2 |
| OQ-ABI-09 | Tool ABI 是否同享 N-2 minor 兼容（OQ-AI-01）：若同享，Tool Envelope 的 `envelope_version` 需纳入本文的版本矩阵 | 第三方工具作者、IPC 事件面 | 同口径 N-2 minor + 6 个月弃用，纳入 `xtask ctx-compat --matrix` | P2 |
| OQ-ABI-10 | 版本区间的「地板」由谁定义与升降（本文默认 `s.proto_min = c.proto_min` 之上的交集算法） | 客户端兼容策略、安全更新强制力 | 地板由 sessiond 侧配置 + 签名清单下发，降地板须 ADR | P1 |

---

**变更记录**：v1 初版，自 03-spec §3.4/§3.5/§3.8 与 §4 展开；记录两条与 03-spec 的显式差异（【差异-01】CRC 强制、【差异-02】CBOR 编码），均已在 §8.2 挂 ADR 追认；新增可测指标 N-1…N-8 全部标注【新增】并集中列于 §8.1，未获 TSC 确认前不作为发布门禁。跨文档一致性对齐（AR-24…AR-29）：§范围外对 kernel/03 的 OQ 交叉引用由 `OQ-K7` 改为 `OQ-RND-07`；§3.4 校验点 `G1…G7` 改名 **`CAP-1…CAP-7`**（避免与全局 CI 门禁 G1–G8 撞号）；§3.6 `SessionLifecycle` 改为 kernel/04 六态并给出旧名→新名映射；§3.2 输入消息按 kernel/05 §4.3 统一为 **`Input` / `Paste`**；新增 **§3.8 跨分册错误码登记表**；编码（CBOR+POD）与 crc32c 强制、PtyBackend 九元面由 **ADR-0018** 追认（OQ-ABI-01 / OQ-ABI-02 关闭）。
