# 10 · CTO / 工程负责人评审意见

> 角色：CTO / 工程负责人（技术选型终裁、架构与模块边界、构建与发布、质量、治理）
> 状态：终裁建议稿，供 Orchestrator 仲裁；与我冲突之处以 ADR 保留反方记录。
> 依据：`docs/spec/00-glossary.md` 术语基线；编号沿用 `D-xx` / `ADR-xxxx`。

## 一、角色立场与判断依据

我的职责不是让所有人满意，而是在**不可逆节点**上做选择并承担代价。三条前提：

1. **这是 10 年生命周期的软件。** 我优先保护可调试的边界、可回滚的发布、可替换的组件；"省 3 个月、锁死 3 年"的方案一律否决。
2. **性能预算是第一性约束。** 按键到像素 P99<16ms、冷启动<150ms、基线内存<120MB；证明不了达标的栈不进核心路径。
3. **AI-native 的价值来自"可信上下文"。** 结构化会话记录（Session of Record）、能力令牌、可审计工具调用必须由内核原生产出，不能靠抓屏文本。

我不接受"三者取其二"的折中话术。下面每条终裁都有明确的输家。

## 二、关键结论（D-01…D-14）

| # | 结论 | 一句话理由 |
| --- | --- | --- |
| D-01 | 核心 100% Rust；TS 仅存在于插件与可选 Web 层 | 延迟/内存/可嵌入性，跨语言边界即失败点 |
| D-02 | 渲染自研 wgpu 终端表面；**不**自研通用 UI 工具链 | 终端是自绘的，面板用受约束的控件集 |
| D-03 | WebView **仅**用于插件 UI 与设置页，与内核数据面强隔离 | 换取插件 UI 迭代速度，不污染热路径 |
| D-04 | 通信层：进程内直连 + `termai-ipc` 二进制协议（带版本握手） | 零拷贝、可测试、可 headless；不用 gRPC/JSON-RPC 做热路径 |
| D-05 | 存储：自研 append-only **Session Log** + SQLite(WAL) 派生索引 + CAS 大对象 | 高频写与可查询两种负载不可共用一种介质 |
| D-06 | 仓库：Cargo workspace + pnpm workspace，依赖单向、CI 强制无环 | Conway 定律的物理落点 |
| D-07 | 构建：`cargo` + `mise`/Nix 锁定，`cargo-dist`+`xtask` 编排，签名在 CI 隔离区 | 可复现构建高于构建速度 |
| D-08 | 许可：核心 **Apache-2.0**，SDK/协议 **Apache-2.0**，插件商店闭源托管 | 生态与商业化的唯一可持续分割线 |
| D-09 | Agent Runtime 是**一等内核组件**，不是插件 | 否则权限/审计/上下文无法收敛 |
| D-10 | AI 只读结构化状态（OSC 133/633 + Session Log），**不做隐式命令注入** | 安全与"可解释"是同一件事 |
| D-11 | 不 fork 现有终端代码；VT 引擎 clean-room 自研，以其行为测试集为规格 | 许可证与架构自主性 |
| D-12 | 质量：VT fuzz + 行为回放 + 视觉回归 + 性能门禁四件套进 CI | 终端 bug 是回归型 bug，必须机器兜底 |
| D-13 | 发布：trunk-based + nightly/beta/stable 三通道，flag 灰度，崩溃上报默认本地 | 回滚能力是发布能力的上限 |
| D-14 | 无 PTY 的原生 Windows 路径不投入；ConPTY 唯一正典，Win10 1809 为地板 | 降级策略必须有明确地板 |

### 逐条展开（结论 / 理由 / 代价与取舍）

**D-01 语言终裁**：内核、PTY、VT、渲染、Agent Runtime、存储全部 Rust。
理由：长驻进程下 GC 抖动不可接受；同一套代码服务 GUI、headless 与未来 Web。代价：UI 迭代变慢、前端人才池变窄。**取舍**：高频变化的面板由 D-03 外置给 TS。

