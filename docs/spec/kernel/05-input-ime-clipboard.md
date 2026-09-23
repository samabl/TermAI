# 05 · 输入栈 / IME / 剪贴板内核规格（Input / IME / Clipboard Kernel Spec）

> 文件：`docs/spec/kernel/05-input-ime-clipboard.md` ｜ 状态：**v1（P0 信任基座，可施工）** ｜ 上游权威：`HARNESS.md` v1.0
> 冻结证据：`docs/roles/04-terminal-core-architect.md`、`05-frontend-architect.md`、`07-ai-agent-lead.md`、`08-security-privacy-architect.md`、`10-cto-engineering-lead.md`（历史证据，冲突以 HARNESS 为准）
> 下层依据：`docs/spec/02`（A20/A34/UX-G17、§3.5 键位优先级）、`docs/spec/03`（§3.14 T1 输入热路径、OQ-A7）、`docs/spec/05`（§1.4/§2.1）、`docs/spec/06` §3.5（`clipboard` capability）、`docs/spec/07`（§3.3 L3/L6、§3.4 G1/G2/G6）、`ADR-0001`、`ADR-0012`、`ADR-0014`、`ADR-0017`
> 编号空间：本文件属内核域，内部编号为 `K-xx`（结论）/ `IK-Gxx`（门禁提案）/ `OQ-INP-xx`（待决）/ `IN-xx`（配置键）；**不占用** AR/DC/OQ 编号，需落进 HARNESS 的一律在 §8 声明。
## 1. 范围与依据（引用具体 AR/DC 编号）
### 1.1 范围内

| # | 交付域 | 主要依据 |
| --- | --- | --- |
| 1 | 键盘事件管线（平台事件 → 内核编码 → 应用 PTY 写入）与各阶段唯一责任、修饰键/死键/Alt/Meta/Option-as-Meta | AR-03、AR-04、AR-19、DC-16、DC-18、DC-22；§5 key-to-photon |
| 2 | kitty keyboard protocol 与 modifyOtherKeys 的取舍、探测、默认值与回退编码 | **OQ-06**、AR-18、ADR-0012 §复议 3 |
| 3 | IME 组合输入生命周期、候选窗原生定位、preedit 与网格关系、CJK 回归用例 | AR-01（L4）、AR-20、AR-14；ADR-0014 M12-④ / E60 |
| 4 | 焦点契约（网格 / 侧栏 rail 与面板 / 设置窗口 / 命令面板 / tooltip / L4 浮层） | AR-23 第 1–5 条、DC-25、DC-18（单写者）；spec 02 A25/A26/A28/A33 |
| 5 | 选择与复制模型（字符级/视觉行、软换行复制语义、跨折行、宽字符半选、全选、块选择） | AR-22 第 4 条、**AR-23 第 6 条**、AR-14；spec 02 **A20 / A34 / UX-G17** |
| 6 | 剪贴板安全（OSC 52 读写默认值、粘贴保护、bracketed paste、敏感内容与审计） | AR-06、AR-12、DC-33、DC-34、DC-35、DC-39；spec 05 §2.1、spec 06 §3.5 |
| 7 | 鼠标事件、链接点击、无障碍输入路径 | AR-20、DC-37；spec 02 A2 / A8 / UX-G5 |
### 1.2 范围外

PTY/ConPTY 实现与 Transport（→ `02-pty-platform`）；VT 输出序列解析与图形协议限额（→ `01-vt-conformance`）；网格结构与 scrollback、网格渲染与 damage 重绘（→ `03-rendering-pipeline`）；键位表与 Action Registry 的归属与 100% 覆盖率门禁（→ `docs/spec/02` §3.5、DC-08）；AI 工具审批与 Policy Engine（→ `docs/spec/04` / `05`）；插件 ABI 与 capability 令牌签发（→ ADR-0006、spec 06）。
### 1.3 与 HARNESS 的对齐声明

1. 本文件**不新增**与 AR/DC/§5/§8 冲突的结论；所有预算引用 §5 的「发布门禁 / 目标」双列口径（AR-19），不引入第二口径。
2. 本文件提出的**新增可测指标**一律标 `【新增】` 并在 §8 提案；未获 TSC 批准前不得作为对外承诺或阻塞项。
3. 需上级仲裁的空白（输入正确性口径、OSC 52 默认策略的对外契约化）仍写入 §8；其中 `input.shortcut_precedence` 与 spec 02 优先级的交互已由 **AR-29 第 1 条**裁决（核心快捷键在 FocusRouter 层始终优先、冲突以改绑解决），见 §3.3 / IN-05 / OQ-INP-02。
## 2. 关键结论（K-01 起）

