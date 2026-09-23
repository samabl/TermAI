# TermAI 角色评审 · 09 开发者体验与插件生态负责人（DevEx & Plugin Ecosystem Lead）

## 一、角色立场与判断依据

我的 KPI 不是终端帧率，而是"陌生人第一次为 TermAI 写插件并发布"的转化率与留存。终端性能是入场券，生态是复利。

三条不可交换的原则：
1. **插件永不降低终端的正确性与延迟下界**：所有插件回调在关键路径之外，带硬超时与降级。
2. **默认无权限（deny-by-default）**：权限必须可解释、可撤销、可审计。
3. **宿主 API 是产品，不是内部实现泄漏**：契约先行（WIT），类型与文档从契约生成，禁止手写并行定义。

依据：VS Code（extension host 隔离 + 声明式贡献点 + TS 低门槛）、Neovim/WezTerm（命令式配置的吸引力与维护债）、Zed（单一 WASM ABI 的收益与冷启动代价）、各类 dylib 插件系统的 ABI 地狱。

## 二、关键结论

**D1 · 唯一插件 ABI：WIT + WASM Component，第三方原生动态库一律拒绝**
- 理由：单一 ABI = 一份契约、一套工具链、一套权限模型；WASI 零 ambient authority 天然匹配能力安全；语言中立（Rust/Go/TS/Python）。
- 代价：放弃 Chrome DevTools 级 JS 调试体验；需要系统能力的插件被迫走 subprocess。**取舍：可移植性 + 安全 > 原生性能。**

**D2 · 两种运行形态：沙箱组件（默认）+ 进程外 subprocess（白名单）**
- subprocess 需用户显式标记 trusted，不进入市场普通分类、不得直写 host API。理由：git/docker/kubectl/云 CLI 集成的本质就是"调用别人的可执行文件"，用 WASM 模拟只会更差。
- 代价：trusted 插件权限等于用户权限，权限模型出现"断崖"。**取舍：承认断崖并显式标注，优于假装所有插件都能被沙箱化。**

**D3 · 故障域：一个 extension host 进程 + 每插件独立 wasm store + watchdog**
- 插件 trap 只终止该次调用；host OOM/crash 由 watchdog 重启；连续 3 次崩溃进入 safe mode（`termai --safe-mode` 禁用全部插件）。
- 代价：常驻内存（目标空载 ≤80MB，`--no-plugins` 归零）；memory 级隔离弱于进程级。**取舍：内存换进程爆炸。此点预期与性能负责人冲突。**

**D4 · 能力模型：manifest 声明 + 首次逐条授权 + 调用时校验**
首版词表：`fs.read/fs.write`（路径 scope）、`net`（域名 allowlist）、`exec`（命令 allowlist）、`secrets`、`clipboard`、`session.read/write`、`ui.panel`、`ai.tool.register`。越权一律拒绝并记审计。
- 代价：授权弹窗造成权限疲劳，早期作者会抱怨。**缓解：能力打包为 profile + 一次向导。**

**D5 · SDK：WIT 是唯一真源，TS 优先、Rust 紧随**
`wit/termai-plugin.wit` → wit-bindgen 生成 Rust/TS 类型 + Markdown 文档；npm 包 `@termai/plugin-sdk` 只含生成类型、薄封装与测试工具。API 分级：Tier1 冻结 / Tier2 experimental / Tier3 internal。

**D6 · 扩展点分级**：见 §3.2。原则：**声明式 > 命令式，只读 > 可写，异步 > 同步。**

**D7 · UI 与渲染：声明式装饰 + 隔离 WebView 面板，不给第三方帧内绘制权**
- 装饰 API = cell range + style token + z-order 枚举；UI 面板 = 独立 origin 的 WebView + capability 桥。
- 代价：表达力受限，"自绘终端"类插件做不了。**取舍：终端正确性 > 插件表现力。此点与渲染负责人正面冲突。**

**D8 · 配置：TOML + drop-in，不内嵌 Lua/JS**
层级 `default < system < user < project < env < CLI`；`config.d/*.toml` 合并；`termai config get/set/eval --json`；"可编程配置"由 config provider 插件（纯函数 WIT）承担。不读 shell rc；`termai import` 支持 kitty/alacritty/wezterm/iTerm2。
- 代价：失去"配置即代码"的社区炫技，复杂条件配置啰嗦。**取舍：可静态校验、可生成 schema、可迁移的价值更大。**

**D9 · 脚本化：插件能力 ⊆ Local API 能力，只维护一套契约**
Local API = JSON-RPC 2.0 over UDS / Windows named pipe，携带 `apiVersion`；CLI 一律支持 `--json`；`termai daemon` 服务 headless 与 CI。安全：socket 0600 / ACL、peer credential 校验、token、审计日志。
- 代价：一套 API 同时服务插件与外部脚本，演进节奏耦合。**取舍：绝不维护第二套私有协议。**

