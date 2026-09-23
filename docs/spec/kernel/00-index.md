# 00 · 内核分册索引与登记表（P0 内核评审产物）

> **效力**：本索引位于 HARNESS.md 之下，是 `docs/spec/kernel/01…07` 的导航与登记表，**不新增任何架构结论**。与 AR/DC/§5/§8 冲突时一律以 HARNESS 为准（AGENTS §2、§5）。
> **本索引为派生产物：任一分册或任一裁决（AR/DC/ADR/§5/§8）变更后必须刷新本索引，不得沿用旧数。**
> **本次刷新依据**：七份分册的**当前版本**（01-vt-conformance、02-pty-platform、03-rendering-pipeline、04-session-lifecycle、05-input-ime-clipboard、06-performance-methodology、07-kernel-api-abi）+ HARNESS §2（**AR-24…AR-31**；本次增量补 **AR-30**（PTY 四指标处置 + PR-S 编号消歧）与 **AR-31**（内核 P0 待决项裁决：A 档 10 条已决 + B 档 8 条采纳默认值 + C 档 3 条降 P1 + D1–D6 处置））、§5、§8、§11 + **ADR-0018**（PtyBackend 九元面 / IPC CBOR+POD / crc32c 强制）。
> **口径**：引用决策只写 `AR-xx / DC-xx / OQ-xx / ADR-xxxx / CAP-n`；本系列只补机制，不改结论。新增指标在获 TSC 确认前**不得作为对外承诺或阻塞项**（AR-19、AGENTS §5）。
> **核对声明**：下列编号/数量逐条对照分册正文计得；无法核对者显式写「未核实」（见 L-15）。

## 1. 分册清单与责任边界

| 分册 | 一句话职责 | K（结论数） | 验收编号（个数） | OQ 编号（个数） | owns | 明确不 owns |
| --- | --- | --- | --- | --- | --- | --- |
| **01-vt-conformance.md** | VT/ANSI 一致性：五套件、三车道、扩展协议子集、未识别序列契约、vte 边界与 golden 格式 | K-01…K-10（10） | V-01…V-15（15） | OQ-VT-01…16（16） | VT 解析语义、Grid/行/宽度表、并发合规判定、conformance 产物格式 | 像素/视觉回归（→03）、PTY 实现（→02）、IPC 帧（→07）、Session Log（→04） |
| **02-pty-platform.md** | PTY/Transport 的「唯一允许改字节层」：**九元 PtyBackend**、ConPTY 差异登记、F0/F1/F2 保真契约 | K-01…K-08（8） | PTY-AC-01…12（12；09 升 §5、12 升 §8.2、10/11 不单列，AR-30） | OQ-PTY-01…07（7） | ConPTY/Job Object/POSIX 进程组、Transport 能力矩阵、字节保真 | VT 序列语义（→01）、网格/整形（→03）、Log 物理格式（→04）、IPC 布局（→07） |
| **03-rendering-pipeline.md** | damage→shaping→atlas→draw→present 管线：跨进程切点、渲染镜像、VRM 软换行/裁剪、GPU T0–T3 | K-01…K-14（14） | RP-01…RP-16（16） | OQ-RND-01…09（9） | 渲染镜像、damage/GridDelta 打包、rustybuzz+swash 整形/atlas、帧调度 | VT 语义与宽度表（→01）、PTY（→02）、Log（→04）、IPC 帧封装（→07） |
| **04-session-lifecycle.md** | Session 状态机、Session Log 段格式、checkpoint/重建、attach 与 stdin 单写者 lease、崩溃三级语义 | K-01…K-12（12） | AC-S1…AC-S12（12） | OQ-SES-01…11（11） | 会话状态与持久化、Log 物理格式、恢复、supervisor、订阅背压 | IPC 帧布局（→07）、Grid 结构（→01/03）、PTY 实现（→02）、输入编码（→05） |
| **05-input-ime-clipboard.md** | 输入编码单一收口、IME/候选窗原生、焦点契约、选择复制、剪贴板/粘贴安全 | K-01…K-13（13） | IN-AC-01…15（15） | OQ-INP-01…12（12） | InputEncoder/KeyTranslator/FocusRouter、IME 宿主、复制模型、OSC 52/粘贴屏障 | VT 输出、PTY 写入实现、GridCaretRect/VRM（→03）、IPC 帧布局（→07） |
| **06-performance-methodology.md** | 「怎么测、怎么判、测不准怎么办」：判定状态机、参考机指纹、bench harness、G4-PR/REL | K-01…K-15（15） | A-PM-01…A-PM-14（14；**A-PM-14 = AR-24 第 2 条新增**） | OQ-PM-01…12（12） | 测量方法学、bench-report/fingerprint schema、回归判定、CI 成本 | 门禁数值本身（→HARNESS §5）、各指标实现（→01–05/07） |
| **07-kernel-api-abi.md** | termai-ipc 帧与握手、**CAP-1…CAP-7** 校验点、共享内存传帧、Context schema 版本化、弃用流程、**§3.8 跨分册错误码登记表** | K-01…K-09（9） | IPC-AC-01…10（10）+ N-1…N-8（8，未门禁） | OQ-ABI-01…10（10） | 线格式、消息类型/能力协商、shm 布局、Context 版本化、稳定性治理、跨分册错误码索引 | GridSnapshot/GridDelta 字段集（→03 OQ-RND-07）、Log 文件（→04）、Tool ABI 语义（→04-spec） |

**边界要点**：① 唯一可改字节层是 02 的「事件/元数据层」，字节缓冲层永不可写（AR-25.2）；② 网格真相只在 sessiond，UI 只持可丢弃镜像（03 K-01）；③ 渲染不得自带第二套 wcwidth，列宽真源是 01（03 K-04）；④ 输入字节只能由 05 的 `InputEncoder` 产生（AR-29.3）；⑤ GridSnapshot/GridDelta 的**字段集**归 03（OQ-RND-07，仍开放），07 只定帧封装与丢弃自愈；⑥ **对外错误码从 07 §3.8 收口**（定义仍在各分册，冲突以定义分册为准）。

## 2. 实施依赖顺序

**拓扑（自下而上，箭头 = 「被依赖 → 依赖方」）**