| # | 结论 | 理由 | 代价 |
| --- | --- | --- | --- |
| K-01 | **单一编码收口点**：一切进入 PTY 的输入字节一律由 `termai-core::input::InputEncoder` 产生；键盘 / 鼠标 / 粘贴 / IME commit / API 注入五类来源共用同一编码阶段与同一审计出口，禁止旁路 | 五种来源各自编码必然在 kitty / modifyOtherKeys / 鼠标模式上发散，破坏可回放性与可审计性（AR-04 / DC-34） | 所有平台分支必须汇入一个 trait；新增输入能力必须改内核而非在 UI 层打补丁 |
| K-02 | **输入热路径保持二进制无分配**：输入帧走 termai-ipc（长度前缀二进制），无 JSON、无 AI、无网络、无阻塞锁；编码器用栈缓冲 + 复用 `Vec<u8>` | AR-03 / AR-04 / AR-19 与 §5 key-to-photon 门禁 | 编码器不能借用 serde 便利性，调试需专用 dump 工具 |
| K-03 | **keyboard protocol 默认关（OQ-06）**：会话初始 `KeyboardMode::Legacy`（kitty flags = 0）；仅应用显式请求（`CSI > flags u` / `CSI = flags u` / `CSI < n u`）后切换；TermAI **不主动**发送任何改变应用行为的查询 | OQ-06「否，按应用探测后启用」；AR-18 要求协议层自研但不改默认行为 | 富协议收益只在少数应用上兑现；需维护 flags 栈与探测状态机 |
| K-04 | **kitty 与 modifyOtherKeys 互斥且 kitty 优先**：kitty 模式下再设 `CSI > 4;n m` 不生效，并记一次 `ProtocolConflict` 结构化事件 | 两套编码对同一按键产生不同字节，同时生效不可判定 | 需显式冲突检测与事件定义 |
| K-05 | **preedit 不进网格**：组合态只存在于 L4 原生层；preedit 不写网格、不进 scrollback、不进 PTY、不进 AI 上下文；只有 commit 产生一次 UTF-8 写入 | AR-01（网格是像素真相，非输入真相）、AR-04（可回放真相）、防 reflow / 选区污染 | 需在原生层自绘 preedit 下划线；跨三平台 IME 是本文件最高成本项（role 05 D7） |
| K-06 | **候选窗锚点由网格给出**：`term-render` 提供 caret 像素矩形与逻辑行→视觉行映射，L4 候选窗据此定位；L2 WebView 永不覆盖 | AR-01 §4.2 z-order（L0<L1<L3<L2<L4）；ADR-0001 §决策 3 | 网格层必须暴露稳定坐标接口；DPI / 缩放变化须 ≤1 帧重锚 |
| K-07 | **焦点唯一 + 单写者**：任一时刻恰有一个 focus owner 与一个 stdin 写者 lease（DC-18）；非写者端输入一律丢弃并显示「只读」，**禁止本地回显** | AR-13 多端 attach、spec 02 §3.1 / §3.8；本地回显会造成两端屏幕不一致 | 用户可能「打了字没反应」，必须用状态栏明示而非静默 |
| K-08 | **复制以逻辑行为基准**：选择坐标 = (逻辑行 id, 列, 侧)；软换行开/关与视觉裁剪是纯视觉层，复制结果逐字节相同；折行续行之间不插入任何字符 | AR-22 第 4 条、AR-23 第 6 条、spec 02 A20 / A34 / UX-G17、UX-R12 | 网格层必须维护逻辑行 ↔ 视觉行双射；reflow 后需 epoch 校验 |
| K-09 | **grapheme 边界不撕裂**：选择边界吸附到 grapheme 簇边界；双宽字符在流式选择中向外吸附为整字符，块选择中半选按策略补空格并记 warning | DC-17（grapheme cluster → 列映射）、AR-14 网格对齐 | 半选策略必须文档化；用户可能对「半个宽字符变空格」意外 |
| K-10 | **剪贴板默认拒绝读**：OSC 52 读默认 `deny`；写默认 `guarded`（单行纯文本放行，多行/含控制字符确认）；两者都写审计，但**审计与日志从不记录剪贴板内容** | AR-12（内容零采集）、§6.3 L2/L3 分级、AGENTS §7.4 保守默认；剪贴板劫持 → 粘贴执行是真实攻击链 | tmux / neovim 的「复制到系统剪贴板」体验受损，需来源标记 + 会话级授权缓解 |
| K-11 | **粘贴屏障**：未启用 bracketed paste 的多行粘贴必须确认；含 `ESC[201~` 的载荷一律先消毒；**消毒逻辑不可由配置关闭** | AR-06（危险动作不可静默）、AR-12；括号粘贴哨兵注入可提前结束保护区 | 多行粘贴多一次交互；消毒会改变用户载荷（须显式提示已消毒） |
| K-12 | **鼠标 / 链接默认本机**：应用未请求鼠标模式时鼠标属本机（选择 / 链接 / 滚轮）；Shift 为强制本机逃生阀；链接需显式修饰键点击，非 `http(s)/mailto/ssh` 一律确认 | 终端默认行为与安全；DC-35 默认拒绝 | 需按 DECSET 精确切换，避免 TUI 中鼠标行为漂移 |
| K-13 | **a11y 输入与 AR-20 同边界**：核心任务键盘完成率 100%；网格只暴露原始行文本 + 光标行列 + 选区范围，**不承诺 TUI 语义树**；读屏输入与 IME 共用同一原生文本输入路径 | AR-20、ADR-0012 决策 5；不夸大承诺 | 对 TUI 仅提供 raw 朗读 + 应用模式切换，部分用户会失望（ADR-0012 已接受） |
## 3. 详细设计
### 3.1 输入管线与阶段唯一责任

| # | 阶段 | 所在层 / 进程 | 唯一责任 | 禁止 |
| --- | --- | --- | --- | --- |
| S0 | 平台原始事件 | L4 / UI 进程 | 采集 OS key / IME / mouse 事件：scancode、修饰位、平台 keycode、组合态 | 解释为终端字节 |
| S1 | 焦点路由 `FocusRouter` | ui-native | 判定 focus owner 与写者 lease；非 owner 丢弃或转 chrome action | 把面板 / 窗口输入写入网格 |
| S2 | 平台翻译 `KeyTranslator` | ui-native | 死键合成、修饰键归一、Option-as-Meta、IME preedit / commit、鼠标模式判定 | 决定终端编码；写 PTY |
| S3 | 编码 `InputEncoder` | termai-core（契约）/ ui-native（调用） | 依 `KeyboardMode` 生成字节；粘贴消毒；OSC 52 判定入口 | 触碰 PTY / 网络；读剪贴板内容 |
| S4 | IPC 帧 | termai-ipc | `InputFrame{seq, kind, payload}`，长度前缀 + 版本 / 能力握手 | JSON / gRPC（AR-04） |
| S5 | 单写者闸门 + `PasteGate` | sessiond | lease 校验、粘贴屏障、审计出口、字节分块 | 改写已编码字节（消毒阶段除外） |
| S6 | PTY 写入 | termai-pty | 单次原子 write（不跨帧合并、不重排） | 缓冲 / 延迟刷写 |

```rust
// termai-core::input —— 叶子契约，不得依赖 session / agent / ui / 网络
pub enum KeyCode { Char(char), Named(NamedKey), Dead(u32) }
pub struct Modifiers { shift: bool, alt: bool, ctrl: bool, meta: bool, super_: bool, hyper: bool }
pub enum KeyEventType { Press, Repeat, Release }
pub struct KeyEvent { pub code: KeyCode, pub mods: Modifiers, pub ty: KeyEventType,
                      pub text: Option<CompactString>, pub scancode: u32 }
pub enum InputEvent {
    Key(KeyEvent), TextCommit(CompactString), Paste(PastePayload),
    Mouse(MouseEvent), FocusGained, FocusLost, ApiInject(InjectToken),
}
pub enum KeyboardMode { Legacy, ModifyOtherKeys(u8), Kitty(KittyFlags) }
pub struct KittyFlags { disambiguate: bool, report_events: bool, report_alt_keys: bool,
                        report_all: bool, report_text: bool, report_associated_text: bool }
pub trait InputEncoder: Send {
    fn encode(&mut self, ev: InputEvent, mode: &KeyboardMode, out: &mut InputSink) -> EncodeOutcome;
}
pub trait InputSink { fn write(&mut self, bytes: &[u8]) -> Result<(), InputError>; } // 仅 sessiond 实现，在 lease 内
pub enum EncodeOutcome { Emitted(usize), Consumed, Dropped(DropReason), NeedsConfirm(ConfirmId) }
```

**不变量 P-1**：S3 是唯一可产生 PTY 字节的阶段；`InputSink` 的实现位于 sessiond，且仅持有 stdin lease 的客户端可构造（编译期封装 `LeaseGuard`）。
**不变量 P-2**：`InputEvent::Paste` 必须携带 `origin: ClipboardOrigin`（本机粘贴 / OSC 52 / API 注入 / 插件注入），S5 依来源决定摩擦级别并写审计。
### 3.2 修饰键、死键、Alt/Meta 与 macOS Option-as-Meta

| 场景 | S2 归一化 | Legacy 编码 | 备注 |
| --- | --- | --- | --- |
| Shift + 字母 | shift=true，text = 大写 | 大写 UTF-8 | — |
| Ctrl + 字母 | ctrl=true | `0x01..0x1A` | Legacy 下 Ctrl+Shift+字母丢失 Shift（**已知限制**，由 kitty 模式补齐） |
| Alt + 可打印 | alt=true | `ESC` + UTF-8 | 由 `IN-03` 控制（默认 true） |
| Super / Meta（Mod 键） | meta/super=true | **默认不发往 PTY**，交 chrome Action Registry | `IN-04 input.meta_sends_escape=false`；Mod+K / Mod+B 等冻结键位依赖此默认 |
| 死键（´ + e） | `DeadState` 在 S2 内合成 | 合成后按字符编码 | 合成失败按平台 fallback 输出基字符，**不得双发**（基字符 + 死键） |
| IME preedit | 不进 S3 | — | 只有 `TextCommit` 进 S3（K-05） |
| 功能键 | `Named` | `CSI 1;{mod}X` | kitty 下 `CSI {code};{mod}u` |
| macOS Option | `IN-01 input.macos.option_as_meta=false` → 组合输入；true → alt 位 | 组合时由系统产出变音符；true 时 `ESC` + char | 默认 false 保 CJK / 变音符（§8 OQ-INP-07） |

