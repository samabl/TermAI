# 05 · 安全、隐私与合规规格
> 上游权威：HARNESS.md（唯一终裁）。本文是其下沉规格，不新增与 AR-xx / DC-xx / §5 预算 / §8 门禁 / §10 Anti-features 冲突的结论；角色原文冲突一律以 HARNESS 为准（差异见 §2.4）。
> 证据来源：docs/roles/08（主）、06、04。编号：SEC-R 需求 / SEC-A 攻击树 / SEC-IF 接口 / SEC-AC 验收 / SEC-RK 风险 / OQ-S 待决项。

## 1. 目的与范围

### 1.1 目的
在实现层定义客户端、termai-agent、plugin-host 与云控制面之间的**安全边界、隐私默认值与合规义务**，使「终端是信任基础设施」（§1.2 A1）与「默认拒绝 / 默认 dry-run / 默认本地 / 默认不上传」可被机器验证；安全结论在冲突中优先于自动化率、便利与遥测价值。

### 1.2 范围内
1. 威胁模型：STRIDE 表 + Top3 攻击树 + 信任边界与资产清单（全局）。
2. 授权：capability 令牌粒度、策略合并、签发/校验/吊销（Policy Engine）。
3. AI 执行安全：七条黄金规则（§6.2）的实现落点与检查点（执行器 + watchdog）。
4. 数据保护：脱敏管线、出站分类闸门、L0–L3 分级（Context Builder）。
5. 密钥与可审计：OS keychain 后端、mLock/zeroize、审计哈希链与 SIEM 导出。
6. 供应链与 SDLC：SBOM、签名/TUF、模型权重校验、许可白名单、fuzz/渗透/漏洞 SLA。
7. 合规：GDPR / SOC 2 / 企业 MDM / 数据驻留映射。

### 1.3 范围外
1. AI 意图 schema 与工具 ABI 字段定义 → 归 DC-26 与 AI 规格；本文只约束其安全属性（能力分离、审批挂载点）。
2. VT/PTY 兼容性与图形协议限额 → 归 AR-18 与内核规格（本文只要求其作为第一类不可信输入进 fuzz，DC-37）；云控制面 API 形状、SLO 与部署拓扑 → 归后端规格（本文只给数据边界与闸门规则）；正式认证审计归法务/GRC。

### 1.4 责任共担
1. **TermAI**：默认安全值、脱敏与闸门、审计链、插件与模型供应链、更新签名；**不**对用户主动粘贴进 prompt 的机密与用户自建 MCP server 的行为负责。
2. **用户**：审批判断、密钥保管、敏感会话标记；不承担默认值安全责任（默认必须安全）。
3. **企业客户**：MDM 策略内容、SIEM 归档、内部 L2/L3 口径。
4. **云控制面**：控制面租户隔离与计费计量；**永不**持有 PTY 流、文件内容、命令输出、密钥材料、prompt/completion（AR-11）。

## 2. 需求与约束

### 2.1 不可协商红线（违反即架构事故，PR 直接拒绝）
| # | 约束 | 依据 | 唯一强制点 | 验证 |
| --- | --- | --- | --- | --- |
| 1 | 无人类显式批准，AI 不得执行破坏性/外发型/提权/跨主机操作 | AR-06、§6.1-1 | 执行器 + 审批收据校验 | SEC-AC-02 |
| 2 | secret 永不进入模型上下文、日志与遥测；脱敏在上下文构建器完成 | §6.1-2、DC-33 | Context Builder 单一收口 | SEC-AC-01/13 |
| 3 | 默认本地：scrollback/历史/文件默认不出设备，上传须 opt-in 可撤销 | §6.1-3、AR-11、AR-12 | 出站分类闸门 | SEC-AC-07 |
| 4 | 破坏性与外发型确认不可由配置关闭；企业策略只能加强 | AR-06、DC-35 | Policy Engine 策略合并 | SEC-AC-14 |
| 5 | 插件不得写 PTY/输出流、不得帧内绘制、不得持有 Node/原生句柄 | AR-07、DC-39 | 宿主 API 能力边界 | SEC-AC-04 |
| 6 | 核心链接边界内 GPL/AGPL/SSPL 依赖 = 0；弱 copyleft 按白名单分级准入（ADR-0015） | AR-21 | 链接边界分类器 + cargo-deny / SPDX | SEC-AC-09 |
| 7 | 服务端 prompt/completion 字段 = 0 | AR-11 | CI 静态字段扫描 | SEC-AC-08 |
| 8 | 遥测默认关闭、内容零采集；崩溃上报独立开关且端点归属明确 | AR-12、ADR-0015 §D9 | 遥测 SDK + schema + 端点归属 | SEC-AC-07/18/23 |

### 2.2 需求条目
| ID | 需求 | 依据 |
| --- | --- | --- |
| SEC-R-01 | 100% 出站请求（模型/云/插件网络/MCP）经分类闸门并可在 UI 回看 | AR-12、§8.2 |
| SEC-R-02 | capability 默认拒绝；未声明能力一律拒绝；未知工具/MCP/插件默认 Untrusted | DC-35、DC-28、OQ-09/12 |
| SEC-R-03 | 脱敏失败默认**拒绝发送**并告知用户；不提供「原样发送」开关 | DC-33 |
| SEC-R-04 | 100% AI 动作与人工危险动作可审计；append-only + 本地哈希链；可导出 SIEM | DC-34、P2 退出条件 |
| SEC-R-05 | 模型视为供应链：权重哈希 + 分发通道签名 + 离线校验 | DC-36 |
| SEC-R-06 | VT/PTY/IPC/插件消息持续 fuzz，24h 无 crash，历史 crash 全部回归 | DC-37、§8.1-6 |
| SEC-R-07 | 插件须签名 + 权限清单 + 独立进程隔离；远程吊销 p95 ≤1h | AR-07、DC-38、R9 |
| SEC-R-08 | 插件只读镜像 + 显式注入队列 + 声明式装饰；Tier2 iframe 门禁通过前不开放 | AR-07、AR-08 |
| SEC-R-09 | 密钥只存 OS keychain 或加密文件；mLock + zeroize；禁止明文落盘/日志/argv/env | §6.1-2、DC-33 |
| SEC-R-10 | 数据分级 L0–L3 + 标签只升不降的传播规则 | §6.3 |
| SEC-R-11 | 结构化元数据默认持久，完整原始输出默认不持久；项目记忆不提交 git | AR-13、DC-30、OQ-04/13 |
| SEC-R-12 | 企业 MDM 策略可离线校验、只能加强、不可关闭红线 | AR-06、OQ-17 |
| SEC-R-13 | 漏洞 SLA：Critical 24h 缓解 / High 7d / Medium 30d 修复 | 08 §3.5（HARNESS 未覆盖，见 OQ-S4） |
| SEC-R-14 | SAST/DAST/依赖审计每 PR；季度外部渗透；Tier2 开放前红队 | AR-08、§8.1 |
| SEC-R-15 | 合规：GDPR 最小化/DSAR/DPIA、SOC 2 证据自动化、MDM 下发、数据驻留租户级、市场 SPDX 扫描 | 08 §3.5、OQ-15 |
| SEC-R-16 | 安全机制不得加剧 §5 预算（网关 p95 <120ms、诊断首 token <3s、空闲 RSS ≤120MB） | §5、AR-19 |
| SEC-R-17 | 链接边界按 ADR-0015 分类：同进程 / 并入 / 派生 = 界内；构建期与运行时 subprocess = 界外（须满足 BT/T/AE 条件） | ADR-0015 §D1/D2、AR-21 |
| SEC-R-18 | SPDX 白/黑名单 + fail-closed 表达式求值：未知 / NOASSERTION / 黑名单 = 拒绝；MPL-2.0 准入、LGPL 仅动态链接、EPL/CDDL 逐案审批 | ADR-0015 §D3/D4 |
| SEC-R-19 | 发布签名密钥：离线根 2-of-3 + 云 HSM 子键 2-of-3，仅签哈希；轮换根 ≤24 月 / 子键 ≤12 月；泄露 ≤1h 吊销 | ADR-0015 §D5、DC-36 |
| SEC-R-20 | 插件权限最小化（禁通配）+ 市场强制 SPDX 元数据；捆绑 GPL 二进制须标 bundled-copyleft 并附 source offer | AR-07、ADR-0015 §D2 |
| SEC-R-21 | 每个发布产物附 SBOM + provenance attestation；SLSA Build L2 为地板、L3 为 P3 目标；缺一阻断发版 | ADR-0015 §D6 |
| SEC-R-22 | 审计本地默认 90 天（可配 7–3650 天，红线记录 ≥30 天）；外锚用户/企业自持为默认，第三方时间戳 opt-in | ADR-0015 §D7、DC-34 |
| SEC-R-23 | 崩溃上报端点由控制面团队运营；原始事件 30 天 / 聚合 90 天；客户端脱敏主责 + 服务端二次清洗 | ADR-0015 §D8、AR-12 |

