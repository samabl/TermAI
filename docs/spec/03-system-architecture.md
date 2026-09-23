# 03 · 系统架构规格（System Architecture）

> **权威顺序**：HARNESS.md > docs/spec/00-glossary.md > 本文件。本文是 HARNESS §4「目标架构」的可实现化展开，只补细节、不新增结论。
> **引用约定**：决策一律以 AR-xx / DC-xx / OQ-xx 编号锚定，不复述措辞；与角色原文冲突处一律取 HARNESS。
> **同步义务**：任一条 AR/DC 变更后 24 小时内同步本文（HARNESS §12 维护约定 2）。
> **许可口径**：本文架构许可表述按 **AR-21 / ADR-0013** 对齐（全栈开源、Apache-2.0 OR MIT）；云控制面的**数据边界与隐私约束不变**（AR-11 / AR-12）。(ADR-0013)

## 1. 目的与范围

**目的**：把 HARNESS 的架构仲裁结论落成可编码、可 CI 校验、可验收的系统结构——进程边界、模块依赖方向、通信契约、存储分层、生命周期与恢复语义、云边界、可观测性。

| # | 范围内 | 主要依据 |
| --- | --- | --- |
| 1 | 进程与组件拓扑（UI / sessiond / agent / plugin-host / 云控制面） | HARNESS §4.1 |
| 2 | 渲染分层 L0–L4 与合成规则 | AR-01、AR-02、DC-17、DC-25 |
| 3 | 模块清单与依赖方向（crates / packages / apps） | DC-21、AGENTS §3 |
| 4 | 通信层 termai-ipc 与 Local API | AR-04、DC-22、DC-24 |
| 5 | 存储：Session Log / SQLite 派生索引 / CAS / 迁移 | AR-04、DC-23 |
| 6 | PTY/ConPTY 与 Transport 抽象（SSH / 容器 / WSL） | DC-16、DC-19、AR-18 |
| 7 | sessiond 生命周期、多端 attach、崩溃恢复 | AR-13、DC-04、DC-18 |
| 8 | 前端状态模型（Core 唯一权威 / UI 是 projection / 布局纯函数） | 05·D3/D4、AR-09 |
| 9 | 云控制面服务拆分与边界（控制面-only） | AR-11、AR-12、AR-21 |
| 10 | 可观测性（OTel trace 贯通、默认本地） | AR-12 |
| 11 | 关键时序：输入热路径 / AI 闭环 / 插件调用 / 跨设备同步 | AR-16、AR-06、AR-07、AR-11 |

**范围外**：产品信息架构与交互范式（产品/UX 规格）；VT 序列语义与兼容矩阵细节（04）；工具 ABI 字段语义与 eval 集（07）；威胁模型、脱敏规则库、审计合规口径（08）；token 具体值与动效曲线（设计系统规格）；构建产物、签名流程、发布列车（工程与发布规格）。
**用法**：实现者开工前须指出改动落在哪个 Phase、满足哪条 DC、受哪条预算（§5）与门禁（§8）约束；新增/删除模块、改变对外契约或数据边界必须开 ADR（AGENTS §5）。

## 2. 需求与约束（逐条引用）

