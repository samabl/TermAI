# ADR-0014：平台支持矩阵、参考机 SKU、GPU 降级与 nightly / CI 成本口径

- 状态：Accepted
- 日期：2025-01-01
- 决策者：发布工程负责人 + QA 架构师 + 首席性能工程师（联合终裁），项目发起人授权（HARNESS §11 OQ-26 / OQ-28 工程部分）
- 关联：AR-01、AR-02、AR-14、AR-19、AR-21；DC-16、DC-17、DC-37、DC-40；HARNESS §5、§7（P0/P2）、§8.1、§8.2、§11（OQ-26、OQ-28）；ADR-0012（门禁 / 目标两列口径）、ADR-0013（全栈开源，Apache-2.0 OR MIT）
- 实现位置：~~crates/termai-xtask（bench / perf-gate / matrix / dist / sign）~~ **被 ADR-0028 取代**（该 crate 不存在；§5 测量与门禁的最终主场为 `tools/bench/`，见 ADR-0028）、tests/matrix/、tests/conformance/、crates/termai-render 与 crates/termai-gpu（后端选择与软件光栅）、crates/termai-pty、docs/spec/07-engineering-quality-and-release.md、CI 定义与 machine-fingerprint.json / nightly-exception.json / ci-cost.json

## 背景与问题

HARNESS §5 只规定「release 构建 + SSD」，未固定硬件；§8.2 只写「4 shell × 3 字体后端全绿」，未列具体项；OQ-26 / OQ-28 把参考机 SKU、矩阵枚举、arm64 覆盖、GPU 降级、nightly 中断口径与 CI/签名成本留成空白。空白对应的具体失败模式：

1. **基准不可比**：开发者笔记本 / 云 runner / 自托管机混跑时，同一 commit 的冷启动差异可 >2×，门禁数字失去意义（HARNESS §9 R7、R12）。
2. **矩阵不可执行**：不知道装哪个 shell、哪个字体后端、在哪台机器跑，「12 组合全绿」无法落成 CI job，也无法估成本（§9 R10）。
3. **降级无契约**：GPU 能力探测失败、驱动 TDR、WebView / GPU 后端不可用时行为未定义，用户看到黑屏、崩溃或静默掉帧（AR-20「能力边界必须明示」）。
4. **nightly 口径漏洞**：签名 / 公证 / 上游 registry 中断若直接判失败，A9「连续 30 天」不可能达成；若一律豁免，则被假绿灯掩盖真实缺陷（§8.2）。
5. **成本无上限**：36 组合矩阵 + 24h fuzz × 4 target 的云机时可轻易失控（OQ-E10）。

不可违反的约束（本 ADR 不修改其中任何一条）：AR-01（网格绝不经 WebView）、AR-02（WebView 为可选依赖，缺失降级纯原生面板）、AR-14（不承诺 GPU subpixel AA）、AR-19（门禁 / 目标两列不得合并）、AR-21（全栈开源，Apache-2.0 OR MIT）、DC-16（Windows 唯一生产路径 ConPTY，Win10 1809 为地板）、DC-17（wgpu + rustybuzz/swash）、DC-37（持续 fuzz）。

## 可选方案

| 方案 | 描述 | 结论 |
| --- | --- | --- |
| A 单一物理参考机 | 沿用 §3.8.1「一台 8 核 4K@120Hz 机器跑全部指标」 | 否决：无法覆盖 macOS 与低端地板；换机即基线断裂，且无「显示类」指标专用机 |
| B 全云 runner 基准 | 以 CI 云 runner 作为唯一测量环境 | 否决：硬件型号与邻居噪声不可控，MAD/中位数 >2% 常态化，门禁无法判定 |
| C 三档自托管参考机（RM-A / RM-B / RM-C）+ 门禁项一一映射 | 门禁机、地板机、显示机分工，全部指纹化登记 | **采纳** |
| D 平台矩阵「按需支持」 | 只支持最新 OS + x64 | 否决：与 DC-16 的 Win10 1809 地板、P0「三平台 IME 矩阵」退出条件冲突 |
| E arm64 全平台 v1 门禁 | Windows / Linux / macOS arm64 同时进 v1 门禁 | 否决：Windows arm64 的 ConPTY 与 WebView2 arm64 生态未成熟，P0 资源不足以承担 |
| F 把「4 shell × 3 字体后端」缩为 4 个主场组合 | 每个 shell 只测其主场 OS | 否决：与 HARNESS §8.2 的 12 组合全绿冲突；改为 12 组合全部保留、以统一最低测试集控成本（决策 3） |
| G nightly 一律豁免外部中断 | 外部服务失败全部不计 | 否决：假绿灯风险；改为 EXTERNAL 分类 + 证据 + 连续升级条件（决策 5） |
| H 不设 CI 成本上限 | 矩阵与 fuzz 自由增长 | 否决：OQ-E10 明确要求上限；改为量级估算 + 月度封顶 + 自动降采样（决策 6） |

