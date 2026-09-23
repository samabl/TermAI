# 内核规格 01 · VT/ANSI 兼容性与一致性评审

> **效力**：本文位于 HARNESS.md 与 `docs/spec/03-system-architecture.md` 之下；与 AR/DC/§5 预算/§8 门禁冲突时以 HARNESS 为准。本文**只补机制、不改结论**。
> **范围**：termai-vt 的一致性口径、套件清单与运行方式、扩展协议优先级与子集边界、未识别序列行为契约、AR-18（vte 依赖置后）落地、conformance 进 CI 的格式与产物。
> **引用约定**：决策一律写 AR-xx / DC-xx / OQ-xx 编号，不复述措辞。本文新增的可测指标一律标 **【新增】**，且只在 §8 定义，不与 §5 并列。
> **编号空间（AR-28 第 2 条）**：本分册内部问题编号统一为 **OQ-VT-NN**（如 OQ-VT-01）；其它分册用各自前缀（OQ-PTY / OQ-RND / OQ-SES / OQ-INP / OQ-PM / OQ-ABI）。
> **上游依据**：AR-01、AR-02、AR-03、AR-04、AR-06、AR-14、AR-18、AR-19、AR-20、AR-22、AR-23；DC-15、DC-16、DC-17、DC-19、DC-20、DC-21、DC-22、DC-23、DC-24、DC-37；HARNESS §5 / §7 / §8.1；`docs/spec/03` §3.4/§3.6/§3.7；`docs/spec/07` §3.3/§3.4/§3.7；`docs/spec/02` §3.12；ADR-0012 / ADR-0014 / ADR-0015；角色 04（冻结证据）。

## 1. 范围与依据

| 依据 | 对本文件的强制含义 |
| --- | --- |
| AR-18 | vte 仅作**依赖**（非 fork）置于 trait 边界之后；OSC/kitty/iTerm2/Sixel/grid 语义层自研；锁版本 + 记录上游差异；替换只按 trait 边界发生 |
| AR-19 / §5 | 只用「发布门禁 / 目标值」双列口径；本文**不引入**与 §5 并列的预算，新增量一律进 §8 |
| AR-01 / AR-02 | 网格与图形协议在 Rust 侧；WebView 不承载任何 VT 语义，WebView 缺失不影响 G1 |
| AR-03 | 内核不依赖 AI/网络：全部一致性用例必须离线、headless、可单机复跑 |
| AR-04 / DC-22 / DC-24 | OSC 133/633/7 的语义经 core-dto IDL 定义并版本化；热路径禁 JSON；测试只经 headless + Local API，不私开后门 |
| AR-06 | 剪贴板类「外发型」序列按 L2/L3 处理；相关确认不可由配置关闭 |
| AR-14 | conformance 只判**网格与状态语义**；像素与 AA 归 §8.1-3（L7 视觉回归），两者不得互相掩护 |
| AR-22 / AR-23 §6 | 软换行默认开=视觉折行、关=视觉裁剪；**不改字符内容、网格列数、复制结果**（对应 `spec 02` A20/A34） |
| DC-16 / DC-19 / §3.6 | ConPTY 与 Transport 的语义差异必须逐项登记，且**不得污染 parser 车道结论** |
| DC-20 / DC-21 | termai-vt 不反向依赖 render/session；滚动缓冲与 grid 语义分离 |
| DC-23 | Session Log 为唯一真相：conformance 断言的状态必须可由 Log 重建比对 |
| DC-37 / §8.1-6 | VT fuzz 24h 无 crash 且累计 ≥10⁸ 次执行；历史 crash 100% 转回归语料 |
| §7 | P0 出口 = vttest + esctest 全通过；P1 = OSC 133/633、Local API、headless |
| §8.1-1 | vttest / esctest / kitty 套件 **100%**；xterm 兼容用例 **≥99%**（差异登记在案） |
| ADR-0012 | 分阶段 VT 策略与替换触发条件原文（本文 §3.10 量化） |
| ADR-0014 | T0/T1/T2/T3 能力边界；E60 扩展套件（Sixel / kitty graphics、OSC 9/777、OSC 8 超链接、kitty keyboard 探测、剪贴板所有权切换、bidi）；门禁只在 T0 判定 |
| ADR-0015 | 测试工具与语料属「构建期独立进程工具」，位于链接边界外（BT/T/AE 条件）；但**测试依赖不得把 GPL/AGPL 引入链接边界** |

**与 HARNESS 的差异登记（均为操作定义补充 / 收窄 / 范围明确化，不改任何 AR/DC/§5/§8 数值）**

| # | 事项 | 性质 | 处置 |
| --- | --- | --- | --- |
| D-1 | 三个测试车道（parser / transport / e2e）与 G1 的对应关系 | 操作定义补充：§8.1-1 未定义「在哪一层判 100%」 | 采纳，见 §3.1 / §5 |
| D-2 | 未识别序列的配额、计数键；诊断**不落原始字节** | 收窄：隐私（§6.1 secret）+ DoS 安全默认值 | 采纳；新增量进 §8（OQ-VT-01） |
| D-3 | OSC 52 **读取**永不实现 | 收窄：HARNESS 未规定剪贴板读取 | 采纳；复议条件见 §7 |
| D-4 | iTerm2 内联图像计入 v1 必须（受限子集） | 范围明确化：ADR-0014 T2 行已枚举该协议族 | 采纳，子集见 §3.5 |
| D-5 | G1 需要独立 `cargo xtask conformance` 动词 | **冲突点**：`docs/spec/07` §3.2.1 规定 xtask 子命令增删须走 ADR | 提 ADR；过渡期用 `cargo xtask test --suite conformance`（OQ-VT-12） |

**范围外**：像素级视觉回归（§8.1-3，归 05 / L7）；PTY 与 Transport 实现（`spec 03` §3.6）；AI 工具 ABI 与审批（DC-26…DC-35）；termai-ipc 帧格式（`spec 03` §3.4）。

## 2. 关键结论

### K-01｜通过率必须双数：严格通过率与门禁通过率
- **结论**：每套件同时报告 `R_strict = P/(E−S_cap)` 与 `R_gate = (P+X)/(E−S_cap)`。崩溃、超时、未执行**一律计 F**；`S_cap` 只允许来自套件 manifest 的**静态能力前置**（如 `graphics.sixel`），禁止运行时人工 skip。100% 套件要求 `R_strict=1 且 X=0`；xterm 套件要求 `R_gate ≥ 0.99` 且 `|X|` 受上限约束（OQ-VT-04）。
- **理由**：把 skip 关在门外、把崩溃算作失败，是唯一能防「跑不起来就绿灯」的口径；双数防止用登记差异掩盖未实现。
- **代价**：环境抖动会直接红，必须用 RM-A 自托管 runner；T2/T3 上必然不可用的图形用例只能由能力前置排除，从而**不得**计入任何门禁结论（对齐 ADR-0014 的 NON-GATING）。

