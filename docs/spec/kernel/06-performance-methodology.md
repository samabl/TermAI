# 06 · 性能与测量方法论（P0 信任基座）
> 本文件只规定「怎么测、怎么判、测不准怎么办」，**不新增、不放宽、不合并**任何预算数值。
> HARNESS §5 的「门禁列 / 目标列」双口径（AR-19）在本文件中原样引用；本文件新增的任何可测指标一律标 **【新增】** 并登记在 §8。
> 与 ADR-0014（参考机 / 后端阶梯 / CI 成本 / nightly 口径）一致：本文件是 ADR-0014 在「测量仪器与方法」维度的可施工展开。

## 1. 范围与依据（引用具体 AR/DC 编号）
| 类型 | 依据 | 本文件如何使用 |
| --- | --- | --- |
| 分层与热路径 | **AR-01**（网格不经 WebView）、**AR-03**（AI 不进 PTY→像素）、**AR-04**（热路径禁 JSON/gRPC） | 决定 key-to-photon 探针必须落在 L0/L1 原生进程内，不得把 WebView 计入热路径 |
| 会话与进程 | **AR-13** / **DC-18**（sessiond 唯一真源）、**DC-22**（termai-ipc 二进制）、**DC-16**（ConPTY） | 决定 t2/t3 测量点在 IPC 与 PTY 边界；RSS 进程集合必须显式枚举 |
| 渲染 | **DC-17**（wgpu + swash）、**AR-14**（无 subpixel AA）、**DC-20**（line-arena 10k 行） | 决定帧时用 vblank 时间戳而非 CPU 帧耗时；字体后端固定 |
| 预算口径 | **AR-19** + HARNESS **§5** 全表 | 门禁/目标分列输出，禁止互相掩护 |
| 门禁 | HARNESS **§8.1-4**（回归 >5% 阻断合并）、HARNESS **§8.2**（可靠性 / 发布）、spec 07 §5 A6/A9、spec 07 §3.3（L4/L8）、§3.4.3、§3.4.4 | 决定 G4 的 CI 落点与判定状态机 |
| 参考机 | **ADR-0014** 决策 1/4/5/6（RM-A/B/C、T0–T3、EXTERNAL、3000 美元上限） | 机器绑定、指纹、基线重设、成本上限 |
| 供应链与许可 | **ADR-0015**（SLSA L2 地板、链接边界） | 安装包度量的 provenance 前置；bench 依赖的许可准入 |
| 隐私 | **AR-12** / **DC-33** / spec 05 SEC-AC-07 | 语料与报告不得外流内容；bench 无网络句柄 |
| a11y/画质 | **AR-20**（能力边界明示）、**AR-22/AR-23**、spec 02 A7/A17 | reduced-motion 冻结与视觉回归复用同一冻结契约 |
| 角色原文（冻结证据） | roles **04** §3.3/§七、**05**、**07**、**08**、**10** §3 | 指标来源与反方意见出处 |

## 2. 关键结论（K-01 起）
| # | 结论 | 理由 | 代价（自认） |
| --- | --- | --- | --- |
| **K-01** | 门禁数值只在 **RM-A / RM-C 的 T0 后端**上判定；云 runner、T1–T3、无呈现后端（headless backend）一律 NON_GATING（**注意：AR-24 的「吞吐 = headless（sessiond-only）」是测量口径，不是后端降级**，见 K-09 / §3.2） | ADR-0014 决策 1 铁律 1/5；跨机/跨后端数字不可比 | 参考机成为单点供给；缓解见 §6 |
| **K-02** | 统计分三层：**样本 → Run 统计量 → N≥10 个 Run 的中位数**。报告必须三层同出 | spec 07 §3.8.3-1；隐藏任一层都会掩盖噪声 | 报告体积与运行时上升 |
| **K-03** | 尾部指标（P99）以 **Run 内 ≥10⁵ 样本**为有效性前提，不足则该 Run INVALID | P99 在小样本上无意义（spec 07 §3.8.3-1） | key-to-photon 单次 Run 需 ≥10⁵ 次注入 |
| **K-04** | MAD / 中位数 > 2% → INCONCLUSIVE；它**既非 PASS 也非 FAIL**，不得用云 runner 顶替 | spec 07 §3.8.3-2；ADR-0014 铁律 5 | 噪声大时门禁停摆，必须修环境而非放行 |
| **K-05** | **G4-PR（合并阻断）与 G4-REL（发版阻断）是两个口径**：前者单次有效运行 >5% 即阻断（§8.1-4），后者需连续 2 夜复现（spec 07 §3.8.3-3） | HARNESS 效力高于 spec 07，冲突时以 HARNESS 为准 | PR 存在假阳性阻断；差异登记 §8 OQ-PM-03 |
| **K-06** | **测量自身必须先过自检**：场景冻结（D0 字节相等）+ 双跑一致（D1 统计稳定）；同 commit 同机器指纹连测两次 **verdict 不得翻转**（AR-27）；未过自检或翻转直接 INVALID，**不得与基线比对** | 「光标未冻结 → 4 渲 3 哈希」事故的方法学回应；对齐 spec 07 §3.4.4 与 design-gates B4「自检先于比对」 | 每次门禁多跑一遍场景；CI 时长 +8%~15% |
| **K-07** | key-to-photon 的门禁数字由 **in-band 探针**（显示器 actualPresentTime）产出；**高速相机只做周期性偏置标定与争议仲裁**，不参与每次判定 | 相机不可自动化（ADR-0014 要求 ≥1000fps 校验探针偏差），无法进 CI | 探针偏置无法每次自证；靠 D 标定 + 不确定度预算覆盖 |
| **K-08** | **1GB 语料不入库**；入库的是生成器 + manifest + 每分片 SHA-256 + ≤8MB 冒烟片；生成物内容寻址、字节可复算 | 仓库体积、许可、CI 拉取成本、压缩不确定性 | 首次基准需现场生成（约 3–6 min）；生成器成为新的正确性面 |
| **K-09** | 吞吐 = **headless（sessiond-only）**：PTY 首字节 → 解析 + grid + damage 完成（**不含 shaping / present**）；聚合用**按字节加权的调和平均** | **AR-24 第 3 条**裁决吞吐口径 = headless（sessiond-only）；§5 的「解析 **+** 渲染」由「门禁吞吐 headless + UI 侧最终 grid 哈希/丢帧断言」共同覆盖 | headless 口径不含 shaping，跨版本比较需固定 UI 侧断言；测量定义见 §3.2 |
| **K-10** | 帧时用**显示器 vblank 时间戳**（DXGI GetFrameStatistics / VK_GOOGLE_display_timing / Metal presentedTime）；丢帧按**缺 vblank**定义 | CPU 帧耗时系统性低估；AR-19 要求最坏场景以门禁为准 | 依赖各后端 API 可用性；不可用时该指标 INVALID |
| **K-11** | 空闲 RSS 的进程集合（**AR-24 第 1 条**）显式定义为**核心进程组 = sessiond + 原生 UI ≤120MB**；L2 WebView 就绪后增量 **≤60MB**、插件宿主空载 **≤80MB** 各自**独立记账**、不计入核心组基线；**必须同时报告总内存**（不允许只报核心组） | **DC-38** 明确「插件宿主不占用核心 120MB 基线」；AR-02 明确 WebView 可选；AR-20 诚实原则要求报总内存 | 定义与用户任务管理器所见不同，必须在 UI/文档明示（AR-20）；总内存可能看起来超预算 |
| **K-12** | 24h RSS 斜率用 **Theil–Sen 稳健斜率 + 1h 步进检测**，且负载必须是**确定性 soak 脚本**而非空转 | 空转 24h 测不出泄漏；OLS 被单次 GC 峰值带偏 | soak 脚本自身成为维护面；需脚本哈希入库 |
| **K-13** | 冷启动门禁口径 = **WARM**（热页缓存、无特权）；**COLD-DISK 为【新增】非门禁观测** | drop_caches/purge/EmptyStandbyList 需特权且跨 OS 不可比 | 二进制膨胀导致的冷启动退化不被门禁捕获；由安装包门禁 + 发版前观测兜底 |
| **K-14** | 无相机时的替代折算：key_to_photon ≈ T_probe + D_max，并派生内部门槛 T_probe.p99 ≤ 16ms − D_max | 保留可信上界而不自欺；D_max 全部来自可核对的器件/路径项 | D_max 是上界不是实测，结论偏保守（§8 OQ-PM-02） |
| **K-15** | 无法进 CI 的指标（24h 全量 soak、相机标定、服务端 SLO、成本看板）必须给出**替代护栏**，且替代护栏**不得被表述为门禁** | AR-20 诚实原则；spec 07 Q1/Q2 反橡皮章 | 这些指标的回归只能在周度/发版前捕获，存在最长 7 天窗口 |