**D10 · 分发与商业：市场不抽成、不做 DRM；核心 Apache-2.0**
市场只负责命名空间、Ed25519/sigstore 签名、SBOM、分级审核；收费由作者自带 license key。官方收入来自企业治理（SSO、策略、私有 registry、审计）与 Verified 认证。
- 代价：放弃最顺手的平台抽成，可能被竞品用免费市场补贴打。**取舍：生态增速 > 短期分成。**

**D11 · 兼容性：N-2 minor 支持 + 6 个月 deprecation + codemod**
插件声明 `engines.termai`；破坏性 WIT 变更需 RFC + 2 名 maintainer；缺失能力返回结构化错误而非 panic。

**D12 · IDE 集成：唯一集成面是 Local API**
VS Code / JetBrains / Neovim 客户端都是普通消费者（也可由社区以插件形式实现），不维护 N 套私有协议。终端内嵌 IDE 场景（如 VS Code terminal）反向使用同一 Local API。

**D13 · 脚手架与度量**：`termai plugin new|dev|test|publish --lang ts|rust`，模板自带 CI 与签名；指标见 §7。

## 三、详细设计

**3.1 插件清单 `plugin.toml`**
```toml
id = "dev.acme.git-branch"
version = "0.4.1"
engines = { termai = ">=0.3,<0.5" }
runtime = "wasm"            # wasm | subprocess(trusted)
[capabilities]
session = ["scrollback:read"]
exec = ["git"]
[contributes]
commands = ["git.branch.copy"]
ai_tools = ["git.recent_branches"]
```

**3.2 扩展点分级**
| 级别 | 扩展点 | 开放性 |
|---|---|---|
| Tier1 稳定 | 命令/命令面板、快捷键、配置 schema、状态栏项、CLI 子命令、补全源、AI tool、OSC133 语义事件、主题 token | 全开放并冻结 |
| Tier2 试验 | 侧栏/面板 UI、buffer 装饰、hyperlink provider、市场 provider、只读性能探针 | 开放但可破坏 |
| Tier3 禁止 | 帧内任意绘制、直改 VT 状态机、同步阻塞 host 调用、注入终端输出流 | 永不开放 |

**3.3 配额与降级**
| 资源 | 默认 | 上限 | 超限行为 |
|---|---|---|---|
| CPU | 每秒 fuel 配额 | 用户可提 | 中止本次调用 + 提示 |
| 内存 | 64MB | 512MB | trap + 标记插件 |
| host 调用超时 | 500ms | 2s | 返回 Timeout，UI 不阻塞 |
| IPC 速率 | 100 次/秒 | 1000 | 限流 + 审计 |

**3.4 热重载**：`plugin dev` 采用组件重新实例化 + 状态丢弃（仅 dev）；生产禁止热重载，避免"只有开发态正确"的插件进入市场。

## 四、与其他角色的接口与依赖

| 对象 | 我必须得到 | 我承诺 |
|---|---|---|
| 终端内核 | 稳定只读 surface/scrollback API、OSC 133/8/1337 语义事件、带 epoch 的坐标（reflow 后可判失效） | 插件绝不写 PTY 或输出流；回调硬超时；Tier1 变更提前 6 个月公告 |
| 渲染负责人 | 装饰层 z-order 契约、合成时机、"丢帧而非卡帧"策略 | 装饰只在帧边界提交，单帧预算 ≤2ms，超时丢弃 |
| AI/Agent 负责人 | tool registry 契约、工具结果 untrusted 标记、审批策略枚举（read/mutate/destructive） | 插件 AI tool 默认需审批；工具输出不参与指令解析 |
| 安全负责人 | capability 词表评审、签名与密钥托管、审计 event schema | 越权必拒并记审计；签名校验失败必拒装 |
| 平台/构建 | 三平台 CI、WASM AOT 预编译流水线 | 破坏性变更提供 codemod 与迁移文档 |
| 产品/商业化 | open-core 边界裁定 | 市场免费不抽成的技术实现（不内置付费墙） |

## 五、被否决的方案与反方意见