## 决策

### 决策 1：参考机三档 SKU 与门禁映射

**唯一结论：采用 RM-A / RM-B / RM-C 三档自托管物理参考机，§5 每一项门禁数字只绑定其中一档的一级 GPU 后端。云 runner 结果一律 NON-GATING。**

**RM-A「门禁基准机」——全部计算类门禁的唯一判定机**
- CPU：AMD Ryzen 7 7700X（8C/16T，4.5GHz 基频）或等效（Intel Core i7-14700；判据：Cinebench R23 单核 ≥1900 且多核 ≥14000，物理核 ≥8）
- 内存：32GB（2×16GB）DDR5-5200 CL40 双通道
- 存储：1TB PCIe 4.0 NVMe（顺序读 ≥6.5GB/s）
- GPU：NVIDIA GeForce RTX 4060 8GB（Vulkan 1.3 + DX12 FL12_1），驱动版本锁定并记入指纹
- 显示：2560×1440@165Hz（DisplayPort，VRR 关闭）
- OS 三驻留：Windows 11 24H2 x64 / Ubuntu 24.04.1 LTS x64（GNOME + Wayland）/ macOS 14.5（Mac mini M2 Pro，10 核 CPU / 16 核 GPU，32GB，512GB SSD）
- 用途：冷启动、吞吐、空闲 RSS、24h RSS 斜率、插件宿主空载

**RM-B「主流开发机 / 支持地板」——声明最低支持规格，并承载 L6 矩阵与地板阈值**
- CPU：≥4 物理核 / 8 线程（Intel Core i5-1235U、AMD Ryzen 5 7530U 或等效；Cinebench R23 单核 ≥1200）；macOS 侧为 MacBook Air M1 8GB
- 内存：16GB
- 存储：NVMe SSD（顺序读 ≥2GB/s）
- GPU：集成显卡（Intel Iris Xe / AMD Radeon 680M / Apple M1 8 核 GPU）
- 显示：1920×1080@60Hz，DPI 缩放 100% / 125% / 150% 三档
- OS：Windows 11 24H2 x64（Win10 1809 仅为兼容地板，只跑 L6 子集）/ Ubuntu 24.04 LTS x64 / macOS 13.6
- 地板阈值 F1–F6（**非** §5 门禁，是「最低支持规格」承诺）：F1 冷启动 ≤300ms；F2 key-to-photon P99 ≤33ms；F3 1080p60 帧时 ≤16.7ms；F4 吞吐 ≥250MB/s；F5 空闲 RSS ≤160MB；F6 安装包 <60MB（与 §5 一致）

**RM-C「高刷 / 4K 机」——显示与延迟类门禁的唯一判定机**
- CPU / 内存 / 存储：同 RM-A
- GPU：NVIDIA GeForce RTX 4070 12GB（或 AMD Radeon RX 7800 XT）；macOS 侧为 Mac Studio M2 Max
- 显示：主 3840×2160@144Hz（DP 1.4 DSC，门禁运行时锁定 120Hz）+ 副 2560×1440@240Hz
- 用途：4K@120Hz 帧时、key-to-photon P99、网格对齐、视觉回归 golden、高 DPI
- 后端：一级后端（决策 4 的 T0）开启；VRR 关闭、HDR 关闭（HDR 仅作观感验证，不作门禁）

**门禁项 → 机器映射（唯一）**

| HARNESS §5 门禁项 | 测量机 | GPU 后端 | 备注 |
| --- | --- | --- | --- |
| 冷启动到可输入 P95 ≤150ms | RM-A | T0 | ≥100 次采样 |
| key-to-photon P99 ≤16ms | RM-C | T0 | 高速相机 ≥1000fps 校验探针 |
| 4K@120Hz 网格帧时 <8.3ms | RM-C（4K 锁 120Hz） | T0 | 全 damage 合成基准 |
| 解析 + 渲染吞吐 ≥500MB/s | RM-A | T0 | 1GB 语料 |
| 空闲 RSS（1 万行）≤120MB | RM-A | T0 | 稳定 60s 后采样 |
| 24h RSS 斜率 <1MB/h | RM-A | T0 | 长跑独占 |
| 插件宿主空载 ≤80MB | RM-A | — | 独立进程采样 |
| 安装包 <60MB | 构建产物 | — | 与机器无关 |
| AI 网关附加延迟 / 诊断首 token | 服务端 SLO 埋点 | — | 与参考机无关 |
| 控制面单 MAU 成本 | 成本看板 | — | 与参考机无关 |
| 视觉回归 diff ≤0.1% | RM-C | T0 | 参考光栅器 + 原生 golden |
| 网格对齐误差 ≤0.5px | RM-C（100%/125%/150%/200% DPI） | T0 | 渲染用例 |
| 硬编码色值 = 0 | 静态检查 | — | 与机器无关 |