### K-02｜三个测试车道，ConPTY 不污染 parser 门禁
- **结论**：L0 parser lane（字节直喂 `VtBackend`，无 PTY 无渲染）、L1 transport lane（经 Local PTY / ConPTY / SSH / Container / WSL）、L2 e2e lane（termai-headless + Local API + replay）。**100% 套件在 L0 判定**，L1 只判字节保真与网格一致且差异逐项登记，L2 承载 §8.1-2 回放门禁。
- **理由**：ConPTY 会重编码输出、插入额外序列（DC-16 / 角色 04 §D2），若把 100% 挂在 ConPTY 之上，Windows 门禁将永久性红或被迫注水。
- **代价**：同一缺陷可能在两条车道上以不同现象出现，需要一次归因；L1 的 ConPTY 差异登记表会长期存在条目（成本记入 R4）。

### K-03｜多 oracle 仲裁顺序：规范 > 文档 > 实现
- **结论**：判定优先级固定为 **ECMA-48 / Unicode 标准 > xterm ctlseqs 文档 > xterm 实现行为 > esctest 期望**。当 xterm 实现与规范冲突且我方选择规范时，该用例进入差异登记表并附条款引用；仲裁结果由 T1 记录在用例的 `oracle_note` 字段。
- **理由**：单一实现 oracle 会把上游 bug 固化成我方规格；多 oracle + 显式仲裁能把「跟随谁」变成可审计决定。
- **代价**：需要维护 oracle 采集环境（Xvfb + 固定 xterm 版本 + 固定 locale/font），并承担少数用例「与主流终端观感不一致」的代价。

### K-04｜差异登记表：准入靠证据，复核靠双签
- **结论**：登记项 owner = T1 Core Kernel；复核 = T1 lead + 1 maintainer **双签**；涉及剪贴板/超链接/通知/图形/字符串上限时**必须** Security 会签，涉及用户可见行为差异时必须 01/02 会签。准入四条件：① 附最小复现目录；② 在 RM-A + T0 后端可复现；③ 附 oracle 分歧证据（条款或 ctlseqs 行号）；④ 严重度非 S1。有效期 ≤2 minor 且 ≤6 个月（先到者为准，对齐 DC-40），过期即 G1 红。
- **理由**：差异登记是唯一被允许的「已知偏差」，它的可信度完全取决于证据与签字；单签 + 无到期日就是橡皮章（`spec 07` Q2）。
- **代价**：登记流程变慢，小差异也要走会签；换取的是「差异登记表可信」。

### K-05｜扩展协议优先级与子集边界见 §3.5（v1 必须 / 可选 / 永不）
- **结论**：v1 必须 = OSC 133、OSC 633、OSC 7、OSC 0/2、OSC 8、OSC 9/777、OSC 52 写、Sixel、kitty graphics、kitty keyboard（实现+探测，默认不开启）、iTerm2 内联图像（受限子集）；永不 = OSC 52 读取、未知 DCS 原文透传、iTerm2 文件落盘（v1）。
- **理由**：`spec 07` M12 第 ⑧⑨ 项与 ADR-0014 的 E60 已把上述协议纳入 v1 必修，本文件只负责划出「必须实现的子集」与「明确不做」的边界。
- **代价**：图形与通知面扩大 DoS 与钓鱼面，必须靠配额、限流与 scheme 白名单兜底（OQ-VT-02/06/07）。

### K-06｜未识别序列四态契约：忽略 / 透传 / 回显 / 计数
- **结论**：默认**忽略**（完整消费、绝不吞后续字节）；**透传**仅限白名单（tmux passthrough、kitty graphics、DECRQSS、XtGETTCAP）；**回显永远禁止**（含参数与中间字节）；**计数**为唯一可见面，且只记「计数键 + 类别 + 字节长度 + final byte」，不记参数内容。完整规则见 §3.4。
- **理由**：「不认识的序列吃掉后续输出」的事故根因是字符串态无终止/无上限地吞字节。用「三种终止（BEL / ST / 上限）+ ESC 中止 + CAN/SUB + 计数」把它变成一条可测不变式。
- **代价**：极少数依赖「未知序列原文进网格」的调试脚本会失效；诊断能力被刻意限制（这是隐私要求，不是缺陷）。

### K-07｜AR-18 落地：trait 边界 + 精确钉版本 + 差异记录 + 量化替换触发
- **结论**：对外契约是自研 `VtBackend` / `EscapeSink`（§3.2），**绝不**直接暴露 `vte::Perform`；依赖声明用精确版本（`vte = "=0.15.0"`）+ `Cargo.lock` 入库；上游差异记录在 `tests/conformance/registry/upstream-divergence.toml`（schema 见 §3.10）；替换触发条件量化见 §3.10，只有满足其一才启动 clean-room 替换。
- **理由**：trait 边界把「vte 语义」关在 termai-vt 内部，使替换成本可预算；精确钉版本把「上游静默行为变化」变成 CI 上的显式失败。
- **代价**：失去自动获得上游 minor 修复的便利（安全补丁走 ≤7 天升级窗口）；`ParseError` 的字节偏移在 vte 后端上需字节级回放才能定位（§3.10，性能仅影响诊断路径）。

### K-08｜软换行 / 视觉裁剪是纯视觉层，VT 语义不随之改变
- **结论**：网格结构保持「逻辑行 × 列」；折行与裁剪只影响呈现。**OSC 8 超链接归属逻辑行**、选区与复制取完整逻辑行、折行提示不参与选区、不进字符流、不被 AI 上下文读取。超列宽字符在裁剪态下仍存在于网格，但不上屏。
- **理由**：AR-22、AR-23 第 6 条与 `spec 02` A20 / A34 的「开/关两态复制结果逐字节相同」是正确性断言，VT 侧必须定义数据归属才能实现它。
- **代价**：渲染层需要逻辑行→视觉行的二次映射与超链接区间投影，跨层 bug 面增加（UX-R12），需专门的回归用例（§5 V-10）。

### K-09｜golden 网格快照 + 回放 + 最小复现是唯一真相格式
- **结论**：所有套件的判定都归约为「同一语料 → 规范化网格快照 + sink 调用轨迹 + 计数器」的比对；golden/replay/repro 三种格式由本文件唯一定义（§3.7/§3.8），报告 schema 为 `conformance-report.json`。
- **理由**：一套格式才能让 vttest / esctest / kitty / xterm / 回放共用失败定位与最小复现管道；否则五个套件五套脚手架。
- **代价**：需要写一层「套件 → 语料」的采集适配器（vttest 是交互式程序，必须用驱动器录制/回放，见 §3.9）。

