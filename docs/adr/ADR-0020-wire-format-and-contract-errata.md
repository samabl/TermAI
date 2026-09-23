# ADR-0020：实现期线格式与契约 errata（SD-01 至 SD-05）

- 状态：Accepted
- 日期：M0 交付期
- 决策者：项目总负责人（Orchestrator），依据 M0 交付期实现发现
- 关联：AR-04（热路径禁 JSON / 结构化事件总线）、AR-25 第 2 条（逐字节差分、禁止采样）、AR-28 第 1 条（接口面扩张按对外契约变更处理）、DC-22（termai-ipc）、DC-23（Session Log）、DC-40（兼容窗口 N-2 minor）、AR-20（诚实原则）
- 影响实现：kernel/07 §3.1、§3.3；kernel/04 §3.2.2、§4；kernel/05 §3.1；docs/plan/m0-spec-defects.md
- 前置：本 ADR 不修改任何 AR / DC 结论，只修正**分册正文中的实现不可落地表述**（属 AGENTS.md §5 的 errata 范畴）

## 背景与问题

M0（信任基座纵向切片）在落地 IPC 帧与 Session Log 段时，发现五处规格表述**无法按其字面实现**，若不登记，各实现者会各自发明线格式，直接破坏 DC-22 / DC-23 的跨进程与跨版本兼容。五处证据、复现与影响见 docs/plan/m0-spec-defects.md（SD-01 至 SD-05）。

核心矛盾可归纳为两类：

1. **伪代码与文字契约冲突**：kernel/07 §3.1 与 kernel/04 §3.2.2 用 repr(C) 结构体描述线格式，但同一节的文字契约（24 字节头、CRC 覆盖范围）与 repr(C) 的对齐规则互相矛盾。
2. **规则缺少承载字段/类型**：kernel/07 §3.3 的协商规则要求区分 required / optional 能力，但 Hello 只有一个 caps 列表；SessionId 的宽度在 kernel/04 与 kernel/07 之间未统一。

## 决策

### D1 线格式以「显式小端字节序」为唯一权威，禁止 repr(C) 直转（关闭 SD-01、SD-03）

- **IPC 帧头**：固定 24 字节，布局为 len:u32 / ver:u16 / msg_type:u16 / flags:u16 / rsv:u16 / corr_id:u64 / crc32c:u32（偏移 0/4/6/8/10/12/20）；crc32c 覆盖 header[0..20] 与 payload 的拼接。kernel/07 §3.1 中的 repr(C) 伪代码应予删除，或把 corr_id 拆为两个 u32 以保持真实 8 字节对齐。
- **Session Log 记录帧**：固定 24 字节头 len:u32 / type:u16 / flags:u16 / seq:u64 / ts_ns:u64，payload 紧随，crc32c 覆盖「type..payload」（即自偏移 4 起）。
- **理由**：被所有消费方实际依赖的契约是**字节偏移与数值**（FRAME_HEADER_LEN = 24、CRC 覆盖范围、rsv 非 0 即拒帧），不是 C 结构体布局。用 repr(C) 直转在 64 位平台上必然产生填充，从而与同一节的数值契约冲突。显式字节序同时消除了「不同编译器/目标平台布局漂移」这一类隐患。
- **代价**：编解码需手写，无法用 transmute 走捷径（本项目本来就 forbid unsafe，代价为零）；字段增删必须同步改编解码与测试。

### D2 Hello 增加 required 能力列表，HelloAck 回传未知 optional 项（关闭 SD-02）

- **决策**：Hello 追加 required: Vec<CapId>（客户端不可缺少的能力）；服务端 required 能力也必须由客户端声明，否则 Refused(CapUnknown)；未知 optional 能力从交集中静默移除、置 DEGRADED，并在 HelloAck 中以 unknown_optional 显式回传被移除项。
- **理由**：协商规则要求区分 required / optional，而单一 caps 列表不可区分；不回传被移除项则客户端无法解释自己为何处于降级态（违反 AR-20 诚实原则）。
- **代价**：属新增字段（minor），落在 DC-40 的 N-2 兼容承诺内；服务端需多维护一条对偶规则。

### D3 SessionId 统一为 128 位 ULID（关闭 SD-05）

- **决策**：termai_core::SessionId(pub u128)，字典序即时间序；termai-ipc 的 Hello.session_claim 使用该类型而非裸整数。
- **理由**：kernel/04 §4（会话域 owner）已明确 SessionId 为 ULID，且 Log 段内排序、attach 恢复与审计关联都依赖「字典序 = 时间序」；kernel/07 §3.3 未给宽度，按其实现会与 Log 不可互操作。
- **代价**：u128 使 IPC 与 Log 的会话字段各多 8 字节；换来的是跨模块类型统一（编译期即可发现不一致）。

### D4 叶子契约的 trait 参数必须写 &mut dyn Trait，且不额外施加 Send 超 trait（关闭 SD-04）

- **决策**：InputEncoder::encode 的第三个参数写成 &mut dyn InputSink；InputEncoder 不声明 Send 超 trait，Send 需求由上层持有者在其类型上声明。
- **理由**：&mut InputSink 不是合法 Rust 类型；而给叶子契约强加 Send 会把实现者的约束面扩大，却无对应收益（跨线程需求属于使用侧）。
- **代价**：上层若要把编码器跨线程搬运，需自行保证其类型 Send。

### D5 本 ADR 不修改任何分册正文

- **决策**：本 ADR 只登记 errata 与实现期权威解释；分册正文的修订作为**独立文档 PR** 执行，且必须逐条引用本 ADR。
- **理由**：AGENTS.md §5 的「不修改历史」纪律：变更需可追溯，实现不得以「反正代码是对的」为由静默改写设计文档。

## 后果

**正面**：线格式不再依赖编译器布局，四个模块（ipc / session / (未来) 插件镜像 / 多端 attach）有了同一份可实现契约；五处缺陷都有编号与处置，后续评审可逐条核对。

**负面**：kernel/07 与 kernel/04 需要一次正文修订（删除 repr(C) 伪代码、补 required 字段、补 SessionId 宽度注记、修 trait 签名）；修订前，**实现以本 ADR 与 docs/plan/m0-spec-defects.md 为准**。

## 反方记录与复议条件

- **反方（对 D1）**：有人认为保留 repr(C) 更贴近实现、可读性更好，只需把 corr_id 拆成两个 u32 即可两者兼得。**接受该修补作为等价方案**（D1 已写出），但仍要求 CI 断言 FRAME_HEADER_LEN 与字段偏移，避免「结构体定义变了但没人发现」。
- **反方（对 D3）**：u64 足以为单机 7×24 会话计数，u128 有过设计之嫌。**否决**：这不是容量问题，而是跨模块类型一致性问题；kernel/04 已按 ULID 裁定，反向修改会话域结论的代价高于 8 字节。
- **复议条件**：任一决策的变更需 TSC 2/3 多数加 90 天公示（AGENTS.md §5）；D2 若被判定为破坏性变更，则必须走 capability 协商而非 minor（DC-40）。

## 关联决策与实现位置

- 实现：crates/termai-ipc/src/frame.rs（D1 IPC 头）、crates/termai-ipc/src/handshake.rs（D2）、crates/termai-session/src/log.rs（D1 Log 记录帧）、crates/termai-core/src/lib.rs（D3）、crates/termai-core/src/input/mod.rs（D4）。
- 证据与复现：docs/plan/m0-spec-defects.md。
- 待办：docs/adr/README.md 索引补登 ADR-0019 与 ADR-0020；HARNESS §11.2 记 CR-12。