**铁律**
1. §5 门禁数值只在 RM-A / RM-C 的 **T0 后端**上判定。降级到 T1/T2/T3 时运行仍可用，但**门禁判定为失败**（性能门禁必须在 T0 达标）。
2. 每台参考机登记 machine-fingerprint.json：CPU + 微码、内存、GPU + 驱动版本、显示器与刷新率、OS build、电源计划。任一字段变化 → **重置基线并在 docs/audit/ 公示**。
3. 参考机为自托管物理机；测量期禁用自动更新 / 屏保 / 索引 / 休眠；Windows 固定「高性能」电源计划、Linux 用 performance governor、macOS 禁止 App Nap；预热 2 次丢弃。
4. 等价替换规则：CPU 单核与多核相对基准机劣化 ≤5%；GPU 同 API 等级且驱动版本锁定；显示器像素与刷新率不得低于门禁项要求。替换须重设基线并公示。
5. 云 runner 仅可用于 S0–S3 非性能项与 PR 冒烟，输出标记 NON-GATING；参考机不可用时门禁项标 INCONCLUSIVE，**不得**用云 runner 顶替。

### 决策 2：平台支持矩阵

**唯一结论：v1 门禁架构 = {Windows x64、Linux x64、macOS arm64}；Windows arm64 不进 v1，进入 P2。**

| OS | 版本地板 | 架构 | v1 状态 | GPU 一级后端 | 主包格式 | 次包格式 |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Windows 10 1809（17763）+ / 11 | x64 | **一等公民（门禁）** | DX12（FL12_1） | MSI（per-user，Authenticode） | MSIX（企业 / Store 可选）、portable ZIP、winget manifest |
| Windows | 同上（参考机 = 11 24H2） | arm64 | **非 v1；P2 进入** | DX12 | （P2：MSI arm64） | （P2：portable ZIP） |
| macOS | 13.6 Ventura+ | arm64 | **一等公民（门禁）** | Metal 3 | notarized DMG（universal2） | .app ZIP、Homebrew cask |
| macOS | 13.6 Ventura+ | x64 (Intel) | **支持但无性能门禁** | Metal 3 | notarized DMG（universal2） | .app ZIP |
| Linux | glibc ≥2.35（Ubuntu 22.04 LTS）/ Fedora 38+ | x64 | **一等公民（门禁）** | Vulkan 1.3 | AppImage | .deb（Ubuntu 22.04/24.04）、.rpm（Fedora 38+/RHEL9）、Flatpak（Flathub）、tar.gz |
| Linux | 同上（参考机 = Ubuntu 24.04.1 LTS） | arm64 | **支持（headless 一等 / 桌面 Beta），无性能门禁** | Vulkan 1.3（仅） | tar.gz（headless）= 一等；.deb | AppImage（Beta） |
| 任意（headless） | Windows 10 1809+ / macOS 13.6+ / glibc 2.35+ | x64 + arm64 | 一等公民 | 无（无 GPU 呈现） | 静态 tar.gz / zip | — |

补充规则：
1. **WebView**：Windows WebView2 evergreen（企业可 fixed）、macOS WKWebView 系统自带、Linux 最低 WebKitGTK 2.40+。WebView 缺失或不可信 → 降级为纯原生文本面板（AR-02、OQ-02），**不阻塞核心可用性**。
2. **Linux 桌面环境**：GNOME 42+ 与 KDE Plasma 5.24+ 为支持目标；Wayland 为默认，X11 为兼容路径；其余 DE 为 best-effort，不进 L6 门禁。
3. **macOS x64（Intel）**：v1 提供构建与发布、跑 L1–L3 与 L6 子集，**不做性能门禁**；P3 复核是否随 Apple 支持终止而淘汰。
4. **Linux arm64**：v1 提供 headless（一等）与桌面 Beta 产物；跑 L1–L3、L5、L6 子集。**升为门禁架构的 P2 门槛**：① Ubuntu 24.04 arm64 与 Fedora 40 arm64 上 L6 子集全绿；② 具备一台 arm64 RM-C 等效机（Snapdragon X Elite / Ampere Altra）；③ 一级后端 Vulkan 在两大发行版均达 §5 门禁。三者全满足才升格。
5. **Windows arm64**：v1 不发布。**进入 P2 的门槛**：① ConPTY arm64 可用且 Job Object 进程树语义与 x64 一致；② WebView2 arm64 evergreen 可装载；③ 有一台 Snapdragon X Elite 参考机并入 RM-A / RM-C；④ L6 子集全绿。进入后标注 Beta 一个 minor，且**不提供 x64 模拟路径**（只出 native arm64 产物）。
6. 支持声明与实测必须一致：任何未列入本表的 OS / 架构组合，安装器与文档一律标注「不支持」，不提供「也许能跑」措辞（AR-20 诚实原则）。

