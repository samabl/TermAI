# 插件生态与开发者体验规格

> 领域规格 06 · 依据 `HARNESS.md` v1.0 与角色原文 `09-devex-ecosystem-lead.md`（主）、`05-frontend-architect.md`、`08-security-privacy-architect.md`。
> **效力**：与 HARNESS 的 AR / DC / 预算 / 门禁 / Anti-features 冲突时一律以 HARNESS 为准；本文件只回答「如何实现」，不产生新约束。角色原文中的「M2 末」已按 HARNESS §7 统一为 **P2 末**。
> **许可口径**：市场与商业化条款按 **AR-21 / ADR-0013** 对齐（全栈开源、Apache-2.0 OR MIT）；商业承诺（v1 零抽成、无 DRM、主题永久免费）**不变**。(ADR-0013)

## 1. 目的与范围

**目的**：定义 TermAI 插件系统的对外契约、运行时边界、授权模型、发布链路与开发者体验指标，使第三方作者能**零特权发布**，同时保证终端的正确性与延迟下界不被第三方抵押（AR-07、AR-19）。

**范围内**：① `plugin.toml` 清单与 `wit/termai-plugin.wit` 契约及其 SDK 生成管线；② 两种运行时（WASM Component / trusted subprocess）、能力词表、授权 UX、配额与故障域；③ 插件宿主进程、插件 UI、Local API、CLI/headless、IDE 集成；④ 脚手架、发布链路、兼容策略、官方标杆插件与生态度量。

**范围外**（不自行发明结论，空白写入 §7）：① 远程/Web 形态下插件的执行位置；② 市场「分级审核」的分级标准与人日 SLA；③ 主题（纯数据，DC-12）是否与插件共用链路；④ 新增扩展点、capability 词表变更、WIT 破坏性变更 —— 一律走 RFC → ADR（HARNESS §0.3）。

**读者**：内核 / 前端 / 安全 / AI 负责人、插件作者、发布与法务。

## 2. 需求与约束

| # | 需求 / 约束 | 来源 | 落地条款 |
| --- | --- | --- | --- |
| 1 | 唯一 ABI = WIT + WASM Component；第三方原生 dylib 一律拒绝 | AR-07 / DC-39 / §10-9 | §3.2、§3.3 |
| 2 | 第二形态 = trusted subprocess（git/docker/kubectl/云 CLI 白名单），须用户显式标记，**不进市场普通分类** | AR-07 / OQ-12 | §3.3 |
| 3 | 插件宿主是独立进程，WASM 为进程内子隔离层；签名 + 权限清单强制 | AR-07 / DC-38 | §3.3、§3.7 |
| 4 | 插件只能「只读镜像 + 显式注入队列 + 声明式装饰」；**永不帧内任意绘制、永不写 PTY/输出流** | AR-07 / §10-10 | §3.4、§3.8 |
| 5 | v1 只开放 Tier1 声明式 view schema（token 子集 + 组件白名单）；Tier2 隔离 iframe 在安全红队通过后开放 | AR-08 / DC-09 / DC-25 / OQ-11 | §3.8 |
| 6 | 扩展点三级：Tier1 冻结开放 / Tier2 试验 / Tier3 永禁 | DC-39 | §3.4 |
| 7 | capability-based **默认拒绝**；企业策略只能加强、不能削弱 | DC-35 / AR-06 | §3.5 |
| 8 | 权限分包为 profile + 首次逐条授权 + 可撤销（缓解权限疲劳） | AR-06 / 角色09 D4 | §3.5 |
| 9 | 破坏性与外发型操作的确认**不可由配置关闭** | AR-06 / §0 / §6.1 | §3.5、§3.10 |
| 10 | 插件宿主空载 ≤80MB（仅启用插件时计入，**不占用核心 120MB 基线**）且**按需启动**；配额与降级（fuel / 内存 / 超时 / IPC 速率）由宿主强制 | §5 / DC-38 / AR-19 / AR-07 | §3.6、§3.7 |
| 11 | 崩溃 3 次进 safe mode；`termai --safe-mode` 禁全部插件；`--no-plugins` 归零 | DC-38 / 角色09 D3 | §3.7 |
| 12 | Local API = JSON-RPC over UDS/named pipe（含 apiVersion），同时服务插件、CLI `--json`、headless、CI、IDE；**只维护一套契约** | DC-24 | §3.9、§3.10 |
| 13 | 通信走 termai-ipc（二进制 + 能力协商）；热路径禁用 JSON；JSON-RPC 仅调试与第三方入口 | DC-22 / AR-04 | §3.9 |
| 14 | WIT 为唯一真源；类型与文档从契约生成；**禁止手写并行定义** | 角色09 D5 | §3.2 |
| 15 | 工具 ABI 含 risk/idempotent/timeout/approval_policy/render_hint；插件 AI tool 默认需审批、输出不可信 | DC-26 / AR-06 / R5 | §3.4、§3.5 |
| 16 | 审计 append-only + 本地哈希链（操作者/审批人/结果）；脱敏在上下文构建器内完成，插件不得绕过，失败默认拒绝发送 | DC-34 / DC-33 / §6.1 | §3.5、§3.9 |
| 17 | 兼容 N-2 minor + 6 个月 deprecation + codemod；破坏性 WIT 变更需 RFC + 2 maintainer | DC-40 | §3.14 |
| 18 | 客户端/服务端/SDK/协议/工具链统一 **Apache-2.0 OR MIT**（全栈开源；CI 校验 SPDX 表达式）；链接边界 GPL/AGPL/SSPL = 0；v1 市场**零抽成、无 DRM** | AR-21 / §8.1-5 | §3.13 |
| 19 | 签名校验失败 100% 拒装；供应链缓解含透明度日志与远程吊销 ≤1h | §7 P3 / §9 R9 | §3.13 |
| 20 | 生态与安全门禁：TTFHW ≤5min（成功率 ≥95%）、热重载 p95 ≤300ms、崩溃隔离 100%、1.0 时 ≥50 过审插件；插件消息 fuzz ≥10⁸ 次无崩溃、逃逸用例 0 成功 | §8.2 / §8.1-6 / DC-37 / §9 R10 | §3.12、§5 |
| 21 | 核心 100% Rust；TS 仅存在于 Web 面板与插件 SDK；依赖单向无环 `core ← {agent, plugin-host}` | DC-15 / DC-21 | §4.1 |
| 22 | WebView 为**可选**运行时依赖，缺失或不可信时降级为纯原生文本面板 | AR-02 | §3.8 |
| 23 | 遥测默认关闭、内容零采集；插件不得引入未声明的出站连接 | AR-12 / §8.2 | §3.9、§6 |