## 3. 详细设计
### 3.1 判定状态机（唯一入口）
```text
Verdict ::= PASS{margin} | FAIL{delta,kind} | INCONCLUSIVE{reason}
          | INVALID{reason} | SKIP{reason} | NON_GATING{reason}
evaluate(metric, machine, fingerprint, baseline):
  if !fingerprint.recorded                -> INVALID(FP_MISSING)           # ADR-0014 铁律 2
  if fingerprint != machine.now()         -> INVALID(FP_MISMATCH)          # 换件/驱动变更
  if backend != T0                        -> NON_GATING(BACKEND_DEGRADED)  # 铁律 1
  if !scene_frozen()                      -> INVALID(SCENE_NOT_FROZEN)     # K-06，D0
  if !double_run_consistent(eps_self)     -> INVALID(MEASUREMENT_UNSTABLE) # K-06，D1
  if runs.valid < 10                      -> INVALID(INSUFFICIENT_RUNS)
  if metric.is_tail && any(run.n < 1e5)   -> INVALID(INSUFFICIENT_SAMPLES)
  if mad_over_median(runs) > 0.02         -> INCONCLUSIVE(NOISE_FLOOR)     # K-04
  v = median(run.stat) + calibration_delta(metric)
  return (v <= gate) ? PASS(gate - v) : FAIL(v, gate)
```
回归判定（G4-PR / G4-REL 分流，见 K-05）：
```text
regression(metric, baseline, nights, context):
  d  = (median_now - baseline.median) / baseline.median
  if d <= 0.05                             -> PASS
  if context == PR                         -> FAIL(REGRESSION_PR)     # §8.1-4，单次即阻断
  if nights.consecutive >= 2 && d2 > 0.05  -> FAIL(REGRESSION_REL)     # spec 07 §3.8.3-3
  if context == RC_FULL                    -> FAIL(REGRESSION_REL)     # 发版前全套一次即确认
  else                                     -> INCONCLUSIVE(SUSPECT_SINGLE_NIGHT)
```
### 3.2 §5 逐指标测量方法（内核可测项）
| 指标（§5 门禁 / 目标） | 测量点 t0 → t1 | Run 定义 | 工具 | 语料 | 采样 | 统计量 | 不确定判据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 冷启动到可输入 **P95 ≤150ms** / ≤100ms | CreateProcessW / posix_spawn 前 QPC → 含 ready-sentinel 的那一帧的 actualPresentTime，且输入管线已活（注入 1 键并观察到回显，不计时） | 1 次进程启动 = 1 样本 | cargo xtask bench --metric startup（release；termai-bench 启动器） | 无文件语料；固定 shell = bash×FreeType（RM-A/Linux）；另跑 pwsh×DirectWrite 交叉核对 | ≥100 次启动 / Run，N≥10 Run | Run 内 **P95**；报告 = N 个 Run-P95 的**中位数** | 启动后仍在非 T0 → INVALID；Run-P95 的 MAD/中位数 >2% → INCONCLUSIVE |
| key-to-photon **P99 ≤16ms** / P99 ≤8ms、P50 ≤4ms | §3.3 的 t0→t8 + 折算 D | 1 次按键 = 1 样本；1 Run = ≥10⁵ 次注入 | cargo xtask bench --metric latency + 注入器 + present API | 固定交互脚本（单字符回显 / 多字符粘贴 / TUI 重绘各 1/3） | Run 内 ≥10⁵；N≥10 Run | Run 内 P50/P99；报告 = 各 Run 统计量的**中位数** | 由 §3.3 的 D 不确定度决定；相机校验 abs(bias) >4ms → INVALID |
| 4K@120Hz 网格帧时 **<8.3ms**、丢帧 **<0.1%** | actualPresentTime(n-1) → actualPresentTime(n) | 1 Run = ≥10⁴ 帧 | cargo xtask bench --metric frame（RM-C，144Hz 面板**锁 120Hz**、VRR/HDR 关） | 合成 damage 脚本（滚动 + 全宽重绘 + SGR + CJK + emoji），固定种子 | ≥10⁴ 帧 / Run，N≥10 Run | Run 内 P50/P99/Max + 丢帧率；报告 = 各 Run P99 与丢帧率的**中位数** | EDID 哈希不符 → INVALID；检测到其它可见窗口/合成器抢占 → INVALID |
| 解析+渲染吞吐 **≥500MB/s**（**headless（sessiond-only）口径，AR-24 第 3 条**） | PTY master 首字节可读 → 解析 + grid + damage 完成（**不含 shaping / present**）；UI 侧另断言最终 grid 哈希 + 丢帧 <0.1% | 1 Run = 完整 1GB 语料一遍 | cargo xtask bench --metric throughput | 生成式 1GB 语料（§3.7 分片表），每片 SHA-256 校验 | ≥10 Run | 按字节加权调和平均；报告 = N 个 Run 值的**中位数** | 喂数端无 ≥2× 余量（null sink 对照）→ INVALID；UI 侧哈希/丢帧断言失败 → FAIL |
| 空闲 RSS（1 万行）**≤120MB** | 稳定 60s 后的驻留集采样 | 1 次采样 = 1 样本；1 Run = 10s 内 30 次 | cargo xtask bench --metric rss + 进程采样（smaps_rollup / task_info / PDH） | 由 API 断言 scrollback 恰好 10 000 行；无插件 | 30 次 / Run，N≥10 Run | Run 内**中位数**；报告 = N 个 Run 中位数的中位数 | 存在额外 TermAI 进程 → INVALID；行数未达 10k → INVALID |
| 24h RSS 斜率 **<1MB/h** | 每 60s 采一次，24h | 1 Run = 1 次 24h | cargo xtask bench --soak --metric rss | 确定性 24h soak 脚本（§3.7），脚本哈希入库 | 1440 点 / Run；周度 1 Run | **Theil–Sen** 斜率 + 1h 步进检测 | 机器非独占、发生休眠/降频 → INVALID；残差 MAD >0.5MB → INCONCLUSIVE |
| 插件宿主空载 **≤80MB**；WebView 就绪后增量 **≤60MB** | 两者**各自独立记账**（**AR-24 第 1 条 / DC-38**：均不计入核心组 120MB 基线）；插件宿主独立进程稳定后采样；WebView 取 UI 进程「未加载 → 就绪稳定」的 RSS 增量 | 同 RSS | xtask bench --metric plugin-host / rss --include=webview | 插件空载（0 插件）与 1 个官方标杆插件两态；WebView 加载 AI / 设置面板 | 30 次 / Run，N≥10 Run | 中位数 | 与核心基线混算 → 报告 INVALID（口径错误） |
| 安装包 **<60MB** | 发布产物字节数（压缩后 / 公证后） | 1 次构建 = 1 样本 | cargo xtask dist + size 断言 | 三平台主格式各一 | 每次构建 | 精确值（无需统计） | 未签名/公证件缺失（EXTERNAL）→ SKIP；artifact 口径见 §8 OQ-PM-01（已决：AR-31 第 7 条） |