### K-10｜默认 `$TERM` 保持 `xterm-256color`，同时提供 `termai` terminfo
- **结论**：v1 默认 `TERM=xterm-256color`、`TERM_PROGRAM=TermAI`、`COLORTERM=truecolor`；另行发布 `termai` / `termai-256color` terminfo（`tic -x` 编译零告警、`infocmp` 基线 diff 稳定）。能力探测（DA1/DA2、`XTGETTCAP`、kitty query）用于进取特性，不用于基础行为。**已决（AR-31 第 2 条）：v1 默认 `xterm-256color` + 随包 `termai` terminfo，P2 评估切换。**
- **理由**：默认保兼容是入场券；自定义 terminfo 先作为 opt-in，避免第三方 terminfo 缺失导致的显示退化（角色 04 §3.6 的取舍）。
- **代价**：默认 `$TERM` 无法表达 TermAI 独有能力，需靠能力查询补齐；`termai` terminfo 存在分叉维护成本。

## 3. 详细设计

### 3.1 测试车道与门禁映射

| 车道 | 被测对象 | 输入注入 | 判定物 | 与门禁的关系 |
| --- | --- | --- | --- | --- |
| L0 parser lane | `termai-vt` 的 `VtBackend` + `EscapeSink` 适配器 | 语料字节流直喂（无 PTY、无渲染、无 UI） | 规范化网格快照 + sink 调用轨迹 + `VtCounters` | **G1 的 100% / ≥99% 只在此判定**（RM-A + T0） |
| L1 transport lane | Local PTY / ConPTY / SSH / Container / WSL | 同一语料经 PTY 写入 | 字节保真统计 + 网格一致性 + 差异条目 | 门禁项：**差异 100% 登记**；ConPTY 差异不得回写 L0 结论 |
| L2 e2e lane | termai-headless + Local API + 图形限额 | `.trec` 回放脚本 | 最终网格哈希 + 结构化事件 + 退出码 + 限额行为 | **G2 行为回放 ≥99.5%**（§8.1-2）；图形限额与降级在此验证 |

图形协议在 L0 只验证「DCS/APC/Sixel 载荷被完整接收、不污染网格、计数器正确」，在 L2 验证配额、降级与能力协商。**任何车道的失败都必须产出最小复现目录（§3.8），否则视为脚手架缺陷，不是用例失败。**

### 3.2 核心接口签名（`crates/termai-vt/src/backend.rs`）

```rust
/// 唯一对外解析契约；vte 只存在于 impl 内部（AR-18）。
pub trait VtBackend: Send {
    fn advance(&mut self, bytes: &[u8], sink: &mut dyn EscapeSink) -> AdvanceReport;
    fn reset(&mut self);
    fn id(&self) -> BackendId;          // Vte { version } | CleanRoom { rev }
    fn caps(&self) -> BackendCaps;      // 见下；能力差异必须显式声明，不得静默
}

pub struct AdvanceReport {
    pub consumed: usize,                        // 已消费字节数（偏移推进的唯一来源）
    pub first_error: Option<ParseError>,        // 首个未识别/畸形序列；vte 后端由诊断模式逐字节回放补齐
    pub counters: VtCounters,                   // K-06 的计数键
}

#[derive(Clone, Copy)]
pub struct ParseError { pub offset: u32, pub state: ParserState, pub kind: ParseErrorKind, pub final_byte: u8 }

pub struct BackendCaps {
    pub eight_bit_c1: bool,        // UTF-8 模式下是否识别 8-bit C1
    pub byte_offsets: bool,        // 是否原生报告字节偏移（vte = false → 诊断模式逐字节回放）
    pub osc_len_limit: u32,        // 【新增】§8 OQ-VT-01
    pub dcs_len_limit: u32,        // 【新增】§8 OQ-VT-01
}

pub trait EscapeSink {
    fn print(&mut self, ch: char);                                              // 网格字符（含 U+FFFD 替换）
    fn execute(&mut self, byte: u8);                                            // C0
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
    fn osc_dispatch(&mut self, params: &[&[u8]], term: StringTerm);              // BEL | ST | Aborted | Overflow
    fn dcs_hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);
    fn dcs_put(&mut self, byte: u8);
    fn dcs_unhook(&mut self, term: StringTerm);
    fn sos_pm_apc(&mut self, kind: StringKind, params: &[&[u8]], term: StringTerm);
}

pub enum StringTerm { Bel, St, Aborted, Overflow, Cancelled }
```

`vte` 通过私有 `VteAdapter<'a>(&'a mut dyn EscapeSink)` 实现 `vte::Perform` 接入；**`vte::Perform`、`vte::Parser`、`vte::Params` 不得出现在任何 crate 的公开签名中**，由 `cargo xtask depcheck` 的 API 泄漏检查 + 一条编译期用例（外部 crate 仅依赖 `termai-vt` 即可编译）守住。

### 3.3 解析状态机与字符串终止规则（对应职责 4、5）