### 2.3 关联的既有预算与门禁（只引用，不重定义）
1. **发布门禁（§8.1）**：依赖与许可（GPL/AGPL 链接边界 = 0、SBOM、cargo-deny/cargo-audit/npm audit）、fuzz 24h 无 crash 且插件逃逸用例 0 成功。
2. **AI 安全（§8.2）**：L2/L3 未审批执行 = 0、注入用例拦截 ≥95%、出网密钥命中 = 0、100% 出网可在 UI 回看。
3. **隐私与性能**：默认配置抓包零用户未发起连接、遥测关闭时上传 0 字节、服务端 prompt/completion 字段 = 0、P4 退出条件含越权用例 0 通过与审计可导出 SIEM（§8.2、§7）；同时安全机制不得劣化 §5 预算（网关附加延迟 p95 <120ms / p99 <300ms、诊断首 token P95 <3s、空闲 RSS ≤120MB、插件宿主空载 ≤80MB，DC-38）。

### 2.4 与角色原文的差异登记（以 HARNESS 为准）
1. 08 主张 Apache-2.0/MIT 双许可 → 原 AR-10 曾要求**单一许可证**；**该项已被发起人裁决推翻**：AR-21 / ADR-0013 定为 **Apache-2.0 OR MIT 双许可 + 全栈开源**（OQ-01 已关闭）。本文按双许可处理（ADR-0015）。
2. 06 主张云端 BSL 1.1 → 原 AR-10 不采用 BSL/SSPL，改为闭源服务；**该商业形态部分已被 AR-21 取代**：云端服务源码同样公开，而 BSL/BUSL/SSPL 仍在链接边界黑名单（ADR-0015 §D4）。
3. 08 主张危险动作「逐条批准」→ AR-06 采用分级摩擦 + 二次输入，否决逐条弹窗与零确认。
4. 08 认为「WASM 即沙箱」不成立 → AR-07 定为进程隔离 + 签名 + 权限清单，WASM 仅作进程内子隔离层。

## 3. 详细设计

### 3.1 威胁模型

#### 3.1.1 资产与信任边界
**资产**：SSH 私钥/ssh-agent/kubeconfig/云 token/连接串（最高价值目标，仅经 handle 引用）；PTY 输出、文件内容、命令输出与报错（**不可信输入**，可能含 secret）；模型输出、工具返回值、MCP 返回（**不可信指令源**，数据与指令必须分离）；插件代码（**不可信代码**）；审计链、策略文件、更新元数据（完整性靠哈希链/签名）；模型权重、prompt 模板、发布签名密钥（供应链信任根）。
**信任边界**：可信基 = 内核 + 渲染 + 密钥服务 + 执行器（仍须内存安全与 fuzz）；一切外来文本跨边界即降信任；云 = 控制面可信、数据面零信任（AR-11 硬底线）；OS 平台 API 受信但版本可变（AR-02）。

#### 3.1.2 STRIDE 表
| 威胁 | 典型场景 | 缓解 | 落点 | 依据 |
| --- | --- | --- | --- | --- |
| Spoofing | 插件冒充官方；伪造模型端点；伪造 MCP server | Ed25519 签名 + publisher 身份；端点证书固定；MCP 默认拒绝 | 分发 + Provider 层 | SEC-R-02/05/07 |
| Tampering | 篡改审计日志、插件更新、企业策略文件 | 哈希链 + Merkle checkpoint；TUF 元数据（含防回滚单调性）；签名策略 | 审计 + 更新器 | SEC-R-04/12 |
| Repudiation | 「是 AI 自己删的库」 | 不可抵赖审计：操作者（人/插件/模型）+ 审批人 + 审批收据哈希 | 审计服务 | DC-34 |
| Information Disclosure | scrollback 中的 token 进入 prompt 或被上传；日志泄密 | 出站分类闸门 + 脱敏管线 + 默认本地 | Context Builder | SEC-R-01/03 |
| DoS | 超长行、恶意 ANSI、Sixel 炸弹打爆解析或渲染 | 解析配额、内存上限、图形协议限额、fuzz 回归 | 内核 | DC-37 |
| Elevation of Privilege | 插件越权读文件/出网/调 shell；AI 子进程提权 | capability 默认拒绝 + 进程隔离 + 最小权限子进程 | Policy Engine + watchdog | DC-27/35 |

#### 3.1.3 Top3 攻击树

**SEC-A1｜注入诱导外传**（目标：L3 凭据出设备）
1. 攻击者污染不可信文本（README、docker logs、网页、MCP 返回）植入指令 → 模型产出 shell.exec 意图读取凭据并外发。
2. **阻断 A/B（意图 + 污点）**：只接受结构化意图，拒绝 shell -c 拼接；AST 预分析识别「读凭据 → 网络外发」；不可信值不得成为 argv/路径/重定向目标。
3. **阻断 C/D（能力 + 审批）**：AI 子进程默认无网络，net 能力逐主机授予；外发型为 L3，必须确认 + 二次输入目标主机名（AR-06）。
4. **阻断 E（闸门）**：即使凭据进入上下文，脱敏与出站分类闸门仍拦截（SEC-R-03）；检出靠注入回归集（拦截 ≥95%，§8.2）；残余：新型注入绕过 AST 与语义分类，靠红队迭代。

**SEC-A2｜插件窃取凭据**
1. 恶意或被投毒插件安装后请求 fs 读写 ~/.ssh、secret 通配、net 通配。
2. **阻断 A（准入）**：权限清单静态校验 + 人工审核（宽能力必审）+ 最小化规则（SEC-R-20/AR-07）。
3. **阻断 B/C（隔离）**：plugin-host 独立进程 + 每插件独立 wasm store（DC-38），宿主不继承密钥与 FS 句柄；WASI 零 ambient authority，能力经宿主代理且无通配，网络逐目的地校验。
4. **阻断 D（运行期）**：3 次崩溃进 safe mode；越权即时审计并吊销；检出指标为逃逸用例 0 成功、越权读 key/发包拦截率 100%（§8.1-6）；残余：宿主 API 逻辑漏洞（唯一攻击面，靠 fuzz + 红队）。

