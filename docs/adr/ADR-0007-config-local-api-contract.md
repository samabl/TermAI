# ADR-0007：配置体系、Local API 与脚本化契约

- 状态：Accepted
- 日期：2025-01-01
- 决策者：Orchestrator（综合 10 角色评审）
- 关联：AR-09、DC-24；DC-08、DC-15、DC-22、DC-25、DC-40；HARNESS §7（P1）、§10.11

## 背景与问题
1. 分歧：极客向角色主张配置即代码（内嵌 Lua/JS），以获得可编程配置与社区吸引力；工程与产品侧要求配置必须**可静态解析、可校验、可迁移、可被 GUI 无损往返编辑**。
2. 内嵌配置语言的代价：任意代码执行面、沙箱难题、错误栈泄露、设置 UI 无法保证往返一致、企业策略难以静态校验。
3. 脚本化需求真实存在：CI、headless、IDE 集成、插件都需要统一入口，且**不能出现三套契约**（插件一套、CLI 一套、IDE 一套）。
4. 约束：核心 100% Rust，TS 仅在 Web 面板与插件 SDK（DC-15）；热路径禁 JSON/gRPC（AR-04），但控制面/脚本面可以用 JSON。

## 可选方案（至少 2 个，含被否决项）
| 方案 | 主张方 | 否决 / 采纳理由 |
| --- | --- | --- |
| A 内嵌 Lua/JS 作为配置语言 | 极客向 | 否决（Anti-features §10.11）。任意代码执行、GUI 往返困难、企业策略难静态校验；可作为**插件作者语言**复议 |
| B 仅 TOML 单文件、无层级合并 | 简化派 | 否决。无法满足 default/system/user/project/env/CLI 的企业与项目级差异化需求 |
| C TOML + drop-in + 层级合并 + 统一 Local API | Orchestrator | **采纳**（AR-09、DC-24） |

## 决策
1. **配置格式 = TOML + drop-in + 层级合并**，优先级：default < system < user < project < env < CLI；合并语义必须可解释、可打印（termai config get 显示最终值与来源层）。
2. **不内嵌 Lua/JS 配置语言**；可编程配置由 **config provider 插件**承担（WASM，受同一权限与审批模型约束）。
3. 提供 **termai import**（kitty / alacritty / wezterm / iTerm2）与 **termai config get/set/eval --json**。
4. **Local API = JSON-RPC over UDS / named pipe（含 apiVersion）**，同时服务插件、CLI --json、headless、CI、IDE 集成；**只维护一套契约**（DC-24）。
5. 传输边界：Local API 用于控制面与脚本面；**PTY/网格热路径不走 JSON-RPC**（走 termai-ipc 二进制，DC-22）。
6. 契约治理：apiVersion 显式声明；兼容 N-2 minor + 6 个月 deprecation + codemod（DC-40）；headless 模式（termai daemon）必须能跑 CI（P1 退出标准）。
7. 命令面板 = Cmd+K，功能覆盖率 100% 为 CI 门禁（DC-08）；命令面板动作表与菜单差集必须为空。

## 理由
1. 配置的**可预测性**优先于表达力：终端配置错误会直接导致无法输入或渲染异常，静态可校验是关键。
2. 单一 Local API 契约把插件、CLI、headless、IDE 的维护成本从 O(N) 降到 O(1)，并让「所有会话状态可经 API 读取」成为 P1 可验收目标。
3. config provider 插件给极客留了出口，同时把任意代码执行关进与插件相同的沙箱与审批模型（ADR-0006）。

## 后果（正面 / 负面 / 需要接受的代价）
- 正面：配置可校验/可迁移/可 GUI 往返；脚本化与 CI 一体化；企业策略可静态校验；插件与 CLI 共享契约。
- 负面：配置即代码的社区吸引力被牺牲（极客向反弹）；层级合并的调试体验需要在 UI 上额外投入。
- 需要接受的代价：
  1. 必须实现并文档化层级合并语义与来源追溯（否则用户无法理解最终值）。
  2. Local API 的 apiVersion 兼容窗口需要长期维护与契约测试。
  3. 高级可编程需求只能通过 WASM 插件实现，启动与权限成本高于「写一段 Lua」。

## 反方记录与复议条件
- **反方（极客向，保留原文）**：拒绝内嵌脚本语言降低了配置的灵活性与社区粘性；真正的重度用户会用脚本生成配置。
- **复议触发条件**：若社区压力足够（可量化为配置类 issue/RFC 数量与用户调研占比阈值），则以「**Lua 作为插件作者语言**」而非配置语言回归（AR-09 明文）。
- 不可放宽：配置层永不引入任意代码执行；插件不得因配置入口获得额外权限。

## 关联决策（DC-xx）与实现位置
| 决策 | 内容 | 实现位置 |
| --- | --- | --- |
| DC-24 | Local API = JSON-RPC over UDS/named pipe，单一契约 | termai-local-api（schema/ 版本化） |
| DC-22 | 热路径走二进制 IPC，JSON-RPC 仅控制/调试 | termai-ipc |
| DC-08 | Cmd+K 命令面板覆盖率 100% 门禁 | ui-native（action registry）+ CI 差集检查 |
| DC-15 | 核心 Rust，TS 仅 Web 面板与插件 SDK | apps/cli、plugin-ui-sdk |
| DC-40 | apiVersion 兼容 N-2 minor + deprecation | local-api 契约测试 |