死键状态机（S2 内，纯本地、无 I/O）：

```text
Idle --dead(d)--> Pending(d, deadline = 2s)
Pending(d) --compose(c)--> Idle   (emit Char(cluster(d, c)))
Pending(d) --non_compose(k)--> Idle (emit per IN-02: Platform | BaseCharAndKey | DropDead)
Pending(d) --timeout / focus_lost--> Idle (emit fallback)
```
### 3.3 kitty keyboard protocol 与 modifyOtherKeys 的取舍

- **探测（应用驱动，唯一标准路径）**：应用发 `CSI > flags u`（压栈并设置）、`CSI = flags u`（仅设置）、`CSI < n u`（弹栈）；查询 `CSI > u` 应答 `CSI ? flags u`。TermAI **永不**主动发送这些序列（K-03）。
- **能力宣告（终端→应用，不改变编码状态）**：`$TERM=xterm-256color` 保兼容；`TERM_PROGRAM=TermAI`；DA1 响应 + `termai` terminfo 的 kitty 能力项；`XTGETTCAP` 仅应答能力清单。宣告与启用严格解耦，避免「一宣告即改行为」。
- **modifyOtherKeys**：仅当应用显式 `CSI > 4 ; n m`（n ∈ {0,1,2}）且在 `KeyboardMode::Legacy` 下生效；与 kitty 同设时按 K-04 记 `ProtocolConflict` 并保持 kitty。
- **回退编码（未启用时）**：见下表；Legacy 为默认且必须 100% 兼容 xterm 用例集（§8.1 G1 ≥99%）。

| 按键 | Legacy（默认回退） | ModifyOtherKeys(2) | Kitty(disambiguate + report_all) |
| --- | --- | --- | --- |
| Ctrl+a | `0x01` | `0x01` | `CSI 97;5u` |
| Ctrl+Shift+A | `0x01`（丢 Shift） | `CSI 27;6;65~` | `CSI 65;6u` |
| Alt+x | `ESC x` | `ESC x` | `CSI 120;3u` |
| Esc | `0x1B` | `CSI 27;1;27~` | `CSI 27u` |
| Enter | `CR` | `CR` | `CR` |
| Left | `CSI D` / `SS3 D`（DECCKM） | 同 Legacy | `CSI 1;1D` |
| 文本输入（IME commit） | UTF-8 原样 | UTF-8 原样 | UTF-8，且不重复作为按键上报（report_text 语义） |

**协议状态机（sessiond 侧，按 pane 独立）**：

```text
Legacy --"CSI > F u"--> Kitty(F) ∈ stack           // 压栈并设置
Legacy --"CSI = F u"--> Kitty(F)                   // 仅设置
Kitty(F) --"CSI < n u"--> Kitty(pop n) | Legacy     // 栈空回 Legacy
Kitty(F) --"CSI > 4;n m"--> Kitty(F) + ProtocolConflict
Legacy --"CSI > 4;n m"--> ModifyOtherKeys(n)
任意 --"CSI ! p"（DECSTR 软复位）--> 编码模式复位，flags 栈清空
```

**键位优先级（AR-29 第 1 条）**：L4 平台原生 > **核心快捷键（在 FocusRouter 层始终优先）** > 网格内应用模式 > 用户 / 插件绑定。**核心快捷键不得让位于应用模式**——冲突以**改绑按键**解决，否则 Win/Linux（Mod = Ctrl+Shift）上用户无法可靠呼出命令面板与侧栏、也无法从卡死的应用中逃生。**网格内应用模式**的可测定义 = { DECCKM 置位 } ∪ { DECPAM 置位 } ∪ { `KeyboardMode != Legacy` }。保留逃生集永不被应用抢占：`Mod+Shift+Esc`（切换 `IN-05 input.shortcut_precedence = core_first | app_first`，session 级；默认 core_first）、窗口控制、IME 候选 / 菜单。**与 spec 02 的交互见 §8 OQ-INP-02（已决：AR-29）。**
### 3.4 IME 规格

```text
Idle --begin(preedit)--> Composing{preedit, caret_in_preedit, anchor_row, anchor_col}
Composing --update--> Composing
Composing --commit(text)--> Idle + emit TextCommit(text)      // 唯一产生 PTY 字节的出口
Composing --cancel--> Idle                                     // 无字节
Composing --focus_lost--> Idle { commit | cancel }（由平台决定，两者都不写网格）
Composing --resize / reflow / dpi_change--> Composing + re-anchor（≤1 帧；重锚失败 → clamp 主屏）
Composing --writer_lease_lost--> 禁止 commit：丢弃并提示「只读」，不写 PTY
```

| 平台 | API | 平台侧责任 | 降级 |
| --- | --- | --- | --- |
| Windows（x64，v1 门禁） | TSF（`ITfThreadMgr` + composition） | preedit 文本、候选列表、组合范围 | IMM32 兼容层；两者失败 → preedit 关闭并在状态栏明示 |
| macOS（arm64，v1 门禁） | `NSTextInputClient`（setMarkedText / insertText / firstRectForCharacterRange） | 组合文本、候选窗位置请求 | 系统必备，无降级 |
| Linux / Wayland | `text-input-v3`（fcitx5 / ibus） | 组合文本、set_cursor_rectangle | XIM（仅 X11）；两者皆无 → preedit 关闭 + UI 明示（AR-20 诚实边界）。**已决（AR-31 第 5 条）：P0 只要求 X11/fcitx5；Wayland 无 `text-input-v3` 时登记偏差 + UI 明示，不计 P0 失败** |
| Linux / X11 | XIM + fcitx5 | 组合文本 | preedit 关闭 |

**原生定位契约**：`PreeditHost::caret_anchor() -> GridRect`（屏幕物理像素 + DPI scale）；候选窗由平台层在 L4 创建，网格提供矩形，**WebView / 侧栏面板 / 设置窗口永不覆盖**（违反即架构事故，AR-01）。渲染装饰（preedit 下划线、候选高亮）不得写入网格单元格。

**CJK 回归用例集**（属 §5 门禁，不在本文件另立门禁）：ADR-0014 **M12-④「IME 组合输入与候选窗定位」**（12 组合全绿）+ **E60「IME 候选窗高 DPI 位移」「剪贴板所有权切换」**（4 主场组合）；本文件补充用例清单：日文假名→汉字转换、中文拼音多候选翻页、韩文谚文组合 / 分解、全角标点、预编辑中切换 pane、预编辑中窗口失焦、预编辑中 resize / reflow、候选窗在主屏边缘的 clamp、DPI 100/125/150/200% 切换、Emoji 候选提交（ZWJ 序列原子提交）。
### 3.5 焦点契约