## 3. 详细设计

### 3.1 插件清单 `plugin.toml`

`plugin.toml` 是**安装期单一真源**，随包签名、宿主逐字段校验；**展示型元数据外置**到 `market.toml`（宿主不信任其内容，仅市场 UI 消费），避免「文案改动触发信任重审」。

| 字段 | 类型 | 必填 | 说明与约束 |
| --- | --- | --- | --- |
| `id` | string | ✔ | 反向 DNS 命名空间，市场唯一，不可变更（转移需 registry 仲裁） |
| `name` / `description` | string | ✔ | 展示名 ≤40 字符；描述 ≤200 字符 |
| `version` | semver | ✔ | 每次发布必须递增；市场拒绝重复版本号 |
| `license` | SPDX | ✔ | 须在允许清单内（OQ-06-07）；缺失即拒发 |
| `authors` / `repository` / `categories` / `keywords` / `icon` | array / url / path | ✔ / ○ | 至少 1 位作者；分类**不得含 `trusted`**（AR-07）；icon 禁止外链 |
| `engines.termai` / `engines.wit` | range / semver | ✔ | 安装期拒绝不满足项 + 加载期二次校验；N-2 窗口（DC-40） |
| `runtime.kind` / `entry` | enum / path | ✔ | `wasm`（默认）/ `subprocess`；包内相对路径 |
| `runtime.component` / `max_size_kb` | semver / int | ✔ / ○ | 组件模型版本；体积预算默认 3072KB（≤3MB，R10） |
| `runtime.argv` / `cwd` / `env_allowlist` / `stdio` | — | subprocess 必填 | 仅白名单命令；env 默认空；stdio 由宿主接管 |
| `capabilities.*` | table | ✔ | 见 §3.5 词表；未声明即不可调用 |
| `contributes.*` | table | ○ | 见 §3.4；出现 Tier3 名称直接拒载 |
| `activation.events` / `lazy` / `limits.*` | array / bool / table | ○ | 按需启动触发器；`limits` **只能收紧、不能放宽**宿主硬上限（§3.6） |
| `trust.publisher` / `trust.keyid` | string | ✔ | 与包签名交叉校验；不一致即拒装 |

```toml
# plugin.toml — 安装期单一真源；随包签名，宿主逐字段校验
id          = "dev.acme.git-branch"
name        = "Git Branch Tools"
description = "分支切换、最近分支补全与 AI 诊断工具"
version     = "0.4.1"
license     = "Apache-2.0"
authors     = [{ name = "Acme Dev", email = "dev@acme.example" }]
repository  = "https://github.com/acme/termai-git-branch"

engines = { termai = ">=0.3,<0.5", wit = "1.0.0" }                 # N-2 minor 窗口（DC-40）
runtime = { kind = "wasm", entry = "plugin.wasm", component = "0.2.0", max_size_kb = 3072 }

[capabilities]                # 默认拒绝：未声明即不可调用
session = ["scrollback:read"] # 只读镜像（L0）
exec    = ["git"]             # argv allowlist，禁止 shell -c
fs      = ["$WORKSPACE/**:read"]
net     = []                  # 空 = 无网络
secrets = []                  # 空 = 不可使用任何密钥
ai_tool = ["register"]

[contributes]
commands      = [{ id = "git.branch.copy", title = "Copy branch name" }]
keybindings   = [{ command = "git.branch.copy", default = "ctrl+shift+b" }]
status_bar    = [{ id = "git.branch", text = "{{branch}}", align = "right" }]
completion    = [{ id = "git.branches", trigger = "git checkout " }]
ai_tools      = ["git.recent_branches"]
config_schema = "config.schema.json"
osc_events    = ["133;D", "7"]    # 只读订阅，不可改写

activation = { events = ["onCommand:git.branch.copy", "onSessionStart:git"], lazy = true }
limits     = { fuel_per_call = 50000000, memory_mb = 64, host_call_timeout_ms = 500, ipc_rate_per_sec = 100 }
trust      = { publisher = "acme.example", keyid = "did:key:z6Mk..." }   # 与包签名交叉校验
```

**规范要点**：① `runtime.kind = "subprocess"` 即使清单声明，也**必须由用户显式标记 trusted 后才可启用**（另需 `argv`/`cwd`/`env_allowlist`/`stdio`），否则拒载；② 宿主对未知字段忽略并告警，不得因未知字段崩溃；③ `engines` 不满足在安装期即拒绝，不进入「装了但跑不起来」状态。