补充规则（全部可施工）：
1. **冷启动双观测【新增】**：同时输出 startup.process_to_first_frame 与 startup.first_frame_to_input_ready（诊断用，非门禁），否则回归无法归因到「进程初始化」还是「输入管线」。
2. **吞吐子指标【新增】**：throughput.headless（**门禁项**，sessiond-only = 解析 + grid + damage）与 throughput.end_to_end（**非门禁观测**，含 shaping / present，仅用于归因）。headless 下降 → 归因解析 / grid；headless 稳定而 end_to_end 下降 → 归因渲染（**AR-24 第 3 条**）。
3. **帧时子指标【新增】**：frame.cpu_enqueue（damage→submit）、frame.gpu_exec（submit→fence）、frame.present_wait（present 调用→vblank）。
4. **帧时场景必须冻结光标**：门禁场景取 cursor.steady；cursor.blink 作为**独立场景变体**输出为非门禁观测 **【新增】**（该变体即本轮事故根因所在，必须长期可见，见 §8 OQ-PM-10）。
5. §5 中非内核项（视觉回归 ≤0.1%、硬编码色值 = 0、网格对齐 ≤0.5px）由 spec 07 §3.4.4 与 tools/design-gates 承载，本文件只强制「冻结 + 自检先于比对」这同一契约；服务端项（AI 网关附加延迟 p95 <120ms / p99 <300ms、诊断首 token P95 <3s、控制面 ≤0.05 美元/MAU）由在线埋点与成本看板承载，见 §3.8。
6. **总内存必须报告（AR-24 第 1 条）**：任何 RSS 报告不得只报核心组，必须并列 `rss.core`（sessiond + 原生 UI）、`rss.webview`（就绪后增量）与 `rss.plugin_host`（空载）并给出 `rss.total`。

### 3.3 key-to-photon 探针设计（不依赖高速相机的可信数字）
**时间链（唯一编号）**

| 标记 | 含义 | 采集方式 | 是否 in-band |
| --- | --- | --- | --- |
| t0 | 按键进入 OS 输入栈 | uinput 的 input_event.time；Windows SendInput 前 QPC；macOS CGEventPost 前 mach_absolute_time | 是 |
| t1 | 应用事件循环收到输入 | native loop 入口打点 | 是 |
| t2 | 输入帧写入 termai-ipc | DC-22 帧边界 | 是 |
| t3 | sessiond 写 PTY/ConPTY | write 返回 | 是 |
| t4 | 应用回显从 PTY 读出 | read 返回 | 是 |
| t5 | VT 产生 grid delta | DC-17 管线 | 是 |
| t6 | GPU command buffer submit | wgpu 打点 | 是 |
| t7 | present 调用返回 | swapchain present | 是 |
| t8 | **显示器实际扫描出该帧** | DXGI GetFrameStatistics（PresentCount/SyncRefreshCount）/ VK_GOOGLE_display_timing.actualPresentTime / Metal presentedTime | 是 |
| t9 | 像素稳定（GtG 完成） | **相机可见**；in-band 不可见 | 否 |

**门禁值**：T_probe = t8 − t0；key_to_photon = T_probe + D，其中 D = D_input_stack（t1−t0 中不可见的部分）+ D_display（t9−t8）。
D 由**相机标定**得到：bias_probe = median(T_cam − T_probe)，要求 abs(bias_probe) ≤ 4ms 且其 95% CI ≤ 0.5ms，否则 INVALID。

**相机标定规程（人工、周期）**：≥1000fps 相机同时拍「按键指示灯/机械触点 LED」与「屏幕上固定探针单元区域」；ROI 取屏幕**首行**（固定扫描相位）；预热 ≥5 帧后取 ≥1000 对样本；记录面板型号与 GtG 规格。触发条件：指纹变更、季度、争议仲裁、门禁值与体感明显不符。

**相机误差预算**

| 误差源 | 量级 | 方向 | 处理 |
| --- | --- | --- | --- |
| 键盘 PCB 扫描 + USB 轮询 | 0.5–8ms（1000Hz→1ms，125Hz→8ms） | 正（相机可见，探针不可见） | RM-C 固定 1000Hz 键盘并写入指纹；计入 D_input_stack |
| 注入路径旁路 HID（uinput/SendInput 非真 HID） | 0.2–1.5ms | 负 | 与相机配对求 bias_inject 并单列 |
| 探针自身开销 | <20µs | 正 | null-input 对照实测并扣除 |
| vblank 时间戳精度 | ≤1 帧周期（8.33ms@120Hz）或 API 精度 | 正 | 只接受 actualPresentTime 类 API，报告 API 名 |
| 面板 GtG | 0.5ms(OLED)–10ms(IPS) | 正 | 相机标定 D_display；无相机用型号 datasheet 上界 |
| 扫描行相位 | 0–1 帧周期 | 双向 | 探针图案固定首行；相机取首次变化帧 |
| 合成器 / VSync 排队 | 0–2 帧 | 正 | 独占显示会话；报告 present 队列深度 |
| 相机曝光 / 量化 | 1ms 量化（±0.5ms 零均值）+ 卷帘拖影 | 双向 | ≥1000fps + 全局快门；取多帧中位 |

**无相机替代指标与折算【新增】**

| 替代指标 | 覆盖路径 | 无法覆盖 | 折算 |
| --- | --- | --- | --- |
| M1 t7 − t0（输入到 present 提交） | OS 输入栈 + 会话 + 解析 + 渲染 | vblank 排队 | 加 D_queue 上界 |
| M2 t8 − t7（present 到实际 vblank） | 呈现排队 | — | 与 M1 相加得 T_probe |
| M3 t5 − t3 / t6 − t5 / t7 − t6 | 解析 / 渲染 / 提交 | — | 纯归因用 |
| M4 t4 − t0（到回显可读） | 不含渲染 | 渲染与呈现 | **不得**用作门禁值 |

**保守折算公式（无相机时的门禁口径）**：T_probe.p99 ≤ 16ms − D_max，其中 D_max = 键盘 1000Hz 轮询上界 1ms + OS 输入栈上界 3ms + 面板 GtG 上界 G（RM-C 面板实测 datasheet）。D_max 的每一项必须可核对；不得使用「经验系数」。当 D_max 使内部门槛 ≤0 时必须判 INVALID，而不是放宽门槛。

### 3.4 参考机绑定、指纹与基线重设
**绑定（与 ADR-0014 决策 1 一一对应，不新增机器）**：计算类（启动、吞吐、RSS、斜率、插件宿主）→ **RM-A**；显示与延迟类（帧时、key-to-photon、网格对齐、视觉 golden）→ **RM-C**；**RM-B 只判地板阈值 F1–F6，不判 §5 门禁**。等价替换规则（ADR-0014 铁律 4）：CPU 单/多核相对基准机劣化 ≤5%、GPU 同 API 等级且驱动锁定、显示器像素与刷新率不低于门禁项要求。