| 类别 | 编号 | 对架构的强制含义 |
| --- | --- | --- |
| 进程边界 | AR-01 | 网格与关键动画由 Rust+wgpu 自绘，WebView 只承载 AI 面板/设置/插件视图 → UI 进程内必须多子表面合成，而非单栈 |
| 进程边界 | AR-03 | AI 产品上一等、进程上独立、契约上只读订阅+受控写入 → termai-agent 独立进程；内核不链接任何 AI SDK、不持有网络句柄 |
| 进程边界 | AR-07 | 插件宿主独立进程，WASM 为进程内子隔离 → plugin-host 不得与 sessiond 共享地址空间 |
| 进程边界 | DC-03 | Workspace > Tab > Pane(≤8)；三类「会话」禁止混称 → IDL 命名空间强制区分 PtySessionId / AgentSessionId / WorkspaceId |
| 进程边界 | AR-13、DC-18 | sessiond 为会话唯一真源，可多端 attach；UI 崩溃不影响会话进程 |
| 通信 | AR-04 | 跨进程走 termai-ipc（长度前缀 + 版本/能力协商）；热路径禁用 JSON/gRPC；结构化事件惰性生成、按订阅裁剪；schema 归 core-dto IDL；兼容窗口 ≥2 minor |
| 通信 | DC-22 | JSON-RPC 仅作调试与第三方入口，不得进入热路径 |
| 通信 | DC-24 | Local API = JSON-RPC over UDS/named pipe（含 apiVersion），同时服务插件、CLI --json、headless、CI、IDE；只维护一套契约 |
| 存储 | AR-04、DC-23 | Session Log（append-only 分段 + 校验和）为唯一真相；SQLite(WAL) 只作可重建派生索引；大对象走 CAS；格式版本 + 迁移器兼容 ≥2 大版本 |
| 存储 | AR-13 | 结构化会话元数据默认持久；完整原始输出 scrollback 持久默认关闭（可选加密） |
| PTY/Transport | DC-16、DC-19、AR-18 | PtyBackend 抽象；Windows 唯一生产路径 ConPTY（Job Object 管进程树，Win10 1809 地板）；原生 SSH（russh）与容器/WSL 统一为 Transport，禁止 shell out 系统 ssh；vte 作依赖置于 termai-vt trait 边界之后（非 fork） |
| 渲染 | AR-01、AR-02 | WebView 为可选运行时依赖；缺失或不可信时降级为纯原生文本面板（功能子集可用） |
| 渲染 | DC-17、DC-25、AR-14 | wgpu + rustybuzz/swash、多页字形 atlas、grapheme cluster shaping 再映射列；灰度 AA + 网格对齐 ≤0.5px；原生受限控件集 ≤20，绝不自研通用 UI 工具链 |
| 渲染 | AR-20 | UI 层承诺 a11y（WCAG 2.2 AA）；网格内 bidi 必须走 Unicode Bidi 算法 |
| AI/工具 | AR-06 | 风险分级 L0/L1/L2/L3/U；授权 once/session/rule；L2 每次 dry-run + 确认；L3 每次确认 + 二次输入目标名；确认不可由配置关闭 |
| AI/工具 | AR-05、DC-26/27/28/35 | 单 Agent 主循环 + 受控子 Agent 委派；工具 ABI 固定字段集；shell 执行在独立 watchdog + AST 预分析，解析失败按更高风险处理；MCP 独立进程沙箱 + Policy Engine 二次校验；capability 默认拒绝 |
| AI/工具 | DC-33、DC-34 | 脱敏在 Context Builder 内完成，失败默认拒绝发送；审计 append-only + 本地哈希链，可导出 SIEM |
| 插件 | AR-07、AR-08 | 唯一 ABI = WIT + WASM Component；Tier1 声明式 view schema；Tier2 隔离 iframe 待红队门禁（逃逸 0 + CSP 拦截 100%）；插件只有只读镜像 + 显式注入队列 + 声明式装饰 |
| 插件 | DC-38、DC-39 | 宿主独立进程 + 每插件独立 wasm store + watchdog，3 次崩溃进 safe mode；宿主按需启动，空载 ≤80MB；扩展点三级 |
| 云 | AR-11 | 控制面-only；PTY 流/文件内容/命令输出/密钥材料永不进服务端；AI 网关只路由与计量；服务端 prompt/completion 字段 = 0；同步仅 CRDT 小文档；传输 Connect + SSE，WS 仅双向协作 |
| 云（许可） | AR-12、AR-21 | 遥测默认关闭（opt-in）、内容零采集、所有出站可分类可审计；客户端/服务端/SDK/协议/工具链统一 **Apache-2.0 OR MIT**（全栈开源、无闭源云端模块；不用 BSL/SSPL），链接边界 GPL/AGPL/SSPL = 0（弱 copyleft 准入见 ADR-0015）(ADR-0013) |
| 性能 | AR-19、**AR-24** | 预算统一为「发布门禁 / 目标值」双列口径（§5），最坏场景以门禁为准；**RSS 口径 = 核心进程组（sessiond + 原生 UI），不含 WebView（L2）与插件宿主**——二者各自独立记账（WebView 就绪后增量 ≤60MB、插件宿主空载 ≤80MB），且**必须同时报告总内存**（不允许只报核心组） |

## 3. 详细设计

### 3.1 进程与组件拓扑（HARNESS §4.1）

    ┌─ termai-desktop (UI 进程) ─────────────────────────────┐
    │  L0/L1/L3 原生表面(Rust+wgpu)  L2 WebView  L4 平台原生  │
    └───────▲───────────────────────┬────────────────────────┘
            │ termai-ipc (二进制+版本/能力握手) │ Local API
    ┌───────┴───────────────────────▼────────────────────────┐
    │ termai-supervisor (每用户单实例：看护与重启)             │
    │  ├─ sessiond     会话唯一真源: PTY·VT·Grid·Scrollback    │
    │  │               Transport: Local|SSH|Container|WSL      │
    │  ├─ termai-agent 单主循环·Context Builder·Policy·Router │
    │  ├─ termai-tools 工具 ABI + shell watchdog(AST 预分析)   │
    │  └─ plugin-host  WASM Component store + watchdog         │
    └───────┬───────────────────────┬────────────────────────┘
            │ Session Log(唯一真相)  │ 派生
    ┌───────▼─────────┐  ┌──────────▼────────┐  ┌────────────┐
    │ Log 分段+校验和  │  │ SQLite 索引 + 审计 │  │ CAS(BLAKE3)│
    └─────────────────┘  └───────────────────┘  └────────────┘
            │ 仅出站、可审计、默认 BYO-key 直连
    ┌───────▼────────────────────────────────────────────────┐
    │ 云控制面(可选): Auth/OIDC·Entitlement·Sync(CRDT)        │
    │ AI Gateway(路由+计量)·Plugin Registry·Telemetry·Billing │
    │ ✗ 不持有 PTY 流 / 文件 / 输出 / prompt / 密钥           │
    └────────────────────────────────────────────────────────┘

| 进程 | 生命周期 | 持有状态 | 禁止事项 |
| --- | --- | --- | --- |
| termai-desktop (UI) | 随用户会话 | 渲染表面、UI 临时态、输入焦点 | 不持有 PTY 真相、不解析 VT、不发 AI 出站请求 |
| sessiond（内含 Local API 监听器） | 常驻 7×24（用户级），supervisor 看护 | PTY 句柄、VT 状态机、Grid、Scrollback、Shell Integration、Log 写端 | 不链接 AI SDK、不持有网络句柄、不依赖 UI、无 TCP 默认监听 |
| termai-agent | 按需/常驻，watchdog 看护 | Agent Session、Context Builder、Policy Engine、Model Router | 不直接写 PTY（须显式授权）、不进 PTY→像素路径 |
| termai-tools + watchdog | 随工具调用 | shell 子进程树、AST 预分析结果 | 默认无网络、只读 FS 白名单、不转发 SSH agent |
| plugin-host | 按需启动，空载 ≤80MB | WASM store、插件注册表、能力令牌 | 不写 PTY/输出流、不做帧内绘制、不持 Node/原生句柄 |
| SQLite 索引 + CAS | 纯派生、可删除 | 索引、审计链、CAS 大对象 | 不是真相源；删除后必须可由 Log 重建 |
| 云控制面 | 可选、无状态 | 身份/权益/同步文档/元数据 | 不持有 PTY 流、文件、输出、prompt、密钥 |

