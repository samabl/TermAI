# TermAI

> **AI 驱动的现代终端软件**：终端模拟器 + 结构化上下文总线 + 内嵌 Agent + 插件生态。
> 目标是让「从报错到修复」的全部上下文交给一个**可审计**的 AI —— 而不是做一个带聊天框的终端。

**当前状态：M0 交付中（信任基座纵向切片，headless）。** 设计与仲裁已冻结（10 份角色评审、31 条仲裁决议 AR、40 条决策登记 DC、领域规格 + 7 份内核分册、ADR-0001…ADR-0020）。设计 token 与设计门禁工具链**已绿**；内核侧已落地 termai-core / termai-ipc / termai-session / termai-vt / termai-pty 与 sessiond / termai CLI。

> **M0 的范围与诚实边界**：见 [docs/plan/mvp-delivery-plan.md](docs/plan/mvp-delivery-plan.md)。
> M0 **不主张** P0 出口达成（P0 出口含 vttest/esctest 全绿、三平台 IME/CJK 矩阵、性能门禁进 CI）；M0 主张的是**契约冻结 + 垂直链路可运行 + 已实现部分门禁全绿**。GPU 网格渲染、IME、AI Agent、插件均从 M1 起。

### 开发者快速开始

    cargo test --workspace
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    npm run tokens:check
    npm run design:check:static
    node tools/kernel-gates/check.mjs

门禁清册见 [docs/spec/07-engineering-quality-and-release.md](docs/spec/07-engineering-quality-and-release.md) 与 [AGENTS.md](AGENTS.md) 第 4 节。

---

## 一句话定位

| 维度 | 结论 |
| --- | --- |
| 差异化 | **AI 能读到什么（结构化上下文）、能否安全写回（能力 + 审批）、能否审计回滚（审计链 + 撤销快照）** |
| 主用户 | SRE / 平台工程师、高级软件工程师 |
| 技术栈 | 核心 100% Rust（PTY / VT / wgpu 渲染 / sessiond / Agent Runtime）+ TS 仅用于 Web 面板与插件 SDK |
| 架构 | Native Grid + Web Shell 混合：终端像素由 Rust 直提 GPU，AI/设置/插件 UI 走系统 WebView |
| 许可 | **全栈开源，Apache-2.0 OR MIT 双许可**；商业形态为托管/支持/企业服务；公共插件市场 v1 零抽成 |
| 北极星 | Accepted AI Actions per WAU ≥ 5 |

## 不可协商的六条（违反即架构事故）

1. AI 不进入 PTY→像素热路径；内核不依赖 AI、不依赖网络。
2. 终端字符网格绝不由 WebView 渲染。
3. 破坏性与外发型操作的确认**不可由配置关闭**。
4. 插件不得拦截/改写 PTY 字节流，不得获得 Node/原生句柄。
5. PTY 流、文件内容、命令输出、密钥材料**永不进入服务端**。
6. secret 永不进入模型上下文、日志与遥测。

---

## 文档地图（读这一份就够）

| 文件 | 内容 | 什么时候读 |
| --- | --- | --- |
| **[HARNESS.md](HARNESS.md)** | **项目唯一权威总纲**：20 条仲裁决议 AR、40 条决策登记 DC、目标架构、统一预算、路线图、质量门禁、风险、待决议 | **所有人，第一步** |
| [AGENTS.md](AGENTS.md) | 面向 AI/协作者的仓库工作约定、阅读顺序、提交与门禁规则 | 动手改代码前 |
| [docs/spec/00-glossary.md](docs/spec/00-glossary.md) | 术语基线（共识层） | 遇到术语歧义时 |
| docs/spec/01-product-and-metrics.md | 产品定位、用户、功能矩阵、指标树 | 产品/优先级讨论 |
| docs/spec/02-ux-and-design-system.md | 信息架构、交互模型、设计 token、AI 视觉语法、可访问性 | 做 UI/交互 |
| docs/spec/03-system-architecture.md | 进程拓扑、模块边界、通信、存储、时序 | 做架构/内核/前端 |
| docs/spec/04-ai-agent-platform.md | Agent 循环、工具 ABI、上下文工程、模型路由、评测 | 做 AI 能力 |
| docs/spec/05-security-privacy-compliance.md | 威胁模型、capability、脱敏、审计、合规 | 涉及权限/数据/网络 |
| docs/spec/06-plugin-ecosystem-and-devex.md | 插件 ABI、扩展点、SDK、市场、Local API | 做生态/插件 |
| docs/spec/07-engineering-quality-and-release.md | 测试分层、CI 门禁、发布与回滚、平台矩阵 | 做工程/CI/发布 |
| docs/adr/ | 架构决策记录（每条 AR 的可追溯版本）与变更流程 | 要推翻既有决策时 |
| docs/roles/01–10-*.md | **10 份角色评审原文（冻结证据）**：含全部推理、被否决方案与反方意见 | 想知道「为什么不是那样」 |

## 路线图（按依赖）

| Phase | 主题 | 交付定义 |
| --- | --- | --- |
| P0 | 信任基座：PTY / VT / GPU 渲染 / 跨平台 / IME | vttest + esctest 全绿；性能门禁进 CI |
| P1 | 结构化上下文：Session Log、OSC 133/633、Local API、headless | 会话状态可经 API 读取；索引可从 Log 重建 |
| P2 | AI 一等公民：内联 Agent、审批、审计、回滚、eval | 100% AI 动作可审计；L2/L3 未审批执行 = 0 |
| P3 | 生态：WASM 插件、MCP、兼容层 | 第三方零特权发布插件；TTFHW ≤5min |
| P4 | 团队/企业：同步、策略、SSO/SCIM、私有部署 | 无外网环境功能完整；越权用例 0 通过 |
| P5 | 远程 / Web / 协作 | 端到端加密 + 明确权限模型 |

## 贡献与决策流程

- 任何变更先落到 HARNESS 的 AR/DC 编号上；**未裁决的冲突走 RFC → ADR**，不得由单个模块自行决定。
- 无法达成一致时，按 HARNESS §1.2 的三条产品公理裁决：**信任 > 正确性 > 功能丰富度**。
- 关键指标门禁见 HARNESS §5 与 §8；达不到门禁不得合并。

## 许可

**全栈开源**：客户端、云端服务、SDK 与协议统一采用 **Apache-2.0 OR MIT 双许可**。官方商业形态是托管、支持与企业服务（源码同样公开）。链接边界规则见 [HARNESS.md](HARNESS.md) 的 AR-21 与 `docs/adr/ADR-0015`。