```text
L0 基础   [01 VT: byte→Grid 语义 / 宽度表 / Grid 模型]   [02 PTY·Transport: 字节来源 / F0 通道 / 能力面]
                                   │  F0 字节直喂 + 宽度表 + Grid
             ┌─────────────────────┼──────────────────────────┐
             ▼                     ▼                          ▼
L1 消费   [03 渲染: 镜像/damage/整形/atlas/帧]      [04 会话: 状态机/Log/恢复/supervisor]
             │  依赖 01 列宽表·Grid·GridDelta         │  依赖 02 PtyBackend·Channel；产出 grid_digest/Log
             ▼                                        ▼
L2 协议   [07 IPC/ABI: 24B 帧 + 握手 + CAP-1…CAP-7 + shm 承载 03 的 GridSnapshot/Delta 与 04 的 attach/lease]
             │
             ▼
L3 输入   [05 输入/IME/剪贴板: 依赖 03 的 GridCaretRect·VRM、07 的 Input/Paste 帧、02 的原子 write]
             │
             ▼
L4 度量   [06 性能方法学: 依赖 01 语料/吞吐、02 PTY 时延、03 帧时/atlas、04 soak、05 输入分摊、07 握手]
```

**依赖规则**：单向无环（DC-21）；01/02 互为对偶但**不互相依赖实现**——02 保证字节到达、01 定义字节语义；03 与 04 可并行；07 是 03/04 的承载层，不得反向依赖 03/04 的具体字段语义；06 只依赖各分册的**只读测试接口**，禁止出货 crate 依赖 bench。

**P0 阻塞项（未落地则 P0 出口/G1 无法判定）**

| # | 阻塞项 | 出处 | 阻塞对象 |
| --- | --- | --- | --- |
| B-1 | sessiond/PTY 提供**字节直喂 L0 parser 的接口**（AR-25.4，P0 实现约束非优化） | AR-25.4；01 §3.1 L0 车道；02 §3.1 | G1 在 Windows/降级后端的判定 |
| B-2 | 三个车道分离（L0/L1/L2），100%/≥99% **只在 L0 判定** | AR-25.1；01 K-02 | G1 口径可信度 |
| B-3 | 每个 hop 强制声明 F0/F1/F2；`conpty-rules.toml` 机器可读，scope=data 即 block | AR-25.2/25.3；02 §3.2、§3.6 | Windows 字节保真门禁 |
| B-4 | `VtBackend`/`EscapeSink` trait + golden/replay/repro 三格式 | 01 K-07/K-09、§3.2/§3.7/§3.8 | G1 失败定位、G2 回放 |
| B-5 | **九元 PtyBackend** + `PtyCapabilities` 稳定（面扩张已由 **ADR-0018 D1** 追认，AR-28.1） | 02 §3.1；ADR-0018；OQ-PTY-05 | 04/05/07 的接口面冻结 |
| B-6 | GridSnapshot/GridDelta/ScrollOp **字段集**与 rev/丢弃自愈语义（OQ-RND-07） | 03 §3.3；07 §3.1/§3.5 | headless、多端 attach、插件镜像 |
| B-7 | Session Log 段格式（magic/CRC32C/record types）+ lease 帧位冻结（AR-26.5） | 04 §3.2/§3.4 | 屏幕恢复、审计链、P5 协作写 |
| B-8 | termai-ipc 24B 帧头 + 握手 + capability 七校验点（**CAP-1…CAP-7** 见 07 §3.4） | 07 K-01…K-04、§3.4 | 全部跨进程契约与 fuzz 靶面 |
| B-9 | `termai-core::input::InputEncoder` 单一编码收口 | AR-29.3；05 K-01/§3.1 | 输入字节等价 100%、审计收口 |
| B-10 | §5 每条指标引用 kernel/06 的测量定义；测量先过 D0/D1 自检 | AR-24.3、AR-27；06 §3.1/§3.6、**§3.9 映射表** | G4 判定有效性与可比性（**§5 现行 19 行全覆盖，L-11 已修复**） |
| B-11 | `cargo xtask conformance` verb 归属（新 verb 须 ADR） | 01 D-5；OQ-VT-12 | G1 运行方式（过渡期用 `test --suite`） |

## 3. 跨分册接口登记表

| 接口 | 提供方 | 消费方 | 载体 | 分册 / 章节 | 稳定性等级 |
| --- | --- | --- | --- | --- | --- |
| **GridSnapshot / GridDelta 字段集** | 字段集：01/03（`grid.rs`，core-dto 生成）；帧封装：07 | 03 镜像、05 SelectionEngine、插件只读镜像、headless | termai-ipc POD 帧 + SPSC 共享内存环 | 03 §3.3；07 §3.1/§3.2(0x0180–0x0183)/§3.5；04 §3.4 | 对外契约：capability 协商 + 兼容 ≥2 minor（AR-04 / DC-40）；**字段集仍待 OQ-RND-07 裁决** |
| **保真标签 F0 / F1 / F2（最终形态）** | 02（`PtyCapabilities.fidelity` / `TransitCaps.fidelity`） | 01 L1 车道、03 渲染层、UI 徽章、审计 | Rust enum `Fidelity` + `conpty-rules.toml`（severity/scope） | 02 §3.6（7 个 hop 各自声明）、§3.2；01 §3.1 | **每 hop 强制声明**（AR-25.2）。落位：PTY/应用↔VT = **F0**；sessiond↔UI（IPC）= **F0**；Local = **F0**；**WSL = 取决于底层 Transport**（经 ConPTY → **F1/F2 + Lossy**，未登记改写即失败；直连 WSL 自身 PTY → **F0**，AR-31 D5）；Container = **F0(通道)×F2(守护侧)**；SSH = **F0(数据)×F1(协议注释)**；UI 视觉层 = **F2**；AI/插件 = **F2(只读镜像)** |
| **PtyBackend 接口面（九元）** | 02（termai-pty）：spawn / write / read / resize / signal / kill / wait / process_tree(含 tree_snapshot) / capabilities | 04 sessiond、01 L0/L1 车道、05 原子 write | Rust trait + `PtyError`/`PtyCapabilities`/`Sig` | 02 §3.1、§3.5；**ADR-0018 D1** | 面扩张（6→9）**已由 ADR-0018 D1 追认**（AR-28.1）；`PtyCapabilities` 是否进 Local API 仍待 OQ-PTY-05 |
| **Session Log 记录格式** | 04（termai-session / termai-store） | 恢复器、agent、UI、审计、Local API | append-only 段文件（`TMAILOG\0` 64B 头、crc32c、13 类 record） | 04 §3.2 | DC-23：格式版本 + 迁移器兼容 ≥2 大版本；sealed 段不可写 |
| **租约与 attach 协议** | 04 §3.4；07 帧位 | UI、agent、plugin-host、Local API | IPC 帧（`ATTACH_REQ/ACK`、`LEASE_*` 0x0304–0x0307）+ `LeaseEvent` Log | 04 §3.4；07 §3.2、§3.4(CAP-2) | **P1 冻结帧位与语义，P5 启用跨端协作写**（AR-26.5 / OQ-ABI-05） |
| **跨分册错误码登记表** | **07 §3.8**（定义仍在 01/02/04/07 各分册） | 各层调用方、CLI（退出码 2/3/4）、UI | Rust enum + IPC `Error`(0x0007) + Log + JSON-RPC `code` | 07 §3.8；01 §3.2/§3.3；02 §3.1；04 §3.4 | 数值即契约：改语义 = major（07 §3.7）；**03/05/06 未暴露，显式「暂不登记」并给出理由与触发条件**（07 §3.8 缺口说明，L-18 已处置） |
| **capability 校验点** | 07 §3.4 七入口（**CAP-1…CAP-7**）+ `termai-core::capability::require()` 纯函数 | sessiond、plugin-host、Local API、agent | 纯函数 + `CapDenied`/`CapRefPresent` 帧 + `AuditRecord` | 07 §3.4（表内编号 **CAP-1…CAP-7**，已不再使用 G1…G7） | `act` 词表新增走 ADR；审计 fail-closed（K-09）；与全局 CI 门禁 G1–G8 的撞号已消除（L-2） |
| **bench-report / machine-fingerprint** | 06 §3.4、§3.7 | 01 V-13、02 PTY-AC-09…12、03 RP-01…16、04 `soak-report.json`、07 N-1…N-8 | JSON（`schemaVersion 1.0.0`，9 个 legacy 字段 + `fingerprintSha256`） | 06 §3.4、§3.7、§5（A-PM-01…A-PM-14） | 新增字段向后兼容 1 minor；扩展字段是否强制走 ADR-0014 公示 = OQ-PM-11 |
| **Context 事件 schema** | 07（core-dto IDL，10 tag） | agent、UI、plugin-host、Local API | IPC **CBOR** 事件帧（0x0202）+ golden vectors | 07 §3.6 | `major.minor`，minor 只增字段、tag 永不复用、N-2 兼容 ≥95% |
| **Input / Paste（统一命名）** | 05（termai-core::input）；帧：07 **`Input`/`Paste` = 0x0100/0x0101** | ui-native、sessiond、agent、插件注入 | Rust trait + IDL `core-dto::input::v1` + IPC POD | 05 §3.1、§4.3；07 §3.2 | 单一编码收口（AR-29.3）；旧名 `InputKey/InputPaste` 已废止（L-6）；**数值权威已落位 05 §4.3（Input = 0x0100 / Paste = 0x0101，与 07 §3.2 一致；L-14 已修复）** |
| **GridCaretRect / RowId / WrappedContinuation / VRM** | 03（termai-render） | 05 IME 锚点、复制、命中测试、a11y、plugin 装饰 | Rust 接口 + 逻辑行↔视觉行映射 | 03 §3.8、§4；05 §3.4/§3.6、§4.5 | 软换行/裁剪为纯视觉层（AR-23§6）；复制恒取逻辑序 |
| **配置键与优先级** | 05（IN-01…12）、03（font/wrap/AA/gpu.backend）、07（CAP-6 `config.write`） | ui-native、termai-render、sessiond | TOML 层级合并（default<system<user<project<env<CLI） | 05 §4.4；03 §3.7；07 §3.4 | 对外行为默认值；`IN-05` 默认已改为 `core_first`（AR-29.1/29.2，L-3 已修复） |