### 3.2 WIT 唯一真源与 SDK 生成管线

唯一真源 = `wit/termai-plugin.wit`（package `termai:plugin@1.0.0`），定义 world `plugin`（导入 host 接口、导出 plugin 接口）与全部 Tier1 类型。生成管线单向产出，**禁止手写并行定义**：

| # | 生成物 | 工具链 | 消费者 |
| --- | --- | --- | --- |
| 1 | Rust bindings | wit-bindgen | Rust 模板、第一方标杆插件 |
| 2 | TS 类型 | wit-bindgen-js / jco + componentize-js | `@termai/plugin-sdk`（**TS 优先**） |
| 3 | API 参考文档（Markdown） | 从 WIT doc comment 生成 | 文档站（Tier1 函数 100% 覆盖） |
| 4 | `plugin.toml` JSON Schema | schemars + capability 枚举 | 脚手架、市场审核、作者 IDE 补全 |
| 5 | Local API method catalog + capability 词表枚举 | 与 WIT 同源 | §3.9 契约、CLI `--json`、IDE、授权 UX、审计 schema |

- **SDK 形态**：`@termai/plugin-sdk` 只含生成类型 + 薄封装 + 测试工具；CI 校验生成物与 WIT 的**内容哈希一致**，并拒绝 SDK 内出现未被生成的手写类型。
- **语言优先级**：TS 优先（生态规模由最低门槛语言决定），Rust 紧随（首等公民、双首屏模板）；其他语言经 Component Model 可选支持，不单独维护绑定。

### 3.3 两种运行时与权限断崖（显式标注）

| 维度 | `wasm`（默认） | `trusted subprocess`（白名单） |
| --- | --- | --- |
| 隔离机制 | 宿主进程内独立 wasm store（线性内存）+ WASI 零 ambient authority | 独立 OS 进程，但**无沙箱语义** |
| 能力上限 | 仅清单声明且被用户授权的 capability | **等价用户权限**（FS / 网络 / 凭据 / SSH agent 视环境而定） |
| 可审计性 / 审查价值 | 每次 host 调用可拦截、可计量、可拒绝；组件字节码可静态 + 动态扫描 | 仅进程级（argv / env / exit code）可审计；同权限 native 行为不可穷尽验证 |
| 崩溃影响 | trap 只终止本次调用 | 仅影响该插件进程 |
| 市场分类 | 普通分类，可评审可扫描 | **不进入市场普通分类**，须用户显式标记 trusted |
| side-load | 允许，永久标记 Untrusted（OQ-12） | **禁止**；side-load 插件一律禁用 subprocess 与 secrets |
| 与终端交互 | 只读镜像 + 注入队列 + 声明式装饰 | **完全相同**；不得直写 host API / PTY |

**断崖的强制外化**（AR-07 / §0）：① UI 必须显示红色「等价用户权限」徽章，位置与插件名同级；② 首次启用须**二次确认 + 输入插件 id**；③ 启用/禁用 trusted 标记均记 DC-34 审计；④ 企业策略可**禁止** subprocess tier，不得放宽为默认允许。**承认断崖并显式标注，优于假装所有插件都能被沙箱化**（角色09 D2）。

### 3.4 扩展点三级清单

| 级别 | 扩展点 | 开放性 |
| --- | --- | --- |
| **Tier1 冻结** | 命令 + 命令面板（DC-08 覆盖率 100%）、快捷键、配置 schema（AR-09）、状态栏项、CLI 子命令、补全源、AI tool（DC-26）、OSC 133/633 只读订阅、主题 token 引用（DC-12）、声明式 view schema、只读 scrollback 镜像、声明式装饰（cell range + style token + z-order 枚举） | 全开放并冻结；变更走 RFC + 6 个月 deprecation |
| **Tier2 试验** | 侧栏/面板 UI（沙箱 iframe，红队门禁后）、超出声明式子集的 buffer 装饰、hyperlink provider、市场 provider、只读性能探针 | 开放但**可破坏**；须标注 experimental + 用户 opt-in |
| **Tier3 永禁** | 帧内任意绘制、直改 VT 状态机、同步阻塞 host 调用、注入/写终端输出流或 PTY、持有 Node 或原生句柄、静默网络访问、安装期脚本执行 | 永不开放（§10-10、§4.4） |

**升格机制**：每季度评审插件请求 Top10；**连续两季度 Top3 请求受阻**则触发 Tier2 → Tier1 升格评审（仍需红队与兼容评估）。Tier1 冻结 ≠ 不可扩展，而是「只增不删」。

### 3.5 能力词表与授权 UX