### 决策 3：「4 shell × 3 字体后端」完整枚举

**唯一结论：12 个组合全部保留为 v1 必修（每个组合跑统一的最低测试集 M12 全绿）；4 个主场组合额外跑扩展套件 E60。**

**字体后端的定义（三者都是「系统字体发现 + 回退 + 度量」提供者；栅格化统一由 swash 生成 GPU atlas，DC-17 与 AR-14 不变）**

| 字体后端 | 定义 | 适用 OS | 关键依赖 |
| --- | --- | --- | --- |
| **DirectWrite** | 经 windows-rs 用 IDWriteFontCollection / IDWriteFontFallback 做字体枚举、CJK / emoji cascade 回退与字形度量；不使用 GDI / ClearType 位图（不承诺 subpixel AA） | Windows | DirectWrite（系统组件） |
| **CoreText** | 用 CTFontCreateForString cascade list + CTFontDescriptor 枚举与变体解析，取得度量；不启用系统 subpixel AA | macOS | CoreText（系统框架） |
| **FreeType / fontconfig** | fontconfig 负责匹配与回退顺序，FreeType 只取度量与 outline（FT_Load_Glyph），栅格化仍由 swash 完成 | Linux / BSD | fontconfig ≥2.13、FreeType ≥2.11、Noto Color Emoji（CBDT/COLRv1） |

三者为**互斥的 OS 原生后端**，不支持跨 OS 混用（Windows 无 fontconfig、Linux 无 DirectWrite）。

**shell 与其最低版本**

| shell | 最低版本 | Windows 运行时 | macOS 运行时 | Linux 运行时 |
| --- | --- | --- | --- | --- |
| **bash** | macOS 系统 3.2 / Linux ≥5.1（集成脚本必须 3.2 安全，禁用 4.x-only 语法） | MSYS2 / Git for Windows 原生 x64 bash ≥5.2 | /bin/bash 3.2 + brew bash ≥5.2 | 发行版自带 ≥5.1 |
| **zsh** | ≥5.8 | MSYS2 原生 x64 zsh ≥5.9 | 系统 zsh ≥5.9 | 发行版 / 包管理器 ≥5.8 |
| **fish** | ≥3.3 | MSYS2 原生 x64 fish ≥3.7 | brew fish ≥3.7 | 发行版 ≥3.3 |
| **pwsh** | ≥7.2 LTS（推荐 7.4） | 官方 MSI 7.2+ | 官方 pkg 7.2+ | 官方包 7.2+ |

说明：Windows 侧 bash / zsh / fish 一律走 **MSYS2 / Git for Windows 原生 x64 构建 + ConPTY 启动**；**WSL 是 Transport，不是 shell 矩阵的一环**。Windows PowerShell 5.1 只作为「能被打开的普通终端」，**不列入 shell 集成矩阵**（PSReadLine 2.2+ 的 OSC 633 门禁只对 pwsh ≥7.2 生效）。

**12 组合矩阵与最低测试集**

| # | 组合 | OS / 后端 | 最低集 | 扩展集 | v1 |
| --- | --- | --- | --- | --- | --- |
| 1 | bash × FreeType | Linux x64 | M12 | E60 | 必修（主场） |
| 2 | bash × CoreText | macOS arm64 | M12 | — | 必修 |
| 3 | bash × DirectWrite | Windows x64（MSYS2 bash） | M12 | — | 必修 |
| 4 | zsh × FreeType | Linux x64 | M12 | — | 必修 |
| 5 | zsh × CoreText | macOS arm64 | M12 | E60 | 必修（主场） |
| 6 | zsh × DirectWrite | Windows x64（MSYS2 zsh） | M12 | — | 必修 |
| 7 | fish × FreeType | Linux x64 | M12 | E60 | 必修（主场） |
| 8 | fish × CoreText | macOS arm64 | M12 | — | 必修 |
| 9 | fish × DirectWrite | Windows x64（MSYS2 fish） | M12 | — | 必修 |
| 10 | pwsh × FreeType | Linux x64 | M12 | — | 必修 |
| 11 | pwsh × CoreText | macOS arm64 | M12 | — | 必修 |
| 12 | pwsh × DirectWrite | Windows x64（官方 MSI） | M12 | E60 | 必修（主场） |