**machine-fingerprint.json**（ADR-0014 铁律 2 的必填字段 + **【新增】** 扩展字段）：
```json
{ "id": "RM-A-win11-24h2", "role": "RM-A",
  "cpu":   {"model":"...","microcode":"...","cores":8,"threads":16,"r23_single":1980,"r23_multi":15200},
  "mem":   {"size_gb":32,"modules":"2x16","speed":"DDR5-5200","timings":"CL40"},
  "gpu":   {"model":"RTX 4060","driver":"551.xx","api":"dx12","fl":"FL12_1"},
  "display":{"model":"...","edid_sha256":"...","mode":"2560x1440@165","vrr":false,"hdr":false,
             "dpi_scale":100,"panel_gtg_ms":3.0},                 // 【新增】panel_gtg_ms
  "storage":{"model":"...","fw":"...","seq_read_mbps":6800},      // 【新增】
  "os":    {"name":"Windows 11","build":"26100.xxxx","arch":"x64"},
  "power": {"plan":"High performance","governor":"n/a","app_nap":false},
  "clocks":{"source":"QPC","qpc_freq_hz":10000000},               // 【新增】
  "fonts": {"set_sha256":"...","render_backend":"directwrite"},   // 【新增】
  "input": {"inject":"SendInput","keyboard_report_hz":1000},      // 【新增】
  "hypervisor": false,                                            // 【新增】
  "updated_at":"...", "superseded_by": null }
```
**换件 / 重装后的基线重设流程**（任一指纹字段变化即触发）：
```text
1. 冻结门禁：受影响指标全部置 INVALID(FP_MISMATCH)，不许"照旧比对"
2. 写 docs/audit/rm-baseline-<machine-id>-<ts>.md：旧指纹 -> 新指纹 diff + 换件原因
3. 标定：xtask bench --metric all --repeat 10 --baseline-mode=new
4. 冷基线窗口：新基线每指标 MAD/中位数 <= 2%，且两次全量重复的 medians 差 <= eps_self
5. 通过后写入 tests/bench/baseline/<fingerprint-id>.json；旧基线标 SUPERSEDED 但保留
6. 等价替换额外判定【新增】：另跑"纯 CPU 固定微基准"以把 CPU 替换与 GPU 替换解耦
```
### 3.5 回归流程与「5% 阻断」的具体判定
```text
nightly / PR 运行 -> evaluate()
 |- INVALID / INCONCLUSIVE -> 不进回归判定；写 flaky-ledger；
 |    同一指标 7 天内 2 次 -> 测量环境 P1（spec 07 §3.8.3-2，先修环境）
 |- PASS (d <= 5%)         -> 记录 margin（AR-19：达标余量入技术债登记，spec 07 §3.9）
 |- FAIL (d > 5%)
      |- context == PR     -> 立即阻断合并（HARNESS §8.1-4）；PR 内必须附归因 + 复现或撤回
      |- context == 夜间   -> 标 SUSPECT_SINGLE_NIGHT（不阻断）
             |- 连续第 2 夜仍 d > 5%      -> 阻断发版（spec 07 §3.8.3-3）+ 开 P1（§9 R7）
             |- 发版前全套运行一次复现     -> 同样阻断发版
```
**误阻断申诉**：仅当申诉方提供「同 fingerprint、同场景、N≥10 全量重跑」且 d ≤ 5% 的证据时，T5 可撤销一次 PR 阻断并登记；季度统计误阻断率（spec 07 Q2 反橡皮章）。

### 3.6 可复现性自检（仿 design-gates selftest）
**D0 场景冻结自检（字节相等，对应 design-gates「同状态连渲两次哈希相同」）**：对场景每一帧计算 grid_fingerprint = SHA-256(codepoint+attr 全量) ⊕ SHA-256(damage rect 列表)；同一场景连跑两次，**逐帧指纹必须完全相等**，否则 FAIL(SCENE NOT FROZEN)、直接 INVALID，**不进入基线比对**。
冻结手段（缺一不可）：terminal.cursor.blink=false、prefers-reduced-motion: reduce（AR-22 / spec 02 A7）、bench 专属**冻结时钟**（feature bench，见 §4）、字体加载完成后再起场景、GPU atlas 预热后再计时。

**D1 双跑一致自检（统计稳定）**：同 metric 连续两次 Run，eps_self = max(指标噪声地板【新增】, 2 × MAD/中位数上限)：

| 指标族 | eps_self | 依据 |
| --- | --- | --- |
| 场景指纹 / 网格哈希 | **0（字节相等）** | K-06，D0 |
| 帧时（P99） | 5% | 单帧事件离散度高 |
| 启动（P95） | 3% | 进程启动含 OS 调度 |
| 吞吐（中位数） | 2% | 稳态带宽 |
| RSS（中位数） | 1% | 分配器稳定 |
| 安装包（字节） | 0 | 确定性产物 |

超差 → INVALID(MEASUREMENT_UNSTABLE)，并输出**差异定位信息**（哪个子指标、哪一段、对照 artifact 路径），禁止只报一个比值。

**cargo xtask bench --selftest（证明门禁不是恒绿）**：在临时目录 / 合成驱动上注入必然故障，逐条断言被对应断言捕获，任一条未捕获则 selftest 自身退出 1：

| 注入 | 必须被捕获为 |
| --- | --- |
| 场景开启光标闪烁（blink=true） | SCENE_NOT_FROZEN / D0 |
| 场景注入依赖 Instant::now() 的随机内容 | SCENE_NOT_FROZEN / D0 |
| 双跑抖动注入 3%（> eps_self） | MEASUREMENT_UNSTABLE / D1 |
| 强制 --backend t1 | NON_GATING（不得 PASS） |
| 篡改 fingerprint 的 GPU 驱动字段 | INVALID(FP_MISMATCH) |
| 语料某分片 SHA-256 不匹配 | INVALID(CORPUS_DRIFT) |
| 吞吐路径注入 +10% 睡眠 | FAIL（回归） |
| 吞吐路径注入 +1% 抖动 | PASS（且 margin 正确） |
| 把 Run 内离散抬高到 MAD/中位数 = 5% | INCONCLUSIVE（不得 PASS/FAIL） |
| 删除基线文件 | SKIP + 打印生成命令（不得静默 PASS） |
| 报告缺 legacy 字段（§4） | schema 校验 FAIL |
| 单 metric 输出缺扁平投影 | 兼容性 FAIL |

**测量自证验收【新增】**：同一 commit 在同一 fingerprint 上跑 G4 全集两次，逐指标 **verdict 不得翻转**，且 abs(Δmedian) ≤ eps_self；**一旦翻转即判 INVALID(MEASUREMENT_UNSTABLE)，不得与基线比对、不得据此阻断**（**AR-27**）；产出 bench-repro-report.json 作为 P0 退出条件（§5 A-PM-01）。

### 3.7 benchmark harness 规格
**目录（全部在 tests/bench/ 与 crates/termai-bench/）**
```text
crates/termai-bench/            # 仪器库：Measurement / DeterminismCheck / gate()（§4）
crates/termai-xtask/            # 唯一入口：bench | perf-gate | matrix | dist（spec 07 §3.2.1）
tests/bench/
  corpus.manifest.json          # 分片权重 + 种子 + 每片 sha256 + 总 sha256     [入库]
  corpus/generate.rs            # 确定性生成器（固定 PRNG + 只读种子）           [入库]
  corpus/seeds/                 # 公有领域文本种子，<=8MB，含许可说明            [入库]
  corpus/smoke/                 # <=8MB 冒烟子集，每 PR 用                       [入库]
  corpus/shards/                # 1GB 生成物                                     [.gitignore]
  scenes/frame-4k120.json       # damage 脚本（帧数/种子/光标态）                [入库]
  scenes/soak-24h.json          # 24h 确定性负载脚本                             [入库]
  baseline/<fingerprint-id>.json# 基线（每 fingerprint 一份）                     [入库]
  fingerprints/<machine-id>.json# 机器指纹                                       [入库]
```
**1GB 语料：怎么造、是否可提交**

