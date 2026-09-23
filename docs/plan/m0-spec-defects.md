# M0 交付期发现的规格缺陷登记（SD-01 起）

> **性质**：本文件是**实现期发现**，不是设计权威。每条给出证据、我方处置与需要的追认动作。
> **纪律**（AGENTS §5）：修正既有结论必须走 ADR；本文件只做**登记**，处置落在代码注释与本表。

## SD-01｜kernel/07 §3.1 的 repr(C) 帧头不可能是 24 字节

- **证据**：该节同时声明 FRAME_HEADER_LEN = 24、crc32c 覆盖 header[0..20] 与 payload 的拼接，并以 #[repr(C)] 给出字段顺序
  len:u32, ver:u16, msg_type:u16, flags:u16, rsv:u16, corr_id:u64, crc32c:u32。
  按 repr(C) 布局：corr_id:u64 在偏移 12 处无法 8 字节对齐，编译器插入 4 字节填充，结构体尺寸变为 **32 字节**，
  且 crc32c 落在偏移 24 而非 20。
- **冲突点**：字段表把 rsv 放在偏移 10（注释称「用于补齐 8 字节对齐」），但 10+2=12 并非 8 的倍数；该注释与 repr(C) 规则互相矛盾。
- **真正被依赖的硬契约**是三个数值：FRAME_HEADER_LEN = 24、CRC 覆盖 header[0..20] 与 payload、rsv != 0 即拒帧。
- **本 M0 处置**：crates/termai-ipc/src/frame.rs **不使用 repr(C)**，改为显式小端逐字节编解码，权威布局：
  0..4 len / 4..6 ver / 6..8 msg_type / 8..10 flags / 10..12 rsv / 12..20 corr_id / 20..24 crc32c。
  测试已锁定 corr_id 与 crc32c 的字节偏移。
- **需要的追认**：新开 ADR 修正 kernel/07 §3.1（删除 repr(C) 伪代码，或把 corr_id 拆为两个 u32 以保持真实对齐）。
  **追认前以本处置为准**。

## SD-02｜kernel/07 §3.3 的 required 能力规则缺少承载字段

- **证据**：协商算法要求「客户端声明但服务端未知的 required 能力必须显式拒绝」，且「未知 optional 静默移除并置 DEGRADED」，
  但同节的 Hello 只有一个 caps: Vec<CapId>，HelloAck 也没有回传被移除项的结构。
- **后果**：required / optional 不可区分，规则不可实现；实现者会各自发明字段，造成跨端不兼容。
- **本 M0 处置**：crates/termai-ipc/src/handshake.rs 为 Hello **追加** required: Vec<CapId>，
  并在 Outcome::Negotiated 回传 unknown_optional。属**新增字段（minor）**，落在 DC-40 的 N-2 兼容承诺内。
- **需要的追认**：ADR 把该字段并入 kernel/07 §3.3，并明确「服务端 required 能力也必须由客户端声明」的对偶规则（本 M0 已实现并测试）。

## SD-03｜kernel/04 §3.2.2 记录帧存在与 SD-01 同源的布局陷阱

- **证据**：[len:u32][type:u16][flags:u16][seq:u64][ts_ns:u64][payload][crc32c:u32]；若按 repr(C) 落地，
  尾部 crc32c 与前缀会因 8 字节对齐产生填充；而 §3.2.2 又要求 len 只计 payload、CRC 覆盖 type..payload（即自偏移 4 起）。
- **本 M0 处置**：termai-session 用显式小端布局：记录头 = 24 字节（len:u32 / type:u16 / flags:u16 / seq:u64 / ts_ns:u64），
  CRC 覆盖 [4 .. 24+len]。文字契约不变，只是不允许用 repr(C) 直接转型。
- **需要的追认**：与 SD-01 合并写入同一份 ADR。

## SD-04｜kernel/05 §3.1 的 trait 签名不是合法 Rust，且 Send 约束未说明

- **证据**：fn encode(&mut self, ev: InputEvent, mode: &KeyboardMode, out: &mut InputSink) ——
  InputSink 是 trait，裸写不能作为参数类型；应为 &mut dyn InputSink。同处声明 pub trait InputEncoder: Send。
- **本 M0 处置**：使用 &mut dyn InputSink（保持对象安全）；InputEncoder **不加 Send 超 trait**，
  理由：叶子契约保持最小约束，Send 需求由上层持有者在其类型上声明，避免给所有实现者强加约束。
- **需要的追认**：errata 记录这两点，避免实现分叉。