**D-02 渲染终裁（最重要且最高风险）**：终端表面用 wgpu 自绘，字形用 swash+fontdb，行级脏矩形 + 字形图集。
理由：终端渲染高度特化（等宽网格、连字、GPU 图集、Sixel/Kitty），通用 UI 框架反成负担。
代价：**我须自维护一套受约束控件集**（输入框/列表/树/弹层），这是最大长期成本。**取舍**：绝不自研通用布局与主题引擎，自研 UI 面积压缩到 ≤20 个控件。

**D-03 WebView 边界**：插件 UI 与设置页运行在系统 WebView，经 `termai-plugin-bridge` 以能力令牌访问内核，**无直接 PTY/Fs/进程权限**。
理由：插件生态的迭代速度即产品广度。
代价：渲染一致性受 WebView2/WKWebView/WebKitGTK 影响（Linux 尤差），内存 +30–60MB。**取舍**：默认不加载，其崩溃不得影响会话。

**D-04 通信层**：进程内 trait 直连；跨进程/headless 用 `termai-ipc`（长度前缀 + MessagePack/Cap'n Proto + 版本与能力协商），JSON-RPC 仅作调试与第三方入口。
代价：需协议版本治理与 fuzz；**取舍**：换来延迟与 headless 复用。

**D-05 存储**：Session Log 为 append-only 分段 + 校验和；SQLite(WAL) 存历史、索引与 Agent 审计；大对象走内容寻址 CAS。
理由：写热点与查询热点性质相反。代价：以 Log 为唯一真相、SQLite 可重建——**可重建即可删除**。

**D-06 仓库结构（节选）**：

```
termai/
  crates/  termai-pty  termai-vt  termai-render  termai-gpu  termai-session
           termai-store  termai-agent  termai-tools  termai-plugin-host
           termai-ipc   termai-core  termai-xtask
  apps/    termai-desktop  termai-headless  termai-web(后续)
  packages/ plugin-sdk-ts  plugin-ui-runtime  webview-shell
  plugins/ 官方插件（独立版本线）
  docs/ adr/ spec/ roles/
```

硬性依赖规则：`core ← session ← {agent, plugin-host}`，`render` 只依赖 `vt`+`gpu`，**任何人不得依赖 `apps/*`**，CI 用 `cargo-deny` + crate 图检查强制无环。这就是把团队拓扑写进代码；代价是早期摩擦。

**D-08 许可终裁（争议最大）**：核心与 SDK 用 Apache-2.0（含专利授权），插件商店/同步/团队协作等商业服务闭源。
理由：终端采用门槛由发行版与企业法务决定，GPL/AGPL 会杀掉大量企业采用；插件商店是可持续变现点。
代价：云厂商可能白嫖；**取舍**：以生态规模而非许可护城河竞争。**明确反对** AGPL+双许可路线。

**D-09/D-10 Agent 架构**：Agent Runtime 在 `termai-agent`，工具经 `termai-tools` 以 `Tool + Capability + RiskTier` 注册；默认 dry-run，R2+ 需确认，全部调用写审计流；上下文来自 Session Log 的结构化事件，**不做屏幕抓取**，数据与指令严格分离（防 prompt injection）。
代价：实现远慢于直接投喂输出；**取舍**：换来可审计、可复现、企业可售。

## 三、详细设计

**构建与工具链**：`rustup` 固定 toolchain，`mise`/Nix 锁定 node、pnpm 与交叉工具链，`cargo xtask dist` 统一产物。交叉编译：Linux 用 `cargo-zigbuild` 或容器；macOS 仅在 mac runner 构建；Windows 用 MSVC + `windows-rs`。产物：Windows `.msi`/`.exe`（Authenticode + Azure Trusted Signing）、macOS `.dmg`/`.app`（Developer ID + notarization）、Linux AppImage 为主 + `.deb`/`.rpm`/Flatpak。**签名私钥只存在于 CI 隔离环境。**