**SEC-A3｜供应链投毒**
1. 攻击者在依赖、插件包、更新元数据或模型权重中植入恶意内容。
2. **阻断 A（构建）**：链接边界分类器 + SPDX 白/黑名单 + cargo-deny / npm audit + CycloneDX SBOM（§8.1-5）；边界内 GPL/AGPL/SSPL = 0，构建期与运行时进程外工具须满足 BT/T 条件（ADR-0015 §D2）。
3. **阻断 B/C（分发）**：发布与插件 Ed25519 签名 + TUF（root/timestamp/snapshot/targets，离线 root），签名失败 100% 拒装（P3）；透明度日志 append-only，可远程吊销（p95 ≤1h，R9）。
4. **阻断 D（运行时物料）**：模型权重 SHA-256 清单 + 通道签名 + 加载前离线校验，禁用不安全反序列化格式（§3.8）；残余：维护者签名密钥被盗（缓解见 SEC-RK-03 与 ADR-0015 §D5 的离线根 + 2-of-3 门限）。

### 3.2 Capability 模型（默认拒绝）

#### 3.2.1 令牌粒度
一条 capability 是**不可再分的（主体 × 资源 × 动作 × 约束）**四元组，CBOR 编码 + Ed25519 签名，与 termai-ipc/IDL 同版本节奏（AR-04）。

| 字段 | 说明 | 示例 |
| --- | --- | --- |
| sub | 主体：user / plugin:<id>@<ver> / agent:<session> / mcp:<id> / subproc:<name> | plugin:foo@1.2.0 |
| act | 动作：fs.read、fs.write、shell.exec、net.connect、pty.mirror、stdin.write、secret.use、audit.read、ui.declare | fs.read |
| res | 资源：路径 glob / 主机名 / 工具名 / secret handle / session id | /ws/proj/** |
| cst | 约束：TTL、一次性、速率、脱敏级别、主机白名单、taint 上限 | ttl=900s |
| apr / dlg | 审批凭据哈希（审批收据或企业策略条目 id）/ 是否可委派（默认否，委派不得超出原集） | sha256:… / false |
**禁通配**：不支持 net/fs/secret 通配；网络能力必须是显式主机白名单；无法静态判定时按「不可授予」处理（默认拒绝，DC-35）。

#### 3.2.2 签发 / 校验 / 吊销
| 环节 | 规则 |
| --- | --- |
| 签发 | 仅 **Policy Engine**（termai-agent 内）签发；输入 = 企业策略 ∩ 用户授权 ∩ 会话上下文 ∩ 审批收据；L2/L3 令牌与 plan_hash 绑定且一次性 |
| 校验 | 三个**强制检查点**：sessiond 的 IPC/PTY 入口、plugin-host 宿主 API、Local API broker（DC-24）；每次调用校验，不接受「发一次长期有效」，失败即 deny + 审计 |
| 令牌传递 | 不落盘、不进日志、不跨进程暴露明文（经 UDS 句柄传递）；进程退出即失效 |
| 吊销 | ① 会话级立即（用户撤销/会话关闭）；② 设备/用户级本地 ≤30s；③ 插件/版本级远程吊销 p95 ≤1h（R9）；吊销源 = 用户 UI、Policy Engine、registry CRL、企业 MDM |
| 离线 | CRL 不可达时启用宽限期（**提案 72h**，OQ-S9）；超期拒绝**新增**高权限能力，存量能力按 TTL 自然过期 |

#### 3.2.3 策略合并与企业覆盖规则
1. **取严原则**：有效策略 = 授权取**交集**、拒绝取**并集**；任一层的 deny 不可被下层放宽（deny-override）。
2. **层序**：builtin 默认 < 用户 < 项目（.termai/）< 企业（MDM/策略文件）< 运行时单次请求；企业层**只允许收窄能力集与提升审批档位**。
3. **与 AR-09 的关系**：配置合并仍遵循 AR-09（default < system < user < project < env < CLI）；但**安全策略不适用 CLI/env 覆盖**，确认阈值与能力集由 Policy Engine 统一取严，避免强制开关削弱红线（AR-06 不可协商）。
4. **不可关闭项**：破坏性/外发型/提权/跨主机确认、脱敏、审计，任何层均不可关闭（§2.1）。

### 3.3 七条黄金规则（§6.2）的实现落点
| # | 规则 | 实现落点 | 机制细节 | 验证 |
| --- | --- | --- | --- | --- |
| 1 | 能力与意图分离 | 工具 ABI（DC-26） | 模型只产出 tool + args 并按 json_schema 校验；**执行器**把意图编译为进程调用；ABI 不接受字符串命令形态的工具 | CI 静态：无字符串命令通道 |
| 2 | 禁止 shell -c 拼接 | ArgvBuilder | 每个 argv 元素是带类型的值；拒绝含 NUL/换行的参数；用于**展示**的转义字符串只渲染、绝不回解析（防二次注入） | SEC-AC-01 |
| 3 | 默认 dry-run | 计划器 + 风险分级 | 生成 Plan（argv、影响面读/写/网络/提权、目标主机与路径、可否回滚）→ 渲染 → 需 plan_hash + 审批收据才执行；dry-run 与 exec 走同一构造路径 | SEC-AC-02 |
| 4 | 危险动作逐条批准 | 审批挂载点 | 风险分级 L0/L1/L2/L3/U（AR-06）：L2 每次 dry-run + 确认，L3 追加二次输入目标名/短语 + 不可回滚徽章；**L2/L3 不可被 rule/session 授权覆盖** | SEC-AC-02 |
| 5 | 污点追踪 | Context Builder → 执行器 | 不可信源标签（PTY 输出/文件/网页/工具返回/MCP）随值传播；不可信值不得直接成为 argv/路径/URL/重定向目标，须转义并标注来源；若同为 L3 来源则直接拒绝 | SEC-AC-03 |
| 6 | 最小权限执行 | watchdog 子进程（DC-27） | 默认**无网络**（Linux netns 空集 + Landlock/seccomp；macOS seatbelt；Windows AppContainer + Job Object）；FS 只读白名单；不转发 SSH_AUTH_SOCK/agent；env 与句柄白名单；提权独立审批且到期回收；超时杀进程树 | SEC-AC-04 |
| 7 | 可回滚 | 快照 + 撤销栈 | 写操作前置快照（工作区文件 + git 引用）；「撤销上次 AI 操作」**只覆盖工作区文件与 git 可逆操作**（AR-06）；UI 必须明示边界，禁止暗示容器/远端可回滚（Anti-feature 13） | SEC-AC-15 |
**AST 预分析（DC-27）**：进入执行器的脚本先做 shell AST 解析（sh/bash/zsh/fish/pwsh 子集），提取重定向目标、管道、命令替换、变量展开、通配、提权命令、网络工具（curl/wget/nc/ssh/scp）、破坏性工具（rm/mkfs/dd/truncate/chmod -R）与 eval/管道进 shell 的组合；**解析失败（动态构造、here-doc、eval）→ 风险上浮一档、按 U（L2 起步）处理并说明原因**；AST 只用于风险分级，**不得**当作安全性证明。

### 3.4 脱敏管线（阶段 RED-0…RED-6）
**唯一收口**：脱敏只在 Context Builder 内完成（DC-33），且是进入任何出站上下文、日志与遥测的**唯一入口**；其他组件不得有等价实现（CI 静态检查 + 依赖方向约束，AGENTS §3）。

| 阶段 | 动作 |
| --- | --- |
| RED-0 会话开关 | 会话/Pane 级「敏感会话禁 AI」与 AI 全局关闭；关闭时零上下文构造、零出站 |
| RED-1 特征检测 | 正则族：PEM 私钥头、云凭据（AKIA/ASIA 前缀）、GitHub token（ghp_/gho_/ghs_）、模型 key（sk-）、Slack（xox）、Stripe（sk_live_）、JWT（三段 base64url）、连接串（scheme://user:pass@host）、kubeconfig client-key-data、.env 中 KEY/TOKEN/SECRET/PASSWORD 赋值 |
| RED-2 熵检测 | 对长度 ≥20 的 token 计算 Shannon 熵，阈值 ≥4.0 bits/char；覆盖 base64/base32/hex 字母表；**接受误报**（08 §5.4） |
| RED-3 反向匹配 | 对 OS keychain/加密库中所有密钥值做精确匹配（Aho-Corasick）；在 mLock 缓冲区执行、索引随用随建、用后 zeroize、绝不落盘 |
| RED-4 替换 | 命中片段替换为**类型化占位符**（如 SECRET:PEM:1）；映射表仅存于本地受限进程；模型只见占位符，需真实值时经 secret://handle 受控引用 + secret.use 能力 + 审批 |
| RED-5 收据 | 产出脱敏收据（命中类别计数、规则版本、占位符数量）供闸门与审计使用；**收据本身不含原文**，随请求绑定 |
| RED-6 失败处置 | 检测器不可用、超时、规则集签名校验失败、管线异常 → **拒绝发送**并告知用户三个可执行动作：移除该片段 / 标记会话为禁止 AI / 手动改写；**不提供「原样发送」开关** |
**本地模型同样脱敏**（08 D3）：不依赖模型自律，不因目标是本地模型而降级；日志与审计复用同一管线，禁止打印命令全文与文件内容（AGENTS §6）。

### 3.5 出站分类闸门与数据分级
| 等级 | 数据 | 本地模型 | 云模型 | 遥测 | 日志 | 同步 |
| --- | --- | --- | --- | --- | --- | --- |
| L0 公开 | 版本、帮助文本 | 允许 | 允许 | 允许 | 允许 | 允许 |
| L1 内部 | 命令名、错误码（脱敏后） | 允许 | 允许 | opt-in | 脱敏 | 允许 |
| L2 敏感 | 路径、源码、主机名、业务日志 | 允许 | **仅逐次显式同意** | 禁止 | 脱敏 | 仅 E2E 加密可选备份 |
| L3 机密 | 凭据、私钥、token、客户数据 | **禁止入上下文** | **禁止** | 禁止 | 禁止 | 禁止 |
**闸门判定顺序**（每次出站：模型、云、插件网络、MCP、遥测）：① 解析目的地（domain + 传输 + 证书固定）；不可解析 → deny；② 取本次上下文所有数据项的**最高**分级标签（标签只升不降，派生数据继承来源最大值）；③ 校验脱敏收据的存在性与规则版本，缺失或不达标 → deny；④ 校验 capability（net.connect:host）与企业策略收窄后的有效集；⑤ L3 恒 deny、L2 需逐次同意记录、L1 需 opt-in 状态位；⑥ 记录审计（目的地、字节数、分类、判定、策略 id）后放行；⑦ UI 出站清单 100% 可回看，可一键撤销 opt-in（AR-12）。
**云边界**：控制面只承载身份/权益/同步小文档/路由计量/插件分发/遥测；**永不**持有 PTY 流、文件、输出、密钥、prompt/completion（AR-11）；服务端 prompt/completion 字段数 = 0（CI 静态强制）；遥测 schema 出现命令/路径/内容字段即阻断。

### 3.6 密钥管理
| 平台 | 主后端 | 降级路径 |
| --- | --- | --- |
| Windows | DPAPI + Credential Manager | 无（系统必备） |
| macOS | Keychain（SecItem） | 无（与公证/签名发布链对齐） |
| Linux 桌面 | Secret Service（libsecret/D-Bus） | libsodium secretbox 加密文件（0600）+ Argon2id 口令，**必须显式告警**，禁止静默降级为明文（08 §3.3） |
| 无桌面 / headless | libsodium 加密文件 | 必须用户口令（CI/服务器场景） |
**内存与生命周期**：mLock/VirtualLock 锁定页 + Rust zeroize 显式擦除；禁止经 Debug/panic/日志打印；**禁止经 argv/env 传递秘密**（同用户可读 /proc/*/environ），改用句柄或管道；持有秘密的进程禁用 core dump（Linux PR_SET_DUMPABLE=0、macOS 同族机制、Windows dump 排除）。生命周期：导入 → handle 化 → 使用（能力 + 审批）→ 轮换 → 撤销 → 销毁；handle TTL 到期失效，撤销同步吊销关联 capability。
**AI 可见性**：模型只见 secret://handle 与占位符；审计只记录 handle 与用途，不记录值；企业 KMS 只托管我方凭证，**不托管用户密钥**（AR-11、06 D10）。

