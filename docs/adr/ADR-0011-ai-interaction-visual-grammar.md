# ADR-0011：AI 交互范式与输出视觉语法

- 状态：Accepted
- 日期：2025-01-01
- 决策者：Orchestrator（综合 10 角色评审）
- 关联：AR-16、AR-17；DC-01、DC-02、DC-05、DC-08、DC-11、DC-13；HARNESS §1.4、§5、§8.2

## 背景与问题
1. 行业默认：AI 能力以「聊天框」为中心入口，输出自由 markdown 直出。
2. 终端场景的差异：用户注意力在网格上，切换焦点到聊天框的成本高；自由 markdown 会破坏列宽、复制语义与 scrollback 语义。
3. 争议：聊天优先派认为对话是最通用的表达；内联派认为 AI 应进入用户当前所在的行。
4. 视觉风险：AI 输出若直出原始 HTML/自由 markdown，会出现不可控的宽度、颜色、链接与滚动条行为，冲击 DC-13（对比度门禁）与 DC-14（视觉回归 ≤0.1%）。
5. 信任要求：错误是一等公民 artifact，永不吞 stderr、永不美化退出码（DC-05、§6.1）。

## 可选方案（至少 2 个，含被否决项）
| 方案 | 主张方 | 否决 / 采纳理由 |
| --- | --- | --- |
| A 聊天框为中心入口，自由 markdown 渲染 | 聊天优先派 | 否决。焦点切换成本高；自由 markdown 破坏列对齐、复制与视觉回归门禁 |
| B 全部改为全屏 Chat 模式 | 沉浸派 | 否决。终端任务上下文在网格内，全屏模式丢失上下文；也违背 DC-02 的入口顺序 |
| C 固定入口顺序 + 结构化 schema 渲染 + 诊断优先 | Orchestrator | **采纳**（AR-16、AR-17） |

## 决策
1. **入口顺序固定**：**内联 Agent（?? 前缀）为第一入口 → 侧边面板为主场（宽度 ≤40%，可收成徽标）→ ghost text 仅补全（不生成副作用命令）→ 全屏 Chat 仅面板次级模式 → 整行灰字预测默认关闭**（AR-16、DC-02）。
2. **视觉语法**：AI 输出必须经结构化 schema 渲染——Inline Hint / Command Explain / Diff Block / Diagnosis Card / Risk Ladder；**禁止自由 markdown 或原始 HTML 直出**。
3. scrollback 规则：行数 >40 或被折叠的 diff **不计入终端 scrollback**（避免污染复制与回放）。
4. **错误是一等公民 artifact**：永不吞 stderr、永不美化退出码；保留原始输出 + exit code + 所在 Pane（DC-05）。
5. **价值排序**：故障诊断 > 上下文感知命令生成 > 大规模运维（Phase 4+）；明确不做通用聊天、代码编辑器、自然语言 Todo（AR-17）。
6. 风险呈现：四级风险色阶（DC-11）与命令面板 Cmd+K 100% 覆盖（DC-08）必须与入口逻辑一致。
7. 渲染约束：所有 AI 组件只能使用 token 子集（DC-09）；AI 面板属 L2 WebView，其崩溃不得影响会话（ADR-0001）。

## 理由
1. 内联优先把「提问 → 建议 → 执行」压进同一行，减少焦点切换，直接提升 Accepted AI Actions per WAU（NSM ≥5）。
2. 结构化 schema 让输出可校验、可测试、可视觉回归、可本地化（与 DC-14、AR-20 的 a11y/i18n 承诺一致）。
3. 诊断优先与 AR-17 一致：补全类能力会被 shell 补全蚕食，只有诊断是 AI 独有且难以被替代的护城河。

## 后果（正面 / 负面 / 需要接受的代价）
- 正面：AI 输出可控、可回归测试；scrollback 语义不被污染；入口唯一，学习成本低；诊断卡片可直接挂审批与撤销。
- 负面：表达力受限——复杂的解释性内容需要预定义 schema 而非自由排版；新输出形态必须走 schema 扩展流程。
- 需要接受的代价：
  1. 必须维护 render_hint 与 schema 版本（与 DC-26 工具 ABI 的 render_hint 字段对齐）。
  2. ghost text 仅补全意味着不能从 ghost text 触发副作用命令，用户需显式接受建议。
  3. Schema 扩展需要设计系统与前端共同评审，节奏比「直接输出 HTML」慢。

## 反方记录与复议条件
- **反方（聊天优先派，保留原文）**：对话是最通用的交互形态，限制入口顺序会压制探索性使用；结构化 schema 无法覆盖长尾解释需求。
- **反方（表达力派）**：禁止自由 markdown 会降低输出可读性，尤其在大段诊断说明与多语言场景。
- **复议触发条件**：
  1. 若可用性测试显示侧边面板使用率显著低于全屏 Chat，则重评面板与全屏的层级（但**内联仍为第一入口**）。
  2. 若 schema 长尾需求持续超出现有能力，则以「**新 schema 类型**」而非「放开自由 markdown」的方式扩展。
- 不可放宽：禁止原始 HTML 直出；永不吞 stderr / 永不美化退出码；ghost text 不得生成副作用命令。

## 关联决策（DC-xx）与实现位置
| 决策 | 内容 | 实现位置 |
| --- | --- | --- |
| DC-02 | 入口顺序固定 | ui-native（内联 ?? 前缀）+ web-shell（面板） |
| DC-05 | 错误 artifact 化 | sessiond（退出码/输出）+ web-shell（Diagnosis Card） |
| DC-11 | 四级风险色阶 | web-shell（Risk Ladder 组件） |
| DC-09 / DC-13 / DC-14 | token 子集、对比度、视觉回归 | tokens codegen + Playwright diff |
| DC-26 | render_hint 与工具 ABI | termai-agent（tool envelope） |