## SD-05｜SessionId 宽度在 core 契约与 kernel/04 之间不一致

- **证据**：kernel/04 §4 明确 `pub struct SessionId(u128);  // ULID，排序友好`；
  而 kernel/07 §3.3 的 `Hello.session_claim: SessionId` 未给宽度。若两处按不同宽度落地，
  attach 帧与 Log 的会话标识会不可互操作。
- **本 M0 处置**：以 **kernel/04（会话域 owner）为准**，`termai_core::SessionId(pub u128)`；
  `termai-ipc` 的 `session_claim` 改为该类型（而非裸 u64）。已由编译期类型统一。
- **需要的追认**：kernel/07 §3.3 补注 `SessionId = termai-core 的 u128 ULID`。

## SD-06｜CI 门禁编号口径在 HARNESS / AGENTS / spec 07 / ci.yml 之间不一致（登记性缺陷）

- **证据**：HARNESS §8.1 与 AGENTS §4 称「CI 六件套」（不编号）；spec 07 §3.4.1 编号 G1–G6 之后又把 **G7（设计静态）/ G8（设计浏览器）** 列为合并阻断，实为 8 项；ci.yml 头注释与 design 作业名写「S1-S9 / B1-B9」，而 tools/design-gates 实际执行 **S1–S10 / B1–B10**（kernel/00-index §4 亦写 S1–S10 / B1–B10）。
- **影响**：不改变任何门禁语义，但会让评审与实现者按错误的编号集合核对覆盖度（例如误以为设计静态门禁只有 9 项）。
- **本 M0 处置**：不改门禁语义，只统一表述——登记为 **HARNESS §11.2 CR-13**；ci.yml 注释已更正为 S1–S10 / B1–B10。
- **需要的追认**：spec 07 §3.4.1 的 A2 行补注「六件套 = §8.1 的 6 条主题，不等于 6 个 G 编号；G 编号空间含 G7/G8」。
- **类型**：登记性（registry）而非线格式/契约缺陷，因此**不需要 ADR**，走 CR 登记即可。

## SD-07｜attach 协议缺少 msg_type 数值（数值即契约无法落地）

- **证据**：kernel/04 §3.4 定义了 ATTACH_REQ / ATTACH_ACK / TAIL_REPLAY / DETACH_NOTICE / GRID_SNAPSHOT / GRID_DELTA / LEASE_* 七类消息，kernel/00-index §3 只给出 LEASE_* 的数值（0x0304–0x0307）；kernel/07 §3.2 的 msg_type 表**没有** ATTACH_REQ / ATTACH_ACK / TAIL_REPLAY / DETACH_NOTICE 的任何取值。
- **后果**：kernel/07 §3.2 明确「数值即契约，永不复用」，因此实现者无法合法地自行取值——自行发明数值会造成跨端不兼容，且违反 §3.8「未登记即暴露视为契约事故」。
- **本 M0 处置**：不发明数值。以**已有已登记数值**表达 M0 的 attach 子集：
  只读 attach = 客户端发 GRID_SNAPSHOT(0x0180) 空载荷请求，服务端按 corr_id 回分块 GRID_SNAPSHOT（ENC=CBOR + MORE_CHUNK/LAST_CHUNK）；
  取写权 = LEASE_ACQUIRE(0x0304) → LEASE_GRANT(0x0305) 或 CAP_DENIED(0x0302)。
  TAIL_REPLAY、DETACH_NOTICE、LEASE_TRANSFER 的线上语义**不在 M0**（M0 只做到「快照 + 租约 + 输入」）。
- **需要的追认**：新 ADR 为 attach 族分配 msg_type 值并冻结（建议单列 0x0500 段），否则 P1 的 tail replay 无法实现。

## SD-08｜M0 实现期发现的次级契约缺口（上限口径 / 组合字符 / vte 上游差异 / OSC 52 语义）

### SD-08.1 OSC 与 DCS 长度上限口径不一致

- **证据**：kernel/01 的 OQ-VT-01 采纳值为 OSC 1 MiB / DCS 16 MiB / SOS-PM 1 MiB；M0 的实现（更保守）为 OSC 64 KiB / DCS 1 MiB，并通过 `BackendCaps` 对外暴露。
- **处置**：M0 **保留更保守的值**（AGENTS §7.4：不确定时取更保守的安全默认值），因为 OQ-VT-01 仍处开放状态，且上限经 caps 可协商；差异在此登记。**需要的动作**：errata 或 ADR 二者取一并冻结，禁止两个数字长期并存。