## 4. 可执行验收登记表

> **门禁编号消歧**：`G1…G8` = spec 07 §3.4.1 CI 门禁（**G7 = 设计静态 S1–S10；G8 = 设计浏览器 B1–B10**）；`UX-G1…UX-G17` = spec 02 §5.3 体验门禁；`tokens:check` = `tools/tokens/check.mjs`（package.json）；`L1…L8` = spec 07 §3.3 CI 层；**`PR-S0…PR-S5`** = spec 07 §3.4.2 PR 分级（§3.4.2 已就地消歧，AR-30 第 5 条）；设计静态门禁仍为 `S1–S10`（G7）。两套编号不得再以裸 `S` 混用（L-16）。

| 编号（组） | 一句话判定 | 测量方法所在分册 | 已进 CI | 关联门禁 | 自动/人工 |
| --- | --- | --- | --- | --- | --- |
| V-01…V-05 | vttest/esctest/kitty 100%；xterm ≥99%；terminfo diff 空 | 01 §3.9/§5 | 是（PR-S3 子集 / nightly PR-S4 全量） | G1 | 自动 |
| V-06 | `.trec` 行为回放通过率 ≥99.5% | 01 §3.8/§5 | 是（PR-S2/PR-S4） | G2 | 自动 |
| V-07…V-08 | 未识别序列四态 100%；字符串上限后 Ground 字节照常上屏 | 01 §3.4/§5 | 是（PR-S3/PR-S4） | G1 | 自动 |
| V-09 | E60 + M12 12 组合扩展协议全绿 | 01 §3.5/§5 | 是（nightly） | G1 + L6 | 自动 |
| V-10 | 软换行开/关复制逐字节相同、OSC 8 区间不变 | 01 §5 | 是（PR-S3/nightly） | G1 + G8（UX-G17） | 自动 |
| V-11…V-15 | trait 差分一致；VT fuzz；吞吐；repro 齐备；report schema | 01 §3.10/§5、06 §3.2 | 是（PR-S4/nightly/周度） | G1、G4、G6 | 自动 |
| PTY-AC-01…02 | F0 字节恒等；ConPTY 改写 100% 登记、block/data=0 | 02 §5 | 是（每 PR / nightly） | G2；G1（ConPTY 分量） | 自动 |
| PTY-AC-03…04 | 进程树闭环 2s/200ms；POSIX resize 100ms 内可见、zombies=0 | 02 §5 | 是（PR + nightly + 周度 soak） | L2/L6/L8 | 自动 |
| PTY-AC-05…07 | 7 应用×2 平台矩阵；Transport 能力一致；接口不变量 ≥10⁶ 次 | 02 §5 | 是（nightly / PR） | L6/L2/L1 | 自动 |
| PTY-AC-08 | PTY fuzz 24h / ≥10⁸ 次 | 02 §5 | 是（PR smoke + nightly） | G6 | 自动 |
| PTY-AC-09…12 | PTY-LAT-1（升 §5 门禁 P99 ≤2ms）/ PTY-LOSSY-1（并入 AR-25.3）/ PTY-RSS-1（并入 AR-24.1）/ PTY-ORPHAN-1（升 §8.2 验收）【新增】 | 02 §5、§8.1 | 是（**已决：AR-30 第 1–4 条**） | G4（09）/ §8.2（12） | 自动 |
| RP-01…RP-04 | key-to-photon P99 ≤16ms；4K 帧时 <8.3ms；吞吐 ≥500MB/s；RSS ≤120MB + 子预算 | 03 §5、06 §3.2 | 是（nightly / 周度 / 发版前） | G4 | 自动 |
| RP-05…RP-06 | 网格对齐 ≤0.5px（4 DPI）；视觉回归 ≤0.1% | 03 §5 | 是（PR-S3 / G7-S7） | G3 / G7 | 自动 |
| RP-07…RP-10 | cluster↔列 100%；WrapMode 切换 0 GridDelta；复制一致；atlas 驱逐无差 | 03 §5 | 是（PR，阻断合并） | G1/G2、UX-G17 | 自动 |
| RP-11…RP-12 | DPI 切换 ≤1 帧/≤300ms【新增】；空闲 CPU <1.0%、未聚焦 present=0（**已决 AR-24.2**） | 03 §5、§3.6 | 是（nightly） | G4 | 自动 |
| RP-13…RP-14 | T0–T3 降级矩阵、非 T0 判 FAIL；DeviceLost ≤2s 重建 | 03 §3.7/§5 | 是（nightly） | G4 / L4 | 自动 |
| RP-15…RP-16 | 插件装饰不改 grid 真相；bidi 行内 UBA、复制逻辑序 | 03 §5 | 是（PR+安全套件 / nightly） | G6 / L6/E60 | 自动 |
| AC-S1…AC-S4 | 状态机封闭；CRC 尾部截断恢复；重建一致；重建期 spawn=0 | 04 §5 | 是（PR L1/L3） | L1/L3 | 自动 |
| AC-S5…AC-S6 | UI 重连 <2s；sessiond SIGKILL 后 P95 ≤2s / P99 ≤5s（**AR-26.4**） | 04 §5 | 是（nightly L8） | §8.2 / G4 可靠性 | 自动 |
| AC-S7…AC-S8 | lease 唯一性（万次争抢双写=0）；磁盘上限与滚动 | 04 §5 | 是（PR + nightly + 周度） | L5/L8 | 自动 |
| AC-S9 | AI 关闭零开销四断言（无进程/无链接/零分配/劣化<1%） | 04 §5 | 是（静态 PR + nightly） | AR-03 / L4 | 自动 |
| AC-S10 | 24h RSS <1MB/h、漂移 <5%、fd 斜率 = 0 | 04 §5 | 是（周度 + 发版前） | G4 / §8.2 | 自动 |
| AC-S11…AC-S12 | 退避/熔断序列与通知；慢订阅者不阻塞热路径且丢弃可见 | 04 §5 | 是（nightly） | L2/L4 | 自动 |
| IN-AC-01…IN-AC-02 | 键盘编码套件 100%/≥99%；key-to-photon P99 ≤16ms | 05 §5、06 §3.2 | 是 | G1、G4 | 自动 |
| IN-AC-03 | 输入侧分摊 P99 ≤1.15ms【新增】 | 05 §3.9/§5 | 部分（OP 报告，未批准前不阻塞） | G4 附加报告 | 自动 |
| IN-AC-04 | IME 12 组合全绿；候选窗漂移 ≤2px【新增】 | 05 §5 | 部分（截图矩阵 + 脚本） | L6 / UX-G5 | **人工 + 脚本** |
| IN-AC-05…IN-AC-06 | preedit 期 PTY 字节=0；焦点 F-1/F-2/F-3 不变量 | 05 §5 | 是 | G2 / UX-G14、G7 | 自动 |
| IN-AC-07…IN-AC-10 | 复制两态 BLAKE3 相同；选择边界；粘贴消毒 100%；OSC 52 默认 deny、审计零内容 | 05 §5 | 是 | UX-G17、G2、G6 | 自动 |
| IN-AC-11 | 鼠标模式与链接 scheme 白名单、Shift 逃生阀 | 05 §5 | 部分（esctest + 手动矩阵） | G1 | **部分人工** |
| IN-AC-12 | a11y 键盘完成率 100%、6 条 UI 流程 | 05 §5 | 否 | UX-G5、A2/A8 | **人工** |
| IN-AC-13…IN-AC-15 | 输入脚本回放 ≥99.5%；fuzz 24h；配置/降级矩阵 | 05 §5 | 是 | G2、G6、L6 | 自动 |
| A-PM-01…A-PM-03 | 测量自证零翻转（**AR-27**）；D0 场景冻结；selftest 12 注入全捕获 | 06 §3.6/§5 | 是（nightly / PR） | G4 | 自动 |
| A-PM-04…A-PM-05 | 冷启动 P95 ≤150ms；key-to-photon 门禁 + 相机偏置 ≤4ms | 06 §3.2/§3.3/§5 | 部分（相机标定人工、周期） | G4 | 自动 + **人工标定** |
| A-PM-06…A-PM-08 | 4K 帧时 <8.3ms；吞吐 ≥500MB/s；RSS core ≤120MB / plugin-host ≤80MB | 06 §3.2/§5 | 是（nightly） | G4 | 自动 |
| A-PM-09…A-PM-14 | 24h 斜率；安装包 <60MB；report schema；成本 ≤$3000；机器绑定；**空闲 CPU / 零出帧（AR-24.2）** | 06 §3.2/§3.7/§5 | 部分是（周度/构建/月度看板） | G4 / 发布 / 治理 | 自动 |
| IPC-AC-01…IPC-AC-03 | 帧层 fuzz 24h；握手矩阵；7 校验点（CAP-1…CAP-7）不可绕过 | 07 §5 | 是 | G6 / §8.2 | 自动 |
| IPC-AC-04…IPC-AC-06 | 审计链 fail-closed；shm 八约束；N-2 schema ≥95% | 07 §5 | 是 | §8.2、G6、DC-40 | 自动 |
| IPC-AC-07…IPC-AC-08 | 未知消息三类处理；内核纯依赖（无 AI/网络/UI） | 07 §5 | 是（PR） | G5 / AR-03 | 自动 |
| IPC-AC-09…IPC-AC-10 | 单写者租约唯一；降级路径不劣化 L0/L1/L3 | 07 §5 | 是（发版门禁） | §8.2 / 发布 | 自动 |
| N-1…N-8【新增】 | 握手 P99/输入 gate/shm 投递/快照吞吐/require/未知 tag/N-2 通过率/shm 驻留 | 07 §8.1 | **否**（TSC 确认前不入门禁） | 待定 | 自动测量、未门禁 |
| 设计静态 S1–S10 / 浏览器 B1–B10；`tokens:check` | 键位三平台展开零冲突（S10，AR-29.7）；**键位行为与 Action Registry 一致（B10，AR-29.7）**；零横向滚动；token 零硬编码 | tools/design-gates、tools/tokens | 是 | G7（S1–S10）、G8（B1–B10） | 自动 |
| UX-G1…UX-G17 | 体验层门禁（含 UX-G17 软换行关=裁剪且复制逐字节一致） | spec 02 §5.3 | 是（原型的自动化子集） | UX-Gn | 自动为主 + 少量试听/读屏**人工** |