| 方案 | 反方最强理由 | 我的反驳 | 保留意见 |
|---|---|---|---|
| E1 纯 Node/Electron + xterm.js | 生态与开发速度碾压 | 内存/启动/渲染上限锁死，与 AI-native 定位不符 | 作为 Web/远程版可选前端保留 |
| E2 第三方原生 dylib | 性能与系统调用无妥协 | 无能力模型、无崩溃隔离、ABI/GC/panic 地狱，市场无法安全审核 | 一等方与 trusted subprocess 覆盖 95% 需求 |
| E3 内嵌 Lua 配置 | Neovim 证明命令式配置有狂热用户 | 引入隐式插件运行时却无权限、无配额、无版本约束 | 若社区压力足够，作为"Lua 插件作者语言"而非配置语言回归 |
| E4 开放渲染 hook | 能做出惊艳的终端增强 | 破坏正确性、绑定内部渲染器、性能不可预测 | 若装饰 API 表达力不足，按 §6 机制升格 |
| E5 市场抽成 + DRM | 直接收入与治理抓手 | 作者会流向不抽成的市场，DRM 只惩罚诚实用户 | 企业私有 registry 可收费 |
| E6 只支持 Rust/WASM 作者 | ABI 干净、质量高 | 生态规模由最低门槛语言决定，TS 必须优先 | Rust 仍为首等公民，双首屏模板 |
| E7 每 IDE 独立协议 | 单 IDE 体验最优 | 维护 N 套协议必然腐烂 | 允许 IDE 侧薄客户端自行聚合 |

**预期冲突点（提请 Orchestrator 仲裁）**：① 与渲染负责人：WebView 面板 vs 纯自研 UI；② 与终端内核：装饰 API 是否触碰 VT 语义；③ 与安全负责人：Local API 与 subprocess 扩大攻击面；④ 与性能负责人：extension host 常驻内存；⑤ 与商业化：市场不抽成 + Apache-2.0。

## 六、风险与缓解

| 风险 | 缓解 | 触发式决策门 |
|---|---|---|
| WASM 冷启动/体积劝退作者 | AOT 预编译、实例池、组件 ≤3MB 预算 | — |
| componentize-js DX 不达标（调试器弱、npm 原生模块不可用） | 指标 gate：M2 末 TTFHW ≤5min 且热重载 p95 ≤300ms | 未达标即启动 fallback：每插件 V8 isolate 第二运行时 |
| 装饰 API 表达力不足 | 每季度评审插件请求 Top10 | 连续两季度 Top3 受阻即 Tier2 → Tier1 |
| 权限疲劳 | 能力 profile、一次向导、静默续期 | — |
| 生态冷启动 | 官方维护 10 个标杆插件（git/k8s/docker/AI tool 包） | — |
| 恶意插件窃取 scrollback | 网络出口 allowlist、审计、市场扫描 | — |
| WIT 契约被内部需求侵蚀 | Tier1 变更需 RFC + 2 maintainer | — |

## 七、可验证验收标准

1. TTFHW ≤5 分钟：三平台 CI 上 `plugin new ts` → hello world 命令可见，成功率 ≥95%。
2. `plugin dev` 热重载 p95 ≤300ms。
3. Tier1 host 调用 p99 ≤1ms；插件全开 vs 全关，终端输入延迟 p99 差异 ≤0.5ms。
4. 崩溃隔离：注入 100 次 trap/panic/OOM，核心会话存活率 100%，无数据丢失。
5. 配额生效：超 fuel/内存/超时用例 100% 被中止且 UI 有提示。
6. 文档：Tier1 WIT 函数 100% 有生成文档与可运行示例；每个扩展点 ≥1 示例。
7. 发布链路：`plugin publish` 到市场可见 ≤5min；签名校验失败 100% 拒装。
8. 迁移：对 N-1 插件样本集，codemod 自动迁移成功率 ≥90%。
9. 生态：1.0 时 ≥50 个过审第三方插件、≥10 个提供 AI tool；6 个月内外部贡献者 commit 占比 ≥20%。
10. 兼容：N-2 minor 插件在最新 host 上功能测试通过率 ≥95%。

## 八、待决议问题

1. UI 面板技术栈：隔离 WebView（我主张）vs 自研 UI DSL vs 完全不开放第三方 UI —— 需与渲染负责人共同裁决。
2. 是否允许 side-load 自签名插件？我主张允许，但永久标记 Untrusted 并禁用 subprocess tier 与 secrets。
3. 核心许可最终形态：Apache-2.0（我主张）vs MPL-2.0 vs AGPL + 商业双许可。
4. 官方企业私有 registry 与免费公共市场如何并行，如何避免"功能阉割"争议。
5. componentize-js 决策门的确切指标与 fallback 触发日期（建议 M2 末）。
6. 远程/Web 形态下插件在哪里执行（服务器 / 浏览器 / 双端）？这决定 Local API 是否必须直接成为远程 API。
7. AI 插件 tool 的自动批准边界：只读工具能否免审批？谁定义 destructive 分类？
8. 是否对第三方开放可选图形协议（如 Kitty graphics）与只读渲染性能探针。
