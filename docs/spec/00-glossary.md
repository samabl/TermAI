# 术语基线（Glossary）

> 本文件由总负责人（Orchestrator）维护，与各角色观点无关；用于统一 TermAI 全套设计文档的用词。
> **效力层级**：HARNESS.md（唯一权威总纲）> 本术语表 > 角色文档原文。术语表只统一"用词"，不裁决"设计取舍"；
> 本表与 HARNESS 冲突时以 HARNESS 为准，并应立即修订本表（不得反过来用术语表推翻 AR/DC）。

## 1. 终端内核（Terminal Core）

| 术语 | 全称 / 英文 | 含义（本项目口径） |
| --- | --- | --- |
| PTY | Pseudo-Terminal | 伪终端：内核提供的 master/slave 设备对，终端模拟器持有 master 端，shell 运行在 slave 端。 |
| ConPTY | Console Pseudo Terminal | Windows 10 1809+ 的伪终端 API，是 Windows 上运行现代终端应用的标准方式（替代已废弃的 winpty）。 |
| VT | Virtual Terminal | DEC 定义的终端行为规范；"VT 兼容"指对 ANSI/VT 转义序列的实现程度。 |
| CSI | Control Sequence Introducer | 转义序列前缀 `ESC [`，承载光标、颜色、擦除、模式设置等指令。 |
| OSC | Operating System Command | 转义序列前缀 `ESC ]`，承载标题、cwd、超链接、命令边界（OSC 133/633）、剪贴板、通知等扩展能力。 |
| Shell Integration | Shell 集成 | 通过 shell hook 注入 OSC 序列，把"命令边界 / 退出码 / cwd / 提示符"结构化地告诉终端，是 AI 理解会话的语义基础。 |
| Scrollback | 滚动缓冲 | 终端保留的历史输出；AI 上下文的来源之一，也是隐私与内存的主要边界。 |
| Reflow | 重排 | 容器尺寸变化时对已换行文本重新折行，保证复制与阅读正确。 |
| Ligature | 连字 | 等宽字体的字形合并（如 `=>`、`!=`）；是否启用需可配置，因为它影响列对齐与复制语义。 |
| Graphics Protocol | 图形协议 | 在终端内显示图片的扩展：Sixel、Kitty Graphics Protocol、iTerm2 Inline Images。 |
| Multiplexer | 终端复用器 | tmux / zellij / screen 一类提供会话持久化、分屏、断线重连的工具；TermAI 需明确"替代还是兼容"。 |
| Headless Mode | 无头模式 | 不启动 GUI、仅通过 CLI/IPC 驱动内核的模式，用于自动化、CI 与脚本化。 |

## 2. AI 与 Agent

| 术语 | 含义（本项目口径） |
| --- | --- |
| Agent Loop | 模型 → 工具调用 → 观察结果 → 再推理的循环；本项目 Agent 的基本执行单元。 |
| Tool（工具） | Agent 可调用的受控能力，如 `shell.exec`、`fs.read`、`git.diff`；必须带权限声明与审计痕迹。 |
| Capability（能力令牌） | 授予插件/工具/Agent 的最小权限单位（如 `shell.exec:readonly`、`net:api.example.com`）；默认拒绝。 |
| Dry-run | 只展示将要执行的命令与影响，不产生副作用；AI 执行路径的默认第一态。 |
| Risk Tier（风险分级） | 对拟执行操作的破坏性分级，**唯一枚举为 L0 只读 / L1 幂等写 / L2 破坏性 / L3 不可逆或外发 / U 不可判定**（依据 HARNESS AR-06）；决定确认摩擦级别与授权档（once/session/rule）。旧写法 R0–R3 已废弃。 |
| Prompt Injection | 来自终端输出、文件内容、网页、工具返回值等不可信文本，试图劫持 Agent 行为的攻击。本项目中"数据与指令"必须分离。 |
| Context Engineering | 对送入模型上下文的采集、裁剪、压缩、脱敏与排序的系统化工程。 |
| Secret Redaction | 在进入模型上下文、日志或遥测前识别并掩蔽凭据（正则 + 熵检测 + keychain 反向匹配）。**唯一收口点在 Context Builder，失败默认拒绝发送。** |
| RED-n（脱敏管线阶段） | 脱敏管线的阶段编号，**唯一枚举为 RED-0…RED-6**（会话开关 / 特征检测 / 熵检测 / 反向匹配 / 替换 / 收据 / 失败处置）；勿与风险分级 L0–L3/U、风险登记册 R1–R12 混用。 |
| BYO-key | 用户自带模型供应商 API Key；隐私敏感场景的必备选项。 |
| Model Router | 依据任务类型、隐私等级、成本与延迟预算选择本地/云端模型的调度层。 |
| Eval | 面向 Agent 的可重复评测集与判分标准；与单元测试同等地位。 |
| MCP | Model Context Protocol，用于接入外部工具/数据源的标准协议（本项目视为工具来源之一，需做信任分级）。 |

## 3. 平台与工程

| 术语 | 含义（本项目口径） |
| --- | --- |
| wgpu | Rust 生态的跨平台 GPU 抽象层（Vulkan/Metal/DX12/OpenGL 后端），自研渲染器的候选基础。 |
| WebView UI | 以系统 WebView 承载 HTML/CSS/JS 界面的方案（Tauri 式），体量小但渲染一致性受系统版本影响。 |
| 本地优先（Local-first） | 数据与能力默认在本机完成，云端仅作为可选增强；断网可用是硬性验收项。 |
| ADR | Architecture Decision Record，一次架构决策的不可变记录（背景 / 选项 / 决策 / 后果）。 |
| RFC | 面向较大变更的提案流程，产出经评审后落地为 ADR 或规格。 |
| NSM | North Star Metric，北极星指标。 |
| JTBD | Jobs To Be Done，用户"雇佣"产品去完成的任务。 |
| MoSCoW | Must / Should / Could / Won't 的需求优先级方法。 |
| 验收标准（AC） | Acceptance Criteria，可验证、可自动化的完成定义。 |
| P0/P1/P2 | 缺陷或事项的严重度/优先级分级。 |

## 4. 文档代号约定

- 角色评审原文：`docs/roles/NN-<role>.md`
- 领域规格：`docs/spec/NN-<topic>.md`
- 决策记录：`docs/adr/ADR-NNNN-<slug>.md`（含 `docs/adr/README.md` 索引与流程）
- 总纲（唯一权威汇总）：`HARNESS.md`
- 决策编号：`D-xx`（HARNESS 中的关键结论）、`ADR-xxxx`（可追溯的架构决策）。