**人工项汇总（不进自动 CI 或需人判）**：IN-AC-04（IME 截图矩阵）、IN-AC-11 手动部分、IN-AC-12（NVDA/VoiceOver）、A-PM-05 相机标定（周期/争议）、功耗抽测（AR-24.2 降为人工目标）、N-1…N-8（未门禁）、PR-S5（CODEOWNERS 双签）。

## 5. 新增指标与待决问题登记表

> **计数口径（本次按分册逐条重算；AR-31 回填后）**：OQ 总数 **77**（01:16 / 02:7 / 03:9 / 04:11 / 05:12 / 06:12 / 07:10）。
> **已决 = 25**（全部指向具体 AR/ADR，见 §5.1；AR-31 新增 A 档 10 条 + D5 关闭 OQ-PTY-06）；**部分已决 = 1**（OQ-SES-04）；**仍开放 = 52**（完全开放 51 + 部分已决 1）。
> **阶段分布（77 条）**：**P0 = 27、P1 = 41、P2 = 5、P3 = 1、P5 = 3**。（旧版索引报「P0=51 / P1=69 / P2=10」共 130，与其自身 77 条总数矛盾，且含大量非 OQ 行；本次以分册 §8 表的 OQ 行为唯一口径。）
> **已决阶段分布**：P0 = 16、P1 = 9。**开放阶段分布**：P0 = 8（全部为 B 档「已采纳默认值」）、P1 = 35（含 OQ-SES-04）、P2 = 5、P3 = 1、P5 = 3。
> **阶段口径说明**：上一行「阶段分布 P0 = 27」为**注册阶段**；其中 OQ-VT-12 / OQ-INP-10 / OQ-PM-12 三条已按 AR-31 D3/D4 降为 P1，故「开放阶段分布」按**现行阶段**把这 3 条计入 P1（P0 开放 = 8、P1 开放 = 35）。
> **【新增】可测指标**：不另立跨分册编号，按各分册 §8 / 行内【新增】标注登记（定位见 §5.5）；旧版「54 项」无法逐条复核，已按 AR-31 补充第 2 条关闭（L-15，设计上不做）。
> **计数对比（AR-31 前 → AR-31 后）**：OQ 总数 77 → 77（编号集未变）；**已决 14 → 25**（AR-31 A 档 10 条：OQ-VT-03 / OQ-VT-09 / OQ-PTY-01 / OQ-INP-01 / OQ-INP-06 / OQ-INP-12 / OQ-PM-01 / OQ-PM-04 / OQ-PM-07 / OQ-PM-08；D5 关闭 OQ-PTY-06）；部分已决 1 → 1；**仍开放 63 → 52**（完全开放 62 → 51）。**P0 开放 21 → 8**（关闭 A 档 10 条 + C 档 3 条降 P1；余下 8 条即 B 档，状态为「已采纳默认值（AR-31 采纳分诊建议）」）；**P1 开放 33 → 35**（+OQ-VT-12 / OQ-INP-10 / OQ-PM-12 降级，−OQ-PTY-06 关闭）。
> **按分册（OQ 总数 / 已决 / 部分已决 / 仍开放含部分）**：01 = 16/2/0/14；02 = 7/2/0/5；03 = 9/4/0/5；04 = 11/4/1/7；05 = 12/5/0/7；06 = 12/6/0/6；07 = 10/2/0/8。