| focus owner | 可接收键 | 明确不可接收 | 丢失焦点时行为 |
| --- | --- | --- | --- |
| Grid(paneId) | 未被 chrome / 应用模式抢走的键 → S2 → S3 | — | 停止编码；排队未编码事件丢弃；组合态交平台；**不回放** |
| SidebarRail | Tab / 方向键 / Enter / Esc | 字符键 | 焦点栈 pop → 回 Grid |
| SidebarPanel | 面板内控件键；文本输入控件内的字符键 | 把字符键写入 PTY | pop → 回 Grid 或上一 owner |
| SettingsWindow（独立窗口） | 窗口内控件键 | **送 PTY（架构禁止）** | Esc / 关闭 → 焦点回触发元素（AR-23 §3.3） |
| CommandPalette | 搜索、↑↓、Enter、Esc | Esc 之外的旁路 | Esc → 焦点栈 pop |
| Tooltip（L4） | 仅 Esc | 一切其它键；**不夺取 focus owner** | 回触发元素（AR-22 第 3 条） |
| NativeMenu / IME（L4） | 平台语义 | — | 回上一 owner |

```text
on_open(overlay):   focus_stack.push(current); current = overlay
on_close(overlay):  current = if focus_stack.top == overlay { focus_stack.pop() } else { pop_until(overlay) }
on_focus_change(new):
    if current == Grid && new != Grid { encoder.flush_pending(); drop_queued_key_events(); }
    if current != Grid && new == Grid { encoder.reset_modifier_state(); }   // 防止 Alt 卡住
invariant F-1: current != Grid ⇒ 来自该窗口的 PTY 字节数 == 0（由 S5 记账并断言）
invariant F-2: writer_lease 持有者 != 本端 ⇒ 输入丢弃 + 状态栏「只读」，字节数 == 0
invariant F-3: 任一时刻 focus owner 数量 == 1；L4 modal capture 不改 owner，只临时拦截
```

**焦点丢失时的键盘行为**：不发送任何补发 / 回放；不把已按下的修饰键残留传给下一位 owner（F-1 后强制 reset）；进行中的组合输入由平台语义决定 commit / cancel，但**两者都不得写网格或 PTY**（K-05）。
### 3.6 选择与复制

```rust
pub struct GridPoint { pub row: RowId /* 逻辑行稳定 id */, pub col: u16, pub side: CellSide }
pub enum CellSide { Before, After }
pub enum SelectionKind { Stream, Block }
pub struct Selection { pub anchor: GridPoint, pub head: GridPoint, pub kind: SelectionKind, pub epoch: u64 }
pub struct CopyPolicy { pub trim_trailing_ws: bool, pub wide_half: WideHalfPolicy, pub include_html: bool }
pub enum WideHalfPolicy { PadSpace, ExtendOutward }   // 默认 Stream = ExtendOutward，Block = PadSpace
pub struct CopyResult { pub text: String, pub warnings: Vec<SelectionWarning>, pub blake3: [u8; 32] }
```

**行模型**：缓冲行带 `WrappedContinuation` 属性（该行是上一行自动换行的续行）；**逻辑行** = 一行及其后所有连续 `WrappedContinuation` 行；`grid.epoch` 在 reflow / resize 重排后递增，`Selection.epoch != grid.epoch` 时选择失效并要求重选（复用 spec 06 `session.read` 的 epoch 语义）。

**复制算法（唯一实现，A20 / UX-G17 的施工依据）**：

```text
fn extract(sel, grid, policy) -> CopyResult:
  out = ""; warn = []
  for row in sel.rows_in_buffer_order():                 # 逻辑行序，与视觉折行无关
      (c0, c1) = sel.columns_for(row); line = ""
      for (col, cell) in grid.row(row).cells(c0..c1):
          match cell.width:
            Half          -> line += cell.grapheme
            WideL         -> if sel.covers(col, col + 1): line += cell.grapheme
                             else: line += " "; warn.push(HalfWide{ row, col })
            WideR         -> {}                            # 由 WideL 负责，防重复
            Continuation  -> {}                            # 组合字符 / ZWJ 随基字符
      last_of_logical = !grid.row(row + 1).is_wrapped_continuation()
      if last_of_logical && policy.trim_trailing_ws: trim_trailing_blanks(line)  # 只裁逻辑行末尾
      out += line
      if !grid.row(row + 1).is_wrapped_continuation(): out += "\n"               # 仅硬换行插换行
  return CopyResult{ text: out, warnings: warn, blake3: blake3(out) }
```

**规则**：① 续行之间的右端空白与内容**一律保留**（属逻辑行内部字符），只有逻辑行末尾才裁空白；② 折行续行标记、preedit 装饰、检索高亮**永不进入字符流**；③ 软换行开 / 关与视觉裁剪只影响显示，`extract` 输入相同 ⇒ `blake3` 相同（A20）；④ 块选择对半覆盖的双宽字符按 `PadSpace` 输出空格并记 warning，warning 计数进测试断言；⑤ 全选（`select_all`）= 整个 scrollback + 主屏，按同一算法；⑥ OSC 133 命令块复制（`copy_command_output`）复用同一算法，范围来自结构化事件而非屏幕文本。

**剪贴板数据格式与所有权（回应 `docs/spec/03` OQ-A7）**：Core 是唯一剪贴板仲裁者。写入格式为结构化 MIME 白名单：`text/plain;charset=utf-8`（必需，逐字节真相）、`text/html`（可选，由渲染层生成）、`application/x-termai-grid-range`（可选，保留 cell 属性与范围元数据）。L2 WebView 与插件**不得**直接调用 `navigator.clipboard`，必须经 shell-bridge → Local API → Core；跨 L0/L2 拖拽同样由 Core 接管指针与数据（AR-01 §合成规则 3）。
### 3.7 剪贴板安全：OSC 52、粘贴保护与审计

| 能力 | 默认（IN-06 / IN-07 / IN-08） | 可配置 | 企业策略（DC-35） |
| --- | --- | --- | --- |
| OSC 52 **读**（应用请求终端回传剪贴板） | `deny` | `deny / ask / allow`；ask 时按「来源会话 + 长度」提示 | 只能收紧到 `deny` |
| OSC 52 **写**（应用设置剪贴板） | `guarded`：单行且无 C0 / ESC → 静默；多行或含控制字符 → 确认 | `guarded / ask / deny`；**不提供 allow_all 静默多行** | 只能收紧到 `deny` |
| bracketed paste | 应用请求（DECSET 2004）即启用 | 不可由配置强制关闭（应用语义） | 不可削弱 |
| 多行粘贴（**无** bracketed paste） | 确认（行数 + 前 80 字符摘要 + 目标 pane + 风险 L2） | `ask / always`；不提供 `never` | 只能收紧到 `always` |
| 含 `ESC[201~` 的粘贴 | 消毒（移除哨兵 + 明示已消毒） | **不可关闭** | 不可削弱 |
| 含 ESC / C0（`\t`、`\r` 除外）的粘贴 | 确认 + 可选择「按纯文本粘贴」 | `ask / always` | 不可削弱 |
| 插件 / Local API 剪贴板 | `clipboard.read` = L2、`clipboard.write` = L1，默认拒绝（spec 06 §3.5） | 逐条授权，rule 档仅 L0 / L1 | 可禁用 |