| 状态 | 输入 | 动作 | 计数键 |
| --- | --- | --- | --- |
| Ground | C0（除 CAN/SUB/ESC） | `execute()` | — |
| Ground | `0x9B` 且 UTF-8 模式开 | 按 UTF-8 载荷处理（不得当 C1） | `c1_8bit_in_utf8` |
| Ground | `0x9B` 且 UTF-8 关 | 进 CsiEntry | `c1_8bit_used` |
| Ground | ESC | 进 Escape | — |
| Escape | `[ ] P X ^ _` | 分别进 CsiEntry / OscString / DcsEntry / SosPmApc | — |
| Escape | `\` | 空 ST，忽略 | `st_stray` |
| Escape | 其他 | `esc_dispatch()` | 未识别则 `esc_unknown` |
| CsiParam/CsiIntermediate | 非法字节 | 进 CsiIgnore；遇 `0x40–0x7E` 以 `ignore=true` 派发 | `csi_malformed` |
| CsiEntry/Param | 未实现 final | `csi_dispatch(ignore=true)`，只消费该序列 | `csi_unknown` |
| OscString | BEL | `osc_dispatch(term=Bel)` → Ground | — |
| OscString | ESC `\` | `osc_dispatch(term=St)` → Ground | — |
| OscString | ESC + 其他 | **中止字符串**（`term=Aborted`），随后按 Escape 处理该 ESC | `osc_aborted` |
| OscString | 长度 > `osc_len_limit` | `term=Overflow`，丢弃载荷，**回 Ground 继续解析后续字节** | `osc_overflow` |
| DcsPassthrough | ESC `\` | `dcs_unhook(St)` → Ground | — |
| DcsPassthrough | BEL | 属数据，继续 | `dcs_bel_in_data` |
| DcsPassthrough | 长度 > `dcs_len_limit` | `term=Overflow` → Ground（Sixel / kitty graphics 超限同样走此路） | `dcs_overflow` |
| 任意字符串态 | CAN `0x18` / SUB `0x1A` | 中止 → Ground | `string_cancelled` |
| 任意状态 | 非法 UTF-8 序列 | 替换为 U+FFFD 经 `print()` 上屏，**不丢弃整段** | `invalid_utf8` |

**不变式（可断言）**：① 字符串态**必有**终止条件（BEL / ST / 上限 / CAN / SUB / ESC 中止），不存在无界吞字节路径；② `consumed` 单调且 `Σ consumed == 输入长度`；③ 任何序列被忽略后，其后的 Ground 字节必须照常进入 `print()/execute()`。三者在 `tests/conformance/corpus/unterminated/` 上以属性测试 + 语料固定用例双重覆盖。

### 3.4 未识别序列四态契约（职责 4）

| 处理 | 规则 | 反例（必须失败） |
| --- | --- | --- |
| **忽略** | well-formed 但未实现（未知 CSI/ESC/OSC 号/DCS）→ 完整消费该序列，网格零变化，后续字节全部正常解析 | 未知 OSC 之后整行输出消失 |
| **透传** | 仅白名单：`DCS tmux;…`（内层需再次受本契约约束，禁止直接注入网格）、kitty graphics DCS（图形层消费）、DECRQSS、`XTGETTCAP`；其余 DCS/APC/PM/SOS **一律丢弃** | 未知 DCS 原文上屏或进入 scrollback |
| **回显** | **永不允许**把未识别序列回显到屏幕（含参数、中间字节、原始 ESC/BEL） | 用户在屏幕看到 `]11;?` 之类的残片 |
| **计数** | `VtCounters` 只记：计数键、类别、载荷字节长度、首个 final byte。**不记参数内容**（避免 secret 落盘，§6.1 / AGENTS §6）；唯一可见面是用户显式打开的 `termai diag vt --unknown`（本地、不入日志、不入遥测） | 日志里出现 OSC 载荷原文 |

计数键全集：`csi_unknown`、`csi_malformed`、`esc_unknown`、`osc_unknown{num}`、`osc_aborted`、`osc_overflow`、`dcs_unknown`、`dcs_overflow`、`dcs_bel_in_data`、`string_cancelled`、`st_stray`、`c1_8bit_in_utf8`、`c1_8bit_used`、`invalid_utf8`。计数可作为回放断言点（`.trec` 的 `ASSERT_COUNTER`），也用于生成「本会话被忽略的序列摘要」结构化事件（供 UI 提示，不含参数）。

### 3.5 扩展协议优先级与子集矩阵（职责 3）

| 协议 | v1 状态 | v1 必须实现的子集 | 明确不做 / 永不 | 依据 |
| --- | --- | --- | --- | --- |
| OSC 133 | **必须** | `A` / `B` / `C` / `D;exit` | 不定义新的 133 子命令；不猜测缺失退出码；`P;` 属性统一走 633 路径 | M12 ⑧ |
| OSC 633 | **必须** | `A` / `B` / `C` / `D;exit` / `E;cmd` / `P;Cwd=` / `P;IsWindows=` | 命令文本不进 AI 上下文/遥测（L2 敏感，§6.3），仅本地结构化持久 | M12 ⑧、AR-13 |
| OSC 7 | **必须** | 百分号解码、`file://host/path` 解析、host 归属标记；远程 cwd 不得直接用于本地文件操作 | 不跟随 OSC 7 自动 chdir 本地进程 | M12 ⑨、AR-06（跨主机） |
| OSC 0 / 2 | **必须** | 标题设置；剥离控制字符、长度上限、**剥离 bidi 覆盖字符**（防标题欺骗） | 标题不参与 OSC 8 / 不进 AI 上下文 | M12 ⑨ |
| OSC 8 | **必须** | 超链接区间、`id=` 分组、hover 显示目标域名、点击需修饰键、scheme 白名单（OQ-VT-07） | 不允许 `javascript:` / `data:`；不允许 OSC 8 改变复制文本 | E60 |
| OSC 9 / 777 | **必须** | `9;body`、`777;notify;title;body`；**先分子命令**（`9;4;…` 是 ConEmu 进度，不是通知） | 不支持通知内 action button / 不解析可点击载荷；限流 OQ-VT-06 | E60 |
| OSC 52 | 写 = **必须**，读 = **永不** | 写：目标集合 `c/p/s/0-7`、base64 解码、载荷上限（OQ-VT-08） | **读取（`?`）永不实现、不可由配置开启** | E60「剪贴板所有权切换」、AR-06 |
| Sixel | **必须** | DECSIXEL 基本指令、256 色 + 调色板、RLE `!`、`$` / `-`；配额与超限降级 | 不支持 ReGIS / DECELG；不做动画 | E60、角色 04 §3.5 |
| kitty graphics | **必须** | 直接传输（`t=d`）、分块（`m=1`）、PNG/RGB/RGBA、放置 `a=p`、删除 `a=d`、`z` 序、响应 `OK/ENOENT/EINVAL`、配额 | **不做** Unicode placeholder、动画（`a=f`）、相对放置、文件传输落盘 | E60、ADR-0014 T2 |
| kitty keyboard | **必须** | 增强标志协商、`CSI > u` / `CSI = u` / `CSI ? u` 查询、`CSI < u` 弹出；**默认不开启，按应用探测启用**（OQ-06） | 不默认开启；不改变未协商应用的按键语义 | E60、ADR-0012 复议 3 |
| iTerm2 内联图像 | **必须（受限子集）** | `OSC 1337;File=…[;inline=1]` 单块 base64、`width/height/name/preserveAspectRatio` | **不做** FilePart 分块文件落盘（v1）、`SetUserVar` 之外的 iTerm2 专有属性 | ADR-0014 T2 行、D-4 |
| 未知 DCS / APC / PM / SOS 原文 | **永不** | — | 一律丢弃并计数（§3.4） | K-06 |

降级：T2/T3 后端由 capability manifest 声明 `graphics.* = false`，L0 用例以静态能力前置排除（非人工 skip）；该车道结果标 NON-GATING（ADR-0014），UI 必须明示「无 Sixel」等具体缺项（AR-20 诚实原则）。

### 3.6 Shell 集成状态机（OSC 133/633 + OSC 7）

```
Idle ──A──▶ PromptOpen ──B──▶ PromptClosed ──E;cmd──▶ CommandCaptured
                                    │                        │
                                    └──────────C─────────────┴──▶ Executing ──D;n──▶ Finished
Finished ──(emit CommandBlock{cmd_id, argv?, exit_code, cwd, started_at, duration_ms, confidence})──▶ Idle
```