### 5.1 已决 / 部分已决（26 条，全部有 AR/ADR 指向）

| OQ | 分册 | 阶段 | 状态 | 裁决依据与要点 |
| --- | --- | --- | --- | --- |
| OQ-RND-01 | 03 | P1 | **已决** | **AR-24 第 1 条**：RSS 口径 = 核心进程组（sessiond + 原生 UI）≤120MB；WebView/plugin-host 独立记账且必须报告总内存 |
| OQ-RND-02 | 03 | P1 | **已决** | **AR-24 第 2 条**：空闲 CPU ≤1%、未聚焦/遮挡出帧 = 0 帧/10s 升为门禁；功耗降为人工抽测 |
| OQ-RND-03 | 03 | P1 | **已决** | **AR-24 第 3 条**：帧时 = present 时间戳差；吞吐 = headless（sessiond-only） |
| OQ-RND-04 | 03 | P1 | **已决** | **AR-24 第 3 条**：网格对齐 = glyph 位图原点 vs cell 原点的偏差最大值（4 DPI） |
| OQ-PM-05 | 06 | P0 | **已决** | **AR-24 第 1 条**：rss.core / rss.webview / rss.plugin_host 三分账 + rss.total |
| OQ-PM-03 | 06 | P0 | **已决** | **AR-27**：G4-PR（单次 >5% 阻断合并）/ G4-REL（连续 2 夜或发版前全套）；前置可复现性自检 |
| OQ-SES-01 | 04 | P0 | **已决** | **`docs/spec/03` §3.6 已同步**：补「状态枚举口径（以 kernel/04 为准）」+ 六态映射（`Creating→created`、`Running→running`、`Detached→detached`、`Exited→exited`、`Crashed→recovering`、`Reaped→dead`）；kernel/07 §3.6 tag 1 同口径。**残留已清除**：03-spec 该行把 kernel/04 状态机小节误引为 §3.2（实为 §3.1），已在本轮改为 §3.1（见附录 L-17） |
| OQ-SES-02 | 04 | P1 | **已决** | **AR-26 第 4 条**：sessiond 冷重建 P95 ≤2s / P99 ≤5s，写入 §8.2 |
| OQ-SES-03 | 04 | P1 | **已决** | **AR-26 第 5 条**：P1 冻结 lease 帧位与语义，P5 启用跨端协作写 |
| OQ-SES-11 | 04 | P0 | **已决** | **AR-26**：§8.2 追加 sessiond 预算 + 三层保留语义 + OQ-30 阶段澄清 |
| OQ-INP-02 | 05 | P1 | **已决** | **AR-29 第 1/2/7 条**：核心快捷键在 FocusRouter 始终优先；平台展开须显式登记且 CI 校验零冲突（S10） |
| OQ-INP-03 | 05 | P1 | **已决** | **AR-29 第 5 条**：OSC 52 读默认 deny、写默认 guarded、禁止静默多行 |
| OQ-ABI-01 | 07 | P0 | **已决** | **ADR-0018 D2**：IPC payload = CBOR + POD 双档（POD 热路径 ≤4KiB）；IDL 可落编码 |
| OQ-ABI-02 | 07 | P0 | **已决** | **ADR-0018 D3**：crc32c 由可选升强制（帧头 + shm 槽同语义） |
| OQ-SES-04 | 04 | P1 | **部分已决** | **AR-26 第 1–3 条**：raw ring 默认 volatile、不落长期存储；**仍开放**：加密密钥管理（keychain/轮换） |
| OQ-VT-03 | 01 | P0 | **已决** | **AR-31 第 1 条**：xterm 用例 ≥2000 条，ctlseqs 每条目 ≥1 用例，真实语料占比 ≥20% |
| OQ-VT-09 | 01 | P0 | **已决** | **AR-31 第 2 条**：v1 默认 `xterm-256color` + 随包 `termai` terminfo，P2 评估切换 |
| OQ-PTY-01 | 02 | P0 | **已决** | **AR-31 第 3 条**：owner = T1 内核（`termai-pty`）+ T5 复核；每条 conpty 规则必须带 owner + expires（默认 2 minor） |
| OQ-PTY-06 | 02 | P1 | **已决** | **AR-31 D5**：WSL 保真级别取决于底层 Transport（经 ConPTY → F1/F2 + Lossy；直连 WSL 自身 PTY → F0） |
| OQ-INP-01 | 05 | P0 | **已决** | **AR-31 第 4 条**：§5 只收可自动化的输入字节等价；IME preedit 与候选窗（IN-AC-04）留 §8.2 + L6 + 人工会签 |
| OQ-INP-06 | 05 | P0 | **已决** | **AR-31 第 5 条**：P0 只要求 X11/fcitx5 preedit；Wayland 缺 text-input-v3 时登记偏差 + UI 明示，不计 P0 失败 |
| OQ-INP-12 | 05 | P0 | **已决** | **AR-31 第 6 条**：配置只能加强不能削弱（`ask` 可升 `always`，不得降 `never`）；消毒与审计不可关闭 |
| OQ-PM-01 | 06 | P0 | **已决** | **AR-31 第 7 条**：按平台主格式计（Win MSI / macOS DMG universal2 / Linux AppImage），任一超限即 FAIL；macOS 超限改 thin slice + 双下载 |
| OQ-PM-04 | 06 | P0 | **已决** | **AR-31 第 8 条**：仅不触及 `term-{render,gpu,vt,session,pty}` 的 PR 可 `SKIP(no_hotpath)`；触及热路径一律不得 SKIP |
| OQ-PM-07 | 06 | P0 | **已决** | **AR-31 第 9 条**：按指标族分级（帧时 5% / 启动 3% / 吞吐 2% / RSS 1%） |
| OQ-PM-08 | 06 | P0 | **已决** | **AR-31 第 10 条**：1GB 吞吐语料分片权重入库即冻结，变更需 ADR |