**最低测试集 M12（12 项，覆盖 HARNESS §8.2 要求的全部类别）**
1. 冷启动到可输入（含 shell 集成脚本自动注入）
2. UTF-8 CJK 输入 + 显示（含三档 CJK 回退命中）
3. Emoji 彩色与单色 + ZWJ 序列
4. IME 组合输入与候选窗定位（TSF / NSTextInputClient / fcitx5）
5. reflow：80↔120 列切换后网格与回滚一致
6. 宽字符折行（CJK 双宽 + 组合字符 + 变体选择符）
7. 连字开关（JetBrains Mono ligature 开 / 关各一次）
8. OSC 133 / 633 命令边界与退出码解析
9. OSC 7（cwd）与 OSC 0/2（title）
10. 复制 / 粘贴语义（bracketed paste、多行、选区）
11. shell 集成失效降级（提示符启发式 + 低置信度标注）
12. 环境探测与能力查询（DA / kitty query、$TERM、terminfo）

**扩展套件 E60（≥60 用例，仅 4 个主场组合）**：Sixel / kitty graphics 限额与降级、OSC 9/777 通知、bidi（Unicode Bidi 算法）、超长行与 10k 行回翻渲染、多分屏、CJK 环境变量与路径、Kitty keyboard protocol 探测、超链接（OSC 8）、IME 候选窗高 DPI 位移、剪贴板所有权切换等。

**判定规则**：12 组合的 M12 全绿 = L6 矩阵绿；任一组合失败即 matrix 门禁失败。偏差必须进差异登记表（最小复现 + 豁免期限 + owner），未登记偏差视为门禁失败。

### 决策 4：GPU 降级路径与用户可见行为

**唯一结论：四级后端阶梯 T0→T1→T2→T3，自动降级 + 状态栏明示 + 审计留痕；门禁运行必须停在 T0。**

| 等级 | 后端 | 覆盖平台 | 能力 |
| --- | --- | --- | --- |
| **T0 一级（门禁）** | DX12（Windows）、Metal（macOS）、Vulkan 1.3（Linux） | 全部门禁架构 | 完整：图形协议、连字、VRR、≥120Hz |
| **T1 二级（降级可用）** | Windows：Vulkan（若存在）；Linux：OpenGL 4.5 / EGL；macOS：OpenGL 4.1（仅兼容，不追踪进度） | 全部 | 网格与外壳完整；关闭 VRR / 高刷优化与部分合成效果 |
| **T2 软件光栅（受限可用）** | Windows：WARP（D3D12 软件适配器）；Linux：lavapipe；macOS：term-render 自带 CPU 光栅（swash 栅格化 + CPU 合成 + blit） | 全部（macOS 无系统软件 Vulkan，必须走 CPU 路径） | 仅文本网格；**禁用** Sixel / kitty graphics / iTerm2 图形；禁用模糊与阴影；帧率上限 60Hz；damage 增量重绘 |
| **T3 安全模式（最后手段）** | 无 GPU 呈现加速 | 全部 | 不加载 L2 WebView（改纯原生文本面板，AR-02）；禁用全部图形协议与动画；最低刷新；启动时给出诊断包导出入口 |

**降级触发与行为（唯一口径）**

| 触发 | 动作 | 用户可见行为 | 是否门禁失败 |
| --- | --- | --- | --- |
| 启动能力探测缺少必需特性（Vulkan/DX12/Metal 特性、纹理格式、atlas 尺寸） | 按 T0→T1→T2→T3 顺延选择第一个可用后端 | 状态栏后端徽章 + 「图形能力受限」提示，点击可看 gpu-capability-report.json | 是（门禁须 T0） |
| DeviceLost / 驱动 TDR | 重建 device + 重建 atlas（目标 ≤2s），会话不中断（sessiond 独立进程，AR-13） | 短暂重绘，无会话丢失；连续失败则降级 | 是 |
| 同一会话 10 分钟内 3 次 DeviceLost | 降一级（T0→T1→T2→T3） | 状态栏后端变化 + 结构化事件 + 审计记录 | 是 |
| T2 软件光栅初始化或呈现失败 | 进入 T3 安全模式 | 明确告知「已进入安全模式」并给出诊断导出 | 是 |
| WebView2 / WKWebView 缺失、损坏或不可信 | **不触发网格降级**；仅把 L2 面板降为纯原生文本面板 | 面板功能子集可用，核心终端 100% 可用 | 否 |
| WebGPU / wgpu 的 WebGPU 后端不可用 | **无影响**：WebGPU 不在网格热路径（AR-01），桌面 v1 明确不以 WebGPU 作为网格后端 | 无 | 否 |