| 分片 | 内容 | 权重 |
| --- | --- | --- |
| S1 ascii-bulk | 纯 ASCII 长行 | 30% |
| S2 sgr | 逐字符 truecolor SGR | 15% |
| S3 cjk | CJK 双宽 + 组合符 + 变体选择符 | 15% |
| S4 emoji | emoji / ZWJ / flag | 5% |
| S5 tui | 备用屏全屏重绘 + 光标定位 | 15% |
| S6 osc | OSC 133/633/7/0/2 高频 | 5% |
| S7 graphics | Sixel / kitty graphics（限额内） | 5% |
| S8 longline | 单行 >10 000 列 | 5% |
| S9 pathological | 畸形/边界 ESC（必须不崩溃，DC-37） | 5% |

- 生成：固定种子 PRNG 派生各分片；运行前按 manifest 的 SHA-256 校验，漂移即 INVALID。
- **不提交 1GB 生成物**的理由：仓库克隆体积、LFS 网络依赖与 CI 成本、公有领域种子的许可边界、压缩产物不确定性导致缓存失效（K-08）。
- **入库部分**：生成器 + manifest + 种子 + smoke/（≤8MB）+ 期望哈希。
- **隐私（AR-12 / DC-33 / spec 05 SEC-AC-07）**：生成器不读取任何用户目录；种子仅公有领域文本与合成内容；入库前跑一次 secret 扫描（复用 DC-33 的正则 + 熵检测实现）；bench-report.json **只含聚合数值**，绝不含语料内容、路径或环境变量值；bench 进程不持有网络句柄（cargo-deny 校验无网络依赖）。

**输出契约 bench-report.json**（spec 07 §3.8.2 的 9 个 legacy 字段为**必备子集**，新增字段向后兼容 1 个 minor）：
```json
{ "schemaVersion": "1.0.0", "reportId": "...", "generatedAt": "RFC3339",
  "commit": {"sha":"...","dirty":false}, "toolchain": {"rustc":"...","channel":"..."},
  "runner": {"class":"self-hosted","machineId":"RM-A-win11-24h2","role":"RM-A","backend":"t0/dx12"},
  "fingerprintSha256": "...",
  "environment": {"powerPlan":"High performance","exclusive":true,"warmed":2,"valid":true},
  "selfcheck": {"scene":{"verdict":"PASS","frames":10000,"hash":"..."},
                "doubleRun":{"verdict":"PASS","deltaPct":0.004},"verdict":"PASS"},
  "corpus": {"manifestSha256":"...","shardsOk":9,"bytes":1073741824},
  "metrics": [
    { "metric":"latency.key_to_photon.p99", "value":11.8, "unit":"ms", "samples":100000,
      "runner":"RM-A-win11-24h2", "commit":"...", "toolchain":"rustc ...", "ts":"RFC3339",
      "statistic":"p99","runs":12,"runStat":"median","madOverMedian":0.011,
      "gate":16.0,"target":8.0,"gating":true,"verdict":"PASS",
      "method":"inband-present-timestamp + calibrated-D","dCalibrationMs":1.4,
      "artifacts":["latency-hist.svg","probe-trace.bin"] } ],
  "verdict": "PASS|FAIL|INCONCLUSIVE|INVALID",
  "cost": {"minutes":62,"runnerClass":"self-hosted","estUsd":0.49} }
```
以上 9 个 legacy 字段（metric / value / unit / samples / runner / commit / toolchain / ts + 顶层 commit）逐字保留；单 metric 调用额外输出顶层扁平投影，保证既有消费者不破（spec 07 §4.1「字段稳定」）。

**CI 接线与成本控制（ADR-0014 决策 6，月上限 3000 美元）**

| 流水线 | 触发 | 机器 | 内容 | 墙钟预算 | 失败处置 |
| --- | --- | --- | --- | --- | --- |
| perf-pr（PR-S3 子集，G4-PR） | 每 PR | RM-A（self-hosted） | 启动 30 次 + smoke 吞吐 + 安装包体积 | ≤10 min（spec 07 PR-S3） | 阻断合并 |
| perf-nightly（PR-S4 全量，G4-REL） | 每夜 | RM-A + RM-C | startup ≥100×10 Run、1GB 吞吐 ×10、RSS、帧时 ×10、插件宿主 | ≤90 min | 阻断发版 |
| perf-soak（L8） | 周度 + 发版前 | RM-A **独占** 或专用 soak rig（§8 OQ-PM-06；已采纳默认值：AR-31） | 24h 确定性 soak + 斜率 + 步进检测 | 24h（不占 nightly 墙钟） | 阻断发版 |
| perf-kp-calib | 人工（指纹变更/季度/争议） | RM-C | 相机偏置标定，写 D | 2 h（人工） | 未标定则 key_to_photon 只出 INCONCLUSIVE |
| perf-rc-full | 发版前 | RM-A + RM-C | 全量 + soak + 包体 | ≤3 h | 阻断发版（spec 07 §5 A1/A7） |

- 每条流水线输出 ci-cost.json（pipeline / minutes / runner_class / est_usd，ADR-0014 决策 6/3）。
- 量级（自托管约 0.04–0.08 美元/核时，RM-A 8 核）：perf-pr ≈147 机时/月、perf-nightly ≈45、perf-soak ≈96，合计约 140 美元/月，远低于自托管 1800 美元上限；真正稀缺的是 **RM-A 墙钟**，故 soak 排周末窗口。
- 到 80% 告警、100% 起**自动降采样**：L4 全量→隔夜、soak 周度→双周、E60→仅发版前（ADR-0014 决策 6）。降采样只改**频率**，不改数值；发版前全套不可降。

### 3.8 不能进 CI 的指标与替代护栏（如实说明）
| 指标 | 为何不能进 CI | 替代护栏 | 负责 |
| --- | --- | --- | --- |
| 24h RSS 斜率（全量） | 24h 独占机时，无法进 PR/夜间墙钟 | 周度 + 发版前全量；每 PR 跑「fd/句柄/GPU 资源计数 = 0 增长」+ 1h 斜率代理 **【新增】** | T1/T5 |
| key-to-photon 相机校验 | 需物理相机 + 人工布光/取景 | 内探针门禁 + 周期 D 标定 + D 不确定度进余量（§3.3） | T1 |
| AI 网关附加延迟 / 诊断首 token | 真实流量决定，无法在参考机复现 | 服务端 SLO 埋点 + 滚动 7 天看板；发版门禁要求近 7 天达标 | T6 |
| 控制面单 MAU 成本 | 成本看板，非构建期量 | 月度看板 + 超限 24h 降本方案（ADR-0014） | T6/T5 |
| 电源 / 能效 | 功耗需功耗仪，非 v1 §5 项 | **空闲 CPU ≤1%（聚焦、无输出、60s 均值）与「未聚焦或被遮挡出帧 = 0 帧/10s」已是 §5 门禁（AR-24 第 2 条，本文件不再标【新增】）**；**功耗降级为人工抽测目标（不进 CI）**，禁止文档出现无测量方法的功耗承诺 | T1 |
| 网格对齐 ≤0.5px | 可自动化但依赖真实 DPI / 显示器 | RM-C 上 100%/125%/150%/200% DPI 渲染用例（ADR-0014 映射表） | T2 |

### 3.9 「§5 指标 → 测量定义」映射表（AR-24 第 3 条 / HARNESS §5「映射要求」）

> **效力**：本表是 HARNESS §5 每一行到本文件测量定义的**唯一索引**。任何新增或修改的 §5 指标未在本表登记即不得进入 HARNESS §5（HARNESS §5 表末「映射要求」）。
> **归属**：测量定义、采样次数、统计口径与不确定判据在本文件落地；**语料**可由专项分册提供（HARNESS §5「语料归属」，例如「输入字节等价」的语料在 kernel/05）。
> **覆盖**：HARNESS §5 现行 **19 行**全部登记（H1…H19，行序与 §5 表一致）；C1 为**表外**设计门禁对照（HARNESS §5 无此行），不计入 §5 行数。

