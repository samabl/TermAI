## 一、角色立场与判断依据

我负责 PTY/进程与会话模型、VT 解析、渲染管线、滚动缓冲与内存、性能预算、shell 集成、多路复用与持久会话、远程与文件传输、崩溃恢复、内核 API 边界。判断基准四条：**① 终端第一性是字节保真与确定性延迟**，AI 只能消费内核产出的结构化上下文，绝不允许出现在 PTY→像素的关键路径上；**② 兼容性是可测量的门槛**，用 conformance 套件定胜负，不靠观感；**③ 内核必须能脱离 UI/网络/AI 独立正确运行**，否则测试与远程复用无从谈起；**④ 一切性能承诺必须进 CI 自动回归**。参照对象：Alacritty、Ghostty、WezTerm、kitty、iTerm2、Windows Terminal，以及 ECMA-48、xterm ctlseqs、kitty 协议规范。

## 二、关键结论

**D1 候选栈终裁：否决 A、B、C，采用 D = Rust 内核 + WebView 外壳 + 原生 GPU 终端画布。**
- 理由：终端画布必须由 Rust 直接提交 GPU（低延迟、复杂脚本 shaping、协议支持），而设置/AI/插件面板用 WebView+TS 才有生态与迭代速度。A 的自研原生 UI 工具链是数年成本且无差异化；B 让 WebView 承担终端像素路径，key-to-photon 与 IME 不可控；C 的 Electron 内存与启动开销出局。
- 代价：两套 UI 栈带来输入焦点、IME、无障碍、裁剪与合成的一致性成本。我接受，因为这是唯一同时满足延迟与生态的选择。

**D2 PTY：跨平台 PtyBackend 抽象；Windows 唯一生产路径是 ConPTY（Win10 1809+），POSIX 用 forkpty/openpty。**
- 理由：ConPTY 是微软维护的唯一受支持路径，自研替代品风险与收益不成比例。
- 代价：ConPTY 会重编码输出、插入额外序列、resize 语义有损、延迟高于原生 PTY，且 Windows 无 POSIX 信号。缓解：抽象层预留 NativePty 后端插槽，用 Job Object 管理进程树，用 pseudo-console 事件替代信号。

**D3 VT 解析：复用 vte crate 作基础状态机并 Fork-pin，自研扩展协议层，不自研 ESC 解析器。**
- 理由：ESC 解析是正确性已知、无差异化的重活，vte 已被 Alacritty 千亿级字节验证；差异化在 OSC、kitty、iTerm2、Sixel 扩展与 grid 语义。
- 代价：上游耦合与升级成本；必要时长期维护 Fork。

**D4 渲染：wgpu + rustybuzz/swash/fontdb，自建多页字形 atlas，run-level shaping。**
- 理由：网格要同时处理连字、emoji、阿拉伯文/天城文并保持列对齐。以 grapheme cluster 为单位 shaping，再映射到 cell 列，宽字符跨列、组合字符归并。
- 代价：cluster↔列映射是最易出 bug 的区域，需真实文本回归；GPU 驱动差异需多后端与软件回退。

**D5 多路复用与持久会话：自研 sessiond 作为唯一真源，tmux 仅作兼容客户端（control mode）。**
- 理由：Windows 无 tmux；会话必须成为 AI 可寻址的结构化对象（cwd、退出码、命令边界），tmux 的字节级会话无法承载上下文。
- 代价：重造 tmux 级健壮性是长期义务，早期 bug 风险高。接受，换取跨平台一致与 AI-native 上下文。

**D6 滚动缓冲：line-arena + style-run 压缩，默认 10000 行/10MB 上限，冷行下沉磁盘。**
- 理由：per-cell 对象模型内存放大 5–10 倍。样式 run 复用 + 增量 checkpoint 可压到 1/4 以下。
- 代价：实现复杂、随机访问需索引层；以 fuzz 与不变量断言保障。

**D7 远程：内核原生 SSH（russh），容器/WSL 统一抽象为 Transport，不 shell out 系统 ssh。**
- 理由：只有内核持有字节流，scrollback、shell 集成与 AI 上下文才能一致。
- 代价：需自维护 SSH 栈并兼容 known_hosts/agent/跳板机/各类密钥，安全责任内化。

