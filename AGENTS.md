# AGENTS.md — 协作者与 AI 代理工作约定

本文件规定在本仓库中工作的**强制流程**。它不重复设计内容，只规定「怎么做」。所有设计结论的唯一权威来源是 [HARNESS.md](HARNESS.md)。

## 1. 阅读顺序（开工前必做）

1. `HARNESS.md` —— 尤其 §0（不可协商清单）、§1（产品公理）、§2（仲裁决议）、§5（预算）、§8（门禁）。
2. 与你任务领域对应的 `docs/spec/*.md`。
3. 需要理解「为什么否决了另一种做法」时，读 `docs/roles/` 中的对应原文与 `docs/adr/`。

**禁止**在未读 HARNESS 的情况下修改架构、接口或指标。

## 2. 不可协商约束（违反即视为架构事故，PR 直接拒绝）

| # | 约束 | 依据 |
| --- | --- | --- |
| 1 | AI 不进入 PTY→像素热路径；内核不依赖 AI / 网络 / UI | AR-03 |
| 2 | 终端字符网格不由 WebView 渲染；WebView 不得覆盖原生 IME 浮层 | AR-01 |
| 3 | 破坏性/外发型操作的确认不可由配置关闭；企业策略只能加强 | AR-06 |
| 4 | 插件不得写 PTY/输出流、不得帧内绘制、不得持有 Node/原生句柄 | AR-07 |
| 5 | PTY 流、文件内容、命令输出、密钥永不进入服务端 | AR-11 |
| 6 | secret 永不进入模型上下文、日志、遥测；脱敏在上下文构建器内完成 | AR-12 / DC-33 |
| 7 | 热路径禁用 JSON / gRPC 序列化 | AR-04 |
| 8 | 核心链接边界内不得出现 GPL/AGPL/SSPL 依赖（弱 copyleft 见 ADR-0015） | AR-21 |

## 3. 依赖方向（CI 强制）

    tokens(叶子)  core-dto(叶子)
    term-render(→vt,gpu) → ui-native → shell-bridge → web-shell → plugin-ui-sdk
    core ← session ← { agent, plugin-host }
    apps/* 不得被任何库依赖

- 只允许单向依赖；禁止环。新增 crate/package 必须在 ADR 中说明其依赖位置。
- 跨边界改动需要双重 CODEOWNERS 批准（见 `docs/spec/07-engineering-quality-and-release.md`）。

## 4. 合并前必须通过的门禁（CI 六件套）

1. VT 兼容：vttest / esctest / kitty 套件 100%；xterm 兼容 ≥99%。
2. 行为回放：录制会话回放通过率 ≥99.5%。
3. 视觉回归：像素 diff ≤0.1%（非白名单区域零变化）。
4. 性能门禁：HARNESS §5 全部门禁项；回归 >5% 阻断。
5. 依赖与许可：cargo-deny / cargo-audit / npm audit / SBOM；GPL/AGPL 链接边界依赖 = 0。
6. 安全与 fuzz：VT/PTY/IPC/插件消息 fuzz 24h 无 crash；插件逃逸用例 0 成功。

## 5. 决策与文档约定

- **编号即契约**：引用决策一律写 `AR-xx` / `DC-xx`，不要复述措辞。
- **不修改历史**：`docs/roles/*` 为冻结证据；HARNESS 的既有 AR 只追加不篡改。推翻旧决策 = 新 ADR + 在旧条目上标注「被 ADR-xxxx 取代」。
- **ADR 触发条件**：新增/删除模块、改变 AR 结论、改变对外契约、引入新依赖类别、变更许可或数据边界。
- **指标变更**：任何 §5 预算或 §8 门禁的放宽，必须附基准数据与 TSC 批准。
- **Open Questions**：不允许用「TODO」代替；所有未决问题进 HARNESS §11，并标注必须决策的 Phase。

## 6. 代码与提交规范（实现期生效）

- 语言：核心 Rust（`rustfmt` + `clippy -D warnings`）；Web 侧 TypeScript strict。
- 错误处理：库代码禁止 panic 跨边界；面向用户的错误必须结构化（code + 可操作提示）。
- 日志：禁止打印 secret、命令全文或文件内容；日志需可分类、可脱敏。
- 提交：Conventional Commits；每个 PR 必须关联 AR/DC 编号或 ADR。
- 测试：新功能必须带单元测试 + （若涉及 VT/IPC/插件）fuzz 用例或回放语料。

## 7. 给 AI 代理的额外要求

1. 先 `read` HARNESS 与相关 spec，再动手；不要凭常识推断本项目结论。
2. 产出文档时保持 HARNESS 的编号与口径（AR/DC/Phase/OQ）一致。
3. 任何「更好的方案」都要落在 AR/DC 框架内，并写清**代价与复议条件**——本项目的标准动作是「记录反方意见后做决定」，而不是「不表态」。
4. 不确定时优先选择**更保守的安全默认值**（默认拒绝、默认 dry-run、默认本地、默认不上传）。