- **配对守卫**：无 `C` 的 `D` 丢弃并计 `osc133_unpaired`，**不得**产 CommandBlock；连续 `A` 覆盖并计 `osc133_overlap`。
- **退出码**：缺失即 `exit_code = None`，**绝不猜测、绝不美化**（DC-05）；下游必须能区分 `Some(0)` 与 `None`。
- **置信度**：来源枚举 `OsC133 | OSC633 | Heuristic`。无集成脚本时退化为提示符启发式（提示符正则 + 输出静止窗口），事件必带 `confidence = Low` 与 `source = Heuristic`；Context Builder 必须原样透传给 AI 的不可信信封（`spec 03` §3.6，禁止低置信度伪装成高置信度）。
- **超时**：`PromptOpen` 停留超过提示符态超时（默认见 OQ-VT-05）→ 计 `osc133_stale` 并回 `Idle`。
- **cwd**：`OSC 7` 与 `633;P;Cwd=` 合并为单一 `cwd` 事件，带 `host` 与 `provenance`；两者冲突时以**更晚到达者**为准并计数 `cwd_conflict`。
- **Schema 归属**：`CommandBlock` 由 core-dto IDL 定义，兼容窗口 ≥2 minor（AR-04 / DC-23）。

### 3.7 golden 网格快照格式（v1）

```
TERMAI-GRID 1
meta {"cols":80,"rows":24,"cursor":[17,3],"cursor_visible":true,"alt":false,"wrap":false,
      "origin":false,"modes":"0x0000000000000000","scroll":[0,23],"title":"…","backend":"vte-0.15.0"}
row 0000 0x9f2c… |escaped text with \xNN, \0 for wide-char right half|
attr 0000 0000 0012 fg=#c0caf5 bg=default flags=bold,underline
link 0000 0004 0021 id=h1 target=https://example.com/
end
hash blake3:…
```

- **规范化**：属性 run 按 `(row, col)` 升序；宽字符次列写 `\0`；组合字符保留原码点序列附加在主字符后（**不做 NFC 归一化**）；默认空白写单个空格；未设置属性不写 `attr` 行。
- **哈希**：`hash` 行不计入自身；哈希覆盖 `meta` 至 `end` 的字节。`link` 行是 OSC 8 的归属证据——**逻辑行坐标系**（K-08），与折行/裁剪无关。
- **禁止**：golden 内不得出现时间戳、内存地址、构建哈希、随机 id；出现即视为不规范（CI 校验）。
- 像素比对**不属于本格式**（归 §8.1-3 的 L7 视觉回归），两者不得互相顶替。

### 3.8 回放格式与最小复现产物

`.trec`（文本，UTF-8，行内字节用 `\xNN` 转义，大流量可外挂 `<case>.bin`）：

```
TERMAI-REPLAY 1
meta {"cols":80,"rows":24,"term":"xterm-256color","env":{"TERM_PROGRAM":"TermAI"},
      "requires":["graphics.sixel"],"expected_grid":"blake3:…","deterministic":true}
+0.000000 PTY_OUT 1b5b3f3235 6c
+0.000100 KEY "echo hi\n"
+0.512000 RESIZE 120x40
+0.600000 ASSERT_COUNTER osc_unknown[9001] == 1
+0.600000 ASSERT_EVENT CommandBlock{exit_code:0,source:"OSC633"}
end
```

时间戳为**虚拟时间**，仅用于 damage 合并与静止窗口判定；`deterministic=true` 时忽略真实时钟，保证同输入同哈希。

**失败最小复现（每个失败用例必须产出，缺产物 = CI 红）**：`target/conformance/<suite>/<case>/` 下含 `actual.grid`、`expected.grid`、`diff.txt`（首分歧：**输入字节偏移、状态、解码后的序列、行/列、期望 vs 实际**）、`stream.bin`（原始 PTY 字节）、`input.trec`、`env.json`（cols/rows/term/backend/version/commit/GPU 档）、`repro.sh`、`report.json`。CI 日志内联打印首分歧行。字节偏移来自 `AdvanceReport.consumed` 与 `ParseError.offset`；vte 后端在 `--trace-offsets` 诊断模式下逐字节回放补齐偏移（仅诊断，不影响门禁计时）。

### 3.9 差异登记表、通过率报告与复核流程（职责 1、2、6）

`tests/conformance/registry/deviations.toml` 字段（**准入规则见 K-04**）：

| 字段 | 含义 |
| --- | --- |
| `id` / `case_id` / `suite` | 登记号 / 用例 / 套件（`xterm`、`kitty`、`iterm2`、`sixel` 允许登记；`vttest`/`esctest`/`kitty-core` 恒为空表） |
| `sequence` / `span`(cols,rows) / `requires` | 触发序列 / 适用网格 / 能力前置 |
| `observed` / `expected` / `oracle_note` | 现象 / 期望 / 仲裁依据（条款或 ctlseqs 行号） |
| `severity` | S1 阻塞（禁止登记）/ S2 降级 / S3 装饰 |
| `root_cause` / `workaround` / `upstream_issue` | 根因 / 规避 / 上游反馈链接（如适用） |
| `owner` / `reviewers` / `security_signoff` | 恒为 T1 Core Kernel / 双签名单 / 布尔 |
| `created_at` / `expires_minor` / `expires_at` | 创建 / 过期 minor（≤2）/ 过期日期（≤6 个月） |
| `evidence` | 最小复现目录路径（§3.8），**必填** |

报告 `conformance-report.json`：`{suite, suite_version, oracle, lane, backend, gpu_tier, machine_fingerprint, cases:{total,executed,passed,failed,excluded_cap}, x_registered, r_strict, r_gate, gate, registry_digest, artifacts[], commit}`；`gate` ∈ `PASS|FAIL|NON_GATING|INCONCLUSIVE`（RM-A/T0 之外恒为 `NON_GATING`，对齐 `spec 07` §3.8.3）。

**运行方式**：L0 由 `cargo xtask conformance --suite {vttest,esctest,kitty,xterm,terminfo} --lane parser --report …`（verb 归属见 D-5 / OQ-VT-12；过渡期 `cargo xtask test --suite conformance`）。vttest 为交互式程序，由 `tests/conformance/tools/vttest-driver`（`expect` 式菜单驱动 + 屏幕抓取）转录为 `.trec` + `golden`；esctest 以钉定 revision 运行（Python 驱动 + pyte 参考解释器），版本与 SHA-256 记入 `suites.toml`；xterm 用例集按 ctlseqs 条目 1:1 建案（基数 OQ-VT-03），oracle 为固定版本 xterm + Xvfb（K-03）；terminfo 由 `tic -x` + `infocmp` 基线 diff + `tack` 校验。PR 阶段（`spec 07` S3，≤10min）跑 xterm 子集 + terminfo + 语料回归；nightly（S4）跑全量五套件 + 三车道。