**监督树**：OS → termai-supervisor（每用户单实例）→ { sessiond, termai-agent, termai-tools, plugin-host }。UI 是客户端而非父进程，故 UI 崩溃不导致会话进程树终止（AR-13）。

### 3.2 渲染分层 L0–L4 与合成规则

| 层 | 归属 | 内容 | 合成规则 |
| --- | --- | --- | --- |
| L0 网格 | Rust/wgpu | 字形、SGR、光标、选区 | 主 render target，damage 增量重绘；**禁止走 Web** |
| L1 外壳 | Rust/wgpu | tab、分隔条、状态栏、命令面板 | 同 target 二次绘制 |
| L2 面板 | 系统 WebView | AI 对话、设置、diff、插件视图 | 独立 OS 子表面，Core 定位裁剪；崩溃不影响会话 |
| L3 动画 | Rust | 分屏插值、面板滑入、光标闪烁 | 统一时钟；WebView 不得驱动关键动画 |
| L4 浮层/IME | 平台原生 | 右键菜单、tooltip、候选窗 | 最高 z-order；L2 不得覆盖 |

合成规则（编号即契约）：

1. **单窗口多子表面**：L0/L1/L3 共享同一 render target；L2 为独立 OS 子表面，由 Core 定位与裁剪。
2. **z-order 固定**：L0 < L1 < L3 < L2 < L4；WebView 覆盖候选窗等违规视为架构事故（AR-01）。
3. **指针接管**：跨 L0/L2 拖拽由 Core 接管指针，WebView 只上报命中区域，禁止直接操作网格。
4. **damage 传播**：PTY bytes → VT 状态机 → Grid diff → damage region → shape/atlas → GPU draw → present；L2 重绘不得触发 L0 全量重绘。
5. **降级路径**：WebView 缺失或不可信 → L2 退化为原生文本面板（功能子集可用），L0/L1/L3/L4 不受影响（AR-02）。
6. **帧预算**：4K@120Hz 网格帧时 <8.3ms、丢帧 <0.1%（§5）；未聚焦窗口不主动出帧。