### 3.7 审计链
**结构**：append-only 段文件（与 Session Log 同族，DC-23/AR-04），记录 hash = H(prev_hash ‖ canonical(record))；周期性 Merkle checkpoint；打开方式固定 O_APPEND，代码层**不存在**删除/截断 API（CI 断言）。

| 字段 | 说明 |
| --- | --- |
| seq / ts_utc / mono | 序号、墙钟、单调时钟（防时钟回拨） |
| actor | 类型（human/plugin/agent/model/mcp/system）+ id + 版本 + 签名指纹 |
| session_id / workspace_id | 作用域 |
| action | tool + verb + args_digest（**脱敏后参数的哈希**，永不存原文） |
| risk_tier / taint | L0/L1/L2/L3/U；是否含不可信来源及来源类别计数 |
| decision / policy_id | allow / deny / dry_run（附原因码）+ 生效策略条目 id（可回溯企业策略） |
| approver / receipt_hash | 审批人类型与 id；L3 记录二次输入校验结果（不记输入内容）；审批/脱敏收据哈希防事后替换 |
| result | status、exit code（不存输出正文）、egress{dest, bytes, classification}；附 prev_hash / hash / schema_version |
**诚实声明**：哈希链提供 **tamper-evidence，而非 tamper-proof**——本机管理员/root 可重写整链；缓解为外锚 + 只读介质双写；不得对外宣称「任何人都无法篡改」。外锚三法（ADR-0015 §D7）：① 用户/企业自持（企业自有 SIEM / WORM / S3 Object Lock，为默认且不经我方云）；② 本地导出签名 Merkle checkpoint 链（个人默认可用）；③ 第三方 RFC 3161 TSA / Rekor，仅提交 Merkle root 哈希（L0，opt-in）。锚定节奏 = 每 24h 或每 1000 条记录，先到先锚。
**导出**：Local API audit.export（DC-24，含 apiVersion）流式导出，支持 NDJSON / syslog / CEF / OCSF；企业可推送至自有 SIEM（默认**不**经我方云）；默认导出脱敏，导出未脱敏记录需本地管理员 + 显式二次确认并对导出留痕；**保留期（ADR-0015 §D7）：本地默认 90 天，用户可配 7–3650 天，红线记录（L2/L3 审批、出站拒绝、密钥使用、插件能力授予、吊销）下限 30 天且不可更短；外锚消费级默认关闭、企业 MDM 默认开启**。**覆盖范围**：AI 动作 100%（P2 退出条件）、人工危险动作、插件能力调用、策略变更、密钥使用、出站拒绝、吊销事件。

