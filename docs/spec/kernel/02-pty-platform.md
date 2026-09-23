# 内核规格 02 · PTY 与平台层（PTY & Platform Layer Spec）

> **权威顺序**：HARNESS.md > docs/spec/00-glossary.md > docs/spec/03-system-architecture.md > 本文件。本文只把 AR/DC 落成可施工、可验证的机制，不新增对外结论；任何与 HARNESS 的差异在文首末段显式登记。
> **引用编号契约**：AR-03 / AR-04 / AR-06 / AR-11 / AR-13 / AR-14 / AR-18 / AR-19 / AR-20 / AR-21 / AR-23；DC-04 / DC-15 / DC-16 / DC-18 / DC-19 / DC-20 / DC-22 / DC-27 / DC-37 / DC-38 / DC-40；HARNESS §5（门禁/目标双列）、§8.1（CI 六件套）、§8.2；ADR-0009 / ADR-0012 / ADR-0014 / ADR-0015 / ADR-0017。
**角色**：PTY 与平台层评审负责人。**实现位置**：crates/termai-pty、crates/termai-transport、tests/conformance/、tests/matrix/。
**与 HARNESS 的差异登记（本文自陈）**：
1. docs/spec/03 §3.6 的 PtyBackend 面为「spawn / resize / write / read / close / signal→事件」，本文扩展为 **spawn / write / read / resize / signal / kill / wait / process_tree（含 tree_snapshot）/ capabilities 九元面**（多出 kill、wait、process_tree、capabilities 四个显式面；close 仍作清理面保留）——**不改变 DC-16 结论**，只补足「close 无法表达『杀谁、等谁、看树』」的空缺；若 Orchestrator 认为属对外契约变更，本文按第 8 节请求裁决。
2. 本文提出的**新增可测指标**（PTY-LAT-1 / PTY-LOSSY-1 / PTY-RSS-1 / PTY-ORPHAN-1）不并入 HARNESS §5，统一登记到第 8.1 节待裁决（硬性规则 2）。
3. 既有预算一律引用 §5 原文口径并逐条标注「门禁 / 目标」，本文不发明任何并列数字。
**诚实边界（AR-20 落地）**：ConPTY 的改写不可消除、远程 resize 不可承诺无损、进程不可恢复（AR-13）。三条必须以用户可见文案出现，禁止模糊措辞。

## 1. 范围与依据（引用具体 AR/DC 编号）

**目的**：把「键盘→应用、应用→像素、本机→远端」这条链上**唯一允许改字节的那一层**钉成数据契约，使 PtyBackend / Transport 的每个实现都能被差分测试判定，而不是靠观感。

| # | 范围内 | 主要依据 | 量化绑定 |
| --- | --- | --- | --- |
| 1 | PtyBackend trait 完整签名 + 实现清单（ConPTY / forkpty / openpty） | DC-16、§4.1 拓扑、DC-15 | §3.1、§4.1 |
| 2 | Windows ConPTY 行为差异清单（每条含检测 + 降级） | DC-16、§9 R4、§8.1-1（xterm ≥99%） | §5 PTY-AC-02 |
| 3 | Job Object 进程树管理（创建 / 加入 / 超时 / 孤儿清理 / kill） | DC-16、DC-27、05-spec §3.3 规则 6 | §5 PTY-AC-03 |
| 4 | POSIX 侧（forkpty / openpty、TIOCSWINSZ、控制终端、SIGHUP / SIGWINCH、reap） | DC-16、AR-13 | §5 PTY-AC-04 |
| 5 | 兼容矩阵（vim / htop / neovim / fzf / tmux 嵌套 / less / python REPL） | §9 R4、ADR-0014 §决策 3（M12 第 5/6/11 项） | §5 PTY-AC-05；**门禁映射**：§8.1-1（xterm ≥99%） |
| 6 | Transport 抽象（Local / SSH / Container / WSL）与字节保真契约 | DC-19、AR-04、AR-11、ADR-0009 决策 8 | §5 PTY-AC-01、PTY-AC-06；**门禁映射**：§8.1-2 行为回放 ≥99.5%（跨平台断言用渲染后网格） |
| 7 | 进程树 kill/wait 语义、超时与孤儿清理的可审计性 | AR-06、DC-27、DC-34 | §5 PTY-AC-03 |

**范围外**（不得在本文发明）：VT 序列语义与 vte 边界（AR-18，见内核规格 01）；网格与渲染（AR-01、AR-14）；Session Log 物理格式与索引（DC-23）；IPC 帧布局（DC-22，本文只用其「热路径禁 JSON」约束）；AI 审批语义（AR-06，本文只调用其风险分级作为信号转发门）。

**不可协商（引自 §0 与 §6.1，本文不得削弱）**：AI 不进 PTY→像素热路径、内核不依赖 AI/网络（AR-03）；PTY 流与命令输出永不进入服务端（AR-11）；插件永不写 PTY/输出流（AR-07，本文以接口形态堵死）；核心链接边界内 GPL/AGPL/SSPL = 0（AR-21、ADR-0015）。

## 2. 关键结论（K-01 …）