### 3.10 AR-18 落地：adapter、版本钉、上游差异与替换触发

**上游差异记录** `tests/conformance/registry/upstream-divergence.toml`：`{id, upstream_behavior, spec_ref(ECMA-48/xterm ctlseqs 条款), our_expected, class(语义/能力/性能), workaround(none|sink|adapter|post), affected_cases[], upstream_issue, first_seen_version, recheck_by, status}`。记录字段与差异登记表**分开维护**：前者是「vte 与规范/我方期望的分歧」，后者是「我方与 xterm 用例集的分歧」。

**替换触发条件（满足其一即启动 clean-room 替换评估，走 RFC → ADR；ADR-0012 复议条件 1 的量化）**：

| 触发 | 量化阈值 | 证据产物 |
| --- | --- | --- |
| 上游停滞 | 连续 ≥12 个月无 release 且无安全维护 | 上游 release/tag 与 commit 时间线快照 |
| 协议无法承载 | ≥2 项 P0/P1 必需协议在**不 fork** 前提下无法实现（如字节级透传、字符串上限可控、8-bit C1 策略） | 每项一份失败尝试记录 + 差异记录条目 |
| 差异累积 | 差异记录中 `class = 能力` 条目 >3，或 workaround 成本合计 >1 人月 | 条目清单 + 估算 |
| 安全 | 未修复 upstream CVE 存在 >90 天 | 公告链接 + 影响面分析 |
| 性能归因 | §5「解析+渲染吞吐 ≥500MB/s（门禁）」在 RM-A/T0 失败且归因于 adapter/vte | `bench-report.json` 归因报告 |

**升级策略**：精确版本钉（`=x.y.z`）；安全补丁走 ≤7 天窗口（OQ-VT-10）；任何 vte 版本变更必须在 L0 上重跑全量套件与 trait 契约差分测试（同一语料在 vte 后端与 clean-room 后端上的 sink 调用轨迹必须逐事件一致，`ParseError` 除外）。

## 4. 接口与依赖

**我需要谁给什么**

| 对方 | 我需要 | 用途 / 失败后果 |
| --- | --- | --- |
| T1 Core Kernel | `VtBackend`/`EscapeSink` 实现、`VtCounters`、grid 与 damage 模型、图形层消费接口 | §3.2 契约与 §3.3 不变式的落点；缺失则 G1 无法在 L0 判定 |
| T1 Core Kernel | ConPTY/Local PTY 的字节保真埋点与差异候选清单 | L1 车道与 DC-16 差异登记 |
| T2 Shell & UX | OSC 8 渲染/hover/click、标题显示、软换行与裁剪的逻辑行映射 | K-08 与 E60 的 OSC 8 子集 |
| T3 Agent & Context | `CommandBlock`/`cwd` IDL schema 与置信度透传约定 | §3.6 事件可被 AI 正确消费 |
| T5 DevEx & Release | RM-A 自托管 runner、`xtask` 接线、语料与产物存储、豁免统计 | §3.9 报告与 K-04 有效期执行 |
| Security (08) | 剪贴板/超链接/通知/图形/字符串上限的安全裁定、fuzz 目标 | K-05 的安全子集与 OQ-VT-01/02/06/07/08 |
| Product / UX (01/02) | 通知与超链接的用户可见行为裁定、诊断面板文案 | 收窄项的代价确认 |

**我向谁承诺什么**

| 对象 | 承诺 |
| --- | --- |
| T1 | 五套件 + 三车道的运行方式、`golden/replay/repro` 格式、trait 契约差分测试、上游差异记录 schema |
| T5 | `conformance-report.json` schema、PR/nightly 分级与预算、失败最小复现的完整性与首分歧定位 |
| Security / 产品 | 未识别序列不回显不落原始字节、OSC 52 读取永不实现、图形与通知配额、bidi 覆盖字符剥离 |
| 全体 | 门禁口径不与 §5 并列、不新增预算；新增量一律在 §8 定义并标【新增】 |

## 5. 可验证验收

| # | 验收项 | 测量方法 | 语料 / 工具 | 判据 | CI 落点 |
| --- | --- | --- | --- | --- | --- |
| V-01 | vttest 通过 | vttest-driver 驱动菜单 → 转录 `.trec` 回放 → 网格比对 | 钉定 vttest + driver，L0/L1 | **100%**（`R_strict=1, X=0`，§8.1-1） | G1；S3 子集 / S4 全量 |
| V-02 | esctest 通过 | 钉定 revision，Python 驱动 + pyte 参考解释器 | esctest（SHA-256 记入 `suites.toml`） | **100%**（§8.1-1、§7 P0） | G1；S3/S4 |
| V-03 | kitty 套件通过 | 图形/键盘协议用例 + 响应校验 | kitty 套件 + 自建向量集 | **100%**（§8.1-1） | G1；S4 |
| V-04 | xterm 兼容用例 | ctlseqs 每条目 ≥1 用例（≥2000 条）+ 真实语料 ≥20%（**已决：AR-31 第 1 条**）；oracle = xterm + Xvfb | vim/htop/neovim/fzf/tmux/less/btop | **`R_gate ≥ 0.99`**，且每条差异在登记表内（§8.1-1） | G1；S3/S4 |
| V-05 | terminfo 校验 | `tic -x` 编译零告警；`infocmp` 基线 diff；`tack` 交互项 | `termai`/`termai-256color` + `xterm-256color` | 100%，diff 为空 | G1；S3 |
| V-06 | 行为回放 | `.trec` 回放断言网格哈希 + 事件 + 退出码 | `tests/replay/` 真实会话语料 | **≥99.5%**（§8.1-2） | G2；S2/S4 |
| V-07 | 未识别序列四态 | 四组专项语料：忽略 / 透传白名单 / 回显禁令 / 计数 | `corpus/unknown/` | 100%；回显断言为「屏幕字节零变化」 | G1；S3 |
| V-08 | 字符串上限与中止 | 未终止 OSC/DCS、超限 payload、ESC 中止、CAN/SUB | `corpus/unterminated/` + 属性测试 | 100%；**后续 Ground 字节必须照常上屏** | G1；S3/S4 |
| V-09 | 扩展协议子集 | E60（≥60 用例）+ M12 第 ⑧⑨ 项 | `spec 07` §3.7、ADR-0014 | E60 全绿；M12 12 组合全绿 | G1 + L6；nightly |
| V-10 | 软换行 / 裁剪语义不变 | 软换行开/关两态复制逐字节比对 + OSC 8 区间投影 | `spec 02` A20/A34 用例 | 两态复制**逐字节相同**；链接区间不变 | G1 + G8（UX-G17）；S3/nightly |
| V-11 | trait 契约差分 | 同一语料在 vte 后端与 clean-room 后端上比对 sink 调用轨迹 | `corpus/**` | 逐事件一致（`ParseError` 除外） | G1；S4 |
| V-12 | fuzz | `cargo fuzz` + 语料种子（含全部 conformance 流与历史 crash） | fuzz targets: `vt` | 24h 无 crash 且累计 **≥10⁸** 次（DC-37 / §8.1-6）；crash 100% 转回归 | G6；nightly/周度 |
| V-13 | 性能 | `termai-bench`，release，RM-A + T0 | 1GB 语料 | 吞吐 **≥500MB/s（门禁）**；回归 >5% 阻断（§8.1-4） | G4；S4 |
| V-14 | 失败最小复现完备性 | 扫描每个 failed 用例目录的 8 个产物 | `repro.sh` 可独立重放 | 产物齐备且 `repro.sh` 复现同一首分歧 | G1 自检；S3/S4 |
| V-15 | 计数与报告 schema | `conformance-report.json` 校验 + 断言 `Σcases == total` | JSON Schema + CI | schema 通过；`r_strict/r_gate` 与逐件统计自洽 | G1；S3 |