| capability | 语义 | scope 形式 | 风险级 | 默认 |
| --- | --- | --- | --- | --- |
| `session.read` | 只读 scrollback / 命令块 / 退出码镜像（坐标带 epoch，reflow 后可判失效） | `scrollback:read`、`log:read`、`events:133;D` | L0 | 拒绝 |
| `fs.read` | 读指定路径 | glob（如 `$WORKSPACE/**`） | L0 | 拒绝 |
| `fs.write` | 写文件（走审批或规则） | glob | L1 | 拒绝 |
| `exec` | 以 argv allowlist 执行命令（**禁止 shell -c 拼接**，§6.2-2） | 命令名 + 参数前缀 allowlist | L1 / L2（按命令） | 拒绝 |
| `net` | 出站连接（经宿主代理，域名 allowlist） | FQDN 列表 | L3（外发） | 拒绝 |
| `secrets` | **使用**指定密钥；明文永不进入插件内存，由宿主代签或注入子进程 env | keychain 引用名 | L3 | 拒绝 |
| `clipboard` | 读写剪贴板 | `read` / `write` | 读 L2 / 写 L1 | 拒绝 |
| `session.write` | 向**显式注入队列**提交 stdin 字节（AR-03 要求显式授权） | 目标 pane / session | L2（每次确认） | 拒绝 |
| `ui.panel` | 注册 Tier2 沙箱面板（红队门禁后） | panel id | L1（渲染） | 拒绝 |
| `ai.tool.register` | 注册 AI tool（DC-26 ABI，默认需审批） | tool id + risk | 由工具 risk 决定 | 拒绝 |

**授权 UX 七步**：
1. **安装期**：展示清单声明的 capability 全集、信任等级与运行时；未安装即无任何权限。
2. **首次调用逐条授权**：每条显示 capability + scope 与三档授权（once / session / rule）；**rule 档仅对 L0/L1 开放**，L2/L3 不提供「永远允许」（AR-06）。
3. **profile 打包**：一键勾选一组（Git 集成包 = `fs.read` + `exec:git` + `session.read`；容器运维包 = `exec:docker,kubectl` + `net:registry`；AI 工具包 = `ai.tool.register` + `session.read`）。**profile 仅批量预填，不改变单条独立可撤销性**。
4. **静默沿用**：版本升级**未新增 capability** 时沿用既有授权；**新增 capability 必须重新逐条授权**，禁止静默扩权。
5. **撤销**：`termai plugin revoke <id> [--capability <name>]` + 图形化权限页；撤销即时生效（令牌失效 + 进行中调用中止），记 DC-34 审计。
6. **企业策略**：只能收紧（禁用 capability、强制 trusted 白名单、禁 side-load、禁 net），不得放宽（DC-35 / AR-06）。
7. **越权处置**：一律拒绝 + 写审计 + 返回结构化错误 `CapabilityDenied`；**禁止 panic，禁止静默降级为无权限运行**。

**与脱敏的关系**：插件读取的 scrollback 镜像**已在上下文构建器内脱敏**（DC-33），插件不得接收原始 secret；插件出站流量同样经分类闸门；脱敏失败默认拒绝发送（§6.1-2）。

### 3.6 配额与降级

| 资源 | 默认 | 用户可调上限 | 超限行为 | 计量点 |
| --- | --- | --- | --- | --- |
| CPU fuel | 50M fuel / 调用 | 可提升（记审计） | 中止本次调用 + 提示 + `ResourceExhausted` | 宿主 |
| 内存 | 64MB / 插件 store | 512MB | trap + 隔离 + 连续超限标记「不稳定」 | 宿主 |
| host 调用超时 | 500ms | 2s | 返回 `Timeout`，UI 不阻塞，重试由插件决定 | 宿主 |
| IPC 速率 | 100 次/秒/插件 | 1000 | 令牌桶限流 + 审计事件 | 宿主 |
| 装饰提交 | 单帧 ≤2ms 预算 | — | **丢帧而非卡帧**：超时丢弃该帧装饰 | 渲染层 |
| 组件体积 | ≤3MB | — | 超限拒绝发布（R10） | 发布 CI |
| 激活耗时 | ≤100ms | — | 降级为延迟激活，不阻塞首屏 | 宿主 |
| 宿主空载 RSS | ≤80MB | — | 告警 + 阻断发布（§5） | CI 采样 |

**不变量**：所有上限由宿主侧**强制**，清单 `limits` 只能收紧；配额耗尽**不得**影响会话正确性或输入延迟下界（AR-07 / AR-19）；耗时回调一律在关键路径之外。

### 3.7 故障域、safe mode 与宿主进程

1. **进程模型**：每用户会话 **1 个 `plugin-host` 独立进程**（AR-07 / DC-38），每插件**独立 wasm store**；watchdog 监督 host。
2. **隔离级别**：插件 trap / panic / OOM 只终止该插件实例；host 崩溃由 watchdog 重启（≤2s），会话不受影响。
3. **崩溃计数**：滑动窗口 10 分钟内同一插件 3 次崩溃 → 自动禁用 + 崩溃报告入口；宿主级崩溃 3 次 → 进入 safe mode 并提示。
4. **safe mode / 归零**：`termai --safe-mode` 禁用全部第三方插件（仅保留第一方内置与 Tier1 只读贡献），UI 顶部横幅可退出；`termai --no-plugins` **不启动 plugin-host**（空载归零、0MB），用于排障与最小复现。
5. **按需启动**：无激活事件时 plugin-host 不存在；首个 `onCommand` / `onSessionStart` / AI tool 调用触发启动；空闲 5 分钟（可配置）后 host 退出并释放内存，组件走 AOT 预编译缓存加速下次冷启。预算：空载 ≤80MB，**仅启用插件时计入**，不占用核心 120MB 基线。
6. **状态诚实声明**：插件不得依赖跨崩溃状态；可持久化数据须走显式 API 且标注易失。会话唯一真源在 sessiond（DC-18），插件崩溃**不得导致会话数据丢失**（AR-13）。

### 3.8 插件 UI