```text
fn PasteGate::check(payload, ctx) -> PasteDecision:
  if payload.contains(BP_END)                     -> Sanitize{ removed: BP_END }        # 不可关闭
  if payload.len() > ctx.frame_limit              -> Chunked(n)                          # 依赖 spec 03 OQ-A4
  if ctx.bracketed_paste_active:
      if payload.contains_ctl_except_tab_cr:      -> Confirm{ risk: L1, reason: CtlChars }
      else:                                       -> Allow                              # 应用已进入保护区
  else:
      if line_count(payload) > 1 || payload.contains_ctl():
                                                  -> Confirm{ risk: L2, lines, head80, target_pane }
      else:                                       -> Allow
```

**审计与敏感内容（AR-12 / AR-06 / DC-34）**：剪贴板与粘贴事件写审计，字段固定为 `{ts, actor, session_id, direction, source, decision, reason, byte_len, line_count, policy_version}`——**永不记录内容、摘要、哈希或首字符**（内容零采集；head80 只进入 UI 展示，不落审计与日志）。粘贴内容进入 AI 上下文时必须由 Context Builder 打 `trust: untrusted` 信封并脱敏（spec 04 §3.8、DC-33）；脱敏失败默认拒绝发送。剪贴板内容**永不**进入遥测与崩溃上报（AR-12）。
### 3.8 鼠标、链接与无障碍输入

| 特性 | 触发（DECSET / 修饰） | 行为 | 默认 |
| --- | --- | --- | --- |
| 鼠标上报 | 1000（点击）/ 1002（拖拽）/ 1003（移动）+ 编码 1006（SGR）/ 1015 / 1016 | 仅应用请求时经 S3 编码为字节 | 关 |
| 本机鼠标 | 未请求任一鼠标模式 | 选择、链接、滚轮滚动 scrollback | 开 |
| Shift 逃生阀 | 任意时刻按住 Shift | 强制本机行为，不上报应用 | 开（不可关） |
| 滚轮 | DECSET 1007（alternate scroll） | 转为 ↑ / ↓ 上报 | 应用请求时 |
| 链接 | `OSC 8` 超链接优先；无 OSC 8 时按 `IN-09 links.detect_plain=true` 正则识别 | `Mod+Click` 打开；hover 显示**解码后**目标；http / https / mailto / ssh 直接打开；`file:` 与其它 scheme 需确认 | 见左 |
| 触屏 | 长按显示 tooltip（UX-OQ-17），长按链接显示确认菜单 | 不开浏览器 | 开 |

**无障碍输入路径（AR-20 边界内）**：① 读屏命名来自 `aria-label` 同源 i18n key（AR-22 第 3 条），tooltip 仅视觉增强；② 网格向平台 a11y API 暴露 `AXStaticText`（按**逻辑行**给文本，不按视觉折行）+ 光标行列 + 选区范围，**不构造 TUI 语义树**；③ 「应用模式」切换 = 在「按键直通 PTY」与「读屏 raw 朗读」之间切换，切换动作必须有键盘路径与 `aria-live` 播报；④ 核心任务（开 Tab / 分屏 / 切 Workspace / 恢复 / 调用 AI / 新建终端）键盘完成率 100%（spec 02 A2）；⑤ preedit 与候选窗对读屏可见（经原生 a11y 文本输入 API），preedit 文本走同一朗读路径。
### 3.9 时序与预算分摊

总口径只引用 §5：key-to-photon（本地）**门禁 P99 ≤16ms / 目标 P99 ≤8ms（P50 ≤4ms）**、冷启动到可输入 **门禁 P95 ≤150ms / 目标 ≤100ms**。以下**输入侧分摊为【新增】**（仅覆盖 S0–S6，不含 VT / 渲染 / present），需 §8 批准：

| 段 | 分摊（P99） | 测量点 |
| --- | --- | --- |
| S0→S1 平台分发 | ≤0.50ms | 原生事件时间戳 → FocusRouter 入口 |
| S1→S2 翻译（含死键 / IME 判定） | ≤0.10ms | Router 出口 → Translator 出口 |
| S2→S3 编码 | ≤0.05ms | Encoder 入口 → 字节出 |
| S4 IPC 往返 | ≤0.30ms | 帧 seq 打点（同机 UDS / named pipe） |
| S5→S6 闸门 + write | ≤0.20ms | PasteGate 出口 → write 返回 |
| **输入侧合计【新增】** | **≤1.15ms** | 余量 ≥14.85ms 留给网格路径 |

IME commit 路径约束：一次 commit = **一次** S6 写入（不分片、不逐键）；preedit 渲染延迟属平台所有，**不纳入** §5 门禁，另立输入侧门禁 `IK-G1`【新增】（判据与批准见 §8 OQ-INP-10；AR-31 已降为 P1）。
## 4. 接口与依赖
### 4.1 模块与依赖方向（不新增 crate，符合 DC-21 / AGENTS §3）

| 单元 | 归属 | 依赖 | 说明 |
| --- | --- | --- | --- |
| `termai-core::input` | 叶子契约 | 无（仅 core-dto 类型） | 类型 + trait + 编码器实现，无 I/O |
| `core-dto::input::v1` | IDL 叶子 | 无 | InputEvent / KeyboardMode / MouseEvent / PastePayload，版本化 + 能力协商（AR-04） |
| ui-native | L1 | → term-render、termai-core | 平台事件采集、FocusRouter、KeyTranslator、IME 宿主 |
| term-render | L0 | → termai-vt、termai-gpu | 提供 `GridCaretRect`、逻辑↔视觉行映射、选区渲染 |
| shell-bridge | L2 桥 | → ui-native | L2 剪贴板 / 输入请求中转与裁剪；禁止直连 `navigator.clipboard` |
| sessiond | 会话真源 | → termai-core、termai-pty、termai-ipc | lease 闸门、PasteGate、审计出口、PTY 写入 |
| termai-pty | PTY | → OS | 仅 S6 写入，不含编码逻辑 |

**红线**：`termai-core` 不得依赖 session / agent / ui / 网络（AR-03）；输入热路径禁 JSON / gRPC（AR-04）；新增 crate 或改变 `termai-ipc` 消息布局须走 RFC → ADR + 双重 CODEOWNERS。
### 4.2 关键 trait / 类型签名

```rust
pub trait FocusRouter { fn route(&mut self, ev: PlatformEvent) -> RoutedEvent; fn owner(&self) -> FocusOwner; }
pub trait PreeditHost {                                  // 平台 IME 宿主（ui-native 实现）
    fn begin(&mut self, anchor: GridRect);
    fn update(&mut self, preedit: &Preedit);
    fn commit(&mut self, text: &str) -> InputEvent;      // 产出 TextCommit
    fn cancel(&mut self);
}
pub trait SelectionEngine { fn extract(&self, g: &GridSnapshot, s: &Selection, p: &CopyPolicy) -> CopyResult; }
pub trait ClipboardBroker {                              // Core 唯一仲裁者
    fn read(&self, req: ClipboardRequest) -> Result<ClipboardData, ClipboardError>;
    fn write(&self, req: ClipboardWrite) -> Result<ClipboardReceipt, ClipboardError>;
}
pub trait PasteGate { fn check(&self, p: &PastePayload, ctx: &PasteCtx) -> PasteDecision; }
pub struct LeaseGuard { /* 编译期独占：仅持有 stdin lease 的客户端可构造 */ }
```
### 4.3 IPC / Local API / IDL