### 3.8 供应链安全
| 对象 | 完整性机制 | 校验时机 | 失败动作 |
| --- | --- | --- | --- |
| 客户端发布产物 | 分离签名（Ed25519，云 HSM 子键 2-of-3，仅签哈希）+ 更新元数据签名（TUF） | 下载后、安装前 | 拒装 + 告警 |
| 插件包 | Ed25519 签名 + manifest_hash + SPDX 扫描 | 安装与每次加载（按版本/哈希） | 100% 拒装（P3 退出条件） |
| 更新元数据 | TUF：离线 root（FIPS 140-2 L3、2-of-3 门限、新旧根双签轮换）/ timestamp / snapshot / targets + 版本单调性防回滚 | 每次检查更新 | 判定回滚攻击 → 拒绝并告警 |
| 模型权重 | 权重清单 SHA-256 + 分发通道签名 + 离线校验 | 加载前 | 拒绝加载；不降级为「仅警告」 |
| prompt 模板 | 随模型 bundle 签名 | 加载前 | 拒绝使用 |
| SBOM | CycloneDX，随 release 产物发布 | CI 生成 + 归档 | 缺 SBOM 阻断发布 |
| provenance attestation | in-toto（slsa.dev/provenance/v1）+ DSSE + Sigstore bundle + Rekor 登记；与我方 HSM 手签 manifest 构成双信任根（ADR-0015 §D6） | CI 发布前用 slsa-verifier 独立校验 | 缺失或校验失败 = 阻断发版（SLSA Build L2 地板，P3 达 L3） |
| 企业策略文件 | 签名 + 版本，可离线校验 | 应用前 | 拒绝应用并保持上一有效策略 |
**透明度日志**：插件发布/吊销进入 append-only 日志（Sigstore/Rekor 思路），客户端可校验条目已收录；吊销 p95 ≤1h 生效（R9）。
**签名密钥托管与门限（ADR-0015 §D5，取代 OQ-S3）**：信任根分两层，任何单人不可独立签名——**离线根密钥**（FIPS 140-2 L3 及以上 HSM/离线设备 + ≥2 把 YubiKey 5 FIPS，2-of-3 门限，3 名 custodian 异地分持 Shamir 份额，仅签发/轮换子键与 TUF root，轮换 ≤24 个月，仪式双人在场 + 录像）；**发布签名子键**（云 HSM/KMS 托管且密钥不可导出，2-of-3 人工审批，CI 仅持短期 OIDC 凭据且**只对哈希签名**，轮换 ≤12 个月）；插件/市场子键独立且轮换 ≤6 个月。新旧键重叠验证 ≥30 天；TUF root 轮换须新旧根双签。**泄露应急**：T+0 冻结签名 job 并吊销 CI 临时凭据；≤1h 发布吊销清单与新 timestamp/snapshot；≤24h 门限恢复新根并发紧急产物；≤72h 公开复盘 + 透明度日志补录。私钥严禁进入 CI secret / 仓库 / 日志 / 产物；单人不可签。签名服务中断不判 nightly 失败，但阻断 stable 发布。
**链接边界可执行定义（ADR-0015 §D1，取代本规格此前口径与 docs/spec/07 §3.11 的临时定义）**：边界 = 「与第一方代码在任一发布产物中**同进程执行**，或被**静态 / 动态 / 内联 / 派生并入**该产物」的第三方单元及其派生代码。三条不可协商原则：**P1 同进程即入界**（静态链接、动态链接、dlopen 判定相同，dlopen 不得用于规避静态链接判定）、**P2 派生即继承**（复制 / 修改 / 骨架生成所得继承来源 SPDX）、**P3 未知即拒绝**。边界外（构建期工具、运行时 subprocess、WASM/trusted 插件、OS 系统库）不等于无义务，须满足 **BT1–BT5**（构建期隔离：不进入运行时闭包、独立进程、不逐字复制生成器源码、移除后仍可构建、登记）与 **T1–T6**（运行时分进程、不再分发、可替换、不修改、登记、归因）。完整判定表 LB-01…LB-18（含 proc-macro、vendored C、代码生成器、编译器插件、subprocess 调 GPL/AGPL、插件捆绑）见 ADR-0015 §D2。

**许可白名单与黑名单（ADR-0015 §D4，取代 OQ-S2）**：**允许** Apache-2.0 / MIT / MIT-0 / BSD-2-Clause / BSD-3-Clause / 0BSD / ISC / Zlib / libpng-2.0 / Unicode-3.0 / Unicode-DFS-2016 / CC0-1.0 / Unlicense / BlueOak-1.0.0 / Python-2.0 / PSF-2.0 / Apache-2.0 WITH LLVM-exception；非代码资产另允许 OFL-1.1（字体）与 CC-BY-4.0（文档与图片）。**弱 copyleft 分级准入**：**MPL-2.0 准入链接边界**（文件级：不内联进我方文件、修改文件保持 MPL-2.0 并公开）；**LGPL-2.1/3.0 仅允许未修改的动态链接**（禁止静态链接与 vendored；Rust/npm 包因默认静态链接而禁止，除非证明仅绑定动态系统库）；**EPL-2.0 / CDDL-1.0/1.1 默认禁止，仅逐案审批**。**禁止** GPL-2.0/3.0、AGPL-3.0、SSPL-1.0、BUSL-1.1、Commons-Clause、Elastic-2.0、JSON、PolyForm-*、GFDL/CPAL/OSL/EUPL，以及 CC-BY-NC/ND/SA 作为代码或其依赖。**未知 / 空白 / NOASSERTION / LicenseRef-\* 一律拒绝（fail closed）**。**易错点**：SPDX 的 `BSL-1.0` 是 Boost Software License（允许），Business Source License 的标识是 `BUSL-1.1`（禁止），CI 必须精确匹配、禁止子串匹配。**表达式求值**：`OR` 取允许分支并记录 elected、`AND` 全允许、`WITH` 例外须在白名单、`-or-later` 不因存在可选分支而放行、解析失败即拒绝。

**工具链与门禁（ADR-0015 §D10）**：cargo-deny（licenses / bans / advisories / sources）、cargo-audit / RustSec / OSV、npm audit、SPDX 与许可证头扫描、反规避检查 AE1–AE5；规范策略文件 `third-party/policy.toml` 与 `runtime-tools.toml` / `build-tools.toml` / `boundary-exceptions.toml`，边界清单由 `cargo xtask license --boundary` 输出 `license-boundary.json`。例外仅允许弱 copyleft 与条件宽松类，须**法务 + maintainer 双签**、有效期 ≤180 天；**GPL/AGPL/SSPL/BUSL/Commons-Clause/Elastic-2.0 不得进入任何例外**。插件市场强制 SPDX 元数据（SEC-R-20）。
**本地模型安全**：禁用可执行反序列化格式（pickle/.pt 等），仅允许 safetensors/GGUF 类纯数据格式；模型运行器默认无网络沙箱（同 §3.3 规则 6）。

### 3.9 SDLC 与漏洞管理
| 频次 | 活动 |
| --- | --- |
| 每 PR | clippy -D warnings、单元/回放测试、SAST（Rust + TS）、gitleaks 密钥扫描、依赖与许可门禁、SBOM 增量；涉及 VT/PTY/IPC/插件/MCP 时另跑 fuzz 冒烟 + 崩溃回归 + 注入回归集（§8.2：拦截 ≥95%） |
| 每日 | 24h fuzz（VT/PTY/IPC/插件消息，DC-37；累计 ≥10⁸ 次执行无 crash） |
| 每发布 / 每季度 | §8.1 六件套全量 + §5 性能门禁 + 本规格 §5 验收；季度外部渗透与红队注入/逃逸演练、威胁模型复核 |
| 事件驱动 | Tier2 iframe 开放前红队门禁（逃逸 0 成功 + CSP 拦截 100%，AR-08）；重大架构变更前 STRIDE 复核 |