**测试策略分层**：① 单元（VT 解析、reflow、能力令牌）；② 集成（PTY 会话、存储重建、IPC 协商）；③ 回放式 E2E（录制真实会话字节流 + 输入脚本，断言最终网格与状态）；④ 性能门禁（criterion + `termai-bench`：吞吐、延迟 P99、RSS）；⑤ fuzz（`cargo-fuzz` 打 VT/PTY/IPC/插件消息）；⑥ 兼容矩阵（Shell×OS×字体×WebView）；⑦ 视觉回归（帧哈希阈值）；⑧ 泄漏检测（24h 长跑 + RSS 斜率）。

**CI/CD**：trunk-based，PR 必过 fmt/clippy/test/fuzz-smoke/perf 门禁；`main` 产 nightly，`beta` 双周、`stable` 月度；新功能走 flag，灰度按 channel→cohort→全量，回滚 = 撤回 channel 指针。崩溃上报用自托管 Sentry 兼容端点，默认仅本地，上传需显式同意并脱敏。

**团队拓扑（Conway）**：① Core Kernel（pty/vt/render）；② Shell & UX（应用与控件集）；③ Agent & Context（agent/tools/模型路由）；④ Platform & Ecosystem（plugin-host、SDK、商店）；⑤ DevEx & Release（xtask、CI、签名）。每 crate 设 CODEOWNERS，跨边界改动双签。

## 四、与其他角色的接口与依赖（谁必须给我什么 / 我向谁承诺什么）

| 对方角色 | 必须给我 | 我承诺 |
| --- | --- | --- |
| AI/Agent 架构 | 上下文 schema、工具清单与 RiskTier、Eval 集 | 原生 Session Log、能力令牌、dry-run 与审计流 |
| 渲染/图形 | 帧预算分解、GPU 后端降级需求 | wgpu 抽象、脏矩形 API、软件后备路径 |
| 前端/插件生态 | 插件 UI 生命周期、桥接 API 需求 | 稳定 `termai-ipc`、SDK SemVer + 6 个月弃用期 |
| 安全/隐私 | 威胁模型、脱敏规则库、CVE 响应 SLA | 默认拒绝权限模型、无网络端口默认、可重建存储 |
| 产品 | NSM 定义与验收口径 | 可量化 AC（见第七节）与遥测最小集 |

反向硬边界：**我不接受任何角色要求"内核直接执行 AI 生成的命令字符串"**，必须走工具 + 令牌 + 风险分级。

## 五、被否决的方案与反方意见（保留 tradeoff 记录）

| 方案 | 支持者可能的理由 | 我的否决理由 | 若被推翻的先决条件 |
| --- | --- | --- | --- |
| B 全 WebView UI（Tauri 式） | 开发最快、样式统一、可复用 Web 版 | 输入延迟与 IME/合成不可控，Linux WebKitGTK 风险高 | 提供 P99<16ms 与三平台一致性实测 |
| 自研完整原生 UI 工具链 | 完全掌控、性能最佳 | 等于再造 Flutter/Qt，维护成本吞噬产品迭代 | 团队规模 ≥3 倍且接受 3 年周期 |
| Electron + xterm.js | 生态成熟、招人容易 | 内存 300MB+、启动慢、无法 headless 复用 | 放弃"世界级性能"这一产品前提 |
| fork alacritty/wezterm 内核 | 省 1–2 年 VT 工作量 | 许可与架构自主性受限，数据模型不服务 AI 上下文 | 法务确认许可可行且承担上游合流成本 |
| 核心 AGPL + 双许可 | 防止云厂商白嫖 | 企业采用受阻，AI 时代更需要生态规模 | 商业验证显示 90% 收入来自企业合规授权 |
| Agent 做成插件 | 内核解耦、可替换 | 权限/审计/上下文无法闭环 | 拒绝——安全底线，不设推翻条件 |