| # | HARNESS §5 指标（发布门禁 / 目标） | owner | 本文件测量定义落点 | 门禁 / 载体 | 属 kernel/06 管辖 |
| --- | --- | --- | --- | --- | --- |
| H1 | 冷启动到可输入 P95 ≤150ms / ≤100ms | kernel/06 | §3.2（第 1 行：spawn 前 → ready-sentinel 帧 present）；机器绑定 §3.4（RM-A） | G4 / L4；A-PM-04 | 是 |
| H2 | key-to-photon（本地）P99 ≤16ms / P99 ≤8ms（P50 ≤4ms） | kernel/06 | §3.2（第 2 行）+ §3.3（时间链 t0–t9、D 相机标定与保守折算）；§3.4（RM-C） | G4 / L4；A-PM-05 | 是 |
| H3 | PTY 层附加延迟（子指标，AR-30）P99 ≤2ms | kernel/06 | §3.3（时间链 **t3 → t4** 段：sessiond 写 PTY → 应用回显可读；端到端超标时的 PTY 环节归因） | G4 子指标；A-PM-05 | 是 |
| H4 | 4K @120Hz 网格帧时 <8.3ms（丢帧 <0.1%） | kernel/06 | §3.2（第 3 行：present 时间戳差）；§3.4（RM-C 锁 120Hz） | G4；A-PM-06 | 是 |
| H5 | 解析+渲染吞吐 ≥500MB/s | kernel/06 | §3.2（第 4 行）+ §3.2 补充 2（headless = 门禁、end_to_end = 非门禁归因）；K-09 | G4；A-PM-07 | 是 |
| H6 | 空闲 RSS（1 万行）≤120MB（核心进程组 = sessiond + 原生 UI） | kernel/06 | §3.2（第 5 行）+ K-11 + §3.2 补充 6（必须并列 rss.total） | G4；A-PM-08 | 是 |
| H7 | AI 面板（WebView）就绪后 RSS 增量 ≤60MB（独立记账） | kernel/06 | §3.2（第 7 行「WebView 就绪后增量」分支）+ K-11 + §3.2 补充 6 | G4；A-PM-08 | 是 |
| H8 | 空闲 CPU ≤1%（RM-A，聚焦、无输出、60s 均值） | kernel/06 | §5 A-PM-14（判据）；§3.8「电源 / 能效」行（不可进 CI 的替代护栏与降级） | G4；A-PM-14 | 是 |
| H9 | 未聚焦 / 被遮挡时的出帧数 = 0 帧/10s | kernel/06 | §5 A-PM-14（判据）；§3.8「电源 / 能效」行 | G4；A-PM-14 | 是 |
| H10 | 24h RSS 斜率 <1MB/h | kernel/06 | §3.2（第 6 行：Theil–Sen + 1h 步进）+ K-12；不可进 CI 的替代护栏 §3.8 | G4 / L8；A-PM-09 | 是 |
| H11 | 插件宿主空载 ≤80MB（仅启用插件时计入） | kernel/06 | §3.2（第 7 行「插件宿主空载」分支）+ K-11 + §3.2 补充 6 | G4；A-PM-08 | 是 |
| H12 | 输入字节等价（键盘 / 粘贴 / IME commit 全语料回放，逐字节比对）= 100% | **语料 kernel/05；方法 kernel/06** | 方法：§3.6（D0 字节相等自检：同一场景连跑两次逐帧指纹必须完全相等，不等即 INVALID）+ §3.2 补充 5（「冻结 + 自检先于比对」同一契约）；语料：kernel/05 §5 IN-AC | G1 / G2 | 方法在本文件（语料归 05） |
| H13 | 安装包 <60MB | kernel/06 | §3.2（第 8 行：压缩 / 公证后产物字节）；artifact 口径见 §8 OQ-PM-01（已决：AR-31 第 7 条） | 构建 job；A-PM-10 | 是 |
| H14 | AI 网关附加延迟 p95 <120ms / p99 <300ms | 服务端 SLO（T6） | 本文件只登记为「不能进 CI」项与替代护栏：§3.8 | 服务端埋点 + 滚动 7 天看板；非机器门禁 | **否**（服务端） |
| H15 | 诊断首 token P95 <3s | 服务端埋点（T6） | 同 H14：§3.8 | 在线埋点；非机器门禁 | **否**（服务端） |
| H16 | 控制面单 MAU 成本 ≤$0.05/月 | 成本看板（T6/T5） | 同 H14：§3.8；与 A-PM-12 的 CI 成本口径分离 | 月度成本看板；非机器门禁 | **否**（成本） |
| H17 | 视觉回归 diff ≤0.1%（非白名单区域零变化） | tools/design-gates | 方法学在 spec 07 §3.4.4（动画冻结 + 双渲自检 + 基线比对 + 差异 bbox）；本文件只沿用共用契约 §3.2 补充 5。**不属 kernel/06 管辖** | G3（spec 07 §3.4.1）、L7、RP-06、`design:check` | **否** |
| H18 | 硬编码色值 = 0 | tools/tokens | `tokens:check` [5] + 设计静态 **S8**（默认阻断）；本文件不承载测量。**不属 kernel/06 管辖** | G7（S8）、DC-09 | **否** |
| H19 | 网格对齐误差 ≤0.5px | tools/design-gates（判据定义 kernel/03 RP-05） | 本文件登记 RM-C golden 像素测量契约：判据 = `max abs(glyph_bitmap_origin − cell_box_origin) ≤0.5px`（100/125/150/200% DPI，AR-24 第 3 条 / kernel/03 OQ-RND-04），见 §3.2 补充 5、§3.8。**门禁判定不属 kernel/06 管辖** | G3 / G7；RP-05 | **否**（仅测量契约同源） |
| C1 | **〔非 §5 表行〕**双主题对比度：正文 ≥4.5:1；官方主题目标 ≥7:1 | tools/tokens | `tokens:check` [3]（WCAG 2.x 实算 × 深 / 浅两主题）；DC-13、AR-23 §7（深 / 浅双主题均进门禁）。**不属 kernel/06 管辖** | `tokens:check`、DC-13；G7 辅助 | **否** |

**漏项检查（本文件自证）**：HARNESS §5 表当前 **19 行**（冷启动到可输入 … 网格对齐误差）逐行登记为 H1…H19，**无缺号、无重复、无漏项**；C1 为表外对照，不计入 19 行。