**规则**
1. 手动覆盖：配置键 gpu.backend = auto | dx12 | vulkan | metal | gl | warp | lavapipe | software | safe。手动指定的后端不可用时，回退 auto 并提示，不静默忽略。
2. 每次降级必须产出结构化事件（含探测报告、驱动版本、时间戳）并写入本地审计；遥测开启时才允许上报脱敏后的后端等级与驱动版本（AR-12）。
3. 降级能力边界必须在 UI 内明示（AR-20）；不得用模糊措辞掩盖（例如必须写「无 Sixel」，不能写「部分图形特性不可用」）。
4. T2/T3 下**不得**将失败测试标记为通过；性能与兼容门禁在非 T0 后端上一律判失败。

### 决策 5：nightly 中断口径与豁免规则

**唯一结论：中断分 FAIL-REPO 与 EXTERNAL 两类；只有后者可豁免，且必须留证据、双人签署、连续升级。**

| 分类 | 判定 | 是否计入 A9 中断 | 记录要求 |
| --- | --- | --- | --- |
| **FAIL-REPO** | main 上 S0–S4 任一阶段失败、测试失败、门禁红、构建产物损坏 | **计入** | 常规失败日志 + 归因 PR |
| **EXTERNAL** | 签名 / 公证服务不可用（Apple notary、Azure Trusted Signing、HSM / KMS 不可达）、上游 registry 不可用（crates.io / npm / OSV）、云 runner 供给中断、网络出口或机房事故 | **不计入**（但受上限约束） | nightly-exception.json：时间、阶段、外部服务、证据链接、恢复时间、签署人 |
| **EXTERNAL-PARTIAL** | 外部故障只影响签名 / 公证，构建与全部测试可完成 | 不计入；看「构建 + 测试」是否绿，签名单独记录 | 同上，并保留未签名 artifact |

规则：
1. **豁免权限**：只有 Release Owner（T5 当值）+ 1 名 maintainer **双人**可签 EXTERNAL；单人不得豁免。每次豁免必须附外部证据链接（服务状态页 / 错误码 / 时间线）。
2. **不得假绿灯**：测试阶段失败时**禁止**使用 EXTERNAL 豁免；不得在无证据时追溯豁免；不得用豁免掩盖间歇性失败（同一测试 7 天内 2 次失败即按 FAIL-REPO）。
3. **不投递半成品**：EXTERNAL 之夜可以完成构建与测试，但**不得**推进 nightly 通道指针；签名恢复后重跑签名并投递。
4. **连续升级**：同一外部服务连续 **2 夜** EXTERNAL → 开 P1 事故，24h 内给出替代通道（备用 CA / 备用签名服务 / 离线签名 runbook）。任意原因连续 **5 夜** EXTERNAL → 触发发布工程复盘，状态页公示。
5. **A9 核算口径**：滑动 30 天窗口内必须同时满足 ① FAIL-REPO = 0；② EXTERNAL ≤ 6 夜；③ 无连续 >2 夜 EXTERNAL。任一不满足则该窗口重置计数。
6. **反滥用**：季度审计统计 EXTERNAL 占比；**>20% 视为 A9 不可信**，按 §6 Q2 走 TSC 审计。

### 决策 6：CI 与签名成本口径

**唯一结论：按「矩阵规模 × 机时单价 + 固定费」量级估算，月度 CI 硬上限 $3,000；fuzz 与长跑一律自托管，云托管 fuzz 机时为 0。**

**估算公式**

    月成本 ≈ Σ_流水线 ( 单次分钟数 × 月运行次数 × 该类 runner 单价 ) + 签名/公证固定费 + 存储与出口