### 3.3 模块清单与依赖方向（DC-21）

    crates/   termai-core(叶子契约) core-dto(IDL,叶子) tokens(叶子)
              termai-pty termai-vt termai-gpu termai-render termai-session
              termai-store termai-ipc termai-agent termai-tools
              termai-plugin-host termai-xtask
    packages/ plugin-sdk-ts plugin-ui-runtime webview-shell
    apps/     termai-desktop termai-headless termai-web(后续)
    plugins/  官方插件（独立版本线）

    core ← session ← { agent , plugin-host }
    term-render(→vt,gpu) → ui-native → shell-bridge → web-shell → plugin-ui-sdk
    apps/* 不得被任何库依赖

依赖红线（CI 强制，DC-21）：

1. 只允许单向依赖，禁止环；违规 PR 直接阻断（AGENTS §3）。
2. termai-core 不得反向依赖 session / agent / plugin-host；render 只依赖 vt 与 gpu。
3. tokens 与 core-dto 为无依赖叶子；IDL 变更须同时生成 Rust 与 TS 双端类型。
4. 前端链 L0→L1→L2→L3→L4 单向；禁止 L0/L1 依赖 L3/L4。
5. 新增 crate/package 必须在 ADR 中说明其依赖位置。
6. 校验：cargo-deny + crate 图检查 + ESLint 依赖规则 + 依赖图报告；链接边界 GPL/AGPL = 0。

**命名口径（HARNESS §11.2 CR-15）**：本节依赖图中的 term-render / ui-native / shell-bridge / web-shell / plugin-ui-sdk 是**层级角色名**，不是 crate 名；物理 crate 名以 termai-* 为唯一真源（spec 07 §3.1.1/§3.1.2、ADR-0019 D1）。第 116 行同时列出的 core-dto 与 termai-core 当前是**同一个物理叶子**（IDL/DTO 与叶子契约同体，ADR-0019 D1 + ADR-0023 D3）；拆分为独立生成 crate 需新 ADR，并须满足第 3 条的双端 Rust/TS 生成义务。

### 3.4 通信层 A：termai-ipc（DC-22、AR-04）

- **帧格式**：[len:u32][ver:u16][msg_type:u16][flags:u16][corr_id:u64][payload][crc32c 可选]；payload 由 core-dto IDL 生成（Cap'n Proto / MessagePack 由 ADR 定）；**热路径禁用 JSON 与 gRPC**。
- **编码裁定（AR-28 第 1 条，已由 ADR-0018 追认）**：采用 **CBOR + POD 双档**（POD 固定布局、≤4KiB，用于热路径消息；CBOR 用于低频与演进型消息；见 `docs/spec/kernel/07-kernel-api-abi.md` §3.1），且帧内 **crc32c 强制**（IPC 帧头校验 + 共享内存槽校验同语义）。二者属**对外契约变更**，已由 **ADR-0018（D2 / D3）** 追认并走 CODEOWNERS **双签**；kernel/07 的 IDL 可落编码。上一行的原措辞按本条注释理解，不再单独改写。
- **握手**：Hello{proto_range, capabilities[], session_id, client_kind} → HelloAck{chosen_ver, capabilities∩, limits}；不识别的能力必须显式拒绝而非忽略。
- **版本治理**：破坏性变更走 capability 协商，兼容窗口 ≥2 minor（AR-04），与 DC-40 的 N-2 minor + 6 个月弃用 + codemod 对齐。
- **通道**：控制通道（低延迟小消息）+ 数据通道（共享内存环形缓冲，传输网格 snapshot/delta 与批量字节）。
- **流控**：生产端额度 + 消费端 credits；每个 channel 的溢出策略（丢帧 / 阻塞 / 断开）必须显式声明，不得静默丢弃。
- **安全与错误**：仅本地 transport（UDS / named pipe）；跨主机加密与鉴权预留协议位但不默认启用（OQ-18）；错误一律结构化 code + 可操作提示。

### 3.5 通信层 B：Local API（DC-24）

JSON-RPC 2.0 over UDS（POSIX）/ named pipe（Windows），强制携带 apiVersion。**一份契约**同时服务插件、CLI --json、headless、CI 与 IDE 集成。

| 命名空间 | 代表方法 | 默认能力 |
| --- | --- | --- |
| session.* | create / attach / detach / list / close | 读写（受 capability 约束） |
| pty.* | write / resize / signal | 写须令牌 + 审计 |
| grid.* / log.* / context.* | snapshot / delta / subscribe / query / cwd / git | 只读，受脱敏规则 |
| tools.* | list / invoke / approve | 写，须 Policy Engine |
| plugins.* / config.* | list / capabilities / grant / get / set | 写须用户授权或 Core 校验 |
| audit.* / trace.* | query / export | 只读；audit 可导出 SIEM；trace 默认本地 |

约定：只读优先；写操作必须带 capability 令牌并写审计（DC-34、DC-35）；支持幂等键；列表一律 cursor 分页。禁止：默认无 TCP 监听、不暴露 secret、不因 API 调用进入 PTY→像素热路径。

### 3.6 PTY/ConPTY 与 Transport 抽象（DC-16、DC-19、AR-18）

- **PtyBackend trait**：spawn / resize / write / read / close / signal→事件 的跨平台统一面。生产实现：UnixPty（forkpty/openpty）与 ConPty（Job Object 管理进程树，Win10 1809 地板）；NativePty 仅作长期实验插槽，**非生产路径**。
- **接口面差异说明（AR-28 第 1 条）**：`docs/spec/kernel/02-pty-platform.md` §3.1 已把 PtyBackend 由上文 **6 元面**扩展为 **9 元面**（spawn / write / read / resize / signal / kill / wait / process_tree（含 tree_snapshot）/ capabilities；close 仍作清理面保留）。该扩展属**对外契约变更**，已由 **ADR-0018（D1）** 追认并走 CODEOWNERS **双签**；多出面的语义与签名以 kernel/02 §3.1 为准。
- **Transport trait**：Open / Read / Write / Resize / Close + 能力查询（resize、信号、图形协议透传）。实现四类：Local、SSH（russh 原生，兼容 known_hosts / agent / 跳板机 / 多类密钥）、Container（exec/attach）、WSL。**禁止 shell out 系统 ssh**。
- **语义差异**：ConPTY 会重编码输出、插入额外序列、resize 有损、无 POSIX 信号 → 事件层以 pseudo-console 事件替代信号，差异逐项登记在案（§8.1 xterm 兼容 ≥99%）。
- **会话状态机**：Creating → Running ↔ Detached → Exited / Crashed → Reaped；状态跃迁全部作为结构化事件写入 Session Log。
- **状态枚举口径（以 kernel/04 为准）**：本行旧命名与 `docs/spec/kernel/04-session-lifecycle.md` §3.1 的六态不一致，**枚举以 kernel/04 为准**（其 §8 OQ-SES-01），映射：`Creating → created`、`Running → running`、`Detached → detached`、`Exited → exited`、`Crashed → recovering`（显式可观测瞬态）、`Reaped → dead`（终态）。IDL / 审计 / UI 文案统一用新名。
- **VT 边界**：vte 作为基础 ESC 状态机置于 termai-vt 的 trait 之后（AR-18），锁定版本并记录上游差异；OSC 133/633、OSC 7、kitty keyboard/graphics、iTerm2 内联图、Sixel 与 grid 语义层自研。shell 集成失效时退化为提示符启发式并标记低置信度，不让 AI 误信。

### 3.7 存储：Session Log / SQLite / CAS / 迁移（AR-04、DC-23）

| 层 | 介质 | 内容 | 可重建性 |
| --- | --- | --- | --- |
| 真相层 | Session Log：append-only 分段文件 + 每段 CRC32C + 段索引 | 命令边界、退出码、cwd、耗时、时间戳、报错摘要、模式变更（原始字节的保留按下方三层语义） | 不可重建（唯一真相） |
| 派生层 | SQLite (WAL) | 检索索引、审计链、元数据、记忆索引 | 可完整重建，可删除 |
| 大对象层 | CAS（键 = BLAKE3） | 图片、大 diff、长日志、图形协议载荷 | 引用计数 + GC |

**保留语义（AR-26，三层）**：
- **P0 元数据层（默认持久）**：命令边界、退出码、cwd、标题、resize、审计引用——**不含原始输出字节**。
- **P1 原始字节恢复窗口（默认 volatile）**：仅在内存或临时盘保留有限窗口用于崩溃后屏幕重建，**默认不落长期存储**；与下方「持久化默认值」同口径。
- **完整历史归档（opt-in）**：用户显式开启才落盘，且必须可加密、可清除、可审计（AR-12）。

- **分段**：segment 达阈值（默认 8MiB）滚动；写入仅追加 + 顺序刷盘，不做就地更新；周期性 checkpoint 加速恢复。
- **惰性事件**：无消费者时结构化事件零开销；有消费者时按订阅裁剪（AR-04）。
- **持久化默认值**：结构化元数据默认持久；完整原始输出 scrollback 持久默认关闭，可选开启并可加密（AR-13、OQ-04）。
- **格式版本与迁移**：文件头含 magic + format_version + min_reader_version；迁移器单向 upgrade；**兼容读取 ≥2 个大版本**；不可读时只读降级 + 显式告警，绝不静默丢数据。
- **审计**：append-only + 本地哈希链，记录操作者（人/插件/模型）、审批人、结果，可导出 SIEM（DC-34）。

### 3.8 sessiond 生命周期、多端 attach 与崩溃恢复（AR-13、DC-18、DC-04）

- **生命周期**：用户登录 / 首启 → supervisor 拉起 sessiond → 会话按需创建 → 空闲回收（OQ-05：每用户单守护 + 每会话子任务）。
- **多端 attach**：N 个 UI 客户端 attach 同一 PtySession，广播网格 snapshot + delta；attach 时**先发 checkpoint 再追尾部段**以保证屏幕一致，断线重连幂等。
- **单写者原则**：stdin 写入者唯一（lease）；转移必须显式授予并写审计，不得依赖「后到先赢」（协议细节见 §7 OQ-A1）。

| 崩溃对象 | 语义 | 承诺 |
| --- | --- | --- |
| UI 进程 | sessiond 与用户进程不受影响 | <2s 幂等重连 + 屏幕一致（§8.2） |
| sessiond | supervisor 自动重启 | 由最后 checkpoint + 尾部段重建屏幕与结构化元数据 |
| 子进程 | 多数随 PTY master 关闭而终止 | **不自动重启、不自动重放**（DC-04） |

**诚实声明（AR-13）**：**屏幕可恢复，进程不可恢复**。该措辞必须在 UI 与文档中明示；恢复流程绝不自动重放命令或重启进程（DC-04）。

### 3.9 前端状态模型（Core 唯一权威，UI 是 projection）

| 层 | 位置 | 内容 |
| --- | --- | --- |
| 真相层 | Core | Session/Grid、Config、Agent Session、Plugin Registry |
| 传输层 | IDL-first | JSON Schema 生成 Rust+TS 类型；高频网格走二进制 snapshot + delta |
| projection 层 | UI store | 仅 UI 临时态（展开、滚动、草稿、hover）；细粒度 selector 订阅 |
| 视图层 | 原生 + WebView | 渲染与交互；禁止缓存网格真相 |

1. **单向数据流**：UI → patch → Core 校验合并广播 → UI 重投影；严格单向，仅允许 UI 级乐观更新。
2. **命令往返**：任何业务真相变更必须经 Core；UI 不得直写配置或会话状态。
3. **布局是纯函数** `(tree, constraints) → rects`：节点 Leaf(paneId) | Split(dir, ratio, a, b)；插入时同向折叠；稳定 ID 支持拖拽 reparent；浮层独立于树，用单层 z-order 数组；可单测、可无头快照、可动画插值。
4. **流式合帧**：AI 流式 token 按 ≥16ms 合帧后入 store，避免逐 token 触发渲染。
5. **配置**：TOML + drop-in + 层级合并 default < system < user < project < env < CLI（AR-09）；UI 只发 patch，绝不直写。
6. **a11y 与 i18n**：组件声明 role / 焦点序 / 键盘可达，双栈各自映射平台 a11y API（AR-20）；UI 层用 ICU MessageFormat，网格内 bidi 走 Unicode Bidi 算法。

### 3.10 AI 运行时闭环（AR-03、AR-05、AR-06）

组件：termai-agent（单 Agent 主循环 + 受控子 Agent 委派）、termai-tools（Tool ABI）、Policy Engine、Context Builder、Model Router。闭环步骤：

1. **订阅**：只读订阅 Session Log 结构化事件；Context Builder 采集/裁剪/压缩/排序，脱敏在此完成，失败默认**拒绝发送**并告知（DC-33）。
2. **选路**：Model Router 依任务类型/隐私等级/成本/延迟选本地或云端（本地分类 P95 <50ms；诊断首 token <3s）；BYO-key 一等公民，本地嵌入式向量库；离线只做规则解释（DC-29）。
3. **产出意图**：模型只产出结构化意图（tool + args），**禁止可执行命令字符串**（AR-06 规则 1）。
4. **校验**：Policy Engine 二次校验（capability + 风险分级 + 污点追踪）；shell 执行走独立 watchdog + AST 预分析，解析失败按更高风险处理（DC-27）；MCP 走独立进程沙箱（DC-28）。
5. **审批**：once/session/rule；L0/L1 可被规则固化；L2 每次 dry-run + 确认；L3 每次确认 + 二次输入目标名/短语 + 不可回滚徽章；**确认不可由配置关闭**（AR-06）。
6. **执行与审计**：确定性执行器构造 argv（禁 shell -c 拼接）→ 写审计（append-only + 本地哈希链）→ 写 Session Log → 观察结果回灌。
7. **回滚**：写操作前置快照；仅承诺工作区文件与 git 可逆操作，绝不承诺 OS/容器/远端回滚（AR-06）。
8. **输出**：经结构化 schema 渲染（Inline Hint / Command Explain / Diff Block / Diagnosis Card / Risk Ladder），禁止自由 markdown 或原始 HTML 直出（AR-16）。

### 3.11 插件宿主与能力边界（AR-07、AR-08、DC-38、DC-39）

隔离层级：plugin-host 独立进程 → 每插件独立 wasm store（WIT + WASM Component）→ 进程内子隔离 → 3 次崩溃进 safe mode；watchdog 看护；宿主按需启动，空载 ≤80MB（不计入核心 120MB 基线）。

| 维度 | 允许 | 禁止 |
| --- | --- | --- |
| 终端数据 | 只读 scrollback 镜像、显式注入队列 | 写 PTY / 输出流 |
| 渲染 | 声明式装饰（cell range + style token + z-order 枚举） | 帧内任意绘制 |
| 能力 | 经 Local API 的 capability 令牌 | Node / 原生句柄、静默网络访问 |
| UI（v1） | Tier1 声明式 view schema（提交 JSON，Host 渲染） | Tier2 iframe（待红队门禁后开放） |
| 形态 | WASM Component；trusted subprocess 须用户显式标记 | 第三方原生 dylib；进入市场普通分类 |

扩展点分级（DC-39）：Tier1 冻结开放 / Tier2 试验 / Tier3 永禁（帧内绘制、直改 VT、直写输出流）。插件与 Core 之间只经 Local API（DC-24），**只维护一套契约**。

### 3.12 云控制面服务拆分与边界（AR-11、AR-12、AR-21）

| 服务 | 职责 | 禁止 |
| --- | --- | --- |
| Auth/Identity | OIDC-first、设备码 RFC 8628、Ed25519 许可证 JWT 离线可验、企业 SSO/SCIM（均无状态） | 不自建账号体系（Anti-features 15） |
| Entitlement | 许可证与权益派生（无状态） | 不得阻塞客户端启动 |
| Sync | CRDT（LWW-Element-Set + 版本向量）小文档：设置/主题/片段/工作区元数据 | 不同步命令历史与 scrollback（默认） |
| AI Gateway | 只路由与计量；BYO 直连；仅托管额度通道经我方转发且 UI 明示（无状态 edge） | 服务端不落 prompt/completion（CI 静态字段数 = 0） |
| Plugin Registry | 静态 CDN 签名清单 + append-only 透明度日志 + 远程吊销 | 不执行插件、不代理插件运行；**服务端开源不降低审核/签名/吊销要求**（AR-21）(ADR-0013) |
| Telemetry Ingest | opt-in 的本地聚合计数 + 采样 | 内容零采集（无命令文本/路径/代码/域名） |
| Billing/Metering | 只认托管通道（无状态） | 不对 BYO 通道计费 |

边界红线：客户端启动**不得有阻塞式云调用**；断网时终端、AI（BYO + 本地模型）、插件全部可用；传输统一 Connect(gRPC/JSON) + SSE，WebSocket 仅用于双向协作；历史默认仅本地，跨端备份可选且端到端加密。

**许可与数据边界解耦（AR-21 / ADR-0013）**：云端服务源码公开**不改变**本表的禁止列——控制面-only、服务端不落 prompt/completion、PTY 流/文件内容/命令输出/密钥材料永不进入服务端仍是不可协商红线（AR-11、§6.1）；商业形态为托管服务、支持订阅与合规/企业服务，其源码同样公开；企业专有定制只能以独立插件或独立仓库存在，**不得回流核心**。

### 3.13 可观测性（OTel trace 贯通、默认本地）

1. **trace 贯通**：UI → sessiond → agent → tools → 云网关 → 厂商；W3C traceparent 跨进程传播；原生与 WebView 共享 session id。
2. **本地优先**：环形缓冲 + trace 导出命令；默认仅本地，导出须用户显式操作；终端内容永不外传。
3. **遥测**：默认关闭（opt-in）；崩溃上报独立开关；内容零采集；客户端侧脱敏后才允许上报（AR-12）。
4. **可审计性**：所有出站请求可分类、可审计、可在 UI 回看。
5. **指标集**：帧时直方图、慢帧归因、IPC 往返延迟、解析吞吐、RSS 漂移——本地聚合后可选上报。
6. **日志**：结构化、默认不落 PII；禁止打印 secret、命令全文或文件内容（AGENTS §6）。

### 3.14 关键时序

**T1 输入热路径**（门禁 P99 ≤16ms，AR-19）：keystroke → UI 原生层(L4 IME) → termai-ipc 输入帧 → sessiond → PTY/ConPTY write → 应用 → PTY read → VT 解析 → Grid diff → damage region → shape/atlas → GPU draw → present。约束：无 JSON、无 AI、无网络、无阻塞锁，仅 damage 增量重绘。

**T2 AI 工具调用闭环**：?? 前缀 / 面板 → agent 意图 → 工具 ABI → Policy Engine → 审批（分级摩擦）→ shell watchdog 执行 → 审计写入 → Session Log 事件 → Context 更新 → UI 结构化渲染（Hint / Explain / Diff / Diagnosis / Risk Ladder）。

**T3 插件调用**：插件 UI（Tier1 声明式）→ Local API（JSON-RPC over UDS）→ capability 校验 → Core 只读查询 / 注入队列 → 结果回传 → Host 渲染声明式 view；插件崩溃 → safe mode，会话零影响。

**T4 跨设备同步**：本地 SQLite 主副本 → outbox（幂等重放）→ Connect/SSE → Sync 服务（CRDT）→ 其它设备 → 合并入本地 → UI 重投影；冲突可解释、可回滚；命令历史与 scrollback 不在同步范围。

## 4. 接口与依赖（对内模块 / 对外契约）

### 4.1 对内模块接口

| 提供方 | 消费方 | 契约 | 变更规则 |
| --- | --- | --- | --- |
| core-dto (IDL) | 全部模块 | 结构化 schema 与版本 | 破坏性变更走 capability 协商；兼容 ≥2 minor |
| termai-core | session / agent / plugin-host | Session / Grid / Context 契约 trait | core 不得反向依赖；叶子契约 |
| termai-ipc | UI / sessiond / agent / plugin-host / headless | 帧格式 + 握手 + 能力集 | 版本治理 + 持续 fuzz（DC-37） |
| Local API | 插件 / CLI --json / headless / CI / IDE | JSON-RPC + apiVersion | 唯一契约；N-2 minor + 6 个月弃用 |
| termai-vt | render / session | VT trait（vte 在边界之后，AR-18） | 锁定上游版本 + 差异登记 |
| termai-store | session / agent | Log / CAS / 索引读写 | 格式版本 + 迁移器兼容 ≥2 大版本 |
| termai-tools | agent | Tool ABI + 风险分级 + 审批钩子 | 字段集冻结（DC-26） |

### 4.2 对外契约

| 契约 | 对象 | 承诺 | 依据 |
| --- | --- | --- | --- |
| termai-ipc 协议 | 第三方客户端 / IDE / headless | 二进制 + 版本/能力协商 + 结构化错误 | DC-22 |
| Local API | 插件 / 脚本 / CI | JSON-RPC + apiVersion，N-2 minor + 6 个月弃用 + codemod | DC-24、DC-40 |
| 插件 ABI | 插件作者 | WIT + WASM Component；Tier1 view schema；能力清单签名 | AR-07、AR-08 |
| 工具 ABI | 工具 / MCP 作者 | name / json_schema / risk / idempotent / timeout / approval_policy / render_hint + 统一 envelope | DC-26 |
| 主题格式 | 主题作者 | 纯数据 JSON + Schema + 版本 + 可选签名，永久免费开放 | DC-12 |
| 云 API | 生态伙伴 | Connect + SSE + cursor 分页 + Idempotency-Key | AR-11 |

**依赖方向红线（CI 强制，DC-21）**：1) core ← session ← { agent, plugin-host }；2) render → { vt, gpu }；3) apps/* 不得被任何库依赖；4) 出现环即阻断合并；5) 新增模块须 ADR 说明依赖位置（AGENTS §3）。

## 5. 验收标准

| AC | 标准（可量化） | 依据 |
| --- | --- | --- |
| AC-01 | 冷启动到可输入 P95 ≤150ms（目标 ≤100ms）；key-to-photon（本地）P99 ≤16ms（目标 P99 ≤8ms / P50 ≤4ms） | §5、AR-19 |
| AC-02 | 4K@120Hz 网格帧时 <8.3ms，丢帧 <0.1% | §5 |
| AC-03 | 解析+渲染吞吐 ≥500MB/s；安装包 <60MB | §5 |
| AC-04 | 空闲 RSS（1 万行）≤120MB（**口径 = 核心进程组 = sessiond + 原生 UI**，AR-24）；24h RSS 斜率 <1MB/h；WebView 就绪后增量 ≤60MB 与插件宿主空载 ≤80MB 独立记账；必须报告总内存 | §5、DC-38、**AR-24** |
| AC-05 | sessiond 7×24 无泄漏（RSS 漂移 <5%）；UI 崩溃 <2s 重连且屏幕一致；crash-free session >99.9% | §8.2 |
| AC-06 | SQLite 索引可由 Session Log 完整重建且校验一致 | §8.2、DC-23 |
| AC-07 | VT 兼容：vttest/esctest/kitty 100%、xterm 用例 ≥99%、行为回放 ≥99.5%；VT/PTY/IPC/插件消息 fuzz 24h 无 crash | §8.1 |
| AC-08 | 依赖与许可：依赖图无环；链接边界 GPL/AGPL/SSPL = 0；全仓 SPDX 表达式 = `Apache-2.0 OR MIT`（客户端 + 服务端 + SDK + 协议 + 工具链） | §8.1、DC-21、AR-21 |
| AC-09 | 插件越权 100% 被拒并留审计；插件 UI 崩溃 0 次影响会话；iframe 逃逸用例 0 成功 | §8.1、§8.2 |
| AC-10 | 默认配置抓包零用户未发起连接；遥测关闭时上传 0 字节；服务端 prompt/completion 字段数 = 0（不因服务端开源而改变） | §8.2、AR-11、AR-21 |
| AC-11 | AI 网关附加延迟 p95 <120ms / p99 <300ms；诊断首 token P95 <3s | §5 |
| AC-12 | L2/L3 未审批执行 = 0；出网密钥命中 = 0；100% 出网可在 UI 回看 | §8.2 |
| AC-13 | 多端 attach：3 客户端并发 attach 后帧哈希一致；sessiond 重启后屏幕与结构化元数据与崩溃前一致，且无任何命令重放 | AR-13、DC-04（本文补量化） |
| AC-14 | 无 WebView 环境下 L2 降级为原生文本面板，L0/L1/L3/L4 功能与性能门禁不劣化 | AR-02 |

## 6. 风险与缓解

| # | 风险 | 触发条件 | 缓解 | 责任 |
| --- | --- | --- | --- | --- |
| A1 | Native/WebView seam 错位（z-order、DPI、焦点、闪烁、IME） | 候选窗漂移或 CJK 输入异常 | L4 全原生、单窗口多表面合成标准件、seam 自动化测试 | 05 |
| A2 | sessiond 单点故障 / 多端并发写歧义 | 守护进程崩溃或两端同时写 stdin | supervisor 看护 + 崩溃重启 + checkpoint + 幂等重连；stdin 单写者 lease + 审计 | 04/10 |
| A3 | Session Log 格式演进致数据不可读 | 发布后需改格式 | 格式版本 + 迁移器 + 兼容 ≥2 大版本 + 只读降级显式告警 | 10 |
| A4 | 云边界被功能需求侵蚀 | 出现「上传历史换同步」类需求，或「服务端已开源所以可放宽边界」式论证 | 控制面-only 写入不可协商清单；CI 静态禁 prompt 字段；许可开源与数据边界解耦（AR-21 / ADR-0013 不改变 AR-11） | 06/08 |
| A5 | IPC 协议复杂度与 fuzz 面扩大 | 协商失败或畸形帧导致崩溃 | 单一帧格式 + 能力求交 + 24h fuzz + 版本治理 | 10 |
| A6 | 插件供应链投毒 | 签名异常或恶意插件报告 | 签名 + 透明度日志 + 能力最小化 + 远程吊销 ≤1h | 09/08 |
| A7 | Linux WebView 碎片化 | ≥2 发行版插件 UI 渲染异常 | WebView 设为可选依赖 + 原生文本降级面板 | 10/05 |
| A8 | 依赖环悄悄形成 | 跨 crate 私改频繁 | cargo-deny + 依赖图检查 + 季度架构审计 + CODEOWNERS | 10 |
| A9 | 性能预算渐进侵蚀 | 启动 >150ms 或 P99 >16ms 连续两版 | §5 门禁进 CI，回归 >5% 阻断合并 | 10 |
| A10 | Local API 被当作万能后门 | 第三方绕过 capability 直调写接口 | 令牌强制 + 默认拒绝 + 审计 + CI 契约测试 | 09/08 |

## 7. Open Questions

> 仅列 HARNESS 未覆盖的架构空白，**不自行发明结论**；每条给出影响面、建议与必须决策的 Phase，经 RFC → ADR 后回填 HARNESS §11。

| ID | 问题 | 影响面 | 建议 | Phase |
| --- | --- | --- | --- | --- |
| OQ-A1 | 多端 attach 的 stdin 单写者协议（lease 授予/抢占/超时/转移审计）未定义 | termai-ipc 帧类型、协作安全、审计 schema | 显式 lease + 显式转移 + 审计事件，禁止后到先赢 | P5（P1 预留帧位） |
| OQ-A2 | Local API 的鉴权与信任边界：同机多用户 / 容器 / CI runner 下的 UDS 权限模型未规定（DC-24 只定传输） | 安全、企业私有部署、headless CI | 文件权限 + per-client capability 令牌 + 全量审计 | P1 |
| OQ-A3 | SQLite 索引重建窗口内的一致性语义（重建期读操作返回什么、是否阻塞写入） | 存储、AC-06 验收路径 | 双缓冲索引 + 重建完成原子切换 | P1 |
| OQ-A4 | termai-ipc 的流控/背压与最大帧尺寸未定义（大 scrollback 快照、图形协议载荷）；本地 trace 环形缓冲容量上界未定 | 通信、内存上界、DoS 面、诊断预算 | per-channel credits + 显式溢出策略 + 分块传输；分级采样 + 固定容量缓冲 | P1 |
| OQ-A5 | 云 Sync 与本地 SQLite 主副本之间的 outbox 冲突检测与幂等重放责任方 | 同步正确性、离线语义 | 客户端 outbox + 服务端幂等键；服务端不做自动合并 | P4 |
| OQ-A6 | plugin-host safe mode 与 sessiond/supervisor 的监督层级关系（谁重启谁、崩溃预算归属） | 可靠性、DC-38 落实 | supervisor 统一看护；plugin-host 崩溃不触发 sessiond 重启 | P3 |
| OQ-A7 | L0 网格与 L2 WebView 之间的剪贴板/拖拽数据格式与所有权契约 | 跨层交互、粘贴注入安全 | Core 统一仲裁 + 结构化 MIME 白名单 | P1 |

---

**变更记录**：
- v1 初版，由 HARNESS §4 展开；后续任何 AR/DC 变更须同步本文并在 24h 内更新 AC 表。
- 跨文档一致性对齐（AR-24…AR-29）：§3.4 补 payload 编码裁定（CBOR + POD 双档、crc32c 强制，待 ADR 追认）；§3.6 补 PtyBackend 6→9 元面差异说明（对外契约变更须 ADR + 双签）与状态枚举映射（以 kernel/04 六态为准）；§3.7 改写真相层「含原始字节」表述并补 AR-26 三层保留语义。
- 许可口径修订（ADR-0013）：§1 范围表云控制面依据改为 AR-21；§2 约束表「云」行改为「云（许可）」并取 AR-21 口径（全栈开源、Apache-2.0 OR MIT）；§3.12 标题与 Plugin Registry 行补充「服务端开源不降低审核/签名/吊销要求」及许可与数据边界解耦说明；§5 AC-08 / AC-10 增列 SPDX 与「不因开源而改变」表述；§6 A4 补充反侵蚀触发条件。**云控制面数据边界与隐私约束（控制面-only、不落 prompt、密钥不进服务端）未变**（AR-11 / AR-12）。