**漏洞 SLA 与披露**（08 §3.5；建议纳入 §8 门禁，见 OQ-S4）

| 严重度 | 修复 / 缓解 SLA 与目标达成率 |
| --- | --- |
| Critical（可远程利用/凭据外泄/沙箱逃逸） | 24h 内缓解，≥95% |
| High | 7 天内修复，≥90% |
| Medium / Low | 30 天 / 90 天（Low 尽力），≥90% / 提案 |
流程：私密报告渠道（security.txt）→ triage → 修复 + 回归测试 → 协调披露（**提案窗口 90 天**，OQ-S4）→ 公开公告与 CVE → 依赖漏洞由 cargo-audit/OSV 自动建单；任何已发布 crash 必须转为回归用例（DC-37）。

### 3.10 合规映射
**GDPR**

| 义务 | 实现 | 证据 |
| --- | --- | --- |
| 最小化 / 目的限制 | 内容零采集、遥测 opt-in、控制面 only | 遥测 schema CI 校验、§8.2 |
| 默认隐私设计 | AR-12 默认关闭；AR-13 原始输出默认不持久 | 默认值测试用例 |
| DSAR（访问/更正/删除/可携带） | 控制面账号数据经 UI/API 自助导出与删除；本地数据由用户在设置内导出/删除 | audit.export + 数据地图；**响应 ≤1 个月**（SEC-AC-18） |
| DPIA | AI 功能处理高敏数据 → 上线前完成 DPIA 并记录风险与缓解 | DPIA 文档 + 本规格 §3.1 |
| 记录处理活动 / 子处理者 | 子处理者清单 + DPA；跨境走 SCC | 公开数据流清单（OQ-S12） |
| 不用于训练 | Anti-feature 7：服务端保存 prompt/completion 用于训练 = 不做 | CI 字段 = 0 |
| 权利行使自动化 | 删除请求触发控制面删除并回执；本地数据不依赖我方 | 工单 + 审计 |
**SOC 2（TSC）**：CC6 访问控制（capability 默认拒绝 + MDM 取严）、CC7 运营（审计链 + 监控 + 事件响应）、CC8 变更管理（§8.1 六件套作为变更证据 + 签名发布）、CC9 风险评估（季度渗透 + 威胁模型更新）。
**企业 MDM 策略下发**：macOS Configuration Profile（plist）、Windows ADMX/Intune CSP、Linux /etc/termai/policy.toml，三者均须签名 + 离线校验；策略**只能加强**（收窄能力、提升审批档位、禁止特定模型/通道），任何「放宽」条目在加载时被拒绝并审计（SEC-AC-14）；合规证明默认回传**企业自有 MDM**，不经我方云（除非企业显式选择）。
**数据驻留**：租户级（OQ-15），仅控制面数据；EU/US 双区；企业版可选自托管控制面（OQ-17）。

### 3.11 隐私默认值
| 项 | 默认 | 开关 | 依据 |
| --- | --- | --- | --- |
| 遥测 | **关闭（opt-in）**，仅本地聚合计数 + 采样 | 设置 | AR-12 |
| 崩溃上报 | **关闭**，独立开关（与遥测互不连带）；端点由控制面团队运营（ADR-0015 §D8） | 设置 | AR-12 |
| 内容采集（命令文本/路径/代码/域名） | **零采集**，schema 级禁止 | 不可开启 | AR-12 |
| 会话捐赠用于评测 | 关闭，可撤回 | 设置 | AR-11、OQ-10 |
| prompt 缓存 | 同用户缓存关闭；跨用户缓存**永久不做** | 设置 | AR-11 |
| 完整原始输出持久化 | 关闭（可选加密开启） | 设置 | AR-13、OQ-04 |
| 项目记忆提交 git | 不提交，仅显式导出 | CLI | DC-30、OQ-13 |
| 云同步 | 关闭（仅小文档 CRDT；历史需 E2E 加密 opt-in） | 设置 | AR-11 |
| 出站清单可回看 | **100% 可见**（不可关闭） | UI | AR-12 |
| 首次运行 | 不弹向导、不做首屏同意墙（首屏即输入） | — | AR-15 |
同意获取须**非阻断、可就地拒绝、可随时撤回**；其交互形态与 AR-15「零弹窗」的边界见 OQ-S5。

**崩溃端点归属与保留期（ADR-0015 §D8）**：自托管 Sentry 兼容端点（Sentry Self-Hosted / GlitchTip），独立域名与命名空间，与控制面业务库物理隔离。**Cloud Platform（角色 06）运营；Security（08）为数据保护责任人并拥有服务端脱敏规则集；Devex（T5）拥有 schema 与 CI 校验**。**原始事件保留 30 天、聚合 / 符号化 issue 元数据保留 90 天，到期硬删除、无冷归档**（企业策略只能下调，上调需更新 DPIA）。区域随租户驻留（OQ-15），企业可自托管或完全关闭（OQ-E7）。脱敏双层强制：客户端侧为第一道且主责（DC-33，失败即丢弃事件，fail closed），服务端 before_send 清洗为第二道。访问原始事件需 break-glass 双人审批并审计；DSN 按通道独立、泄露可单独吊销；第三方登记为子处理者，不得用于训练。

**审计保留期与外锚（ADR-0015 §D7）**：本地默认 90 天，用户可配 7–3650 天；红线记录（L2/L3 审批、出站拒绝、密钥使用、插件能力授予、吊销）下限 30 天且不可更短；按龄整段删除并生成删除收据。外锚默认消费级关闭、企业 MDM 开启，三法见 §3.7。**口径澄清：审计 ≠ 遥测**——审计默认开启但**仅本地、不含内容**（DC-34 要求 100% AI 动作可审计）；遥测与崩溃上报是出站行为，默认关闭，不得以「审计默认开」推导遥测可默认开。

## 4. 接口与依赖

### 4.1 对内模块
| 模块 | 我需要的 | 我承诺的 |
| --- | --- | --- |
| sessiond / termai-core | 结构化事件与命令边界（OSC 133/633、退出码、cwd）；PTY 输入作为不可信源 | 威胁模型、fuzz 语料与用例、注入缓解建议、审计埋点位置 |
| termai-agent（Context Builder） | 唯一上下文出口、不绕过脱敏 | 脱敏管线、分级标签、上下文收据（DC-33） |
| termai-agent（Policy Engine） | 工具 ABI（DC-26）、审批挂载点、意图 schema | capability 校验 API、策略取严合并、审批收据格式 |
| termai-agent（Model Router） | Provider 抽象与出站目的地枚举 | 出站闸门规则、目的地白名单与证书固定、分类标签 |
| plugin-host | 权限清单、签名与分发流程、宿主 API 面 | 能力 SDK、隔离运行时、远程吊销通道、审计埋点（DC-38） |
| ui-native / web-shell | 审批与风险提示交互位、密钥引用 UI、出站清单视图 | 风险分级 API、占位符渲染契约、避免提示疲劳（AR-06、DC-11） |
| Local API broker / CLI --json | 单一契约（DC-24）、对端凭证校验 | 鉴权模型、audit.export schema |
| Cloud Control Plane | 数据流清单、区域与保留策略、子处理者 | 数据分级、上传闸门规则、DPA 要求、CI 字段约束（AR-11） |
| Release / CI | SBOM 流水线、签名密钥托管 | 许可白名单、漏洞 SLA、披露流程、§8.1 门禁 |