- **v1 只开放 Tier1 声明式**：插件提交 view schema（JSON），Host 渲染，**零插件代码执行**（AR-08）。
- **双渲染后端**：WebView 渲染器（L2，富组件）与**原生文本降级渲染器**（WebView 缺失或不可信时，AR-02）。二者消费同一 schema；schema 必须可被文本后端表达（`plugin test` 可强制预览降级形态）。
- **token 子集 + 组件白名单（≤20，与 DC-25 受限控件集对齐）**：仅允许引用 DC-09 生成的 token 名（**禁止内联色值、CSS、字体、脚本**，CI 校验硬编码色值 = 0）；组件限 text / heading / divider / list / table / key-value / badge / button / input / select / checkbox / progress / tabs / tree / scroll / icon / tooltip / link / diff / code。
- **动作模型**：Tier1 view 只读渲染 + 声明式动作（点击 → 触发已声明的 command 或 ai_tool），动作必须映射到清单已声明项，不允许任意脚本。
- **Tier2 门禁**：独立 origin + `sandbox="allow-scripts"` + `default-src 'none'` + 默认断网 + 经 Core 中转 JSON-RPC + 能力白名单；开启条件 = **逃逸用例 0 成功 + CSP 拦截 100%**（AR-08 / §8.1-6），须归档红队报告。
- **合成边界与 a11y**：插件 UI 不得覆盖 L4 原生浮层（右键菜单 / IME 候选窗），跨层拖拽由 Core 接管指针（AR-01）；声明式 schema 必须声明 role / 标签 / 焦点序，Host 负责映射平台 a11y（AR-20 的「UI 层」承诺），未声明语义的 schema **在 CI 中拒绝发布**。

### 3.9 Local API 契约

- **传输**：UDS（Linux/macOS，socket `0600` 或 peer-uid 白名单）/ Windows named pipe（ACL 限当前用户）。
- **协议**：JSON-RPC 2.0；`apiVersion` 在握手与每条请求中携带；未知 method / 版本返回结构化错误（`-32601` / `UnsupportedApiVersion`），禁止静默兼容。
- **peer credential 校验**：Linux `SO_PEERCRED`；macOS `getpeereid` / `LOCAL_PEERPID`；Windows named pipe impersonation + SID 比对；校验失败直接断开并记审计。
- **权限最小化**：每个 method 声明所需 capability；客户端只能调用其 manifest / 授权范围内的 method。**插件能力 ⊆ Local API 能力**（同一词表生成，DC-24）—— **绝不维护第二套私有协议**。
- **与 termai-ipc 分工**：宿主基础设施与热路径走 termai-ipc 二进制（DC-22 / AR-04，热路径禁用 JSON）；Local API 是**同一 method catalog 的外部投影**，服务插件、`--json`、headless、CI、IDE。

| 分组 | 代表 method | 所需 capability | 写操作 |
| --- | --- | --- | --- |
| `session.*` | `session.list` / `attach` / `input` | `session.read` / `session.write` | `input` = L2（每次确认） |
| `log.*` / `grid.*` | `log.query` / `log.export` / `grid.snapshot`（带 epoch） | `session.read` | 否 |
| `plugin.*` | `plugin.list` / `grant` / `revoke` | **仅桌面 UI / CLI 持用户确认后可调**；插件令牌无此权限 | 是 |
| `config.*` | `config.get` / `set` / `eval` | `config.read` / `config.write` | 写走 patch 校验 |
| `ai.*` | `ai.tool.list` / `invoke` | `ai.tool.register` + 用户审批（AR-06） | 受 DC-26 / §6.2 约束 |
| `audit.*` / `daemon.*` | `audit.tail` / `audit.export` / `daemon.health` / `daemon.version` | `audit.read`；`daemon.*` 无（本机 peer 即授权） | 否 |

- **审计与边界**：全部写操作与授权变更记 DC-34 审计（操作者 = 插件 / CLI / IDE）；Local API 只在本机，远程 attach（P5，OQ-18）须 mTLS + 显式授权，**v1 不提供**（AR-11）。

### 3.10 CLI 与 headless / CI 脚本化

1. `termai daemon`：无 GUI 运行 sessiond + Local API；**headless 不依赖 WebView**（AR-02）。
2. **`--json` 全命令支持**（DC-24）：`termai run --json -- <cmd>`、`termai log export --format jsonl`、`termai plugin list|install|revoke --json`、`termai config get|set|eval --json`（AR-09）。
3. **退出码稳定契约**：0 成功 / 1 用法错误 / 2 业务失败 / 3 权限拒绝 / 4 版本不兼容，CI 可依赖。
4. **无 TTY 语义与 CI 用途**：不弹窗、不阻塞，但 **L2/L3 确认在无 TTY 下必须拒绝执行**（确认不可关闭，AR-06 / §0，headless 不得「跳过确认默认放行」）；插件作者在容器内用 `termai plugin test` 跑确定性伪 PTY + 配额/越权断言，市场审核在隔离沙箱内复用同一 CLI。

### 3.11 IDE 集成

- **唯一集成面 = Local API**（DC-24）。VS Code / JetBrains / Neovim 客户端**都是普通消费者**（可由社区以插件形式实现），**不维护 N 套私有协议**。
- IDE 客户端不因「是 IDE」获得特权：其 capability 走与插件相同的授权 UX（§3.5），越权同样拒绝并审计。
- 终端内嵌 IDE 场景（如 VS Code terminal）**反向使用同一 Local API**；IDE 侧薄客户端可自行聚合调用，但聚合逻辑不得成为第二套契约的事实来源。

### 3.12 脚手架与 TTFHW