- 机时单价口径：自托管物理 runner ≈ $0.04–0.08 / 核时（折旧 + 电费 + 带宽，直接成本）；云托管 Linux 2 核 ≈ $0.008/min、Windows ≈ $0.016/min、macOS ≈ $0.08/min（作为「不许用云跑」的反证基准）。
- 量级估算（按 v1 规模）：
  - PR 流水线 S0–S3：中位 15min × 约 40 PR/日 × 22 日 ≈ 13,200 min/月（云 Linux 口径 ≈ $106/月，自托管更低）。
  - nightly 全量：36 组合 × 约 10min = 360min，加 L4 全量约 60min、L6 约 180min、L7 约 60min ≈ 660min/夜 × 30 ≈ 19,800 min/月。
  - fuzz：4 target × 24h = 5,760 min/夜 → 172,800 min/月。**只能在自托管跑**；按云 macOS 单价折算将 >$13,000/月，故明令禁止。
  - 周度 soak：4 台 × 24h × 4 周 ≈ 23,040 min/月（自托管）。
- 签名 / 公证固定费量级：Apple Developer Program ≈ $99/年；Windows Authenticode（Azure Trusted Signing 基础档 ≈ $10/月 或 EV 证书 ≈ $300–600/年）；HSM / 云 KMS + 2-of-3 门限签名 ≈ $1–5/月/密钥。年固定费量级 **< $5,000**。

**上限与告警**
1. **月度 CI 上限 $3,000**（不含签名固定费与人力）：云托管 ≤$1,200，自托管运维（折旧 + 电 + 带宽）≤$1,800。到 80% 告警、100% 起：自动降采样非紧急流水线（视觉全量 → 隔夜、soak 周度 → 双周、E60 扩展套件 → 仅发版前），并在状态页公示。
2. **墙钟上限**：单次 PR 中位反馈 ≤15min（A15）；单次 nightly 墙钟 ≤6h；fuzz 不占墙钟（后台长跑）。
3. 每次流水线输出 ci-cost.json（{pipeline, minutes, runner_class, est_usd}），纳入月度成本看板；超限由 T5 在 24h 内给出降本方案。
4. 预算调整需附实测数据 + TSC 批准（AGENTS.md §5）；不得通过取消门禁项来降本。

## 理由

1. **可复现性优先**：门禁只有在固定硬件 + 固定驱动 + 固定电源策略下才有跨版本可比性。RM-A/RM-C 的指纹登记把「换了机器」从隐性变量变成显式事件（正面回应 HARNESS §9 R12）。
2. **分层而非削减**：把 12 组合全保留在门禁内（满足 §8.2），用统一 M12 最低集控成本；把低端机作为「支持地板」而非门禁机，既守住 DC-16 的 Win10 地板，又不让门禁数字被弱机稀释。
3. **降级是可观测的承诺**：T0→T3 阶梯让「GPU 不可用」成为有明示、有审计、有边界的可测路径，直接兑现 AR-20 的诚实原则与 AR-02 的 WebView 可选性。
4. **nightly 门禁可达成且不可滥用**：EXTERNAL 分类解决「外部服务中断无法避免」的现实，双人签署 + 证据 + 连续升级 + ≤6 夜上限解决「假绿灯」风险。
5. **成本可控**：把 fuzz 与 soak 明确划归自托管，避免云单价把预算击穿；量级公式让矩阵扩张的边际成本在合并前就可见（OQ-E10）。
6. 全部结论落在 AR/DC 框架内，未放宽任何 §5 门禁数值与 §8 门禁项，符合 AR-19 的两列口径与 AR-21 的开源定位（无专有硬件依赖，BOM 全部可采购）。

## 后果

**正面**
- 性能门禁可复现、可审计、可采购；machine-fingerprint.json 让基线漂移可追溯。
- 「4 shell × 3 字体后端」从口号变成 12 个可执行 job，v1 验收可勾选。
- GPU 降级有唯一契约，用户不会遇到黑屏且能力边界可回读。
- nightly A9 从「看运气」变成有分类、有上限、可审计的指标。
- CI 与签名成本有量级与硬上限，矩阵扩张不再无限膨胀。

**负面 / 必须接受的代价**
- 需要采购并维护至少 3 类参考机（每类 1–3 台，含 macOS 与高刷 4K 显示），产生硬件与运维成本。
- macOS 侧无系统软件 Vulkan，必须由 term-render 自研 CPU 光栅后备（额外实现与测试成本）。
- Windows 侧 bash / zsh / fish 走 MSYS2，矩阵 CI 需要额外安装步骤与稳定的 MSYS2 版本锁定。
- 12 组合 + 36 job 的 nightly 墙钟与排队压力真实存在；已用「M12 最低集 + 4 个主场扩展」压制，但仍需 runner 供给。
- Windows arm64 与 Linux arm64 门禁推迟，意味着这两类用户 v1 只能获得 Beta/子集承诺，可能引发期待落差。

## 反方记录与复议条件