## 4. 接口与依赖
```rust
// crates/termai-bench/src/lib.rs —— 仪器库（test-only，叶子，禁止被任何出货 crate 依赖）
pub type MetricId = &'static str;   // "startup.to_input" / "latency.key_to_photon.p99" / ...
pub enum Unit { Ms, MiB, MiBps, Ratio, Bytes, Px }
pub enum FailKind { RegressionPr, RegressionRel, BackendDegraded, CorpusDrift, SchemaViolation }
pub enum Verdict {
    Pass { margin: f64 }, Fail { delta_pct: f64, kind: FailKind },
    Inconclusive(InconclusiveReason), Invalid(InvalidReason),
    Skip(SkipReason), NonGating(NonGatingReason),
}
pub trait Measurement: Send {
    type Sample: Send;
    fn id(&self) -> MetricId;
    fn unit(&self) -> Unit;
    fn is_tail(&self) -> bool;                       // P99 -> 需要 >= 1e5 样本
    fn min_runs(&self) -> usize { 10 }               // spec 07 §3.8.3-1
    fn preconditions(&self) -> Preconditions;        // fingerprint / backend / corpus hash
    fn warmup(&mut self, n: u32);                    // 丢弃前 2 次
    fn run_once(&mut self, sink: &mut SampleSink<Self::Sample>) -> Result<RunOutcome, BenchError>;
    fn reduce(&self, s: &[Self::Sample]) -> RunStat;  // -> p50 / p95 / p99 / median / mad / max / n
}
pub trait DeterminismCheck {
    type Fingerprint: Eq + std::hash::Hash;
    fn scene_fingerprint(&mut self) -> Result<Self::Fingerprint, BenchError>;    // D0
    fn assert_frozen(&mut self, runs: u32) -> Result<FreezeReport, BenchError>;  // 两次不等即 Err
    fn eps_self(&self) -> f64;                                                   // D1 容差
}
pub fn gate(observed: &RunStat, baseline: &Baseline, policy: &GatePolicy) -> Verdict;
pub fn regression(now: &RunStat, baseline: &Baseline, ctx: RegressionCtx) -> Verdict;
pub fn write_report(r: &BenchReport, flat_projection: bool) -> Result<(), BenchError>;
#[cfg(feature = "bench")]
pub trait Clock: Send + Sync { fn now(&self) -> std::time::Instant; }  // 冻结时钟，仅 bench 编译
```
```text
cargo xtask bench --profile release --metric <all|startup|latency|frame|throughput|rss|soak|plugin-host|package>
                 [--fingerprint <id>] [--backend t0] [--repeat N] [--selfcheck double]
                 [--corpus <manifest>] [--out bench-report.json] [--baseline <path>]
                 [--rc-full] [--non-gating] [--selftest]
退出码：0 PASS / 1 FAIL / 2 INVALID / 3 INCONCLUSIVE（不阻断）/ 4 SKIP
```
依赖方向与边界：termai-bench 放在 crates/（spec 07 §3.1 的 crate 清单已含 termai-bench），只依赖 termai-{vt,render,ipc,session} 的**只读测试接口**；core ← session ← {agent, plugin-host} 方向不变，禁止出货 crate 反向依赖 bench（**DC-21**，CI depcheck 强制）。bench 不持有网络句柄、不写 Session Log、不读用户目录（**AR-03 / AR-12**）。bench 依赖的许可准入受 **ADR-0015** 链接边界判定（P1 同进程即入界；criterion 等仅 test 用途仍按 ADR-0015 判定表走）。

## 5. 可验证验收（测量方法 / 语料或工具 / 判据 / CI 落点）
| # | 验收项 | 判据 | CI 落点 |
| --- | --- | --- | --- |
| **A-PM-01**【新增】 | 测量自证 | 同 commit 同 fingerprint 跑 G4 全集两次：verdict 零翻转且 abs(Δmedian) ≤ eps_self；产出 bench-repro-report.json | perf-nightly 强制步骤；P0 退出条件 |
| **A-PM-02** | 场景冻结 | 场景逐帧 grid_fingerprint 两次完全相等；不等即 FAIL 且不进比对 | perf-nightly（D0） |
| **A-PM-03** | 自检非恒绿 | xtask bench --selftest 12 条注入 100% 被捕获，否则 selftest 自身退出 1 | perf-pr + perf-nightly |
| **A-PM-04** | 冷启动 | startup.to_input P95 ≤150ms（目标 ≤100ms）；Run-P95 MAD/中位数 ≤2% | perf-pr（30 次）/ perf-nightly（≥100×10） |
| **A-PM-05** | key-to-photon | T_probe.p99 + D ≤16ms（目标 P99 ≤8ms / P50 ≤4ms）；Run 内 ≥10⁵ 样本；相机 abs(bias) ≤4ms | perf-nightly（探针）/ 人工（标定） |
| **A-PM-06** | 4K@120Hz 帧时 | P99 <8.3ms、丢帧 <0.1%，于 RM-C 锁 120Hz、T0、光标冻结场景 | perf-nightly |
| **A-PM-07** | 吞吐 | **headless（sessiond-only）≥500MB/s**；加权调和平均；语料 9 片 SHA-256 全对；UI 侧最终 grid 哈希一致 + 丢帧 <0.1%（AR-24 第 3 条） | perf-pr（smoke）/ perf-nightly（1GB×10） |
| **A-PM-08** | 空闲 RSS | rss.core（**核心进程组 = sessiond + 原生 UI**）≤120MB；rss.webview 增量 ≤60MB、rss.plugin_host 空载 ≤80MB 单列独立记账，且必须报告 rss.total（AR-24 第 1 条） | perf-nightly |
| **A-PM-09** | 24h 斜率 | Theil–Sen <1MB/h 且无 ≥1MB/h 步进 | perf-soak（周度 + 发版前） |
| **A-PM-10** | 安装包 | 三平台主格式均 <60MB；artifact 口径见 OQ-PM-01（已决：AR-31 第 7 条） | 构建 job（与机器无关） |
| **A-PM-11** | 报告契约 | bench-report.json 通过 schema 校验；每 metric 含 9 个 legacy 字段；单 metric 有扁平投影 | perf-pr |
| **A-PM-12** | 成本 | ci-cost.json 齐全；月度 ≤3000 美元（自托管 ≤1800）；80% 告警、100% 降采样 | 月度看板 |
| **A-PM-13** | 机器绑定 | 任一门禁结论可回读到 fingerprint id 与 backend=T0；非 T0 结论标 NON_GATING | perf-* 全流水线 |
| **A-PM-14**（AR-24 第 2 条） | 空闲 CPU / 零出帧 | 空闲 CPU ≤1%（聚焦、无输出、60s 均值，单核归一）；未聚焦或被遮挡时 present 增量 = 0 帧/10s | perf-nightly |

## 6. 风险与降级
| 风险 | 触发 | 降级 / 缓解 | 残余代价 |
| --- | --- | --- | --- |
| RM-A / RM-C 不可用 | 硬件故障、排队 | 门禁项标 INCONCLUSIVE（**ADR-0014 铁律 5**）；**不得**用云 runner 顶替；发版前必须有全量 Run | 参考机停摆即发版停摆；PR 侧见 §8 OQ-PM-04（已决：AR-31 第 8 条） |
| 参考机漂移 | 换件、重装、云 runner 混入 | 指纹强制 + 基线重设流程 + docs/audit/ 公示（spec 07 Q12） | 每次换件需冷基线窗口（数小时） |
| 噪声使门禁反复 INCONCLUSIVE | MAD/中位数 >2% | 先修测量环境（独占、电源、时钟）；修不好则该项不得判 PASS | 门禁停摆，但不会假绿 |
| 测量自身失稳（本轮事故类） | 场景未冻结、D1 超差 | D0/D1 双自检 + --selftest；失败即 INVALID 且打印差异定位 | 每次门禁 +8%~15% 时长 |
| 相机标定缺失 | 无相机 / 未标定 | 走 K-14 保守折算 T_probe + D_max；D_max 不可核对则 INVALID | 结论偏保守，可能假红 |
| soak 占用 RM-A | 24h 独占 | 排周末窗口或专用 soak rig（§8 OQ-PM-06；已采纳默认值：AR-31） | nightly 与 soak 抢机器 |
| 语料漂移 / 生成器 bug | SHA 不匹配 | 运行前校验；漂移即 INVALID；期望哈希入库 | 需重新生成（3–6 min） |
| 门禁被橡皮章化 | 豁免泛滥 | 豁免需 TSC + 到期日；季度统计豁免率（spec 07 Q2） | 治理成本 |