### 5.2 P0 开放组（8 条，全部为 AR-31 已采纳默认值）

> **非 OQ 的 P0 待决项指针（AR-31 D2）**：kernel/02 §8.1 的 **PTY-AC-09…12** 标 P0/P1 但非 OQ 行，已由 **AR-30** 全部裁决（见 §附 B 的 L-13），不再作为待决项；原分诊对照见 `00-p0-triage.md` §1.1。

| 编号 | 一句话 | 分册 |
| --- | --- | --- |
| OQ-VT-01、OQ-VT-02 | 字符串态载荷上限 / 图形配额与解码预算（**已采纳默认值（AR-31 采纳分诊建议）**） | 01 |
| OQ-VT-10、OQ-VT-14 | vte 钉版本与补丁 SLA / 8-bit C1 默认策略（**已采纳默认值（AR-31 采纳分诊建议）**） | 01 |
| OQ-PTY-02 | Windows ByteFallback 是否静默（**已采纳默认值（AR-31 采纳分诊建议）**） | 02 |
| OQ-PM-02 | 无相机 D_max 折算（**已采纳默认值（AR-31 采纳分诊建议）**） | 06 |
| OQ-PM-06 | 24h soak 独占 RM-A vs 专用 rig（**已采纳默认值（AR-31 采纳分诊建议）；P0 定窗口 / P1 定 rig，D6**） | 06 |
| OQ-PM-11 | fingerprint 扩展字段是否走 ADR-0014 公示（**已采纳默认值（AR-31 采纳分诊建议）**） | 06 |

### 5.3 P1 开放组（35 条，含 1 条部分已决；AR-31 D3/D4 新增 3 条降级）

| 编号 | 一句话 | 分册 |
| --- | --- | --- |
| OQ-VT-04、OQ-VT-05、OQ-VT-06、OQ-VT-07、OQ-VT-08 | 差异登记上限与豁免率 / 提示符态超时与 OSC 633 文本保留 / 通知限流 / OSC 8 scheme 白名单 / OSC 52 写载荷上限 | 01 |
| OQ-VT-11、OQ-VT-13、OQ-VT-15、OQ-VT-16 | clean-room 替换阈值 / 网格内 OSC 8 满足 AR-20 / iTerm2 子集与能力字段 / conformance 产物保留期 | 01 |
| OQ-VT-12（**降为 P1：AR-31 D3/D4**） | G1 的 xtask conformance verb 归属（占位 = `cargo xtask test --suite`） | 01 |
| OQ-PTY-03、OQ-PTY-05、OQ-PTY-07 | Container F2 判定依据 / PtyCapabilities 是否进 Local API / 用户会话默认 wall 超时（WSL 语义已由 AR-31 D5 关闭） | 02 |
| OQ-RND-05、OQ-RND-06、OQ-RND-07 | Clip 模式光标可见性 / atlas 24MiB 与彩色页 / **GridSnapshot·GridDelta 字段集归属** | 03 |
| OQ-SES-04（部分已决 AR-26.1–3） | raw ring 窗口与加密密钥管理 | 04 |
| OQ-SES-05、OQ-SES-06、OQ-SES-08、OQ-SES-09、OQ-SES-10 | 磁盘硬上限 / checkpoint 频率 / 崩溃后历史元数据注入 / 可靠性门禁候选集 / 设计常量与阈值 | 04 |
| OQ-INP-04、OQ-INP-05、OQ-INP-07、OQ-INP-08、OQ-INP-09、OQ-INP-11 | Local API clipboard./input. / 回应 spec 03 OQ-A7 / macOS option_as_meta / kitty flags allowlist / 粘贴载荷上限 / 裁剪显示语义 | 05 |
| OQ-INP-10（**降为 P1：AR-31 D3/D4**） | 四项【新增】指标批准（占位 = 照测并标【新增】，不阻塞，只进 G4 附加报告） | 05 |
| OQ-PM-09、OQ-PM-12（**降为 P1：AR-31 D3/D4**） | 冷启动 WARM vs COLD-DISK / 纯 CPU 固定微基准（占位 = A-PM-07/09 的 CPU 侧归因） | 06 |
| OQ-ABI-03、OQ-ABI-04、OQ-ABI-06、OQ-ABI-10 | shm 驻留是否计入 120MB / 逐键审计合规底线 / Local API peer 鉴权 / 版本地板定义与升降 | 07 |

### 5.4 P2 / P3 / P5 开放组（9 条）