- **反方（性能工程师，支持方案 A 单一参考机）**：多档机器增加基线维护面，跨档比较更易出错。**反驳**：单一机器无法承载 macOS arm64 与低端地板，且显示类指标需要 4K@120Hz 专机；多档但指纹化比单档失配更可靠。
- **反方（平台工程师，支持 4 shell 只测主场）**：Windows 上 MSYS2 的 bash/zsh/fish 与 ConPTY 组合脆弱、维护成本高，建议缩为 4 组合。**保留意见**：若 MSYS2 组合在 P0 连续两个 nightly 出现环境性 flake（非产品缺陷），可复议将 #3/#6/#9/#10 降为「发版前抽测」。**复议触发条件**：连续 2 个 nightly 的 M12 失败由 MSYS2 / ConPTY 环境引起且产品侧零缺陷，须附失败日志与最小复现。
- **反方（AI/云侧，支持云 runner 基准）**：自托管参考机供给与排队是工程负担。**接受其负担**：基准可信度是 AR-19 的前置条件，云 runner 只做非门禁冒烟。
- **反方（安全，主张 T2/T3 彻底禁用 WebView）**：软件光栅下仍加载 WebView 扩大攻击面。**部分采纳**：T3 安全模式强制不加载 WebView；T2 保留 WebView 但复用同一沙箱与 CSP 契约。
- **反方（发布工程，主张 EXTERNAL 一律不计且不设上限）**：上限会让 A9 因外部事故重置，惩罚工程团队。**反驳**：无上限即假绿灯；≤6 夜 + 连续 ≤2 夜 + 双人签署已经给出充分弹性。
- **复议条件汇总**（必须为可观测数据，且按 ADR-README §4 走流程）：
  1. 参考机档位：RM-A/RM-C 上 §5 门禁连续两个发布周期出现 MAD/中位数 >2% 且无法通过环境修复，可复议调整机器或新增档位。
  2. 12 组合：MSYS2 环境性 flake 条件命中（见上）可缩为「主场必测 + 跨后端抽测」。
  3. arm64：Linux arm64 / Windows arm64 的 P2 门槛全部满足即可升级；反之若 ≥2 个 release 周期仍不满足，维持现状并在状态页公示。
  4. 成本上限：连续 3 个月实际成本 >80% 上限且无降本空间，可复议提额（需 TSC 批准 + 附实测）。
  5. nightly 上限：EXTERNAL 上限被证明误伤（季度内 >6 夜且全部有确证外部证据），可放宽至 8 夜，但不得取消双人签署与连续升级。

## 关联决策与实现位置

| 决策 / 约束 | 内容 | 实现位置 |
| --- | --- | --- |
| AR-01 / AR-02 | 网格绝不经 WebView；WebView 可选，缺失降级纯原生面板（T3） | term-render、webview-shell、shell-bridge |
| AR-14 | 三字体后端均不启用系统 subpixel AA | term-render（font backend trait） |
| AR-19 | 门禁 / 目标两列口径；本 ADR 仅绑定机器，不改数值 | ~~termai-xtask perf-gate~~ **被 ADR-0028 取代**、bench-report.json（测量与门禁的实现主场为 `tools/bench/`） |
| AR-21 / ADR-0013 | 全栈开源 Apache-2.0 OR MIT，无专有硬件/服务依赖 | 仓库根 LICENSE、CI 许可门禁 |
| DC-16 | Windows 唯一生产路径 ConPTY，Win10 1809 地板（arm64 见决策 2） | termai-pty（conpty） |
| DC-17 | wgpu 后端阶梯 T0–T3；栅格化统一 swash | termai-gpu、term-render |
| DC-37 | fuzz 24h × 4 target 自托管；语料入库 | tests/fuzz/、自托管 runner |
| HARNESS §5 | 门禁项 → RM 映射表 | tests/matrix/、machine-fingerprint.json |
| HARNESS §8.2 A6 / A9 | 12 组合 M12 全绿；nightly FAIL-REPO/EXTERNAL 核算 | cargo xtask matrix、nightly-exception.json |
| OQ-E1 / OQ-E2 / OQ-E3 / OQ-E10 | 本 ADR 关闭工程部分 | docs/spec/07-engineering-quality-and-release.md |
| 待同步 | HARNESS §11 的 OQ-26 / OQ-28 需由 Orchestrator 标注「工程部分已由 ADR-0014 裁决」（本 ADR 不修改 HARNESS 本体） | HARNESS.md（Orchestrator 执行） |
| 边界 | 弱 copyleft 准入与「链接边界」定义属 ADR-0015，不在本 ADR 范围 | — |