- **termai-ipc**：新增 `MsgType::Input`（**= 0x0100**；`{seq: u64, kind: u8, flags: u16, payload: Bytes}`，`kind` 区分键盘 / 鼠标 / IME commit / API 注入五源）与 `MsgType::Paste`（**= 0x0101**；大载荷分块；帧上限依赖 `spec 03 OQ-A4`）。**数值与 `kernel/07` §3.2 一致**；**名字与数值的语义 owner 为本分册（kernel/05）§4.3**，kernel/07 §3.2 只定帧封装、通道与丢弃自愈语义（旧名 `InputKey` / `InputPaste` 已废止）。破坏性变更走 capability 协商，兼容 ≥2 minor（AR-04 / DC-40）。
- **Local API（提案，需 ADR）**：`clipboard.get`（capability `clipboard.read`，L2）、`clipboard.set`（`clipboard.write`，L1）、`input.inject`（`session.write`，L2，每次确认，复用 `pty.write` 令牌与审计）。**改变对外契约，必须先 RFC → ADR**（AGENTS §5）；批准前 `clipboard.*` 只对第一方开放。
- **core-dto**：输入事件 schema 版本化；删除 / 改语义需 capability 协商，新增字段向后兼容 1 个 minor。
### 4.4 配置键（TOML，AR-09；层级合并 default < system < user < project < env < CLI）

| ID | 键 | 默认 | 说明 |
| --- | --- | --- | --- |
| IN-01 | `input.macos.option_as_meta` | `false` | false = 组合输入（保变音符 / CJK）；true = Alt 位（§8 OQ-INP-07） |
| IN-02 | `input.dead_key.fallback` | `platform` | `platform / base_char_and_key / drop` |
| IN-03 | `input.alt_sends_escape` | `true` | Alt 前缀 ESC |
| IN-04 | `input.meta_sends_escape` | `false` | Super 默认归 chrome（冻结键位依赖） |
| IN-05 | `input.shortcut_precedence` | `core_first` | **AR-29 第 1 条**：核心快捷键在 FocusRouter 层始终优先；`app_first` 仅作非默认兼容预设，冲突以改绑解决（§8 OQ-INP-02，已决） |
| IN-06 | `clipboard.osc52.read` | `deny` | deny / ask / allow（**AR-31 第 6 条：只能加强不能削弱**） |
| IN-07 | `clipboard.osc52.write` | `guarded` | guarded / ask / deny（**AR-31 第 6 条：只能加强不能削弱**） |
| IN-08 | `paste.confirm_unbracketed_multiline` | `ask` | ask / always（**AR-31 第 6 条：只能加强——`ask` 可升 `always`，不得降 `never`**） |
| IN-09 | `links.detect_plain` | `true` | 无 OSC 8 时的正则识别 |
| IN-10 | `copy.trim_trailing_whitespace` | `true` | 仅逻辑行末尾 |
| IN-11 | `input.kitty.emit_capability` | `true` | 仅宣告能力；不改编码状态 |
| IN-12 | `terminal.soft_wrap` | `true` | 对齐 spec 02 UX-OQ-15 / §3.12（纯视觉层） |

**永不配置化**：粘贴哨兵消毒、本机鼠标 Shift 逃生阀、写者 lease 校验、审计元数据写入；审计写入失败 → 输入 **fail-closed**（DC-34）。
### 4.5 对其他规格的依赖与承诺

| 对方 | 我需要 | 我承诺 |
| --- | --- | --- |
| `03-rendering-pipeline` | `GridCaretRect`、逻辑↔视觉行映射、reflow epoch、选区像素映射 | 选区坐标模型、折行不污染字符流的断言与语料 |
| `03-rendering-pipeline` | `WrappedContinuation` 属性、`RowId` 稳定 id、reflow 语义 | 复制算法与 A20 逐字节断言 |
| `02-pty-platform` | 单次原子 write、写失败结构化错误 | 只在 lease 内写入、字节不回改 |
| `docs/spec/02` | 键位优先级、焦点序、tooltip 规则 | 焦点状态机、F-1 / F-2 不变量、CJK / 候选窗用例 |
| `docs/spec/05`、`06` | capability 定义、脱敏收口 | 审计元数据 schema、无内容落盘证明 |
| `docs/spec/07` | G1 / G2 / G6 / L6 接线 | 语料、回放脚本、fuzz target 输入 |
## 5. 可验证验收（测量方法、语料或工具、判据、CI 落点）

> **口径（AR-31 第 4 条）**：§5 只收**可自动化的输入字节等价**（HARNESS §5 已有「输入字节等价 = 100%（逐字节）」一行）；**IME preedit 与候选窗正确性（IN-AC-04）不进 §5**，保留在 §8.2 主题验收 + L6 矩阵 + 人工会签。

| ID | 验收项 | 测量方法 | 语料 / 工具 | 判据 | CI 落点 |
| --- | --- | --- | --- | --- | --- |
| IN-AC-01 | 键盘编码兼容 | 套件回放 + 逐字节断言 | vttest / esctest / **kitty keyboard 套件**；xterm 用例集 | vttest / esctest / kitty **100%**；xterm **≥99%** 且差异登记 | **G1**（tests/conformance/） |
| IN-AC-02 | key-to-photon（本地） | 内置时序探针 + 高速相机（≥1000fps）校验探针偏差 | `termai-bench --metric latency`，RM-C / T0，≥10⁵ 事件 | **门禁 P99 ≤16ms**；**目标 P99 ≤8ms（P50 ≤4ms）**（§5） | **G4 / L4** |
| IN-AC-03 | 输入侧分摊【新增】 | 分段打点（S0→S6） | 同一探针的 breakdown 输出 | P99 ≤1.15ms（**未批准前不阻塞**；AR-31 已将 OQ-INP-10 降为 P1，占位照测、只进 G4 附加报告） | **G4** 附加报告 |
| IN-AC-04 | IME 组合与候选窗 | 人工 + 脚本矩阵；截图定位候选窗与 caret | **M12-④**；**E60**（高 DPI 位移）；12 组合 | 12 组合全绿；候选窗漂移 ≤2px【新增】（DPI 100/125/150/200%） | **L6 / A6**（tests/matrix/） |
| IN-AC-05 | preedit 不进 PTY / 不污染网格 | 回放断言：组合开始到 commit 之间 PTY 写入字节 = 0；网格 diff = 0 | `termai-headless` + replay 脚本 | 字节数 **0**；scrollback 无 preedit 文本 | **G2 / L3**（tests/replay/） |
| IN-AC-06 | 焦点契约 | 自动化不变量测试 + 字节记账 | F-1 / F-2 / F-3 断言；侧栏 / 设置 / 命令面板用例 | 非 Grid 聚焦时 PTY 字节 **0**；focus owner 唯一；Esc 后焦点回触发元素 | **UX-G14 / G7**（design-gates + 单测） |
| IN-AC-07 | 复制逐字节一致（A20） | 同一选区在软换行开 / 关两态复制，比较 BLAKE3 | 10k 行 golden grid（CJK 双宽、emoji+ZWJ、组合字符、DEC 图形、超长续行） | 两态哈希**相同**；逻辑行末尾外无字符被删 | **UX-G17**（spec 02 §5.3） |
| IN-AC-08 | 选择边界 | 表驱动单测 | 宽字符半选、grapheme 簇拆分、块选择半覆盖、reflow 后 epoch 失效 | 用例 100% 通过；warning 计数符合预期 | **G2** + 单测 |
| IN-AC-09 | 粘贴保护 | 恶意载荷用例 + 回放断言 | 哨兵注入、多行、C0 / ESC、超长、bracketed paste 开 / 关 | 哨兵消毒 100%；未括号化多行 100% 触发确认；未消毒载荷 **0** | **G6** + 单测 |
| IN-AC-10 | OSC 52 策略 | 安全用例 + 审计断言 + fuzz | 默认配置下读 / 写用例；审计字段扫描 | 默认读 **100% deny**；审计 / 日志中剪贴板内容命中 **0** | **G6 / DC-34** 检查 |
| IN-AC-11 | 鼠标与链接 | esctest / kitty 鼠标用例 + 手动矩阵 | 1000 / 1002 / 1003 + 1006 / 1015 / 1016；Shift 覆盖；`file:` 确认 | 模式切换 100%；Shift 逃生阀 100% 生效；非白名单 scheme 100% 确认 | **G1** + 手动 |
| IN-AC-12 | a11y 输入 | NVDA / VoiceOver 流程 + 键盘遍历 | 6 条 UI 层核心流程；核心任务键盘用例 | 键盘完成率 **100%**；6/6 流程通过；不承诺 TUI 语义树（明示） | **UX-G5 / A2 / A8** |
| IN-AC-13 | 行为回放（输入脚本） | 录制输入脚本 → 断言最终网格与状态 | `tests/replay/` | 通过率 **≥99.5%** | **G2 / L3** |
| IN-AC-14 | Fuzz | libFuzzer 长跑 | 输入编码器 + OSC 52 + 粘贴解析入 `fuzz/` targets | 24h 无 crash，≥10⁸ 次执行；历史 crash 100% 回归 | **G6 / DC-37** |
| IN-AC-15 | 配置与降级 | 配置矩阵 | 无 IME、Wayland 无 text-input-v3、无 WebView、T3 安全模式 | 输入可用；preedit 关闭时 UI 明示；`input.*` 回退不静默 | **L6 子集 / A6** |
## 6. 风险与降级

