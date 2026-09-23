<!--
PR 模板 —— 依据 AGENTS.md 第 6 节、HARNESS.md 第 8 节、docs/spec/07 第 3.4 节。
请在提交前逐项勾选；无法勾选的项必须写明原因，不得留空。
-->

## 1. 关联决策（必填）

编号即契约：只写编号，不要复述措辞（AGENTS 第 5 节）。

- AR / DC：
- ADR：
- Phase（HARNESS 第 7 节）：

## 2. 变更类型

- [ ] feat / fix / perf / refactor / docs / test / build / ci / chore

## 3. 影响面判定

- [ ] 触及热路径（term-render / term-gpu / term-vt / term-session / term-pty / termai-ipc）
      -> 必须附可复现的性能证据；热路径 PR 一律不得 SKIP（AR-31 / OQ-PM-04）
- [ ] 触及对外契约（IPC 帧与 msg_type、Session Log 格式、Context schema、WIT）
      -> 需 ADR + CODEOWNERS 双签（AR-28）
- [ ] 跨 owner 目录（E4）-> 两侧各一名 reviewer 批准
- [ ] 引入新依赖类别 -> 必须过 ADR-0015 判定表并登记到 ADR-0019 的准入清单

## 4. 不可协商约束自检（AGENTS 第 2 节；违反即架构事故，PR 直接拒绝）

- [ ] AI 未进入 PTY->像素热路径；内核未新增 AI / 网络 / UI 依赖（AR-03）
- [ ] 终端字符网格未由 WebView 渲染；WebView 未覆盖原生 IME 浮层（AR-01）
- [ ] 破坏性 / 外发型操作的确认仍不可由配置关闭（AR-06）
- [ ] 插件未写 PTY 或输出流、未做帧内绘制、未持有 Node / 原生句柄（AR-07）
- [ ] PTY 流、文件内容、命令输出、密钥材料未进入服务端（AR-11）
- [ ] secret 未进入模型上下文 / 日志 / 遥测（AR-12 / DC-33）
- [ ] 热路径未引入 JSON / gRPC 序列化（AR-04）
- [ ] 链接边界内未新增 GPL / AGPL / SSPL 依赖（AR-21 / ADR-0015）

## 5. 门禁（HARNESS 第 8.1 节六件套；缺一不得合并）

- [ ] G1 VT 兼容（vttest / esctest / kitty 100%，xterm >=99%）
- [ ] G2 行为回放（.trec 语料通过率 >=99.5%）
- [ ] G3 视觉回归（像素 diff <=0.1%）
- [ ] G4 性能门禁（HARNESS 第 5 节；回归 >5% 阻断）
- [ ] G5 依赖与许可（cargo-deny / cargo-audit / npm audit / SBOM）
- [ ] G6 安全与 fuzz（24h 无 crash；插件逃逸用例 0 成功）

未接入的门禁请写明「未判定」，不要勾选。

## 6. 本地实际运行的证据（贴真实输出）

    $ cargo fmt --all -- --check
    $ cargo clippy --workspace --all-targets -- -D warnings
    $ cargo test --workspace
    $ node tools/kernel-gates/check.mjs
    $ npm run tokens:check && npm run design:check:static

## 7. 诚实声明（AR-20）

- [ ] 本 PR 未声称任何未实际运行过的门禁通过；未判定项已显式写「未判定」
- [ ] 若放宽了任何指标或门禁阈值，已附基准数据与 TSC 批准（AGENTS 第 5 节）
- [ ] 未用 TODO 代替未决问题；未决项已进 HARNESS 第 11 节并标注必须决策的 Phase

## 8. 说明：动机、代价与被否决的替代方案

<!-- AGENTS 第 7.3 节：任何「更好的方案」都要落在 AR/DC 框架内，并写清代价与复议条件。 -->