## 六、风险与缓解（Top 10）

| # | 风险 | 触发条件 | 缓解 |
| --- | --- | --- | --- |
| R1 | 自研控件集拖垮进度 | 控件需求 >20 个或出现主题需求 | 硬约束控件集；复杂界面一律 WebView |
| R2 | VT 兼容性长尾 | vttest/回放通过率 <99% | 引入公开测试集与真实会话回放语料 |
| R3 | Linux WebView 碎片化 | 插件 UI 在 ≥2 发行版渲染异常 | WebView 设为可选依赖；提供纯原生降级面板 |
| R4 | 签名/公证中断发布 | CI 证书或 Apple 服务不可用 | 备用证书；发布可暂停但不阻塞 nightly |
| R5 | AI 误操作破坏用户环境 | R2+ 工具无确认被执行 | 默认 dry-run + 风险分级 + 审计 + 撤销快照 |
| R6 | Prompt injection 经终端输出劫持 | 模型按输出内容执行命令 | 数据/指令分离 + 输出标记不可信 + Eval 覆盖 |
| R7 | 性能预算被渐进侵蚀 | 启动 >150ms 或延迟 P99 >16ms 连续两版 | 性能门禁进 CI，回归即阻塞合并 |
| R8 | 团队边界与模块边界脱节 | 跨 crate 私改频繁、CODEOWNERS 失效 | 季度架构审计 + 依赖图报告进周会 |
| R9 | 存储格式演进致数据不可读 | 发布后需改 Session Log 格式 | 格式版本 + 迁移器 + 兼容读取 ≥2 个大版本 |
| R10 | 开源策略摇摆 | 核心许可 12 个月内变更 | 章程规定许可变更需 TSC 2/3 + 90 天公示 |

## 七、可验证验收标准（量化）

1. 冷启动 **P95 ≤150ms**，按键到像素 **P99 ≤16ms**（8 会话）；基线内存 **≤120MB RSS**（1 万行 scrollback），24h 长跑 RSS 斜率 **<1MB/h**。
3. VT 兼容：`vttest` 全通过；回放语料通过率 **≥99.5%**；VT/IPC fuzz **24h 无 crash**。
4. 存储：10GB Session Log 下按命令/退出码/cwd 检索 **P95 ≤50ms**；SQLite 索引可由 Log 完整重建且校验一致。
5. 插件：越权 **100% 被拒**并留审计；插件 UI 崩溃 **0** 次影响会话。
6. 发布：任一 stable 版本 **≤15 分钟**全量回滚；nightly 连续 **30 天**流水线不中断。
7. 兼容矩阵：Windows 10 1809+/11、macOS 13+（arm64/x64）、Ubuntu 22.04+/Fedora 38+；4 种 shell × 3 种字体后端全绿。

## 八、待决议问题（Open Questions）

1. **WebView 是否进入最低支持依赖？**（我倾向：不进，缺失时降级为文本面板。）
2. 核心许可最终选 Apache-2.0 还是 MIT+专利补充？（需法务确认。）
3. Session Log 是否端到端加密，还是仅依赖 OS 磁盘加密？（影响 AI 上下文读取性能。）
4. 是否提供**远程/Web 终端**作为一等能力？若提供，IPC 需提前规划传输加密与鉴权（会改变 D-04 边界）。
5. 插件商店的收入分成与审核 SLA 由谁运营？（影响 Platform 团队编制。）
6. 本地模型（llama.cpp 系）内嵌为内核能力，还是仅作 Model Router 的一个 provider？
7. 是否成立独立 TSC 与治理章程，还是 3 年内维持 BDFL + 公开 ADR？
8. **与 Orchestrator 的冲突点（请仲裁）**：D-02/D-03 混合渲染分层、D-08 Apache-2.0 终裁、D-09"Agent 非插件"底线——若被要求让步，请明确代价承担方。