| 编号 | 一句话 | 分册 | 阶段 |
| --- | --- | --- | --- |
| OQ-RND-08、OQ-RND-09 | v1 是否开启行内 bidi 重排 / 保留 2 组 scale 热页的代价收益 | 03 | P2 |
| OQ-PM-10 | 光标闪烁帧时是否升为门禁 | 06 | P2 |
| OQ-ABI-08、OQ-ABI-09 | 插件 AI tool risk 分类落点 / Tool ABI 是否同享 N-2 兼容 | 07 | P2 |
| OQ-SES-07 | plugin-host / agent 监督层级 | 04 | P3 |
| OQ-PTY-04 | SSH reattach 是否允许启发式 | 02 | P5 |
| OQ-ABI-05、OQ-ABI-07 | lease 抢占·超时·转移细节 / capability 令牌跨设备复用 | 07 | P5 |

### 5.5 【新增】指标登记定位（不另立跨分册编号）

| 分册 | 登记位置 | 备注 |
| --- | --- | --- |
| 01 | §8 表内 OQ-VT-01…OQ-VT-08 与 OQ-VT-16（9 条以 OQ 行登记） | 阈值不与 §5 并列 |
| 02 | §8.1 PTY-AC-09…12（PTY-LAT-1 / LOSSY-1 / RSS-1 / ORPHAN-1） | **已决（AR-30）**：09 升 §5、12 升 §8.2、10/11 不单列（L-13 关闭） |
| 03 | §5 RP-11；§3.2.3 子预算；§3.3 第 5 条 damage 指标；§3.4 atlas 上限 | RP-12 已由 AR-24.2 升为门禁 |
| 04 | §8 OQ-SES-02、OQ-SES-04、OQ-SES-05、OQ-SES-09、OQ-SES-10 与 §3 设计常量（spawn 5s / lease TTL / checkpoint 256 / 退避 / credit） | 部分已由 AR-26 收口 |
| 05 | §5 IN-AC-03、IN-AC-04 漂移、复制语料；§3.9 输入侧分摊；IK-G1 门禁提案 | **AR-31 将 OQ-INP-10 降为 P1**：占位照测并标【新增】，不阻塞发布 |
| 06 | §3.2 补充 1–4；§3.3 无相机折算；§3.4 fingerprint 扩展；§3.6 eps_self / 自证；§5 A-PM-01…14 | A-PM-14 = AR-24.2 |
| 07 | §8.1 N-1…N-8 | 未门禁 |

## 6. 与 HARNESS 的关系

1. **不得冲突**：本系列全部为 `HARNESS.md` 之下位文档；与 AR/DC/§5 预算/§8 门禁冲突时以 HARNESS 为准（各分册文首均声明效力顺序）。
2. **不得并列新预算**：分册新增数值一律标【新增】并集中在各自 §8；**未获 TSC 确认前不得作为对外承诺、不得阻塞发布**（AR-19、AR-24.3、AGENTS §5）。
3. **已裁决须回填**：AR-24…AR-31 与 ADR-0018 的已决项（§5.1）须在对应分册、03-spec 与 HARNESS §11 回填。**ADR-0018 的三项契约变更均已追认**（九元 PtyBackend、CBOR+POD、crc32c 强制），07 的 IDL 可落编码；ADR 落地前只落 tag 表的限制已解除。
4. **门禁数值改动**：任何 §5/§8 的放宽需基准数据 + TSC 批准（AGENTS §5）；分册不得自行放宽。
5. **本索引非权威**：仅作导航与登记；冲突时以 HARNESS 与各分册正文为准。

## 7. 文档地图与最短阅读路径

**新加入者的最短路径（约 60–90 分钟）**：HARNESS §0 不可协商清单 → §2 AR-01/03/04/06/07/11/13 → §5 预算 → §8 门禁 → §2 **AR-24…AR-31** → **ADR-0018** → 本索引 §1（谁负责什么）→ §2（先做什么）→ §3（接口在哪）→ §5（哪些还没定）。

**按角色下钻**：VT/测试 → 01 §2 K-01…K-10、§3.1/§3.2/§3.7/§3.8、§5｜PTY/平台 → 02 §2 K-01…K-08、§3.1/§3.2/§3.6、§5（PTY-AC-09…12 已决：AR-30）｜渲染/GPU → 03 §2 K-01…K-14、§3.1/§3.3/§3.4/§3.8、§5｜会话/存储 → 04 §2 K-01…K-12、§3.1/§3.2/§3.4/§3.5、§5｜输入/IME → 05 §2 K-01…K-13、§3.1/§3.4/§3.6/§3.7、§5｜性能/CI → 06 §2 K-01…K-15、§3.1/§3.4/§3.6/§3.7、§5（含 A-PM-14）｜IPC/ABI → 07 §2 K-01…K-09、§3.1/§3.3/§3.4/§3.5/§3.6、**§3.8（错误码）**、§5。需要「为什么否决另一种做法」时读各分册 §7 与被否决方案表，再下钻 `docs/roles/` 与 `docs/adr/`。

**编号体系速查**：结论 `K-xx`（各分册内独立编号）；验收 `V-/PTY-AC-/RP-/AC-S-/IN-AC-/A-PM-/IPC-AC-`（+ 07 的 `N-1…N-8` 未门禁）；待决 `OQ-VT/PTY/RND/SES/INP/PM/ABI-NN`（AR-28.2）；capability 校验点 `CAP-1…CAP-7`（07 §3.4，非门禁编号）；全局门禁 `G1–G8` 与 `UX-G1…UX-G17` 来自 spec 07 §3.4.1 / spec 02 §5.3；PR 分级 **`PR-S0…PR-S5`**（spec 07 §3.4.2 已就地消歧）与设计静态 `S1–S10` / 浏览器 `B1–B10` 须加限定前缀（L-16 已修复）。

---

## 附：编号缺口登记（本次刷新）

### A. 已修复（历史刷新）——L-1…L-10 全部关闭