### 4.2 对外契约（版本化；兼容窗口 ≥2 minor，破坏性变更走 capability 协商）
| ID | 契约 | 载体 |
| --- | --- | --- |
| SEC-IF-01 | capability 令牌结构（sub/act/res/cst/apr/dlg） | WIT/IDL + CBOR，版本化 |
| SEC-IF-02 | 审计记录与 Merkle checkpoint schema；audit.export 流 | NDJSON / syslog / CEF / OCSF |
| SEC-IF-03 | 遥测 schema（内容字段数恒为 0） | JSON Schema，CI 校验 |
| SEC-IF-04 | 插件权限清单 + SPDX 元数据 | Manifest（签名覆盖） |
| SEC-IF-05 | 企业策略文件 schema（可离线校验、只加强） | TOML / plist / CSP 映射 |
| SEC-IF-06 | 出站分类标签 API（构建器 → 闸门）与脱敏 API（占位符 + secret://handle） | 进程内 trait |
| SEC-IF-07 | 吊销信息源（CRL / TUF targets） | 签名清单 + 拉取协议 |
| SEC-IF-08 | Local API 鉴权模型（对端 uid/gid + 会话令牌；远程留待 OQ-18） | UDS / named pipe |
### 4.3 外部依赖
OS keychain（DPAPI/Credential Manager、Keychain、Secret Service）、libsodium（降级加密与熵工具）、TUF 元数据与签名校验库、透明度日志（Rekor 思路）、**离线 HSM + YubiKey 5 FIPS（根密钥）与云 HSM/KMS（Azure Managed HSM / AWS CloudHSM，发布子键）**、Sigstore（Fulcio / Rekor / cosign）、in-toto 与 slsa-verifier（provenance）、CycloneDX（SBOM）、**自托管 Sentry 兼容端点（Sentry Self-Hosted / GlitchTip）**、可选 RFC 3161 时间戳服务、SIEM 协议（syslog/CEF/OCSF）、OIDC 与设备码 RFC 8628（登录）、OS 沙箱原语（Landlock/seccomp、seatbelt、AppContainer + Job Object）。（ADR-0015）

## 5. 验收标准
| ID | 标准（可量化） | 验证方法 | 口径 | 依据 |
| --- | --- | --- | --- | --- |
| SEC-AC-01 | 100% 出站经分类闸门；50 个真实 secret 样本（PEM/JWT/云 token/连接串）泄露率 = 0 | 闸门埋点 + 样本回归集 | 门禁 | §8.2、08 §7.1 |
| SEC-AC-02 | 危险/外发/提权动作 100% 触发审批；L2/L3 未审批执行 = 0；配置无法关闭 | 用例矩阵 + 配置篡改测试 | 门禁 | AR-06、§8.2 |
| SEC-AC-03 | 注入用例拦截率 ≥95%；污点值进入 argv/路径的用例 100% 拒绝 | 注入回归集 | 门禁 | §8.2、R5 |
| SEC-AC-04 | 恶意插件越权读 SSH key / 出网 / 调 shell 拦截率 100%；逃逸用例 0 成功 | 红队逃逸套件 | 门禁 | §8.1-6、AR-07 |
| SEC-AC-05 | VT/PTY/IPC/插件消息 fuzz 24h 无 crash，累计 ≥10⁸ 次执行；历史 crash 全回归 | cargo-fuzz CI | 门禁 | DC-37、§8.1-6 |
| SEC-AC-06 | 审计链任意单字节修改 100% 可检出；args 原文零落盘 | 篡改用例 + 存储扫描 | 门禁 | DC-34、08 §7.4 |
| SEC-AC-07 | 默认配置抓包零用户未发起连接；遥测关闭时 0 字节上传；崩溃上报关闭时 0 请求 | 网络抓包 + 流量断言 | 门禁 | §8.2 隐私、AR-12 |
| SEC-AC-08 | 服务端代码库 prompt/completion 字段出现次数 = 0 | CI 静态扫描 | 门禁 | §8.2、AR-11 |
| SEC-AC-09 | SBOM 中位于链接边界的 GPL/AGPL 依赖数 = 0 | cargo-deny + SPDX | 门禁 | §8.1-5、AR-21 |
| SEC-AC-10 | 插件签名校验失败 100% 拒装；更新元数据回滚攻击 100% 拒绝 | 篡改包用例 | 门禁（P3） | §7 P3 |
| SEC-AC-11 | 插件吊销到客户端生效 p95 ≤1h | 吊销演练 | 门禁 | R9 |
| SEC-AC-12 | 跨租户/跨用户越权用例 0 通过；RLS 覆盖 100% | 渗透用例 | 门禁（P4） | §7 P4 |
| SEC-AC-13 | 内存 dump、日志、遥测中 secret 命中 = 0；仓库与产物密钥扫描 = 0 | 转储扫描 + gitleaks | 门禁 | §6.1-2 |
| SEC-AC-14 | MDM 策略离线校验通过率 100%；「放宽」条目 0 生效 | 策略用例 | 门禁 | AR-06、SEC-R-12 |
| SEC-AC-15 | 撤销栈：工作区文件与 git 可逆操作 100% 可回滚；UI 边界声明齐全 | 用例 + 文案检查 | 门禁（P2） | AR-06、Anti-feature 13 |
| SEC-AC-16 | 漏洞 SLA：Critical 24h ≥95%；High 7d ≥90%；Medium 30d ≥90% | 事件统计 | 门禁 | 08 §7.7、OQ-S4 |
| SEC-AC-17 | Tier2 iframe 开放前：逃逸用例 0 成功 + CSP 拦截率 100% | 红队门禁 | 门禁（P3） | AR-08、§7 P3 |
| SEC-AC-18 | DSAR 响应 ≤1 个月；导出完整率 100%；删除可验证 | 演练 + 工单抽检 | 门禁（P4） | 08 §3.5 |
| SEC-AC-19 | 脱敏 + 闸门附加延迟 p95 ≤5ms / 1MB 上下文，且不加剧 §5 网关与首 token 门禁 | 微基准 + 端到端 | **提案**（OQ-S10） | §5 |
| SEC-AC-20 | 链接边界内黑名单 SPDX 命中 = 0；白名单外无有效例外 = 0；分类器在 LB-01…LB-18 golden 语料上正确率 100% | 分类器 + cargo-deny + 合成语料 | 门禁 | ADR-0015 §D4/D10、§8.1-5 |
| SEC-AC-21 | 每个发布产物具备 SBOM + provenance attestation，且发布前 slsa-verifier 校验通过（SLSA Build L2 地板；P3 起 L3） | 发布流水线 + 独立校验 job | 门禁 | ADR-0015 §D6 |
| SEC-AC-22 | 审计本地默认保留 90 天且可配 7–3650 天，红线记录 ≥30 天；外锚调用不含内容（仅 Merkle root） | 配置用例 + 外锚抓包 | 门禁 | ADR-0015 §D7、DC-34 |
| SEC-AC-23 | 崩溃端点原始事件保留 ≤30 天、聚合 ≤90 天；脱敏后请求中 secret / 路径 / 命令文本命中 = 0；关闭时请求 = 0 | 保留策略配置 + 样本回归 + 抓包 | 门禁 | ADR-0015 §D8、AR-12 |