| 编号 | 结论 | 理由 | 代价（明确接受） |
| --- | --- | --- | --- |
| **K-01** | **唯一允许改字节的层 = PtyBackend 的「事件/元数据层」，字节缓冲层永不可写。** 数据通路满足 bytes_out_of_read == bytes_into_vt_parser（逐字节恒等差分断言，非采样） | AR-04（唯一真相 = append-only Log）、AR-11（PTY 流不出设备）、AR-14（网格由列数定义）；改字节是「复制结果/渲染不一致」类正确性事故的根因（02-spec UX-R12） | 无法在通路里做「就地脱敏/就地补全」——所有加工必须走旁路事件；Log 中可能是原始敏感字节，只能靠会话级 sensitive_session 开关与「原始 scrollback 持久默认关闭」处理（AR-13 / OQ-04），**不引入内核脱敏**（DC-33 唯一收口） |
| **K-02** | **ConPTY 的 7 类已知改写必须逐条「登记 + 探测 + 分级」，未登记改写即门禁失败。** 差异表是 CI 断言对象，不是文档装饰 | DC-16 定 Windows 唯一生产路径；§9 R4 是显式风险登记；§8.1-1 要求差异 100% 登记在案 | 每次 Windows / Conhost 行为漂移都可能让门禁变红，需人工分级（接受/降级），存在长期维护税 |
| **K-03** | **输入侧「Ctrl 键 → 字节」为默认路径；GenerateConsoleCtrlEvent 仅 best-effort，失败静默降级为写 0x03 并写审计。** 绝不因信号失败阻塞输入 | Windows 无 POSIX 信号；ConPTY 上控制台事件成功率不可承诺（依赖 CREATE_NEW_PROCESS_GROUP 与前台进程组状态） | Ctrl-C 对「自行忽略 SIGINT 的程序」不再可靠；必须向用户明示「Windows 上部分信号语义由应用实现决定」（AR-20） |
| **K-04** | **进程树两套实现、一个语义：Windows = Job Object；POSIX = setsid + 进程组 + killpg。** 超时 / 孤儿清理 / 审计行为跨平台一致 | DC-16 明确 Job Object 管进程树；DC-27 与 05-spec §3.3 规则 6 要求「超时杀进程树」；AR-13 要求 UI 崩溃不影响会话进程，故 Job 句柄由 sessiond 持有 | Job Object 是进程级约束：主动 breakaway 的程序可逃逸（登记为已知限制，靠 ACCOUNTING 信息 + 二次扫描兜底）；POSIX 侧 setsid 后无法精确回收已 daemon 化的孙进程，只能 PPID 扫描 |
| **K-05** | **POSIX 默认 forkpty（单调用、原子分配）；仅在需要 setsid 后自定义环境 / 受控提权 / 登录会话 / 嵌套控制终端时用 openpty + 显式 setsid / TIOCSCTTY。** 由策略选择，不由开发者偏好切换 | 对齐 AR-18 的「不重复已验证苦力」；openpty 的额外控制面只在明确需要时才值得 | 两套路径都要测；openpty 路径的 setsid / TIOCSCTTY 失败必须显式覆盖，否则会静默产生「无控制终端的会话」 |
| **K-06** | **Transport 能力矩阵 = 声明 + 运行时探测 + 失败即降级：对 resize / signal / 图形协议透传 / 字节保真 四项逐项声明；消费者不得假设任何一项存在** | DC-19 要求四类 Transport 统一抽象；AR-11 与各端能力差异必须显式化 | 消费者要写能力分支代码，UI 会出现多种「该会话不支持…」提示；这是 AR-20 要求的代价，不是缺陷 |
| **K-07** | **远程 resize = 远端优先，失败则本地折行/裁剪，绝不编造远端尺寸。** resize 失败必须留下结构化事件 ResizeDegraded | 02-spec C14 与 AR-22/AR-23 要求视觉行为一致（开=折行、关=裁剪、无横向滚动）；durable resize 并非所有目标都有 | 远端 PTY 尺寸可能与窗口不一致：TUI 会画到错误列数；必须显示「远端尺寸 ≠ 窗口尺寸」徽章（AR-20） |
| **K-08** | **断线重连分两档：Attach（本地 checkpoint 重建屏幕，进程状态未知，绝不声称恢复）与 Reattach（仅当 Transport 声明 reattach 能力且用户显式选择时）** | AR-13「屏幕可恢复，进程不可恢复」；DC-04「绝不自动重放命令或重启进程」 | 用户重连后可能面对「屏幕有、进程没了」的会话（尤其 SSH）；UI 必须以一等视觉区分两档 |

## 3. 详细设计

### 3.1 PtyBackend trait（唯一向 sessiond 暴露的面，DC-16）

```rust
// crates/termai-pty/src/lib.rs —— 实现：ConPty / ForkPty / OpenPty / PipeFallback(探测失败降级)
pub enum PtyError { Spawn{errno:i32, stage:SpawnStage}, Io(std::io::Error), NoSuchPty,
                    JobAttachDenied{i32}, SignalUnsupported, Timeout{pid:u32, after:Duration},
                    TreeNotEmpty{live:u32}, Unsupported(&'static str) }

pub struct WinSize { pub cols:u16, pub rows:u16, pub px_w:u16, pub px_h:u16 } // px_* 默认 0，仅图形协议使用
pub struct PtyCapabilities { pub resize: ResizeCap, pub signals: SignalCap, pub graphics_passthrough: bool,
                             pub byte_fidelity: Fidelity, pub job_control: bool }
pub enum ResizeCap { Exact, Lossy, Unsupported }
pub enum SignalCap { Posix, ConsoleEvent, ByteFallback }
pub enum Fidelity { F0, F1(RuleSetId), F2(RuleSetId) } // F0 字节不变 / F1 仅元数据层可改 / F2 渲染层可改写（AR-25 第 2 条）
pub enum Sig { Int, Term, Hup, Quit, Winch, Usr1, Usr2, Kill, Stop, Cont } // 语义枚举，禁止调用方硬编码整数
pub enum SignalOutcome { Delivered, ByteFallback(u8), Unsupported }
pub enum ResizeEffect { Applied, AppliedLossy, LocalOnly } // LocalOnly = 仅本地网格变化（K-07）

pub trait PtyBackend: Send + Sync {
    fn capabilities(&self) -> PtyCapabilities;                 // 纯查询，无副作用；进程生命周期内恒定
    fn spawn(&self, cmd: &Command, sz: WinSize, o: SpawnOpts) -> Result<PtyHandle, PtyError>;
    fn write(&self, pty: &PtyHandle, data: &[u8]) -> Result<usize, PtyError>; // 部分写由调用方循环
    fn read(&self, pty: &PtyHandle, buf: &mut [u8]) -> Result<usize, PtyError>; // 0 = EOF；EINTR 实现层吞掉重试
    fn resize(&self, pty: &PtyHandle, sz: WinSize) -> Result<ResizeEffect, PtyError>;
    fn signal(&self, pty: &PtyHandle, sig: Sig) -> Result<SignalOutcome, PtyError>;
    fn kill(&self, t: &ProcessTree, mode: KillMode) -> Result<ExitInfo, PtyError>; // Force = SIGKILL / TerminateJobObject
    fn wait(&self, t: &ProcessTree, to: WaitTimeout) -> Result<ExitInfo, PtyError>; // 返回即已 reap
    fn process_tree(&self, pty: &PtyHandle) -> Result<ProcessTree, PtyError>;
    fn tree_snapshot(&self, t: &ProcessTree) -> Result<Vec<ProcEntry>, PtyError>; // pid/ppid/name/start_time/cpu
    fn close(&self, pty: PtyHandle) -> Result<(), PtyError>; // 释放 fd/句柄；不隐式杀进程（孤儿见 §3.3）
}

pub enum KillMode { Graceful(Duration), Force }
pub struct SpawnOpts { pub env: EnvPolicy, pub cwd: Option<PathBuf>, pub login: bool,
                      pub latency_budget: Duration, pub detach_policy: DetachPolicy }
pub enum DetachPolicy { Allow, Deny } // 工具子进程默认 Deny（05-spec §3.3 规则 6）
pub struct ExitInfo { pub code: Option<i32>, pub signal: Option<Sig>, pub reaped: bool,
                      pub live_children: u32, pub wall: Duration }
pub struct ProcEntry { pub pid: u32, pub ppid: u32, pub name: String, pub start_time: u64, pub cpu_ms: u64 }
```

接口级不变量（属性测试，见 §5 PTY-AC-07）：① read 返回 0 后再次 read 恒返回 0；② wait 返回必含 reaped == true；③ kill(Force) 后 tree_snapshot 为空；④ capabilities() 在同一进程生命周期内恒定；⑤ 任意错误路径后不泄漏 fd/句柄，close 幂等。

### 3.2 Windows ConPTY 已知行为差异清单（每条：现象 / 检测 / 分级 / 降级）