| 命令 | 职责 |
| --- | --- |
| `termai plugin new <id> --lang ts\|rust --template command\|panel\|ai-tool` | 生成项目、`plugin.toml`、CI workflow（build/test/sign/publish） |
| `termai plugin dev` | watch + 热重载（**仅 dev：组件重新实例化 + 状态丢弃**）、日志、配额与 capability 实时视图 |
| `termai plugin test` | 单测 + 确定性伪 PTY 集成 + 配额 / 越权 / 降级渲染断言 |
| `termai plugin publish` | 打包 + SBOM + SPDX + 签名 + 上传（§3.13） |

- **TTFHW ≤5min 定义**：从 `plugin new ts` 到 hello world 命令在真实终端中可见；三平台 CI 成功率 ≥95%（§8.2 / §7 P3）。
- **热重载 p95 ≤300ms**；**生产禁止热重载**，避免「只有开发态正确」的插件进入市场（角色09 §3.4）。
- **P2 末阶段门禁**：若 TTFHW >5min 或热重载 p95 >300ms，则启用**每插件 V8 isolate 第二运行时**（§7 阶段门禁 / R10）。

### 3.13 发布与分发链路

1. **包格式**：`.termaiplugin`（tar.zst）= `plugin.toml` + 组件/可执行 + assets + CycloneDX SBOM + SPDX 清单 + 签名 + `market.toml`（展示元数据，不参与信任决策）。
2. **签名**：Ed25519 发布者密钥，可选 sigstore keyless（OIDC 身份绑定）；宿主在安装与**每次加载前**校验；**校验失败 100% 拒装 / 拒载**（§7 P3）。
3. **SBOM 与许可**：SBOM 强制；市场强制 SPDX 扫描；**链接边界 GPL/AGPL/SSPL 依赖 = 0**，弱 copyleft（MPL-2.0/EPL-2.0）准入与「链接边界」可执行定义见 ADR-0015（§8.1-5 / AR-21）(ADR-0013)。
4. **分级审核**：L1 自动（签名 + SBOM + 静态扫描 + capability 声明比对）→ L2 人工（新增高危 capability：`exec` / `net` / `secrets` / `subprocess`，或 Verified 申请）→ L3 强审核（subprocess 白名单、企业 registry 上架）；拒绝理由公开（SLA 见 OQ-06-02）。
5. **透明度日志**：append-only、可公开验证（Sigstore Rekor 式）；记录发布 / 撤销 / 审核结论哈希，发布者身份与包哈希可离线验证。
6. **远程吊销 ≤1h**：撤销列表经 TUF 元数据下发，客户端每 1h 检查（+ 推送加速）；命中即禁用并提示（R9）。
7. **商业条款与信任等级**：**v1 零抽成、无 DRM**（AR-21 第 4 条不可变商业承诺），收入来自企业私有 registry、Verified 认证、合规能力与官方托管/支持服务，抽成变更需 TSC 决议 + 90 天公示且不得追溯已发布插件；信任等级 Official（第一方）> Verified（身份核验 + 强审核）> Community（普通审核）> Untrusted（side-load，禁用 subprocess/secrets），UI 必须显示。**服务端与市场实现开源后，L1/L2/L3 审核、包签名与 TUF 吊销仍为强制项**（开源不得作为跳过审核或放宽签名的理由）(ADR-0013)。

### 3.14 兼容策略

| 机制 | 规则 | 门禁 |
| --- | --- | --- |
| N-2 minor | host 可加载声明 N-2 的插件；minor 只增不删 | N-1/N-2 样本集功能测试通过率 ≥95% |
| deprecation | Tier1 变更需 RFC + 2 maintainer + **6 个月公告期**，期间新旧 API 并存并输出结构化 warning | 公告期与迁移文档随版本发布 |
| codemod | 每个破坏性变更随版本提供 codemod | N-1 样本自动迁移成功率 ≥90% |
| 能力缺失 / 版本不匹配 | 返回结构化错误 `CapabilityUnavailable`，**禁止 panic**；`engines.termai` 不满足 → 安装期拒绝 + 明确提示 + 加载期二次校验 | 宿主不因未知字段崩溃；100% 拒绝路径有测试覆盖 |

破坏性 WIT 变更 = major，需 RFC + 2 maintainer（DC-40）；删除任何 Tier1 扩展点等同破坏性变更。

### 3.15 官方标杆插件与生态度量

**第一方标杆插件**（Official 等级，独立 semver，与核心同签名根）：① **git**（分支 / 提交 / 冲突诊断）；② **k8s**（context / namespace / pod 诊断，只读优先）；③ **docker**（容器 / 镜像 / 日志）；④ **AI tool 包**（结构化诊断工具集，工具输出标记 untrusted）。

| 度量 | 目标 | 来源 |
| --- | --- | --- |
| 过审第三方插件数（1.0 时）/ 提供 AI tool 的插件数 | ≥50 / ≥10 | §8.2 / 角色09 §7 |
| 外部贡献者 commit 占比（6 个月内） | ≥20% | 角色09 §7 |
| TTFHW（三平台 CI）/ 热重载 p95 | ≤5min 且成功率 ≥95% / ≤300ms | §8.2 / R10 |

## 4. 接口与依赖

### 4.1 对内模块（依赖方向，CI 强制）