| # | 旧缺口 | 状态 | 修法（对照现行分册） |
| --- | --- | --- | --- |
| L-1 | OQ 命名空间混用 | **已修复** | 统一为 OQ-VT/PTY/RND/SES/INP/PM/ABI；01 §5 表末残留的「OQ-VT-01…K-04」已改为 OQ-VT-01…08（01:338） |
| L-2 | 07 §3.4 与全局门禁 G 编号撞号 | **已修复** | 07 §3.4 校验点改名 **CAP-1…CAP-7**，并显式声明不再使用 G1…G7（07:168） |
| L-3 | 05 快捷键默认值与 AR-29 冲突 | **已修复** | IN-05 默认改为 `core_first`、§3.3 改为「核心快捷键始终优先」、OQ-INP-02 标已决（05:311/129/386） |
| L-4 | Session 状态枚举三方不一致 | **已修复（登记层）** | 07 §3.6 tag 1 改用 04 六态并给旧名→新名映射（07:246）；**OQ-SES-01 已关闭**：`docs/spec/03` §3.6 已补「状态枚举口径（以 kernel/04 为准）」+ 六态映射（03:169） |
| L-5 | 吞吐口径（端到端 vs headless） | **已修复** | 06 K-09/A-PM-07 与 03 RP-03 统一为 headless（sessiond-only）（06:31/343；03:317） |
| L-6 | InputKey/InputPaste 与 Input/Paste 双命名 | **已修复** | 07 §3.2 统一为 **Input/Paste = 0x0100/0x0101**，声明名字以 05 §4.3 为准（07:106） |
| L-7 | 05 引用不存在的分册名 | **已修复** | 05 §1.2/§4.5 已改为 01-vt-conformance / 02-pty-platform / 03-rendering-pipeline（05:21） |
| L-8 | 02 PTY-AC-09…12 与 §8.1-N 双编号 | **已修复** | 02 §8.1 复用 PTY-AC-09…12 单一编号，取消 8.1-N（02:283） |
| L-9 | 验收编号缺号 | **已修复（仍成立）** | 编号连续：V-01…15、PTY-AC-01…12、RP-01…16、AC-S1…12、IN-AC-01…15、A-PM-01…14、IPC-AC-01…10 + N-1…8 |
| L-10 | 无跨分册统一错误码表 | **已修复** | 07 新增 **§3.8 跨分册错误码登记表**（07:283） |

### B. 已修复（本次刷新：L-11 / L-13 / L-14 / L-16 / L-17 / L-18 与 OQ-SES-01）

| # | 缺口 | 状态 | 修法 / 依据 |
| --- | --- | --- | --- |
| **L-11** | AR-24 第 3 条「每条 §5 指标引用 kernel/06 测量定义」只部分达成 | **已修复** | kernel/06 新增 **§3.9「§5 指标 → 测量定义」映射表**：HARNESS §5 现行 **19 行**逐行登记为 H1…H19（owner + 本文件小节 + 门禁载体 + 是否 kernel/06 管辖），**无缺号、无重复、无漏项**；非内核项（视觉回归 ≤0.1% / 硬编码色值 = 0 / 网格对齐 ≤0.5px）显式标注 owner（tools/design-gates、tools/tokens）与门禁（G3、S8、`tokens:check`、RP-05）并写明「不属 kernel/06 管辖」；表外对照 C1 = 双主题对比度（DC-13 / `tokens:check` [3]）；「输入字节等价」按 AR-24 补充写明**语料 owner = kernel/05、方法 owner = kernel/06**（06 §3.9） |
| **L-14** | 输入消息数值权威未落位 | **已修复** | kernel/05 §4.3 显式列出 **`MsgType::Input = 0x0100`**、**`MsgType::Paste = 0x0101`**，并写明与 kernel/07 §3.2 一致、**名字与数值的语义 owner = kernel/05 §4.3**（05:300） |
| **L-16** | S 编号双命名空间（PR 分级 S0–S5 vs 设计静态 S1–S10） | **已修复** | spec 07 §3.4.2 分级改名 **PR-S0…PR-S5**，更新本文件全部引用（`PR-S0`…`PR-S5`、`PR-S0–PR-S3`、`PR-S0–PR-S4`、`PR-S4`），并在 §3.4.2 末补 **AR-30 第 5 条编号消歧** 一句；**历史变更记录段落（07:411/413/419）保持原样不改写**；本索引引用与 kernel/06 §3.7 的裸「S3」同步为 PR-S* |
| **L-17** | kernel/07 §3.6 内部引用笔误 | **已修复** | 07 §3.6 `SessionLifecycle` tag 1 的「六态以 kernel/04 **§3.2** 为准」改为 **§3.1**（已核对：04 §3.1 = Session 状态机；§3.2 = Log 段格式）（07:246）。**残留已清除**：`docs/spec/03` §3.6 的同型笔误（把 kernel/04 状态机引为 §3.2）已在本轮改为 **§3.1**（03:169），与本节修法一致 |
| **L-13** | kernel/02 PTY-AC-09…12 仍是提案未升格门禁 | **已修复** | 02 §8.1 四条全部标「已决（AR-30）」：PTY-AC-09 升 §5 门禁（P99 ≤2ms）、PTY-AC-12 升 §8.2 验收（=100%，≤2s）、PTY-AC-10 并入 AR-25 第 3 条、PTY-AC-11 并入 AR-24 第 1 条，均不单列（02:249–252/283–292）；本索引 §4 表同步 |
| **L-18** | kernel/07 §3.8 自陈未登记项只有一句 | **已修复（显式暂不登记）** | 07 §3.8 缺口说明扩为**逐分册表**：kernel/03（正文无独立错误枚举，失败面为 FrameState 降级 / DeviceLost / RP-13/RP-14 断言）、kernel/05（`InputError` / `ClipboardError` / `EncodeOutcome::Dropped(DropReason)`）、kernel/06（`BenchError` / `Verdict` / `FailKind`）各给**现状 + 处置 + 暂不登记理由 + 触发条件**，并声明登记规则「暴露前先补入本表再冻结」（07 §3.8 表末） |
| **OQ-SES-01** | Session 状态枚举三方不一致（04 六态 vs 03-spec §3.6） | **已决** | kernel/04 §8 标为「已决：采纳 §3.1 六态为唯一枚举；`docs/spec/03` §3.6 已同步」；`docs/spec/03` §3.6 已补「状态枚举口径（以 kernel/04 为准）」一行 + 六态映射（03:169）；kernel/07 §3.6 tag 1 同口径。索引同步：§5.1 已决 13→14 条、§5.2 P0 开放组 22→21 条 |

### C. 已关闭缺口（本次刷新：设计上不做 / 按「不修改历史」纪律，不再保留为未决）

| # | 缺口 | 状态 | 关闭依据 |
| --- | --- | --- | --- |
| **L-12** | **spec 07 历史变更记录口径陈旧**：§变更记录仍写「G7（设计静态门禁 **S1–S9**）与 G8（**B1–B8**）」，而 §2.3-B12、§3.4.1-G7、§5-A17 均已为 **S1–S10 / B1–B10** | **按「不修改历史」关闭** | **AR-31 补充第 3 条**：spec 07 历史变更记录段落保持原样（不改写），不再作为缺口登记；现行层名与引用统一为 **S1–S10 / B1–B10** |
| **L-15** | **【新增】指标跨分册总数未核实**：旧版「54 项」无法逐条复核；本索引改按各分册 §8 定位登记，不维护总数 | **设计上不做（关闭）** | **AR-31 补充第 2 条**：跨分册总数是派生产物且反复漂移，改为**各分册 §8 自持**，索引不得声称跨分册总数 |