## 6. 风险与缓解
| ID | 风险 | 触发条件 | 影响 | 缓解 | 残余风险 |
| --- | --- | --- | --- | --- | --- |
| SEC-RK-01 | 提示注入绕过（R5） | 模型按不可信内容执行命令 | 外泄/破坏 | 数据与指令分离 + 污点追踪 + AST 分级 + 能力默认拒绝 + 审批 | 新型注入，需持续红队（SEC-AC-03） |
| SEC-RK-02 | 审批疲劳致盲确认（R6） | 5 秒内直接确认比例 ≥15% | 审批形同虚设 | 分级摩擦 + 可解释影响面 + 白名单（仅 L0/L1）+ 二次输入仅用于 L3 | 用户仍可能盲目批准 |
| SEC-RK-03 | 供应链投毒（R9） | 签名异常、恶意插件报告 | 大规模失陷 | 签名 + TUF + 透明度日志 + 权限最小化 + 吊销 ≤1h + 离线根 / 云 HSM 2-of-3 门限（ADR-0015 §D5） | 门限仪式的人为失误或恢复份额泄露；靠季度恢复演练与双人仪式降低 |
| SEC-RK-04 | 脱敏误报损害可用性 | 合法高熵数据（构建哈希、base64 产物）被掩蔽 | 诊断质量下降 | 占位符可解释 + 非凭据上下文白名单（片段级豁免）+ 本地模型同样脱敏 | 用户可能直接粘贴原文（见 §1.4、OQ-S6） |
| SEC-RK-05 | 审计链被本机管理员重写 | root/管理员权限攻击者 | 不可抵赖性受损 | 外锚三法（企业自持 SIEM/WORM 默认、本地导出、opt-in 第三方 RFC 3161/Rekor）+ 只读介质双写 + 不夸大承诺（ADR-0015 §D7） | 无外锚时仅 tamper-evidence；消费级默认不外锚 |
| SEC-RK-06 | keychain 不可用 / 无桌面 | Linux 无 D-Bus、容器环境 | 密钥明文风险 | 加密文件 + Argon2id + 显式告警；拒绝静默降级 | 弱口令风险 |
| SEC-RK-07 | WebView 安全更新（R8、AR-02、OQ-03） | WebView CVE；企业锁定 fixed-version | 面板被攻破 | 默认 evergreen（安全优先）；WebView 非最低依赖，缺失降级原生文本面板；面板无密钥、无 PTY 写权限 | fixed-version 企业版补丁滞后窗口 |
| SEC-RK-08 | trusted subprocess 白名单被滥用 | 用户标记任意 CLI 为 trusted | 绕过 WASM 隔离 | 白名单限定（AR-07）、不进市场普通分类、永久标记、无 secrets/subprocess 级联 | 用户自担风险，审计留痕 |
| SEC-RK-09 | 企业策略与本地优先冲突 | 企业要求强制上传审计 / 禁止本地模型 | 用户信任受损 | 策略只加强且有范围边界；上传目标为企业自有系统；离线与本地功能完整性保留 | 内部合规摩擦（OQ-S7） |
| SEC-RK-10 | 云边界被功能需求侵蚀（R11） | 出现「上传历史换同步」需求 | 红线破口 | AR-11 写入不可协商清单；CI 静态禁止 prompt 字段；§2.1 强制点 | 商业压力持续存在 |
| SEC-RK-11 | 合规证据漂移 | 控制项变更未同步 | 认证风险 | §12 维护约定 24h 内同步；SOC 2 证据自动产出（SEC-R-15） | 依赖外部审计配合 |

## 7. Open Questions
| ID | 问题 | 建议 | 影响面 | 决策阶段 | 关联 |
| --- | --- | --- | --- | --- | --- |
| OQ-S1 | 审计链本地默认保留期；是否要求周期性外锚（Merkle root 推送） | **已裁决（ADR-0015 §D7）**：本地默认 90 天、可配 7–3650 天、红线记录 ≥30 天；外锚消费级 opt-in 关闭、企业 MDM 默认开；三法见 §3.7 | 存储、取证、隐私平衡 | **已关闭** | DC-34 |
| OQ-S2 | 依赖许可白名单是否纳入弱 copyleft（MPL-2.0/EPL） | **已裁决（ADR-0015 §D3/D4）**：MPL-2.0 准入链接边界；LGPL 仅未修改的动态链接；EPL-2.0 / CDDL 默认禁止、逐案审批 | 依赖可得性、法务风险 | **已关闭** | AR-21、ADR-0015、OQ-01 |
| OQ-S3 | 发布签名密钥托管形态与门限；是否设定构建来源等级目标 | **已裁决（ADR-0015 §D5/D6）**：离线根 2-of-3 + 云 HSM 子键 2-of-3、仅签哈希、泄露 ≤1h 吊销；SLSA Build L2 地板、P3 达 L3 | 供应链信任根 | **已关闭** | DC-36、ADR-0015 |
| OQ-S4 | 漏洞 SLA、披露窗口（建议 90 天）与是否设漏洞赏金 | SLA 纳入 §8 门禁；90 天协调披露 | 安全响应成本、品牌 | P2 | 08 §3.5 |
| OQ-S5 | 同意获取交互形态与 AR-15「零弹窗/首屏即输入」的边界 | 非阻断、可就地拒绝、设置内撤回；不做同意墙 | 合规、首屏体验 | P2 | AR-12、AR-15 |
| OQ-S6 | 脱敏误报的自助例外机制（非敏感高熵白名单/片段级豁免） | 仅允许移除片段与白名单模式，不允许「原样发送」 | AI 可用性 vs 红线 | P2 | DC-33 |
| OQ-S7 | 企业 MDM 能否强制把审计推送至企业 SIEM | 可强制推送至企业自有系统，不强制经我方云 | 企业合规 vs 用户信任 | P4 | AR-11、AR-06 |
| OQ-S8 | capability 令牌能否跨设备/跨会话复用（远程 attach，影响 IPC 加密与鉴权边界） | 默认不可复用；远程场景新设限定主体 | 远程/协作安全模型 | P5（P1 预留） | OQ-18 |
| OQ-S9 | 离线吊销宽限期长度（提案 72h）与超期后的能力降级 | 72h；超期拒绝新增高权限能力 | 离线可用性 vs 吊销时效 | P2 | SEC-R-07 |
| OQ-S10 | 是否把「脱敏 + 闸门附加延迟 p95 ≤5ms / 1MB」纳入 §5 预算 | 纳入（防安全机制侵蚀性能门禁） | 性能预算口径 | P1 | §5、AR-19 |
| OQ-S11 | 本地模型运行器沙箱强度（无网络 + 只读 FS 对推理性能的影响） | 默认无网络；性能不达标时以显式开关 + 审计让步 | 本地 AI 性能与安全 | P2 | OQ-07、SEC-R-09 |
| OQ-S12 | 是否自动生成对外可发布的「数据处理清单/子处理者列表」 | 生成，随隐私政策版本化发布 | 合规透明度 | P4 | SEC-R-15 |
> 说明：OQ-S2 / OQ-S3 已由 ADR-0015 裁决并关闭；OQ-S10 仍属 HARNESS §11 未覆盖的空白，**不得**由单个模块自行决定，须走 RFC → ADR；企业/合规场景（P4）落地前必须有结论。崩溃端点归属与保留期（原 spec 07 OQ-E7）已由 ADR-0015 §D8 裁决，见本文 §3.11。