V-01/V-02 为 **P0 出口**（§7）；V-04 的差异条目上限、V-07/V-08 的配额阈值、V-09 的图形解码预算均为 **【新增】，定义见 §8**（OQ-VT-01…OQ-VT-08），本表不给出与 §5 并列的数值。

## 6. 风险与降级

| # | 风险 | 触发条件 | 降级行为 | 用户可见后果 |
| --- | --- | --- | --- | --- |
| R-V1 | 上游套件漂移（用例改名/删除/语义变化） | 钉定 revision 的用例集哈希变化 | 拒绝升级并冻结语料；差异写入 `suites.toml` | 无（门禁保持可复现） |
| R-V2 | vte 上游静默行为变化 | L0 全量或差分测试失败 | 回退到上一钉定版本（≤1 天）；开差异记录条目 | 无；安全补丁延迟（升级窗口 ≤7 天） |
| R-V3 | ConPTY 重编码造成 L1 差异 | Windows 车道 xterm 用例失败 | 登记为 DC-16 差异；L0 结论不受影响 | Windows 上个别 TUI 观感异常，登记表可查 |
| R-V4 | 图形协议 DoS（Sixel/kitty 炸弹） | 超配额或解码超时 | 丢弃该图 + 计数 + 结构化事件；超限连续 N 次禁用图形直至重启 | 图片不显示并给出「已超限」提示，终端不卡死 |
| R-V5 | 未识别序列吞输出事故 | `osc_aborted/overflow` 后网格丢行 | 由 §3.3 不变式阻断（回 Ground）；回归语料固定 | 无（若回归失败则门禁红，不得发版） |
| R-V6 | 差异登记橡皮章化 | 单季度豁免 >3 次（`spec 07` Q2） | 差异表冻结，已过期条目强制清理；超限交 TSC | 已知偏差收敛变慢（可接受） |
| R-V7 | 8-bit C1 与 UTF-8 边界 | `c1_8bit_in_utf8` 异常升高 | 以 ECMA-48 + K-03 仲裁；修 sink 层策略 | 极端脚本输出错位，用例层可定位 |
| R-V8 | 软换行/裁剪与 VT 语义泄漏 | 复制内容或 OSC 8 区间与逻辑行不一致 | 关闭软换行仅影响渲染；不一致即 S1 阻断 | 无（正确性优先于视觉） |
| R-V9 | oracle 分歧（xterm 自身 bug） | 与 ECMA-48 冲突且我方取规范 | 差异登记 + `oracle_note`；不改我方行为 | 与 xterm 观感不同的极少数场景，文档可查 |
| R-V10 | 诊断面泄漏内容（参数进日志） | 评审发现日志出现 OSC 载荷 | 立即移除字段；诊断只保留计数与长度 | 无（隐私不可协商，§6.1） |

## 7. 被否决的方案与反方意见

| 被否决方案 | 反方最强理由 | 我方反驳 | 复议条件 |
| --- | --- | --- | --- |
| A 直接以 clean-room 自研解析器为起点 | 架构自主、无上游耦合、终局正确（ADR-0012 反方 10） | 重复已验证的苦力劳动，P0 时间与 fuzz 风险不可接受（ADR-0012 否决 B） | §3.10 任一触发条件成立 → 启动替换评估（ADR + RFC） |
| B Fork vte 并在 fork 内改造 | 上游差异可控、改得动、补丁可自持（角色 04 D3 的 fork-pin 措辞） | AR-18 明确「依赖非 fork」；fork 的上游同步与安全补丁负担高于收益（ADR-0012 否决 A） | 仅当 §3.10 触发且 clean-room 评估未完成时，作为 90 天内的临时过渡 |
| C 单一 oracle = xterm 实现 | 简单、可复现、生态事实标准 | 会把上游 bug 固化成规格；规范与文档才是可引用依据（K-03） | 若多 oracle 维护成本连续 2 个季度超预算 → 降为「xterm + 规范」双 oracle |
| D 允许 skip 计入通过率分母排除项 | 环境不适用的用例不该红（现实主义者） | 正是「跑不动就绿灯」的入口；改为**静态能力前置**（K-01） | 无（不可复议；能力前置已覆盖合法场景） |
| E 未知 OSC/DCS 回显到屏幕（便于调试） | 开发者能看到终端收到了什么 | 「不认识的序列吃掉/污染输出」是经典事故（§3.4 反例） | 无；调试需求由 `termai diag vt --unknown`（本地、不含参数）满足 |
| F OSC 52 读取默认开启 | 远程场景需要读远端剪贴板（tmux 用户刚需） | 剪贴板读取是外发型数据外泄，与 AR-06 / §6.1 冲突（D-3） | 仅当实现为「每次读取须用户显式确认（L3 + 二次输入目标）」时才可复议 |
| G 直接把 `vte::Perform` 作为对外契约 | 少一层适配、性能损失最小 | 语义泄漏使替换成本不可预算，违反 AR-18 的 trait 边界意图（K-07） | 无；若 adapter 被证明是吞吐门禁的归因（§3.10 性能触发），改用「隔离 crate + 编译期禁泄漏」而仍不暴露 vte 类型 |
| H golden 用像素/PNG 帧 | 一图胜千言、覆盖渲染问题 | 与 AR-14 灰度 AA、字体版本强耦合，diff 噪声高；像素属 §8.1-3 的 L7（K-09） | 无；渲染问题由 L7 视觉回归与 L0 网格双层覆盖 |
| I kitty graphics 全量（Unicode placeholder / 动画）进 v1 | 生态完整度、避免二次返工 | v1 成本与依赖不可控；E60 只要求限额与降级（§3.5） | 应用采纳率或 issue 量证明 placeholder 为刚需 → P3 提升 |
| J 差异登记表由单人批准 | 流程快、不阻塞 | 单签差异表就是装饰（`spec 07` Q2）；双签 + 到期日是可信度前提（K-04） | 无（可增加 delegate 名单，但签字数不变） |