| 模块 | 依赖方向 | 输入 | 输出 / 承诺 | 不变量 |
| --- | --- | --- | --- | --- |
| `plugin-host` | `core ← plugin-host`（DC-21） | termai-ipc 只读事件流；Local API 客户端 | 装饰提交、注入队列条目、审计事件、崩溃报告 | 不链接 AI SDK；不持有网络句柄（`net` 经宿主代理）；**不写 PTY** |
| `plugin-runtime-{wasm,subprocess}` | plugin-host 内 | 组件字节 / 白名单可执行 + argv/env + capability set + 配额 | 调用结果 / stdio + 退出码 / 结构化错误 | wasm：每插件独立 store、无 ambient authority；subprocess：仅 trusted、stdio 由宿主接管 |
| `plugin-sdk-ts`（`@termai/plugin-sdk`） | 叶子 | WIT 生成物 | TS 类型 + 薄封装 + 测试工具 | 与 WIT 哈希一致性由 CI 校验 |
| `plugin-ui-sdk` | L4（web-shell 之上） | token 生成物 + view schema JSON Schema | TS 类型 + 组件白名单 | 只生成不手写；禁止内联色值 |
| `tokens` | 叶子（DC-09） | `tokens/*.json` | TS / CSS / Rust | 插件只能引用 token 名 |
| Local API server（sessiond 内） | sessiond | JSON-RPC 请求 | method 结果 + 审计 | peer credential 校验；apiVersion 强校验 |

### 4.2 对外契约

| 契约 | 版本载体 | 兼容窗口 | 消费者 |
| --- | --- | --- | --- |
| WIT `termai:plugin` | package semver + `engines.wit` | N-2 minor；major 需 RFC + 6 个月 | 插件作者、SDK 生成器 |
| `plugin.toml` | JSON Schema（由 WIT 生成） | 只增字段；未知字段忽略 + 告警 | 宿主、市场、脚手架、CI |
| 插件 view schema | JSON Schema（由组件白名单生成） | Tier1 冻结 | WebView 后端 + 原生降级后端 |
| Local API | `apiVersion`（握手 + 每请求） | N-2 minor | 插件宿主、CLI `--json`、headless、CI、IDE |
| 插件包 `.termaiplugin` | 格式版本 + 签名 + SBOM | 格式 major 需迁移器 | 宿主、市场 |
| 市场 / TUF 元数据 | TUF role + 签名 | 向前兼容，可离线验证 | 更新器、吊销链路 |
| 审计事件 schema | 版本化 JSONL（DC-34） | append-only | SIEM 导出、企业合规 |

## 5. 验收标准（引用 HARNESS §5 与 §8）

| 类别 | 验收标准 | 来源 |
| --- | --- | --- |
| 性能预算 | 插件宿主空载 RSS **≤80MB**（仅启用插件时计入，不计入核心 120MB 基线）；`--no-plugins` 下归零 | §5 / DC-38 |
| 性能预算 | 冷启动到可输入 P95 ≤150ms；插件全开 vs 全关的输入延迟 P99 差异 ≤0.5ms | §5 / 角色09 §7 |
| 性能预算 | 24h RSS 斜率 <1MB/h（含宿主）；安装包 <60MB；组件体积 ≤3MB；Tier1 host 调用 p99 ≤1ms；装饰单帧 ≤2ms | §5 / R10 |
| DX | **TTFHW ≤5min**（`plugin new ts` → hello world 命令可见），三平台 CI 成功率 ≥95%；`plugin dev` 热重载 p95 ≤300ms（未达标则 P2 末启用 V8 isolate 第二运行时） | §8.2 / §7 阶段门禁 / R10 |
| DX | Tier1 WIT 函数 100% 有生成文档与可运行示例；每个扩展点 ≥1 示例 | 角色09 §7 |
| 隔离 | 注入 100 次 trap/panic/OOM，核心会话存活率 100%、无数据丢失；崩溃隔离 100% | §8.2 / 角色09 §7 |
| 隔离 | 插件逃逸用例 **0 成功**；越权读 SSH key 或出网拦截率 100%；CSP 拦截 100% | §8.1-6 / 角色08 §7 |
| 配额 | 超 fuel / 内存 / 超时 / IPC 速率用例 100% 被中止且 UI 有提示，会话不受影响 | 角色09 §7 |
| 安全 | 签名校验失败 **100% 拒装**；100% 出站（含插件出站）经分类闸门、出网密钥命中 = 0；越权 capability 调用 100% 拒绝并写审计、审计单字节篡改 100% 可检出 | §8.2 / DC-34 / 角色08 §7 |
| 发布 | `plugin publish` 到市场可见 ≤5min；远程吊销下发 **≤1h**；SBOM（CycloneDX）100% 覆盖且链接边界 GPL/AGPL = 0；任一 stable 版本 ≤15min 全量回滚 | 角色09 §7 / §8.1-5 / §8.2 |
| 兼容 | N-2 minor 插件在最新 host 上功能测试通过率 ≥95%；codemod 自动迁移成功率 ≥90% | DC-40 / 角色09 §7 |
| 生态 | 1.0 时 **≥50 个过审第三方插件**、**≥10 个提供 AI tool**；6 个月内外部贡献者 commit 占比 ≥20% | §8.2 / 角色09 §7 |
| 可访问性与门禁 | Tier1 view 100% 声明 role/标签/焦点序，UI 层 6 条核心流程 NVDA/VoiceOver 可达；§8.1 六件套全绿 | AR-20 / §8.1 / §8.2 |

## 6. 风险与缓解