| # | 风险 | 触发信号 | 降级动作 | 不可降级项 |
| --- | --- | --- | --- | --- |
| IR-01 | 候选窗漂移 / 高 DPI 位移（R2、UX-R2、F2） | E60 用例失败、用户报告候选窗错位 | 重锚 ≤1 帧 → clamp 主屏 → 用户偏移配置；连续失败写差异登记 | 候选窗必须原生（AR-01）；不得改为 WebView 内定位 |
| IR-02 | 平台 IME API 缺失（Wayland text-input-v3 / XIM） | 协议协商失败 | preedit 关闭 + 状态栏明示；直键编码仍可用（**AR-31 第 5 条：登记偏差 + UI 明示，不计 P0 失败**） | 不得静默丢输入；不得伪造 preedit |
| IR-03 | kitty 协议破坏 tmux / vim 体验 | 回归用例或 dogfood 报告异常 | 会话级 `input.kitty.emit_capability=false` / 强制 Legacy；`Mod+Shift+Esc` 切 core_first | 默认必须为 Legacy（OQ-06） |
| IR-04 | 剪贴板所有权被第三方管理器抢走 | 写入后读回不一致（ADR-0014 E60） | 异步重试 ≤3 次 + 退避；失败只提示，**绝不阻塞热路径** | 不得阻塞 S6 写入；不得把剪贴板内容写入日志 |
| IR-05 | OSC 52 确认疲劳 → 用户全局放开 | 确认弹窗频次升高、用户选 allow | 来源标记 + 会话级授权 + 审计；保持读默认 deny | 读默认 deny；审计元数据不可关 |
| IR-06 | 粘贴确认疲劳 → 用户关闭保护 | 盲确认率上升 | 括号粘贴激活时**不再确认**（安全且不打扰）；always 供保守用户 | 哨兵消毒不可关闭 |
| IR-07 | 焦点错位致按键误入 PTY | F-1 断言失败或用户报告「打字进了错误的窗口」 | fail-closed：当前 owner ≠ Grid 时丢弃；补回归用例 | F-1 / F-2 不变量 |
| IR-08 | 软换行关闭被误读为内容丢失（UX-R17） | 用户报告丢字 | 复制始终取完整逻辑行；状态栏 + 已知问题清单明示；一键开启动作 | 裁剪必须纯视觉（A34） |
| IR-09 | 输入事件入审计导致内容泄露 | 审计扫描命中内容 | 元数据白名单 schema + CI 静态检查 | 内容 0 采集（AR-12） |
| IR-10 | 热路径被剪贴板 / IME 同步阻塞 | IN-AC-02 回归 >5% | 剪贴板全异步；IME 查询走独立线程；编码器无锁 | 无阻塞锁、无网络、无 AI（AR-03 / AR-04） |

**GPU 降级联动（ADR-0014）**：T1 / T2 不影响输入编码与 IME（L4 原生）；T3 安全模式不加载 L2 面板，剪贴板改由 Core 原生仲裁（功能不丢），网格输入与 IME **必须仍可用**（P0 退出条件）。
## 7. 被否决的方案与反方意见

| 方案 | 否决理由 | 反方最强理由（保留） | 复议条件 |
| --- | --- | --- | --- |
| A. IME 与候选窗放在 WebView（xterm.js 式） | 违反 AR-01 与 ADR-0001 §决策 3；组合输入跨边界必然错位 | 一次实现、跨平台一致、复用系统文本栈成本最低 | AR-01 复议条件成立（WebView P99 ≤16ms 且 IME 可修正）+ TSC；否则不复议 |
| B. 默认开启 kitty keyboard protocol | 与 OQ-06 直接冲突；改变应用行为、tmux / vim 长尾 | 现代协议一次到位，免探测复杂度；kitty / wezterm 已验证可行 | kitty 套件与 12 组合全绿且 tmux / vim / neovim 回归 0；届时 RFC 改 OQ-06 |
| C. OSC 52 读写默认全允许（生态兼容优先） | 违反 §6.1 默认本地 / 默认拒绝；剪贴板劫持 → 粘贴执行链真实存在 | 远程 tmux / neovim 复制到本地剪贴板是刚需，确认会毁掉体验 | 若「来源标记 + 粘贴前校验 + 劫持用例不可复现」且 TSC 批准，可把**写**放宽为 `allow_single_line`（读保持 deny） |
| D. 选择模型基于**视觉行**（visual row） | 复制会插入折行换行符，直接违反 A20 / UX-G17，属正确性事故 | 实现最简、渲染层直接映射、鼠标命中直观 | **不复议**（正确性硬约束；如需性能优化只能在视觉层缓存，不得改变坐标语义） |
| E. 复制时插入续行标记 / 折行换行符 | 同上，且污染 AI 上下文与用户脚本 | 提升可读性、便于日志粘贴 | 仅允许在 `text/html` 表示中加装饰；`text/plain` 不可 |
| F. 剪贴板由 L2 WebView 直连 `navigator.clipboard` | 破坏 Core 唯一权威与焦点契约；无法统一审计与策略 | 实现省事、Web 生态成熟 | 需 ADR 修改 AR-01 与 DC-25，并满足 IR-04；默认不复议 |
| G. 粘贴保护改为「永不确认」（信任专业用户） | 与 AR-06 的分级摩擦证据冲突（盲确认反升）；哨兵注入无其他防线 | 终端用户皆专业人士，逐次确认制造摩擦 | 若实测 bracketed paste 覆盖率达 100% 且哨兵消毒 100%，可把未括号化多行降级为状态栏提示 + 事后撤销（需 TSC） |
| H. preedit 直接写入网格（提交前即上屏） | 违反 AR-01（网格是真相）、AR-04（可回放）；污染 scrollback / 选区 / AI 上下文 | 实现简单、光标跟随自然、无需原生装饰层 | **不复议**（真相层污染不可逆） |
| I. 输入由 UI 进程直接写 PTY（绕过 sessiond lease） | 违反 AR-13 / DC-18 单写者与多端 attach；审计无出口 | 少一跳、延迟更低 | 若 IPC 往返被实测证明不可接受且仍能保证 lease 与审计，可评估共享内存旁路（仍需 ADR） |
## 8. Open Questions