## 7. 被否决的方案与反方意见
| 被否决方案 | 反方最强理由 | 我方反驳 / 决议 | 复议条件 |
| --- | --- | --- | --- |
| A 用云 runner 作为唯一基准 | 供给稳定、零硬件采购、弹性 | 硬件型号与邻居噪声不可控，MAD/中位数 >2% 常态化（ADR-0014 方案 B） | 云厂商能提供锁定 SKU + 独占 + 指纹可读时重议 |
| B 每次门禁都用高速相机 | 眼见为实，无探针偏置 | 不可自动化、相机 + 布光 + 人工 ≥2h/次，无法进 CI | 出现自动化光学采集方案（固定治具 + 自动分析） |
| C 只用 in-band 探针，不设相机 | 省成本、100% 自动化 | 探针偏置无独立验证，等于自证；§5「测量」列明确要求相机回归 | 若内探针能与 OS/固件的 HID 时间戳直接对齐（高精度输入时间戳 API 普及） |
| D 只测解析吞吐（不算渲染） | 纯 CPU、噪声小、易定位 | §5 原文是「解析 **+** 渲染」；只测解析会让 10× 渲染退化通过 | 无 |
| E 只测 RSS 峰值 | 简单、无需长跑 | 峰值不含泄漏趋势；§5 明确要求斜率 | 无 |
| F 把 1GB 语料提交进 Git（LFS） | 开箱即用、无需生成 | 克隆体积、LFS 网络依赖进 CI、种子许可边界、压缩不确定性（K-08） | 若生成耗时 >10 min 或生成器成为缺陷源，改「预生成 artifact + 校验和下载」 |
| G 单次运行直接比阈值（无 N、无 MAD） | 反馈快、实现简单 | 噪声导致随机红/绿，信任崩塌（§9 R7） | 无 |
| H 用 CPU 帧耗时当帧时 | 实现简单、跨后端一致 | 系统性低估，P99 与丢帧不可见 | 无 |
| I PR 跑完整 24h soak | 覆盖最全 | 墙钟与机时不可承受；spec 07 L8 已定为周度 | 无 |
| J 用屏幕截取 API 代替 present 时间戳 | 不依赖后端 API | 截取本身引入延迟，且看不到 scanout 真相 | 仅作交叉核对，不作门禁 |
| K G4-PR 采用「连续 2 夜」口径 | 假阳性更低 | HARNESS §8.1-4 是合并阻断硬口径；缓冲会放行退化 | 见 §8 OQ-PM-03（季度误阻断率 >10% 时提案，需 ADR） |

## 8. Open Questions
> 全部为 **【新增】** 可测/口径问题；未获裁决前不得作为对外承诺。变更任何既有门禁数值需 ADR + TSC（AGENTS §5）。

| # | 问题 | 建议 | 必须决策的阶段 |
| --- | --- | --- | --- |
| **OQ-PM-01** | 安装包 <60MB 的 **artifact 口径**：macOS universal2 DMG 因双架构显著偏大，是否按「主格式」计？（**已决：AR-31 第 7 条**） | **已决（AR-31 第 7 条）**：按平台主格式计（Windows MSI / macOS DMG universal2 / Linux AppImage），任一超限即 FAIL；macOS 若因 universal2 超限，改用 thin slice + 双下载 | **已决（AR-31）** |
| **OQ-PM-02** | 无相机时 D_max（键盘轮询 + OS 栈 + 面板 GtG 上界）能否作为门禁折算依据？（**已采纳默认值（AR-31 采纳分诊建议）**） | **已采纳默认值**：可以，但 D_max 每项必须可核对，不可核对即 INVALID；先按保守口径跑 2 个发版周期再评估（返工范围：06 §3.3 折算表 + A-PM-05 判据） | P0 |
| **OQ-PM-03**（**已决：AR-27**） | **口径差异**：HARNESS §8.1-4「回归 >5% 阻断合并」为单次即阻断，spec 07 §3.8.3-3 要求「连续 2 个夜间运行复现」 | **已决：分列为 G4-PR（单次 >5% 即阻断合并）与 G4-REL（连续 2 夜复现，或发版前全套一次即阻断发版）**；两者均以「测量先过可复现性自检（verdict 不翻转）」为前置；登记「PR 假阳性阻断」季度统计口径（spec 07 Q2） | **已决（AR-27）** |
| **OQ-PM-04** | 参考机不可用时的 PR 合并政策：无热路径改动的 PR 是否可 SKIP(no_hotpath)？（**已决：AR-31 第 8 条**） | **已决（AR-31 第 8 条）**：按路径范围判定——**不触及 `term-render / term-gpu / term-vt / term-session / term-pty` 的 PR** 可标 `SKIP(no_hotpath)`；**触及热路径的 PR 一律不得 SKIP**（与 ADR-0014「不得用云 runner 顶替门禁」一致） | **已决（AR-31）** |
| **OQ-PM-05**（**已决：AR-24 第 1 条**） | 空闲 RSS 的进程集合与独立记账 | **已决：rss.core = 核心进程组（sessiond + 原生 UI）≤120MB 门禁；rss.webview 就绪后增量 ≤60MB、rss.plugin_host 空载 ≤80MB 各自独立记账、不计入核心组；必须同时报告 rss.total**；口径必须在 UI 明示（AR-20） | **已决（AR-24）** |
| **OQ-PM-06** | 24h soak 是否独占 RM-A，还是新增一台 RM-A 等价专用 soak rig？（**已采纳默认值（AR-31 采纳分诊建议）；AR-31 D6 拆为 P0 窗口 + P1 rig 两部分**） | **已采纳默认值**——**P0 窗口（已采纳）**：使用周末独占 RM-A 窗口；nightly 与 soak 不得同 tick 争抢（单个 CI job）。**P1 rig（已采纳）**：P1 起评估 RM-A 等价专用 soak rig（避免 nightly 停摆）；rig 未决前沿用周末独占窗口。返工范围：06 §6 + CI 排程（单个 CI job） | **P0 定窗口（已采纳默认值）/ P1 定 rig（已采纳默认值）** |
| **OQ-PM-07** | MAD/中位数 ≤2%（spec 07 §3.8.3-2）对 P99 类指标是否足够？帧时与延迟的 run-to-run 离散显著大于吞吐（**已决：AR-31 第 9 条**） | **已决（AR-31 第 9 条）**：按指标族分级——**帧时 5% / 启动 3% / 吞吐 2% / RSS 1%**；分级只改统计窗口，不改门禁数值语义；**放宽任何值需 ADR + TSC** | **已决（AR-31）** |
| **OQ-PM-08** | 1GB 吞吐语料的分片权重（§3.7）是否需 ADR 固定？权重直接决定门禁数值的可解释性与跨版本可比性（**已决：AR-31 第 10 条**） | **已决（AR-31 第 10 条）**：分片权重**入库即冻结**；权重变更需 ADR（变更等于改门禁含义） | **已决（AR-31）** |
| **OQ-PM-09** | 冷启动门禁改判 COLD-DISK（需特权、跨 OS 不可比）还是保持 WARM？ | 保持 WARM 门禁 + COLD-DISK【新增】非门禁观测；若用户投诉首启再提案 | P1 |
| **OQ-PM-10** | 光标闪烁场景的帧时是否升为**门禁项**？本轮事故根因即动画未冻结，当前只作非门禁观测 | P0 保持非门禁观测 + D0 强制冻结；若真机出现闪烁掉帧投诉再升门禁（需 ADR） | P2 |
| **OQ-PM-11** | machine-fingerprint 的扩展字段（EDID、字体集哈希、时钟源、输入设备报告率）是否强制走 ADR-0014 公示流程（**已采纳默认值（AR-31 采纳分诊建议）**） | **已采纳默认值**：纳入 EDID、字体集哈希、时钟源、输入设备报告率；任一变更即触发基线重设（返工范围：06 §3.4 schema + 基线重设流程 + `bench-report` schema 1 minor） | P0 |
| **OQ-PM-12** | 参考机替换的「纯 CPU 固定微基准」具体项目（用于解耦 CPU/GPU 替换）是否需固定为门禁的一部分？（**降为 P1：AR-31 D3/D4**） | **降为 P1**：先作替换判定的证据项，不入 §5；占位 = 复用 A-PM-07 吞吐与 A-PM-09 长跑的 CPU 侧归因；稳定后再考虑升为门禁（需 ADR）。**复评触发**：首次换件 / 需要解耦 CPU 与 GPU 时 | **P1（AR-31 降级）** |