| 风险 | 触发条件 | 缓解 | 残余风险 |
| --- | --- | --- | --- |
| 插件供应链投毒（R9） | 签名异常或恶意插件报告 | 签名 + 透明度日志 + 能力最小化 + 分级审核 + 远程吊销 ≤1h | 维护者密钥被盗 |
| WASM DX 劝退作者（R10） | TTFHW >5min 或热重载 >300ms | AOT 预编译 + 实例池 + 组件 ≤3MB + V8 isolate 第二运行时门 | componentize-js 调试器弱、npm 原生模块不可用 |
| 权限断崖被误用 / 权限疲劳 | 用户盲目标记 trusted；首次授权弹窗过多 | 红色徽章 + 二次确认输入 id + side-load 禁 subprocess/secrets + 审计；profile 打包 + 一次向导 + 已授权静默沿用 | 用户仍可能盲目信任；新增 capability 仍需逐条授权 |
| 装饰 API 表达力不足 | 连续两季度 Top3 请求受阻 | 季度评审 Top10 + Tier2→Tier1 升格机制 | 升格需再走红队与兼容评估 |
| 插件宿主常驻内存 | 空载 >80MB | 按需启动 + 空闲退出 + 独立进程预算（DC-38） | 首次激活存在冷启动成本 |
| WIT 契约被内部需求侵蚀 | Tier1 变更无 RFC | RFC + 2 maintainer + 6 个月公告 + codemod | 内部进度压力 |
| Local API 攻击面扩大 | 同机恶意进程连接 socket | peer credential 校验 + `0600`/ACL + capability 最小化 + 审计 | 同 uid 同权限进程无法区分 |
| 生态冷启动 | 1.0 时 <50 插件 | 官方 4 个标杆插件 + 双首屏模板 + 市场零抽成 + 企业 registry | 需持续投入与社区运营 |

## 7. Open Questions

> 以下为 HARNESS 未覆盖或未明确的重要空白。**本规格不自行裁决**；每条给出影响面、建议与决策阶段。

| # | 问题 | 影响面 | 建议 | 决策阶段 |
| --- | --- | --- | --- | --- |
| OQ-06-01 | 远程/Web 形态（P5）下插件在何处执行：服务端 / 浏览器 / 双端？ | Local API 是否必须成为远程 API、插件沙箱模型、AR-11 云边界与 OQ-18 的 IPC 预留 | P1 规划期仅在 Local API 预留 `attach` 命名空间与鉴权字段，不实现；排除双端执行（与 §6.1-3 本地优先冲突） | P1 规划 |
| OQ-06-02 | 市场「分级审核」的分级标准、审核人日 / SLA、申诉流程未定义 | 发布链路（§3.13）、生态冷启动（≥50 插件）、法务责任 | L1 全自动；L2 高危能力人工 ≤72h；L3 subprocess ≤7 天；标准与拒绝理由公开 | P3 前 |
| OQ-06-03 | 主题（纯数据，DC-12，永久免费）是否与插件市场共用发布 / 签名 / 审核与信任等级？ | DC-12 / AR-21 落地、市场分类、审核成本、视觉一致性 | 共用签名与透明度日志，跳过 capability 审核，独立频道与独立信任等级 | P3 |
| OQ-06-04 | 离线 / 气隙环境的吊销语义：≤1h 远程吊销依赖网络，气隙网如何获得撤销包、宽限期与自动禁用策略未定义 | 企业版（P4）、R9、审计合规、离线可用性 | TUF 撤销包可离线导入 + 默认宽限 7 天 + 企业策略可缩短至 0；宽限期内 UI 明示 | P4 |
| OQ-06-05 | 插件崩溃 / 性能遥测的合规边界：宿主是否代传、是否继承用户 opt-in、是否含终端内容 | AR-12、插件质量、我方是否成为数据处理者 | 宿主仅代传结构化崩溃签名与配额事件，**继承用户 opt-in**，绝不含终端内容；作者自建遥测须独立声明 | P2 |
| OQ-06-06 | 插件 AI tool 的 destructive 分类由谁裁决（角色09 OQ-7 未决） | DC-26 risk 字段、AR-06 的 L2/L3 门禁、「L2/L3 未审批执行 = 0」 | risk 由作者声明 + 宿主静态校验（工具名 / 参数 schema / 所需 capability），冲突取**更高**风险，作者不可下调 | P2 |
| OQ-06-07 | 插件允许的 SPDX 许可白名单；调用外部 GPL 可执行文件（subprocess 形态）是否受 AR-21 链接边界约束 | 市场审核、法务、生态规模、AR-21 链接边界口径（ADR-0015） | 插件代码许可须为 OSI 允许且非 AGPL；调用外部 GPL 可执行文件不构成链接，允许但须在清单中明示 | P3（法务确认） |
| OQ-06-08 | 官方标杆插件的发版节奏与主版本绑定关系（随核心 vs 独立 semver） | DC-40 兼容窗口、Official 信任等级定义、标杆插件的「永远可用」承诺 | 独立 semver + 同签名根 + Official 等级，但必须满足与第三方相同的 N-2 门禁 | P3 |

---

**变更记录**：
- 许可口径修订（ADR-0013）：§2 约束 18 改为 AR-21 口径（全栈开源、Apache-2.0 OR MIT）；§3.13 第 3、7 条补充 SSPL/ADR-0015 与「服务端开源不降低审核与签名要求」；§7 OQ-06-03 / OQ-06-07 的 AR-10 引用改为 AR-21。**保留不变**：v1 公共市场零抽成、无 DRM；企业私有 registry / Verified 认证为收入来源；主题永久免费（DC-12、AR-21 不可变商业承诺）。