## 8. Open Questions

> 未决项不得用「TODO」代替；每条给出影响面、建议值与必须决策的 Phase，经 RFC → ADR 后回填 HARNESS §11。**【新增】= 本文件新增的可测指标，其阈值不与 §5 并列。**

| # | 问题 | 影响面 | 建议值 | 决策阶段 |
| --- | --- | --- | --- | --- |
| OQ-VT-01 | 【新增】字符串态载荷上限：`osc_len_limit` / `dcs_len_limit`（决定内存上界与截断行为）（**已采纳默认值（AR-31 采纳分诊建议）**） | DoS 面、合法大载荷（长标题、图形分块）、§5 空闲 RSS ≤120MB | **已采纳默认值**：OSC 1 MiB；DCS/APC 16 MiB；SOS/PM 1 MiB；超限即 `Overflow` + 计数（返工范围：`termai-vt` 常量 + `corpus/unterminated/` 与 fuzz 种子重建 + §5 RSS 上界重估） | P0（**已冻结：ADR-0023 D2**） |
| OQ-VT-02 | 【新增】图形配额与预算：单图大小、每屏张数、合成/解码头预算（**已采纳默认值（AR-31 采纳分诊建议）**） | 帧时门禁（4K@120Hz <8.3ms）、内存、用户体验 | **已采纳默认值**：单图 ≤16 MiB、每屏 ≤64 张（角色 04 §3.5）；单图解码头 ≤50ms P95、每帧图形合成 ≤4ms（返工范围：01 图形解码器配额 + V-09 用例 + RP-02 帧时归因复核） | P0 |
| OQ-VT-03 | 【新增】xterm 兼容用例集基数与覆盖口径（ctlseqs 1:1？）（**已决：AR-31 第 1 条**） | V-04 的 ≥99% 是否可信、差异表规模 | **已决（AR-31 第 1 条）**：xterm 用例 ≥2000 条，ctlseqs 每条目 ≥1 用例，真实语料占比 ≥20%；G1 数字必须可复现、可解释 | **已决（AR-31）** |
| OQ-VT-04 | 【新增】差异登记表条目上限与豁免率口径 | K-04 的可执行性、`spec 07` Q2 统计 | G1 全局 ≤25 项；单 minor 新增 ≤10；豁免率入季度审计 | P1 |
| OQ-VT-05 | 【新增】提示符态超时值；OSC 633 命令文本的本地保留策略 | §3.6 状态机、AR-13 / OQ-04 持久化默认值、隐私 | 超时 30s；命令文本本地持久但默认不进 AI 上下文与遥测 | P1 |
| OQ-VT-06 | 【新增】通知（OSC 9/777）限流与动作能力 | 钓鱼与骚扰面、E60 用例 | 每应用 ≤5/min、全局 ≤1/s；不支持 action button | P1 |
| OQ-VT-07 | 【新增】OSC 8 scheme 白名单与点击门槛 | 安全（`javascript:`/`data:`）、可用性、a11y | 允许 `https/http/mailto/ssh` + `file`（仅本地 host）；点击需 Cmd/Ctrl；hover 显示 host | P1 |
| OQ-VT-08 | 【新增】OSC 52 写载荷上限与目标集合 | 剪贴板安全、大文本复制、E60 | 上限 1 MiB（base64 解码后）；目标 `c/p/s/0-7`；拒绝其他目标并计数 | P1 |
| OQ-VT-09 | 默认 `$TERM` 与 `termai` terminfo 的取舍与迁移路径（角色 04 §八-3）（**已决：AR-31 第 2 条**） | 兼容性（OQ-VT-10 同源）、能力表达 | **已决（AR-31 第 2 条）**：v1 默认 `xterm-256color` + 随包附带 `termai` terminfo；P2 评估切换 | **已决（AR-31）** |
| OQ-VT-10 | vte 版本钉策略、升级窗口与安全补丁 SLA（**已采纳默认值（AR-31 采纳分诊建议）**） | AR-18 落地、R-V2 | **已采纳默认值**：精确钉 `=x.y.z`（取当前最新稳定）；每 minor 评估一次；安全补丁 ≤7 天；升级须全量 L0 + 差分测试（返工范围：Cargo.toml + `suites.toml` + V-01…V-15 golden 基线重录，**近 A**） | P0 |
| OQ-VT-11 | clean-room 替换触发的最终量化阈值（§3.10）是否符合 ADR-0012 复议条件 1 | 架构自主性、P1/P2 资源分配 | 采纳 §3.10 五条阈值；任一成立即 RFC | P1 |
| OQ-VT-12 | G1 的 xtask verb 归属（新增 `conformance` vs 复用 `test --suite`）（**降为 P1：AR-31 D3/D4**） | D-5 冲突点、`spec 07` §3.2.1 的 ADR 要求 | **降为 P1**：新增 `cargo xtask conformance`（走 RFC → ADR）；占位 = P0 全部 G1 判定用 `cargo xtask test --suite`，并在 `spec 07` §3.2.1 登记过渡用法。**复评触发**：P1 首次 CI 工具链收敛时 | P1 |
| OQ-VT-13 | 网格内 OSC 8 链接如何满足 AR-20（raw 朗读 + 应用模式切换） | a11y 承诺边界、是否会变成新的 a11y 缺口 | UI 层提供「链接清单」面板 + 键盘可达的打开动作；不承诺网格内语义化 | P1 |
| OQ-VT-14 | 8-bit C1 默认策略与 oracle 分歧仲裁结果（**已采纳默认值（AR-31 采纳分诊建议）**） | R-V7、Sixel/kitty 之外的兼容面 | **已采纳默认值**：UTF-8 模式下不识别 8-bit C1，只计数 `c1_8bit_in_utf8`；非 UTF-8 模式按 K-03 识别（返工范围：vte adapter + `corpus/` C1 用例 + V-04 差异表） | P0 |
| OQ-VT-15 | iTerm2 内联图像子集边界与 T2/T3 能力协商字段命名 | D-4、能力 manifest 与 UI 明示文案 | v1 仅 `inline=1` 单块；能力字段 `graphics.iterm2_inline` | P1 |
| OQ-VT-16 | 【新增】conformance 产物的保留期与体积上限 | CI 成本（`spec 07` §3.4.3 月度上限）、失败可追溯性 | 失败产物永久（随 S3 归档）；通过产物 30 天；单产物 ≤50 MiB | P1 |