### SD-08.2 组合字符无法进入 GridSnapshot（真实能力缺口）

- **证据**：`termai_core::grid::Cell.ch` 是单个 `char`，组合标记（combining marks）只能存放在 Grid 的侧表中；因此 **snapshot / golden / digest 都会丢掉组合字符**。
- **影响**：直接触及复制保真（kernel/01 K-08、V-10、UX-G17「软换行开关下复制逐字节相同」）。这是**对外契约级缺口**，不是实现细节。
- **处置**：M0 记录为已知限制；**建议 ADR 为 GridSnapshot 增加 clusters 侧表**（新增字段，minor，落在 DC-40 兼容窗口内），并把 golden 的「组合字符附加在主字符后」规则改为可表达的形态。

### SD-08.3 vte 0.15 从 CsiIgnore 不派发（上游差异，kernel/01 §3.10 要求记录）

- **证据**：kernel/01 §3.3 规定非法字节「进 CsiIgnore；遇 0x40–0x7E 以 ignore=true 派发」，但钉定版本的 vte 0.15 在 CsiIgnore 状态下直接回 Ground，不派发。
- **处置**：实现改为预扫描器检测非法字节并计入 `csi_malformed`，同时单独统计 vte 自身的 ignore=true 情形；差异登记于本文件（符合 kernel/01 §3.10「记录上游差异」）。

### SD-08.4 OSC 52 语义首版过度保守（不合规，已派单修正）

- **证据**：AR-29 第 5 条 = 「OSC 52 **读默认 deny**；**写默认 guarded**（单行放行，多行或含控制字符需确认）；不允许静默多行」。
- **问题**：M0 首版把 OSC 52 一律记为 denied（读写同罪）。**过保守同样是不合规**：它会破坏合法的单行剪贴板写入。
- **处置**：已派单 WS-B 按 AR-29.5 修正为「读 deny / 写 guarded（单行放行 + 多行或控制字符需确认）/ 永不静默多行」，并同步计数键与测试。

## 处置总表

> 处置总览（SD-01…SD-07）。SD-06/SD-07 分别由 HARNESS §11.2 CR-13 与新 ADR 承接。

| 编号 | 分册 | 类型 | 本 M0 处置 | 需要动作 |
| --- | --- | --- | --- | --- |
| SD-01 | kernel/07 §3.1 | 线格式 | 显式小端 24B 布局 | ADR-0020 D1 |
| SD-02 | kernel/07 §3.3 | 契约缺字段 | Hello 追加 required | 新 ADR 并入分册 |
| SD-03 | kernel/04 §3.2.2 | 线格式 | 显式小端 24B 记录头 | 与 SD-01 合并 |
| SD-04 | kernel/05 §3.1 | 签名笔误 | &mut dyn InputSink；不加 Send | errata |
| SD-05 | kernel/04 §4 与 kernel/07 §3.3 | 类型宽度不一致 | SessionId = u128 ULID | kernel/07 补注类型来源 |
| SD-06 | HARNESS §8.1 / AGENTS §4 / spec 07 §3.4.1 / ci.yml | 门禁编号口径 | 统一为 S1–S10 / B1–B10；ci.yml 已更正 | CR-13 已登记；spec 07 A2 补注 |
| SD-07 | kernel/04 §3.4 与 kernel/07 §3.2 | 数值契约缺登记 | M0 只用已登记数值表达 attach 子集；不发明数值 | **已处置：ADR-0023 D1**（新增 0x05xx 段并分配四类消息；0x0504–0x05FF 保留） |
| SD-08.1 | kernel/01 OQ-VT-01 | 上限口径 | 保留更保守值并经 caps 暴露 | **已处置：ADR-0023 D2**（冻结为 AR-31 已采纳值 OSC·SOS·PM 1 MiB / DCS·APC 16 MiB；M0 保守值须撤回） |
| SD-08.2 | termai-core grid DTO | 对外契约缺口 | 记录为已知限制 | **已处置：ADR-0023 D3**（依 kernel/03 §3.3 正式冻结字段集，含 clusters: ClusterTable；golden/digest 必须覆盖） |
| SD-08.3 | kernel/01 §3.3 与 vte 0.15 | 上游差异 | 预扫描器 + 单独计数 | 已在本文登记（§3.10 要求） |
| SD-08.4 | AR-29 第 5 条 | 语义过保守 | 派单 WS-B 改为 guarded | 完成后回归测试 |