**D8 内核 API 边界与插件：版本化能力协议（本地 socket + 共享内存），插件永不进入内核进程。**
- 理由：PTY 读路径上任何用户代码都会破坏延迟与稳定性，插件崩溃不得杀死终端。
- 代价：插件能力受限、需序列化开销；接受，用共享内存传帧与批量数据。

**D9 许可：内核与协议 MIT/Apache-2.0 双许可，应用层同许可，open-core 商业化（团队同步/云会话/企业策略/托管 AI）。**
- 理由：插件生态与 OEM、企业嵌入需要宽松许可，GPL 会阻断专有插件。
- 代价：竞品可 Fork 内核，护城河只能靠 AI 服务、同步与品牌。明确接受。

## 三、详细设计

**3.1 进程与会话模型**：Session = {id, Transport(本地 PTY|SSH|容器|WSL), Terminal{grid, scrollback, modes}, ProcessTree, Context{cwd/env/git/退出码/报错}, Policies}。sessiond 守护进程持有 Session，UI 只是它的客户端，可多端 attach。AI Agent 通过只读订阅 + 受控写入访问 Context，写 stdin 需显式授权。

**3.2 渲染管线**：PTY bytes → vte 状态机 → Perform → Grid diff → damage region → shape/atlas cache → GPU draw → present。damage-based 部分重绘；atlas 按 (font,size,weight,subpixel) 分页 LRU；滚动/选择/搜索走独立合成层；帧率封顶省电，未聚焦窗口不主动出帧。

**3.3 性能预算（P0 门槛，CI 门禁）**

| 指标 | 目标 | 测量方式 |
|---|---|---|
| 冷启动到可输入 | ≤150ms | release 构建，CI 脚本，SSD |
| key-to-photon（本地） | P50 ≤4ms / P99 ≤8ms | 内置时序探针 + 高速相机回归 |
| 解析+渲染吞吐 | ≥500MB/s | 1GB 语料基准，cat 不掉帧 |
| 空闲内存 | ≤120MB RSS | 进程采样 |
| 10k 行回翻 | ≤8ms/帧 | 合成基准 |
| 功耗 | 空闲 CPU <1%，未聚焦不出帧 | 功耗仪 + CI 采样 |

**3.4 Shell 集成**：解析 OSC 133/633（命令边界、退出码）、OSC 7（cwd）、OSC 0/2（title）、OSC 9/777（通知）；自动注入 bash/zsh/fish/pwsh 集成脚本但可禁用；失效时退化为提示符启发式并标记低置信度，不让 AI 误信。

**3.5 图形与文件协议**：Sixel、kitty graphics、iTerm2 inline image 全支持但默认限额（单图 ≤16MB、每屏 ≤64 张）防 DoS；文件传输走 SFTP 子系统 + OSC 1337/kitty 传输，落地前需确认。

**3.6 跨平台差异**：Windows（ConPTY、Job Object、路径/驱动器、VT 输入模式）；macOS（Metal、Option as Meta、Secure Keyboard Entry、公证）；Linux（Wayland/X11 IME、XDG portal）。$TERM 默认 xterm-256color 保兼容，TERM_PROGRAM=TermAI 与能力查询（DA/kitty query）用于进取特性，另附 termai terminfo。

**3.7 崩溃恢复**：sessiond 与 UI 解耦，UI 崩溃不影响进程；scrollback 以追加式段日志落盘（可选加密），重启后重建最后 checkpoint + 尾部段。shell 内进程状态无法回放，文档必须诚实标注：进程不可恢复，屏幕可恢复。

## 四、与其他角色的接口与依赖

**我需要别人给我**：产品/PM 的 AI 能力清单与优先级、确认"内核不依赖 AI"是硬约束；前端/UX 的 Window/Tab 模型、主题 token、快捷键表、IME 与无障碍要求，并接受终端画布为独立合成层；插件方的插件清单与权限模型；安全方的威胁模型与合规范式；平台/发布的签名、公证、更新、遥测边界；AI/Agent 团队的 Context 消费需求。

**我向别人承诺**：版本化内核协议与向后兼容窗口（≥2 个 minor），破坏性变更走 capability 协商；Context 事件（命令开始/结束、退出码、cwd、报错片段）稳定 schema 与时间戳；性能预算进 CI，回归即阻塞发布；不把 AI/网络放在 PTY 关键路径，内核无 UI 也能自测。