| # | 现象（ConPTY 固有） | 检测方法（可自动化） | 分级 | 降级行为（用户可见） |
| --- | --- | --- | --- | --- |
| **W1** | 输出重编码：Conhost 层做 UTF-8/UTF-16 转换与换行规范化，可能把 LF 重写为 CRLF、把 0x9B 等重解释为替代字符 | 回环探针：cmd /c chcp 65001 >NUL & type probe.bin，probe.bin 含 0x00–0xFF 全字节 + LF/CRLF 混合；经 RuleSet 比对 | 接受（登记为 C-W1） | 无 UI 动作；差异进 conpty-rewrite-report.json，UI「已知问题」可查（AR-20） |
| **W2** | 插入额外序列：启动/重绘/清屏时插入 ED/EL/CUP 等控制序列，破坏「原始字节回放」 | 同一探针 + 静态断言：用户未写入的时间窗内出现的 ESC [ 序列按 RuleSetId 白名单判定 | 接受（登记为 C-W2） | 回放语料必须带 conpty 维度标记；跨平台断言只用**渲染后网格**，不用原始字节（见 §3.6） |
| **W3** | resize 语义有损：只对**新内容**生效，已有屏幕内容不按新列数重排；vim 分屏 / htop 出现错位 | vim/htop 先填满屏幕 → 列数 80→120→80 → readback 尺寸与逐帧网格哈希 | 接受（登记为 C-W3） | 走「应用收到 resize 后自行重绘」路径：发 ResizeHint / SIGWINCH 等价事件；状态栏标「resize 有损」徽章 |
| **W4** | 无 POSIX 信号：SIGINT/SIGTERM/SIGWINCH 不存在；GenerateConsoleCtrlEvent 对非绑定进程组静默失败 | 探针子进程（pwsh 空转循环），发一次 signal(Int)，观察 2s 内是否退出；失败计数进 signal-probe.json | 接受（登记为 C-W4） | signal() 返回 ByteFallback(0x03) 并写审计 SignalByteFallback；UI **每会话首次**提示一次（不逐次弹窗，AR-06 反方证据） |
| **W5** | UTF-8 与代码页：非 UTF-8 代码页（936/932 等）下输出乱码或替换字符；chcp 之后才生效 | 启动读 GetConsoleOutputCP / GetConsoleCP 与 65001 比对；探针写 CJK 与 emoji 各 8 字符比对 round-trip | 拒绝静默通过 | 会话启动即 chcp 65001 并记录原始 CP；无法设置时 → 徽章「非 UTF-8 代码页」+ 建议动作；**不承诺** 100% 保真 |
| **W6** | 伪控制台事件：CTRL_CLOSE_EVENT / CTRL_LOGOFF_EVENT / CTRL_SHUTDOWN_EVENT 随窗口生命周期到达，可能被误当用户 Ctrl-C | 事件订阅计数：Close/Logoff/Shutdown 与 CtrlC/CtrlBreak 分开统计；子进程侧探针打印收到的 dwCtrlType | 接受（登记为 C-W6） | 严格按 §3.5 映射表，绝不把生命周期事件折叠为 Sig::Int；关闭窗口先 kill(Graceful) 再 WinClose |
| **W7** | 光标/查询响应重写：DA1/DSR/CPR 响应可能被 Conhost 抢先或延迟回答，应用收到与终端状态不一致的答案 | 主动查询探针：写 ESC[c / ESC[6n 并读响应，与自研 VT 对同一序列的预期响应比对；统计响应延迟分布 | 接受（登记为 C-W7） | 自研 VT **不重复回答**已被 Conhost 回答的查询（去重表）；重复回答即门禁失败（§5 PTY-AC-02） |

**机器可读登记产物**：tests/conformance/conpty-rules.toml 每条形如 { id="C-W2", kind="insert|rewrite|reorder|drop", pattern, scope="conhost-generated|data", severity="accept|degrade|block", owner, expires }。**severity=block 的改写视为门禁失败**；scope="data" 的改写**一律 block**（意味着用户数据被改）。该文件是 §5 PTY-AC-02 的判定输入，也是「差异 100% 登记」的唯一凭证。

### 3.3 Job Object 进程树管理规格（Windows，DC-16 / DC-27）

| 阶段 | 机制 | 关键参数 | 失败处置 |
| --- | --- | --- | --- |
| 创建 | CreateJobObjectW + SetInformationJobObject(ExtendedLimitInformation) | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE（会话 Job 必开）；BREAKAWAY_OK **默认关**，仅 DetachPolicy::Allow 的会话开；ACTIVE_PROCESS 上限 256（防 fork 炸弹，DC-27 最小权限） | CreateJobObjectW 失败 → PtyError::JobAttachDenied，**拒绝 spawn**（不得退化为无 Job 会话，否则孤儿清理失去抓手） |
| 加入 | CreateProcessW 携带 PROC_THREAD_ATTRIBUTE_JOB_LIST（优先，原子）或 spawn 后 AssignProcessToJobObject（回退） | 加入失败 = spawn 失败；回退路径必须在 latency_budget 内完成 | 两条路径都失败 → 杀子进程 + 返回错误；写审计 JobAttachFailed |
| 超时 | JOB_OBJECT_LIMIT_JOB_TIME（硬 CPU 时间）+ 实现层 wall-clock 定时器（Graceful(2s) → TerminateJobObject） | 工具子进程默认 wall 超时 30s/600s（04-spec §3.1.3 工具表口径）；**用户会话默认无超时**（不得因超时杀用户进程） | 超时 → kill(Force) → ExitInfo.signal = Kill；用户会话超时只能由用户操作触发 |
| 孤儿清理 | ① KILL_ON_JOB_CLOSE（sessiond 崩溃/句柄关闭时内核自动收割，主路径）；② 兜底扫描：每 60s 对「PTY master 已关闭但 tree 仍有 live 进程」的会话按策略处理 | 兜底扫描只对**本进程创建过**的 Job 生效（持有 Job 句柄表），不扫描系统全进程（避免越权与性能税） | 扫描到残留 → 记录 OrphanReaped{job, pids, reason} 并审计；策略 = KillOnOrphan（默认）/ KeepOnOrphan（仅用户显式开启的持久会话） |
| kill 语义 | TerminateJobObject（Force）；Graceful 先向前台进程组发 Ctrl 事件/写 0x03，等 2s 再 Force | Graceful 期间不得接受新的 write（会话进入 Draining） | Force 后 tree_snapshot 非空 → 返回 TreeNotEmpty{live}（如实上报，不谎称成功） |
| wait | WaitForSingleObject(job)（Job 内全部进程退出即 signaled）+ 单 pid 等待回退 | 非阻塞轮询用 0 超时；超时不得返回「未 reap」 | wait 返回必须伴随 reap（关闭 pid 句柄）；ExitInfo.reaped 是 API 级承诺 |

**跨平台等价**：POSIX 用 setsid（新会话 = 新进程组）→ killpg / waitpid(WNOHANG)；Linux ≥5.3 用 pidfd 消除 pid 复用竞态，否则 waitpid + start_time 双重校验。

### 3.4 POSIX 侧规格（forkpty / openpty / TIOCSWINSZ / 控制终端 / 信号 / reap）

| 主题 | 规格 | 理由与代价 |
| --- | --- | --- |
| **forkpty vs openpty** | 默认 forkpty；openpty 仅用于 ① 需要 setsid 后自定义 env/cwd 且要避免 fork 后分配；② 受控提权（setuid 到目标用户）会话；③ 需要显式 TIOCSCTTY 的嵌套/登录会话 | K-05；两套路径都必须有测试，openpty 路径的 setsid / TIOCSCTTY 失败必须显式报错而非降级 |
| **控制终端** | 子进程 setsid → TIOCSCTTY（forkpty 已内含）；父进程保留 master fd；**禁止**在 master 关闭前清理 slave | 无控制终端的会话会让 Ctrl-C / SIGHUP 语义丢失（隐蔽错误） |
| **TIOCSWINSZ** | 唯一 resize 原语：ioctl(master, TIOCSWINSZ, &winsize)；cols/rows 必须 ≥1（0 视为 Unsupported）；px_w/px_h 为 0 时置 0 而非继承 | 内核在 winsize 变化时向前台进程组发 SIGWINCH——这是 POSIX 的无损 resize 机制（对比 W3） |
| **SIGWINCH 时序** | 顺序固定：① TIOCSWINSZ；② 若调用方请求，显式 killpg(fg_pgid, SIGWINCH)（幂等）；③ 记录 ResizeApplied{cols,rows,px} 事件；④ 等应用自绘（**不**由我们重排已有内容） | 顺序颠倒会让应用用旧尺寸重绘，产生永久错位；日志顺序即证据 |
| **SIGHUP** | 分两档：ShellExit（子进程自行退出 → **不发** SIGHUP）；SessionClose（用户显式关闭会话或 Transport 断开 → 发 SIGHUP 到前台进程组，2s 后 SIGKILL 到进程组） | 不能对「正常退出」补发 SIGHUP（会打断应用自己的清理）；也不能对「用户关闭」不发（会留下孤儿） |
| **reap 与僵尸** | SIGCHLD 走 signalfd（Linux）/ kqueue EVFILT_PROC（macOS）；收到后 waitpid(-1, WNOHANG) 循环直到 0；**双 SIGCHLD 合并**：一次处理必须 drain 全部已退出子进程 | 不 drain 会积累僵尸（§8.2 可靠性门禁会以 fd/进程计数暴露）；用属性测试断言「N 个子进程退出后 zombies = 0」 |
| **信号映射** | Sig（语义枚举）→ POSIX 具体信号：Int→SIGINT、Term→SIGTERM、Hup→SIGHUP、Quit→SIGQUIT、Winch→SIGWINCH、Kill→SIGKILL、Stop→SIGSTOP、Cont→SIGCONT | 映射表是唯一真相（§3.5），禁止调用方硬编码整数 |
| **macOS / BSD 差异** | 无 signalfd → kqueue(EVFILT_PROC)；无 pidfd → waitpid + proc_pidinfo 校验 start_time | 实现分支不同，但**行为契约相同**（§5 PTY-AC-04 三平台同判据） |

### 3.5 信号语义映射表（唯一真相；禁止调用方硬编码整数）

| 语义 | POSIX | Windows（优先） | Windows（回退） | 备注 |
| --- | --- | --- | --- | --- |
| Int | SIGINT 到前台 pgid | GenerateConsoleCtrlEvent(CTRL_C_EVENT) | write(0x03) | 回退即 K-03；写审计 SignalByteFallback |
| Term | SIGTERM | GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT) | write(0x03) 后 500ms kill(Graceful) | 不把 CTRL_CLOSE_EVENT 折叠进来（W6） |
| Hup / Quit / Usr1 / Usr2 | 对应信号 | Unsupported | — | 明确返回 SignalOutcome::Unsupported，UI 不做假承诺（AR-20） |
| Winch | SIGWINCH | Unsupported（ConPTY 内部处理） | 走 resize() 而非 signal() | 调用方不得「试一下 Winch」 |
| Kill / Stop / Cont | SIGKILL / SIGSTOP / SIGCONT | TerminateJobObject / Unsupported / Unsupported | — | Stop/Cont 在 Windows 无等价物；UI 不渲染「暂停」按钮 |

### 3.6 Transport 抽象与字节保真契约（DC-19 / AR-04 / AR-11）

```rust
// crates/termai-transport/src/lib.rs
pub enum TransportKind { Local, Ssh{ jump: Vec<JumpHop> }, Container{ engine: ContainerEngine }, Wsl{ distro: Option<String> } }

pub trait Transport: Send + Sync {
    fn open(&self, spec: &ConnSpec) -> Result<Box<dyn Channel>, TransportError>;
    fn capabilities(&self) -> TransitCaps;
    fn reconnect(&self, ch: &mut Box<dyn Channel>, mode: ReattachMode) -> Result<ReattachReport, TransportError>;
}
pub trait Channel: Send {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;
    fn write(&mut self, data: &[u8]) -> Result<usize, TransportError>;
    fn resize(&mut self, sz: WinSize) -> Result<ResizeEffect, TransportError>;
    fn signal(&mut self, sig: Sig) -> Result<SignalOutcome, TransportError>;
    fn close(self: Box<Self>) -> Result<CloseReport, TransportError>;
}
pub struct TransitCaps { pub durable_resize: bool, pub reattach: ReattachCap, pub signal: bool,
                        pub graphics: bool, pub max_frame: u32, pub fidelity: Fidelity }
pub enum ReattachCap { None, SameProcess, Unknown }
pub enum ReattachMode { SnapshotOnly, TryRemote } // SnapshotOnly 为默认；TryRemote 仅在 caps.reattach == SameProcess 时允许
```

**字节保真契约（分层，唯一口径）**：三级保真标签，每个 hop 必须声明其一；带 F2 的 hop 不允许出现在需要 F0 的链路上。

| 层 | 标签 | 规则（绝对） | 具体实现约束 |
| --- | --- | --- | --- |
| **PTY / 应用 ↔ VT 解析器** | **F0 逐字节恒等** | 不得插入、删除、重排、重编码任何字节；bytes_out_of_read == bytes_into_vt_parser（ConPTY 的改写发生在**上游**，不计入本层） | termai-pty 读循环只搬字节；任何加工走旁路事件；Session Log 记录**读到的字节**（唯一真相） |
| **sessiond ↔ UI（termai-ipc）** | **F0** | 网格 snapshot/delta 与批量字节不被重编码；热路径禁 JSON（DC-22） | 二进制帧；禁止 UI 侧「文本化再解析」 |
| **Local / WSL 本地跳** | **F0** | 直通；WSL 走本机 named pipe / socket，不经人工层 | 禁止中间进程做行尾/编码规范化 |
| **Container（exec / attach）** | **F0（通道）× F2（守护侧声明）** | 通道内字节恒等；docker exec -it 之类**守护进程侧** TTY 处理属外部实现，必须由 F2 标注并在能力矩阵暴露 | 能力探测：caps.fidelity 由后端在 attach 前探测（Tty=true/false）；UI 显示保真徽章 |
| **SSH** | **F0（数据通道）× F1（协议注释）** | SSH 通道层可插入协议级注释（keepalive/banner）；**任何**进入数据通道的字节必须逐字节透传；窗口调优不得改变字节内容 | 禁止 shell out 系统 ssh（DC-19）；自研 russh 通道只做 pty-req / window-change / signal |
| **UI 视觉层（L0 渲染）** | **F2** | 允许折行 / 裁剪 / 装饰（AR-22 第 4 条、AR-23 第 6 条）：不改字符内容、不改网格列数、不改复制结果 | 复制取逻辑行；折行提示不参与选区与 AI 上下文（02-spec §3.12） |
| **AI / 插件** | **F2（只读镜像）** | 插件只读镜像 + 显式注入队列，**永不写 PTY**（AR-07）；写 stdin 需显式授权（AR-03） | 注入队列经 Local API + capability 校验 |

**远程 resize 与重连语义（K-07 / K-08 落地）**：

| 场景 | 动作 | 用户可见 | 事件与审计 |
| --- | --- | --- | --- |
| 远端支持 durable resize（pty-req / window-change / 容器 resize） | 先远端 resize，成功再更新本地网格 | 无 | ResizeApplied{scope:"remote"} |
| 远端 resize 失败/不支持 | 本地网格按软换行设置折行或裁剪（AR-22/AR-23）；**不**伪造远端尺寸 | 状态栏「远端尺寸 ≠ 窗口尺寸」徽章 + 已知问题入口 | ResizeDegraded{reason} + 审计 |
| SSH 断线（网络） | 进入 Detached；重连默认 SnapshotOnly，屏幕由本地 checkpoint 重建 | 明示「进程状态未知，未自动重放命令」（DC-04） | TransportDetached / ReattachReport{mode} |
| SSH 重连且对端会话保持 | 仅当 caps.reattach == SameProcess 且用户显式选择「尝试远程重连」时执行 | 清晰的两段确认文案 | ReattachRemote{evidence} + 审计 |
| 容器 attach 断开 / 守护重启 | 默认视为进程可能已终止；重建走 SnapshotOnly | 明确「容器会话可能已结束」 | ContainerReattach{pid_alive:bool} |
| WSL 分发重启 | 同容器；另加 wsl --list --running 只读探测 | 同上 | 同上 |

### 3.7 会话状态机（与 03-spec §3.6 对齐，本文补事件与迁移条件）

```
Creating ──spawn ok──▶ Running ──user detach / transport down──▶ Detached
   │                     │  ▲                                        │
   │                     │  └──────── reconnect (SnapshotOnly) ──────┘
   │                     ├──shell exit────────────▶ Exited ──reap──▶ Reaped ──▶ Closed
   └──spawn fail──▶ Failed(结构化原因)              └──IO/Job 失败──▶ Crashed ──reap──▶ Reaped
```

| 迁移 | 触发 | 必写事件 | 不得发生 |
| --- | --- | --- | --- |
| Creating→Running | spawn() 返回句柄 | PtySpawned{pid, sz, cap} | 不得在 slave 未就绪前接受 write |
| Running→Detached | UI 断 / Transport 断 | Detached{reason} | 不得杀进程（AR-13） |
| Detached→Running | 重连 | Reattached{mode, fidelity} | 不得自动重放命令（DC-04） |
| Running→Crashed | Job / IO 级错误 | PtyCrashed{code} | 不得静默降级为「新会话」 |
| *→Reaped | wait() 返回 | PtyReaped{code, signal, live_children} | live_children > 0 不得标记为 Reaped |

## 4. 接口与依赖

### 4.1 我需要谁给什么

| 提供方 | 我需要 | 形态 | 变更规则 |
| --- | --- | --- | --- |
| termai-core / core-dto | PtySessionId、WinSize、Sig、事件 schema（PtySpawned / Resized / SignalByteFallback / Detached / Reaped / OrphanReaped / ResizeDegraded） | IDL 生成 Rust + TS 双端类型 | 破坏性变更走 capability 协商，兼容 ≥2 minor（AR-04） |
| termai-session | 状态机的驱动方与 Log 写入方 | trait 调用 + 事件 | 状态枚举由本文 §3.7 定义 |
| termai-vt | 「同一字节流 → 同一网格」的解析器（本文只保证字节到达） | trait 边界 | vte 锁版本 + 差异登记（AR-18） |
| 安全 / 合规（05-spec） | 工具子进程沙箱策略（netns / seatbelt / AppContainer）与「超时杀进程树」要求 | 策略输入 | 与 05-spec §3.3 规则 6 对齐 |
| 发布工程（07-spec / ADR-0014） | 参考机三档与承载机分工（L2 集成、L6 矩阵） | 机器与 job 定义 | ADR-0014 不得被本文改写 |

### 4.2 我向谁承诺什么

| 对象 | 承诺 | 依据 |
| --- | --- | --- |
| termai-session / UI | PtyCapabilities 在 attach 前可查且同一进程内恒定；kill / wait 的 reaped 语义可依赖 | DC-16、§3.1 不变量 |
| VT / 渲染 | F0：到达解析器的字节与 PTY 读出的字节逐字节相等 | AR-04、AR-11（K-01） |
| 插件宿主 / Local API | **永不**提供写 PTY 的接口；只提供只读镜像与注入队列（写 stdin 走受控授权路径） | AR-07、AR-03 |
| 用户（诚实边界） | 三块文案必须存在：ConPTY 改写不可消除（C-W3/W5/W7）、远程 resize 不可承诺无损（K-07）、进程不可恢复（AR-13） | AR-20 |
| CI（07-spec） | 本节所有判据落在 cargo xtask matrix / cargo xtask replay / tests/conformance/ 的具名 job | §8.1 |

### 4.3 依赖与许可准入（AR-21 / ADR-0015）

| 候选依赖 | 用途 | 许可 | 链接边界判定（LB） | 结论 |
| --- | --- | --- | --- | --- |
| windows-rs（windows / windows-sys） | ConPTY / Job Object / 控制台 API | MIT OR Apache-2.0 | 同进程静态链接，**属链接边界内** | 允许（白名单内） |
| libc / nix | forkpty / openpty / ioctl | MIT OR Apache-2.0 | 链接边界内 | 允许 |
| russh / russh-keys | 原生 SSH（DC-19） | Apache-2.0（以 Cargo.lock 复核 SPDX） | 链接边界内 | 允许，但**必须**在 ADR-0015 白名单登记后方可合入（新依赖类别走 ADR） |
| portable-pty（wezterm 组件） | 可选参考实现 | MIT | 链接边界内 | **不默认采用**：抽象与本文九元面不一致，且会把 ConPTY 差异封装在不可见层；若采用须开 ADR（见 §7-C） |
| 任意 shell-out ssh.exe / plink | 远程 | — | **DC-19 明令禁止**；且触及「subprocess 调 GPL 工具」审查 | 拒绝 |

## 5. 可验证验收

> 预算口径：引用 §5 的一律标「门禁 / 目标」；本节新增指标以 **【新增】** 标注并归入第 8.1 节，**不与 §5 并列表述**。工具链：cargo xtask replay|matrix|perf-gate、tests/conformance/、tests/replay/、cargo-fuzz（DC-37）。

| ID | 判据（可量化） | 测量方法与语料 | 失败定位方式 | CI 落点 |
| --- | --- | --- | --- | --- |
| **PTY-AC-01** | **字节保真（F0）**：Local/Container 链路 bytes_out_of_read == bytes_into_vt_parser 且 source == sink（逐字节，非采样）；SSH 链路协议注释行之外去注释后逐字节相等 | 三方快照：① 回放语料源字节（tests/replay/corpus/pty/，100 条：CJK、emoji、SGR 洪水、Sixel 头、CRLF 混合、跨读边界的残缺 UTF-8 序列）；② Session Log 段原始字节；③ 测试挂钩在 termai-vt 入口前 dump 的字节；用 zstd -19 + SHA-256 三元比对 | 打印**首个差异偏移 + 上下文 64B 十六进制 + 差异分类**（insert/delete/replace/reorder），并写入 tests/conformance/known-rewrites.json；未登记即失败 | cargo xtask replay（L3 / G2），每 PR |
| **PTY-AC-02** | **ConPTY 改写 100% 登记**：conpty-rewrite-report.json 中每条改写都有 RuleSetId；severity=block 或 scope=data 的改写在 Windows x64 上 = **0** | 回环探针（§3.2 W1 的全字节 probe.bin）+ 7 条应用场景；Win10 1809 与 Win11 24H2（RM-B / RM-A，ADR-0014）各跑一次 | 未登记改写 → 打印 rule=UNKNOWN、原始字节、conhost.exe 文件版本与 OS build | cargo xtask matrix（L6，G1 的 ConPTY 分量），nightly |
| **PTY-AC-03** | **进程树闭环**：close(pty) 后 tree_snapshot 为空于 **2s** 内成立；kill(Force) 后同一断言于 **200ms** 内成立；wait() 返回必含 reaped=true；孤儿兜底扫描 60s 周期内 live_children 归零（KeepOnOrphan 会话除外且必须显式打标） | 语料：spawn(shell 启动两个后台 sleep 后 exit)（孙进程）、pwsh 启动独立 cmd（跨进程组）、10 并发会话各 50 子进程 | 失败时 dump process_tree 全表（pid/ppid/name/start_time）+ Job 限额字段 + OrphanReaped 审计时间线 | cargo test --test pty_tree（L2，每 PR）+ L8 soak（周度） |
| **PTY-AC-04** | 关联 §5：本条只定 PTY 层时序，**不重复** key-to-photon P99 ≤16ms（门禁）/ ≤8ms（目标）与 4K@120Hz 帧时 <8.3ms（门禁）的判定。**POSIX resize 时序**：TIOCSWINSZ 后应用在 **100ms** 内 TIOCGWINSZ 读到新值（SIGWINCH 送达）；事件顺序严格为 ResizeApplied → 应用重绘；200 子进程退出后 zombies == 0 | 探针程序 tests/conformance/posix_probe.c（输出 SIGWINCH 计数与 TIOCGWINSZ 值）；三平台各跑（Linux x64 / macOS arm64 / Linux arm64）；resize 序列 80→120→80→40 | 失败时打印 μs 精度事件时间线 + 捕获信号序 + /proc/pid/stat（Linux）或 proc_pidinfo（macOS） | cargo test --test posix_pty（L2）+ matrix L6 子集，nightly |
| **PTY-AC-05** | **兼容矩阵**：7 应用 × {Windows ConPTY, POSIX} 每格 1 条回归用例，「可启动且首屏稳定 + resize 后重绘正确」通过（语义断言，非像素断言）；偏差必须带「最小复现 + 豁免期限 + owner」 | vim / htop / neovim / fzf / tmux 嵌套 / less / python3 REPL，各带固定启动参数与固定测试脚本；执行 tests/matrix/apps/ 场景 | 每格失败输出：应用版本、`$TERM`、capabilities 快照、ConPTY 改写报告 diff、网格 dump | cargo xtask matrix（L6），nightly（承载机 = RM-B，ADR-0014） |
| **PTY-AC-06** | **Transport 能力矩阵**：四类 Transport 各声明 TransitCaps；每次 resize / signal 的 ResizeEffect / SignalOutcome 必须与声明一致（不得声明 Exact 而实际降级）；断线后重连模式必须与 caps.reattach 一致 | 4 类各 3 场景（正常 / 远端不支持 / 中途断线）；SSH 用本地 russh 测试服务器（非系统 ssh，符合 DC-19）；容器用本地 docker/podman（CI 无则 SKIP 并打印原因）；WSL 仅 Windows runner | 打印 TransitCaps 声明 vs 实测差异表 + 事件序列 + 断线时间线 | matrix + replay（L2/L3），nightly |
| **PTY-AC-07** | **接口不变量（属性测试）**：§3.1 五条不变量在 ≥10⁶ 次随机操作序列下无反例；1000 次 spawn/close 后 fd/handle 计数回到基线 | proptest 生成随机 {spawn,write,read,resize,signal,kill,wait,close} 序列；每平台跑一次；fd 计数用 /proc/self/fd（Linux）/ GetProcessHandleCount（Windows）/ proc_pidinfo（macOS） | 反例自动最小化 + 保存为回归种子（tests/fuzz/corpus/pty-seq/） | cargo test --test pty_props（L1/L2），每 PR |
| **PTY-AC-08** | **fuzz 门禁（§8.1-6）**：PTY 通道（write/read/resize 交错 + 畸形字节）24h 无 crash 且累计 ≥10⁸ 次执行（B5 / DC-37） | cargo fuzz run pty，语料 = 回放语料 + 历史 crash + 合成；4 target 之一 | crash 全部回归为语料；栈符号化后进 fuzz 看板 | cargo xtask fuzz-smoke（PR）+ 24h 长跑（nightly，自托管） |
| **PTY-AC-09** | **【新增】PTY-LAT-1**：PTY 层附加时延（keystroke → write 返回 → 首批字节 read 返回）P99 ≤ **2ms**（本地），**不含**渲染（渲染归 §5 key-to-photon P99 ≤16ms 门禁） | termai-bench --pty-latency，≥10⁴ 次采样；RM-A 空闲机器 | 输出直方图 + 慢样本归因（syscall 耗时、锁等待、调度延迟） | perf-gate 子集（nightly）；**已决（AR-30 第 1 条）：升为 §5 门禁（P99 ≤2ms）** |
| **PTY-AC-10** | **【新增】PTY-LOSSY-1**：ConPTY resize 有损事件可测——Lossy 会话在 vim/htop 场景 resize 后网格哈希 **>1** 帧未收敛即计一次；报告每条有 owner 与豁免期限 | PTY-AC-05 的 7 应用场景 + 逐帧网格哈希 | 输出未收敛帧序列 + 应用名 + 尺寸序列 | matrix（nightly）；**已决（AR-30 第 3 条）：并入 AR-25 第 3 条（未登记 ConPTY 改写即门禁失败），不单列** |
| **PTY-AC-11** | **【新增】PTY-RSS-1**：PTY 层 24h 长跑 RSS 斜率 ≤ **0.2MB/h**（**仅 PTY 层**，不取代 §5「24h RSS 斜率 <1MB/h」门禁，仅用于定位归属） | termai-bench --soak --only=pty，100 会话 × 24h，RM-A 采样 | 超限时输出 per-session 内存分解（读缓冲、Job 记录、事件队列） | 周度 soak；**已决（AR-30 第 4 条）：并入 AR-24 第 1 条（RSS 核心进程组口径），不单列** |
| **PTY-AC-12** | **【新增】PTY-ORPHAN-1**：sessiond 被强杀后，Windows 孤儿（Job 内进程）与 POSIX 孤儿（进程组）在 **≤2s** 内被收割的比例 = **100%**（N=100 次） | 脚本：启动 100 会话 → 强杀 sessiond → 2s 后统计残留 | 残留清单（pid/ppid/创建时间/Job 名）+ 平台差异标记 | 周度 soak / 发版前；**已决（AR-30 第 2 条）：升为 §8.2 验收（=100%，会话关闭 ≤2s 回收）** |

## 6. 风险与降级

| 触发条件 | 降级行为 | 用户可见后果 | 恢复与审计 |
| --- | --- | --- | --- |
| ConPTY 能力探测失败（CreatePseudoConsole 不可用，Win10 <1809 或系统损坏） | 降级为 PipeFallback：stdin/stdout 管道，**无 TUI**、无窗口尺寸、无信号 | 状态栏明示「无伪控制台：TUI 应用不可用，仅支持行式程序」；不是黑屏、不崩溃 | 每次启动重探；写 PtyBackendDegraded{kind} + 审计 |
| 探测到**未登记**的 ConPTY 改写（§3.2 severity=block） | 会话进入 SuspectedByteCorruption：继续运行但**禁用**字节保真徽章，回放比对降级为网格比对 | 徽章变「本次会话字节可能被平台改写」；termai diag 提供一键导出报告 | 登记并过门禁后恢复 F0 徽章；审计留痕 |
| Job Object 创建/加入失败 | **拒绝 spawn**（不退化为无 Job 会话），返回结构化错误 | 「无法启动：进程树管理不可用」+ 可操作提示（检查权限/杀软/AppContainer 策略） | 修策略后重试；写 JobAttachFailed |
| 同一会话 10 分钟内 OrphanReaped > 0 两次 | 标记 UnstableTree：禁用 Graceful（直接 Force），提示可能有外部杀软干预 | 会话可继续使用；状态栏徽章 | 事件 + 审计 + 诊断包入口 |
| SSH 断线（网络抖动 / 中间设备） | Detached + 重连默认 SnapshotOnly；**绝不**自动重放命令 | 明示「进程状态未知」；屏幕由 checkpoint 重建，无法重建的区域诚实留空 | 用户显式选择「尝试远程重连」（仅 ReattachCap::SameProcess）；审计 |
| 容器守护重启 / attach 断开 | 同 SSH，并追加「容器可能已结束」提示 | 明确「会话可能已结束」；不启动新容器冒充旧会话 | 探测 pid_alive；审计 |
| 远端 resize 不支持或失败（K-07） | 本地折行 / 裁剪（AR-22、AR-23 第 6 条），远端尺寸保持不变 | 状态栏「远端尺寸 ≠ 窗口尺寸」；TUI 可能画到错误列数 | 用户可手动触发重绘或重启应用；事件 ResizeDegraded |
| WSL 分发不可用 / 未安装 | 选择器条目禁用并给出安装指引；不影响 Local 会话 | 选择器内不可选 + 明确原因 | 只读探测 wsl --list --running；不自动安装 |
| 字节保真断言在 PR 上失败（PTY-AC-01） | **阻断合并**（§8.1 六件套；不提供「观察项」开关，AR-19） | 无用户可见影响（未发布） | 打印首差异偏移 + 分类 + 是否需登记；修复或登记后过门禁 |

## 7. 被否决的方案与反方意见（保留 tradeoff）

| 被否决方案 | 反方最强理由 | 我方反驳 | 复议条件 |
| --- | --- | --- | --- |
| **A. 在 PTY 读路径上做「就地脱敏 / 就地过滤」**（在字节缓冲层抹掉疑似 secret 或改写控制序列） | 安全派：能减少原始字节落盘与入上下文的面；性能派：一次遍历即可，成本可忽略 | 违反 K-01 与 AR-04（唯一真相被改写后不可回放、不可审计）；DC-33 已定死脱敏唯一收口在 Context Builder；就地改写会让「复制结果 ≠ 应用输出」，属 UX-R12 类正确性事故 | 不复议。若要减少落盘面，只能走会话级 sensitive_session 开关 + 原始 scrollback 持久默认关闭（AR-13 / OQ-04），属配置面而非内核改写 |
| **B. 采用系统 ssh.exe / plink 作为 SSH Transport（shell out）** | 务实派：成熟、省人力，known_hosts / agent / 跳板机全部免费 | 违反 DC-19 明令；PTY 流经边界外进程，与 AR-11 的「不出设备」精神冲突；错误语义不可结构化 | 不复议（DC-19 既定）。若未来 RFC 推翻，必须同步修订 ADR-0009 与本规格 §4.3 |
| **C. 引入 portable-pty 直接作为唯一 PtyBackend 实现** | 复用派：wezterm 已在生产验证，覆盖 ConPTY 与 forkpty | 其抽象是 CommandBuilder → Box<dyn MasterPty>，**没有** process_tree / wait / reaped / ResizeEffect / capabilities 五个本文需要的一等面；ConPTY 改写被封在不可见层，无法做 PTY-AC-02 | 若上游补齐九元面并接受本文 Fidelity 契约，可复议为**参考实现**（不取代自有 trait） |
| **D. 统一走 openpty（放弃 forkpty）** | 可控派：字节级控制、无隐藏 fork 行为、便于将来扩展 | 代价是重写已验证路径（AR-18 的同一条理由：不重复已验证苦力），且 setsid / TIOCSCTTY 的失败分支成为新 bug 面 | 若 forkpty 在任一目标平台出现不可绕过的限制（如 macOS 沙箱/签名限制），复议为「仅在受限平台用 openpty」 |
| **E. Windows 侧自研 NativePty（绕过 ConPTY）** | 性能派：摆脱 ConPTY 的改写与延迟，字节真正保真 | 内核态/驱动签名/稳定性风险与收益不成比例（04 角色原文明确保留为「长期实验分支，非主线」）；ADR-0014 亦以 ConPTY 为 v1 唯一生产路径 | 沿用 04 原文：若 ConPTY 连续两个 release 周期引入**不可登记**的改写，或 ResizeCap 永久为 Lossy，启动 NativePty 评估（不自动切换） |
| **F. 把 process_tree / wait 收进 sessiond 而非 PtyBackend** | 分层派：背板应保持最小面，树管理属会话策略 | 会让两套平台实现（Job/进程组）散落在 sessiond 里，跨平台一致性失去单一测试点；PtyBackend 是唯一持有平台句柄的对象（DC-16） | 若 PtyBackend 面被证明确实过大（>12 方法或引入循环依赖），复议拆为 PtyBackend + ProcessTreeBackend 两个 trait（依赖方向不变） |
| **G. resize 时由我们重排已有屏幕内容（模拟无损）** | 体验派：用户不想看到错位 | 违反「网格由列数定义」与 AR-14（不得在渲染层改写字符内容）；重排会改写真实应用的字符流，属正确性事故 | 不复议。只能做「发 SIGWINCH + 提示 + 徽章」 |
| **H. 远程会话默认自动重连并重启远端 shell** | 便利派：用户不想手动 | 直接违反 DC-04（绝不自动重放命令或重启进程）与 AR-13 的诚实声明；会掩盖「进程已死」的事实 | 不复议（DC-04 不可协商） |

## 8. Open Questions（影响面 / 建议值 / 决策阶段）

> §8.1 为**新增可测指标裁决请求**（硬性规则 2）：请 Orchestrator 决定是否升为 HARNESS §5 门禁或 §8 验收项；裁决前这些数字**只作本规格内部判据**，不得对外承诺。本节条目**复用 §5 的 PTY-AC-09…PTY-AC-12 单一编号**（不再另立 8.1-N 编号），二者是同一批指标。**本批四项已由 AR-30 全部裁决（见下表状态），不再作为待决项（AR-31 D1）。**

### 8.1 新增指标裁决请求（不与 §5 并列）

| ID | 指标 | 建议值 | 影响面 | 代价（若升格） | 建议阶段 |
| --- | --- | --- | --- | --- | --- |
| PTY-AC-09（**已决：AR-30 第 1 条，升 §5 门禁**） | PTY-LAT-1（PTY 层附加时延） | 本地 P99 ≤2ms（不含渲染；测量锚点 = sessiond 侧「收到输入帧 → PTY 写入返回」，见 kernel/06） | 内核规格、性能门禁、RM-A 负载 | 需常驻微基准与 RM-A 机时；与 §5 key-to-photon 测量重叠，须防重复计入 | **已决（AR-30）** |
| PTY-AC-10（**已决：AR-30 第 3 条，不单列**） | PTY-LOSSY-1（ConPTY resize 有损事件数） | vim/htop 场景每会话 ≤1 次 / 10 次 resize，且必须登记 owner | Windows 体验、TUI 兼容声明 | 会让 Windows 侧长期背负红灯风险；需与终端内核角色协商改写为「已知问题清单」而非门禁 | **已决（AR-30；并入 AR-25 第 3 条）** |
| PTY-AC-11（**已决：AR-30 第 4 条，不单列**） | PTY-RSS-1（PTY 层 RSS 斜率） | ≤0.2MB/h（仅 PTY 层） | 长跑门禁归属与定位效率 | 增加一处采样点与一套 per-session 分解工具 | **已决（AR-30；并入 AR-24 第 1 条）** |
| PTY-AC-12（**已决：AR-30 第 2 条，升 §8.2 验收**） | PTY-ORPHAN-1（强杀 sessiond 后孤儿收割率） | 100%，≤2s（会话关闭 ≤2s 回收；属生命周期正确性，不进预算表） | 可靠性、Job Object 契约、POSIX 进程组 | Windows 侧受杀软/AppContainer 影响可能 flake，需 EXTERNAL 类豁免口径（ADR-0014 §决策 5） | **已决（AR-30）** |

### 8.2 待裁决问题

| ID | 问题 | 影响面 | 建议值 | Phase |
| --- | --- | --- | --- | --- |
| **OQ-PTY-01** | conpty-rules.toml 的 **owner 与豁免期限**由谁维护（发布工程 / 内核 / 平台组）？（**已决：AR-31 第 3 条**） | 门禁职责、CODEOWNERS（07-spec §3.1.2） | **已决（AR-31 第 3 条）**：owner = T1 内核（`crates/termai-pty`），T5 复核；**每条 conpty 规则必须带 owner + expires（默认 2 个 minor）** | **已决（AR-31）** |
| **OQ-PTY-02** | Windows 侧 Sig::Int 的 **ByteFallback 是否允许静默**（写 0x03 而不提示）？（**已采纳默认值（AR-31 采纳分诊建议）**） | AR-20 诚实原则与 AR-06 反方（确认疲劳）冲突 | **已采纳默认值**：允许静默降级，但**每会话首次**在状态栏显示一次徽章（不弹窗）；审计必写（返工范围：02 信号路径 + PTY-AC-05 的 Windows 列 + 审计 schema） | P0 |
| **OQ-PTY-03** | Container Transport 的 fidelity=F2 判定依据：由我们探测（docker inspect Tty）还是用户声明？ | 字节保真徽章的可信度 | 探测优先（Tty=true/false）+ 用户可覆盖；探测失败按 F2 保守处理（AGENTS §7.4） | P1 |
| **OQ-PTY-04** | SSH reattach 的 SameProcess 判定是否允许启发式（如检测远端 tmux socket）？ | 重连体验 vs DC-04 硬约束 | **不允许启发式自动升级**；必须用户显式选择「尝试远程重连」，且 UI 明示「可能失败、不会自动重放」 | P5（与 OQ-18 对齐） |
| **OQ-PTY-05** | PtyCapabilities 是否进入 Local API 公开契约（第三方 / CI 可读）？ | DC-24 单一契约、插件与 IDE 集成 | 进入（只读），纳入 N-2 minor 兼容（DC-40）；破坏性变更走 capability 协商 | P1 |
| **OQ-PTY-06** | WSL Transport 的 fidelity 与 resize 是否与 Linux 本地等价？（**已决：AR-31 D5**） | Windows 用户对 WSL 的期待 | **已决（AR-31 D5）**：WSL 的保真级别**取决于底层 Transport**——经 ConPTY 承载时按 ConPTY 登记 **F1/F2 + Lossy**（未登记改写即失败，AR-25 第 3 条）；**直接使用 WSL 自身 PTY 通道时才是 F0 + Exact resize**；两者均需 PTY-AC-06 用实机证明 | **已决（AR-31 D5）** |
| **OQ-PTY-07** | 用户会话默认 wall-clock 超时（目前定为「无」）是否需为「被遗忘的会话」设上限？ | 资源占用 vs 不杀用户进程原则 | 默认无超时；提供**按会话**可选 idle-timeout（默认关闭），超时前必须先发 Sig::Hup 而非直接 Force | P1 |

---

**自查记录（定稿前执行）**：① 与 HARNESS 冲突检查——本文未改动任何 AR/DC 结论，差异已在文首「与 HARNESS 的差异登记」显式列出；② 八节齐全且顺序固定；③ 每条验收含测量方法 / 语料 / 判据 / CI 落点（§5 十二行全覆盖）；④ 被否决方案 8 条，均含反方最强理由与复议条件；⑤ 引用编号核对——AR-03/04/06/07/11/13/14/18/19/20/21/23 与 DC-04/15/16/18/19/20/22/27/33/34/37/40 均存在于 HARNESS.md；ADR-0009/0012/0014/0015/0017 均存在于 docs/adr/；⑥ 新增指标全部进 §8.1，未与 §5 并列。