> 本文件不自行发明 HARNESS 结论。需上位仲裁的按 AGENTS §5 进 HARNESS §11（由 Orchestrator 编号，建议号见下）；改动对外契约的额外走 RFC → ADR。

| ID | 问题 | 影响面 | 建议 | 阶段 / 流程 |
| --- | --- | --- | --- | --- |
| OQ-INP-01 | **§5 缺少「输入正确性」口径**：key-to-photon 只度量延迟，不覆盖 IME / 剪贴板 / 选择正确性；把本文件 IN-AC-04 / 07 / 08 / 09 / 10 升为 §5 门禁行还是留作 §8.2 主题验收？（**已决：AR-31 第 4 条**） | 门禁覆盖面、发布阻断点、CI 成本 | **已决（AR-31 第 4 条）：采纳候选 ② 并收窄**——§5 **只收可自动化的输入字节等价**（§5 已有「输入字节等价 = 100%（逐字节）」一行，AR-29 时加入）；**IME preedit 与候选窗正确性（IN-AC-04）不进 §5**，保留在 §8.2 主题验收 + L6 矩阵 + 人工会签 | **已决（AR-31）** |
| OQ-INP-02 | **`input.shortcut_precedence` 与 spec 02 §3.5 优先级的交互**（**已决：AR-29 第 1 条**） | 命令面板 / 侧栏键（`Mod+K` / `Mod+B`，平台展开见 AR-29 第 2/7 条）可达性（AR-23 冻结键位）、tmux / vim 兼容、用户预期 | **已决：核心快捷键在 FocusRouter 层始终优先**（否则 Win/Linux 上应用会吞掉 Mod+K / Mod+B，用户无法可靠呼出核心功能）；IN-05 默认 `core_first`，`app_first` 仅作非默认兼容预设，**冲突以改绑按键解决**；平台展开见 AR-29 第 2/7 条 | **已决（AR-29）**；spec 02 §3.5 已同步 |
| OQ-INP-03 | **OSC 52 默认值是否属对外契约**（**已决：AR-29 第 5 条**） | 终端行为契约、生态兼容 | **已决（AR-29 第 5 条）：读默认 `deny`、写默认 `guarded`（单行放行、多行/控制字符确认），不允许静默多行**；是否需 ADR + 公示按 spec 07 §3.10 由 T5 判定，并进 `termai import` 兼容说明 | **已决（AR-29）** |
| OQ-INP-04 | **Local API 新增 `clipboard.get / set` 与 `input.inject`** | 对外契约（DC-24 唯一契约）、插件权限、审计 | 建议采纳，复用 `clipboard.read / write` 与 `session.write` capability；令牌 + 审计强制 | **P1**；RFC → ADR + 2 maintainer |
| OQ-INP-05 | **回应 `docs/spec/03` OQ-A7**：剪贴板 / 拖拽数据格式与所有权已在本文件 §3.6 给出（Core 仲裁 + MIME 白名单 + shell-bridge 中转） | 跨层交互、粘贴注入安全 | 建议据此关闭 OQ-A7，并在 spec 03 §3.2 补一行 L0↔L2 所有权规则 | **P1**；需 spec 03 owner 会签 |
| OQ-INP-06 | **Linux Wayland `text-input-v3` / XIM 的 v1 支持级别**（无 preedit 时是否仍算「IME 矩阵全绿」）（**已决：AR-31 第 5 条**） | P0 退出条件、Linux 用户体验、矩阵判定 | **已决（AR-31 第 5 条）**：P0 只要求 X11 / fcitx5 组合 preedit 可用；Wayland 缺少 `text-input-v3` 时**登记偏差 + UI 明示，不计 P0 失败**（ADR-0014 差异登记） | **已决（AR-31）** |
| OQ-INP-07 | **macOS `option_as_meta` 默认值**（本文件取 false = 组合）与 macOS 上 `Mod` 映射确认 | 中文 / 变音符输入 vs 老手期望；快捷键表 | 建议保持 false（保 IME / 变音符，符合 A2 正确性优先）；提供首次冲突情境提示 | **P1** |
| OQ-INP-08 | **kitty flags 探测是否需要 per-app allowlist**（默认关闭已定，是否再加「仅白名单应用可 push flags」） | 兼容性 vs 可发现性、安全面 | 建议不设白名单（否则协议形同虚设），改为「flags 变更写本地审计 + 一键回 Legacy」 | **P1** |
| OQ-INP-09 | **粘贴载荷上限与分块**（依赖 `spec 03 OQ-A4` 的 IPC 最大帧） | 大粘贴可用性、DoS 面 | 建议单次粘贴 ≤8MiB，超出分块并经 PasteGate 逐块校验；上限进配置 | **P1**（与 OQ-A4 联动） |
| OQ-INP-10 | **四项【新增】指标的批准**：① 输入侧分摊 P99 ≤1.15ms；② 候选窗漂移 ≤2px（DPI 100/125/150/200%）；③ 复制一致语料 ≥10k 行；④ preedit 期 PTY 字节 = 0（**降为 P1：AR-31 D3/D4**） | 可测性与验收强度 | **降为 P1**：四项照测并标【新增】，不阻塞发布，只进 G4 附加报告；IN-AC-03/04 维持非门禁（占位 = 现有测量方法）。**复评触发**：下一 minor 有 ≥1 个版本数据后评估升为门禁 | **P1（AR-31 降级）** |
| OQ-INP-11 | **AR-23 第 6 条「软换行关闭 = 视觉裁剪」的显示语义细化**：裁剪时续行是否仍作为独立行渲染、是否提供「临时查看」动作 | 用户是否误读为内容丢失（UX-R17）、A34 判据 | 建议：裁剪 = 每逻辑行只渲染首段 + 一键临时开启；续行内容始终保留在缓冲与复制范围，写入已知问题清单 | **P1**；对应 HARNESS **OQ-37（建议）** / spec 02 澄清 |
| OQ-INP-12 | **配置化边界复核**：IN-06 / 07 / 08 的 `allow / ask` 是否与「确认不可由配置关闭」（AR-06）冲突？本文件判定粘贴为**用户发起来源**、非破坏性外发操作，故可配置，但消毒与审计不可关（**已决：AR-31 第 6 条**） | 合规口径、企业策略 | **已决（AR-31 第 6 条）：采纳候选 ③**——配置**只能加强不能削弱**（`ask` 可升为 `always`，不得降为 `never`）；**消毒与审计不可关闭**（与 AR-06「企业策略只能加强」同源，不破坏 AR-29 第 5 条「不允许静默多行」） | **已决（AR-31）** |