**我预期会冲突的判断点（请仲裁）**：① 前端主张统一用 WebView 渲染终端——我否决像素路径下放 WebView；② 插件方主张可拦截/改写 PTY 字节——我否决，最多给只读镜像与显式注入队列；③ PM 希望内核内嵌 AI 调用——我否决，AI 必须是 sessiond 之外的消费者；④ 许可方可能主张 GPL 保护——我主张宽松 + open-core。

## 五、被否决的方案与反方意见

| 被否决 | 反方最强理由 | 我方反驳 | 保留条件 |
|---|---|---|---|
| A 全自研原生 UI | 延迟与一致性最好，无 WebView 包袱 | 数年工具链成本，无差异化，生态为零 | 若 WebView 在任一目标平台不达标则重启评估 |
| B 纯 Tauri 渲染终端 | 一套栈、迭代快、跨平台省事 | 像素经 IPC+DOM，延迟与 IME 不可控 | 仅用于非终端 UI |
| C Electron + xterm.js | 生态成熟、上手快 | 内存/启动/吞吐不达标，无 Sixel/kitty | 可作 Web 远程客户端参考 |
| 自研 ESC 解析器 | 完全可控、无上游 | 无差异化、正确性风险极高 | 仅当 vte 无法承载新协议时局部替换 |
| 依赖 tmux | 久经考验、零开发 | 无 Windows、无法结构化上下文 | 保留 control-mode 兼容客户端 |
| 自研 Windows native pty | 摆脱 ConPTY 延迟与其 bug | 内核态工程、驱动签名与稳定性风险 | 长期实验分支，非主线 |

## 六、风险与缓解

1. **ConPTY 兼容黑洞**：建独立矩阵（vim/htop/neovim/fzf/tmux 嵌套），逐项标注偏差，预留 NativePty 插槽。
2. **cluster↔列映射错误**：真实多语言语料 + fuzz + 屏幕快照 diff 三件套进 CI。
3. **sessiond 单点故障**：supervisor 看护 + 崩溃自动重启 + 客户端幂等重连。
4. **远程安全**：SSH/传输/插件全走威胁建模，默认拒绝、最小权限、密钥不落明文、审计日志。
5. **性能回退**：所有预算指标作为 CI 门禁与发布阻断项。
6. **磁盘隐私**：持久 scrollback 默认关闭或加密，明确告知内容留痕。
7. **GPU 驱动多样性**：多后端 + 软件回退，启动自检失败自动降级。

## 七、可验证验收标准

- **兼容性**：vttest、esctest（pyte）、terminfo 校验、kitty 协议测试套件 100% 通过；xterm 兼容用例通过率 ≥99%，差异登记在案。
- **性能**：3.3 表格全部达标且连续 10 次 release 构建无回退；1GB cat 吞吐 ≥500MB/s。
- **延迟**：key-to-photon P99 ≤8ms（本地）、≤40ms（同城 SSH，含网络）。
- **稳定性**：sessiond 连续 7×24 无泄漏（RSS 漂移 <5%）；UI 崩溃后 <2s 重连且屏幕一致。
- **内存**：默认空闲 ≤120MB；10 万行 scrollback ≤400MB，回翻 P99 帧 ≤8ms。
- **跨平台**：三平台各自 CI 跑通字节保真与 IME 用例；同一录制会话跨平台屏幕 diff 为零。
- **安全**：插件越权拦截率 100%；模糊测试 24h 无 crash。

## 八、待决议问题

1. WebView 运行时选型（系统 WebView2/WKWebView/WebKitGTK vs 自带 CEF）——影响包体、一致性、许可与安全更新。
2. 终端画布与 WebView 的合成方式：同窗口分层合成 vs 独立原生窗口贴附——影响 IME、无障碍、跨屏 DPI。
3. termai terminfo 与 xterm-256color 默认的最终取舍与迁移路径。
4. 持久 scrollback 默认策略：本地加密持久 vs 默认内存、退出即清。
5. sessiond 进程模型：每用户单实例 vs 每会话一进程；与系统服务/开机自启的关系。
6. v1 是否默认开启 kitty keyboard protocol（会改变应用行为，有兼容风险）。
7. 许可最终文本与 CLA/DCO 策略；插件市场的审核责任与商业模式。
8. 多用户共享远程会话的权限模型：谁能写 stdin、谁能读 AI 上下文。
