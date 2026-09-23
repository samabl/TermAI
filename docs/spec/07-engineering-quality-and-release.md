# 07 · 工程质量、测试与发布规格

> 领域：仓库与模块边界、构建与工具链、测试分层、CI 门禁、发布工程、崩溃与遥测、平台矩阵、性能基准、技术债与治理、SBOM 与许可合规。
> 效力：本文位于 HARNESS.md 之下；与 AR / DC / §5 / §8 冲突处以 HARNESS 为准。引用决策一律写编号（AR-xx / DC-xx / R-x / OQ-xx）。
> 上游证据：`docs/roles/10-cto-engineering-lead.md`、`docs/roles/04-terminal-core-architect.md`、`docs/roles/05-frontend-architect.md`（冻结原文）。

## 1. 目的与范围

1.1 **目的**：把 HARNESS §8 的质量门禁与 §5 的非功能性预算落成**唯一可执行的工程契约**——任何 PR、夜间流水线或发版动作都必须能回答三件事：受哪条预算约束、过哪些门禁、失败时如何回滚。
1.2 **覆盖范围**：① 仓库与模块边界（workspace 布局、依赖方向、CODEOWNERS ↔ 团队拓扑）；② 构建与工具链（锁定、xtask / cargo-dist、交叉编译、CI 隔离签名）；③ 测试分层八件套与阈值；④ CI 六件套与 PR 流水线；⑤ 发布工程（trunk-based、三通道、flag 灰度、≤15min 回滚）；⑥ 崩溃与遥测；⑦ 平台矩阵与降级；⑧ 基准与性能测量方法；⑨ 技术债 / 架构审计 / ADR 治理 / SBOM 合规。
1.3 **不覆盖**：产品指标定义（§1.4、DC-07）、AI 工具 ABI 与审批语义（DC-26…DC-35）、视觉 token 内容（DC-09…DC-14）。本规格只规定这些内容的**门禁接线方式**。
1.4 **读者**：T1 Core Kernel、T2 Shell & UX、T3 Agent & Context、T4 Platform & Ecosystem、T5 DevEx & Release，以及发布经理、安全与法务。
1.5 **硬边界重申**（本文全部流程不得削弱）：AR-03、AR-06、AR-07、AR-21、AR-11、AR-12 与 AGENTS.md §2 不可协商清单。

## 2. 需求与约束

### 2.1 不可协商约束（违反即架构事故）

| # | 约束 | 依据 |
| --- | --- | --- |
| C1 | AI 不进入 PTY→像素热路径；内核不链接 AI SDK、不持网络句柄；AI 关闭时核心可用性 100% | AR-03、§1.4 |
| C2 | 终端网格与关键动画不经 WebView；WebView 缺失时降级为纯原生文本面板 | AR-01、AR-02、OQ-02 |
| C3 | 热路径禁用 JSON / gRPC 序列化；跨进程仅 termai-ipc（二进制 + 能力协商） | AR-04、DC-22 |
| C4 | 破坏性 / 外发型确认不可由任何配置或 flag 关闭；企业策略只能加强 | AR-06、DC-35 |
| C5 | 插件不写 PTY / 输出流、不帧内绘制、不持 Node / 原生句柄；插件宿主独立进程 | AR-07、DC-38、DC-39 |
| C6 | PTY 流 / 文件内容 / 命令输出 / 密钥永不出设备；服务端 prompt / completion 字段 = 0 | AR-11、§6.1 |
| C7 | secret 永不进入模型上下文、日志、遥测；脱敏在上下文构建器内完成，失败即拒绝发送 | §6.1、DC-33 |
| C8 | 核心链接边界内 GPL / AGPL 依赖 = 0 | AR-21 |
| C9 | 任一门禁项不得被降级为「观察项」；AR-19 的门禁 / 目标双列口径不得合并 | AR-19、§5 |

### 2.2 工程与发布约束

| # | 约束 | 依据 |
| --- | --- | --- |
| E1 | 核心 100% Rust；TS 仅存在于 Web 面板与插件 SDK | DC-15 |
| E2 | 仓库 = Cargo workspace + pnpm workspace；依赖单向无环，CI 强制 | DC-21 |
| E3 | 依赖硬规则 `core ← session ← {agent, plugin-host}`、`render → vt + gpu`；任何人不得依赖 `apps/*` | DC-21、AGENTS §3 |
| E4 | 跨边界改动需双重 CODEOWNERS 批准 | AGENTS §3 |
| E5 | 存储：Session Log 为唯一真相 + SQLite(WAL) 可重建派生索引 + CAS；兼容 ≥2 大版本 | DC-23 |
| E6 | 兼容 N-2 minor + 6 个月弃用 + codemod；破坏性 WIT 变更需 RFC + 2 maintainer | DC-40、DC-24 |
| E7 | 自绘 surface 受限控件集 ≤20 个；绝不自研通用 UI 工具链 | DC-25 |
| E8 | 发布 = trunk-based + 三通道 + flag 灰度；回滚 = 撤回 channel 指针 | 角色 10 D-13、§8.2 |
| E9 | 崩溃上报默认本地且开关独立；遥测默认关闭（opt-in）；内容零采集 | AR-12 |
| E10 | 签名私钥只存在于 CI 隔离区 | 角色 10 §3 |

### 2.3 预算与门禁约束
- **B1**：HARNESS §5 全部门禁项为发布阻断项；目标值仅作挑战值，达标余量记为工程债（AR-19）。
- **B2**：性能回归 >5% 阻断合并（§8.1-4）；判定方法见 §3.8.3。
- **B3**：视觉回归像素 diff ≤0.1%，非白名单区域零变化（DC-14、§8.1-3）。
- **B4**：VT 兼容 vttest / esctest / kitty 套件 100%，xterm 兼容用例 ≥99% 且差异登记在案（§8.1-1、AR-18）。
- **B5**：行为回放通过率 ≥99.5%（§8.1-2）；fuzz 24h 无 crash 且累计 ≥10⁸ 次执行（DC-37）。
- **B6**：依赖与许可：cargo-deny + cargo-audit + npm audit + CycloneDX SBOM；GPL / AGPL 链接边界 = 0（§8.1-5）。
- **B7**：任一 stable 版本 ≤15 分钟全量回滚；nightly 连续 30 天流水线不中断（§8.2、角色 10 §7.6）。
- **B8**：命令面板功能覆盖率 100%（动作表与菜单差集为空，DC-08）；硬编码色值 = 0（DC-09）。
- **B9**：AI 合入必须带离线 eval，「危险命令率上升」即阻断（DC-31）；模型权重哈希 + 通道签名 + 离线校验（DC-36）。
- **B10**：平台支持矩阵、四 shell × 三字体后端枚举、三档参考机（RM-A / RM-B / RM-C）与门禁-机器一一映射见 §3.7 与 §3.8.1；§5 门禁数值**仅在 RM-A / RM-C 的一级 GPU 后端（T0）上判定**，云 runner 结果一律标记 NON-GATING（ADR-0014）。
- **B11**：nightly「连续 30 天不中断」按 §3.4.3 的 FAIL-REPO / EXTERNAL 口径核算（30 天窗口内 FAIL-REPO = 0、EXTERNAL ≤6 夜且无连续 >2 夜）；CI 与签名成本按 §3.4.3 公式核算并受月度上限 $3,000 约束（ADR-0014）。
- **B12**：设计/体验门禁（`npm run design:check`）的静态层 S1–S10 与浏览器层 B1–B10 全绿才可合并；**视觉回归必须在动画冻结下渲染**（`--force-prefers-reduced-motion` + reduced-motion 媒体覆盖，同一状态连渲两次哈希必须相同）；基线按平台入库（`prototype/baseline/<platform>-<arch>-*.png`），平台缺基线时显式 SKIP 并打印原因（AR-22 / AR-23、ADR-0016 / ADR-0017、`docs/spec/02` §5 UX-G11–UX-G17）。

## 3. 详细设计

### 3.1 仓库与模块边界

**3.1.1 目录布局**
```
termai/
  crates/   termai-{pty,vt,render,gpu,session,store,agent,tools,plugin-host,ipc,core,xtask,bench}
  apps/     termai-desktop  termai-headless  termai-web(后续)
  packages/ tokens  core-dto  plugin-sdk-ts  plugin-ui-runtime  webview-shell
  plugins/  官方插件（独立版本线）
  tests/    replay/  conformance/  visual/  fuzz/  matrix/
  docs/     adr/  spec/  roles/  audit/
```

**3.1.2 团队拓扑 ↔ CODEOWNERS 映射（Conway 落地）**

| 团队 | 拥有路径 | CODEOWNERS | 双签触发条件 |
| --- | --- | --- | --- |
| T1 Core Kernel | `crates/termai-{pty,vt,render,gpu,session,core,store,ipc}` | `@termai/core-kernel` | 改动 termai-ipc 消息布局、Session Log 格式、L0 渲染契约 |
| T2 Shell & UX | `apps/termai-desktop`、`packages/{tokens,webview-shell}` | `@termai/shell-ux` | 改动 token schema、L2 桥接接口 |
| T3 Agent & Context | `crates/termai-{agent,tools}` | `@termai/agent` | 改动 Context schema、RiskTier 语义、审计流 |
| T4 Platform & Ecosystem | `crates/termai-plugin-host`、`packages/{plugin-sdk-ts,plugin-ui-runtime}`、`plugins/` | `@termai/platform` | 改动 WIT 接口、权限清单词汇表 |
| T5 DevEx & Release | `crates/termai-xtask`、`.github/`、`.mise.toml`、`flake.nix` | `@termai/devex` | 改动门禁阈值、签名流程、CI 拓扑 |
规则：每个 crate / package 恰有一个 owner 团队；跨 owner 目录的 PR 两侧各一名 reviewer 批准（E4）；无 owner 目录一律拒绝合并；CODEOWNERS 的目录覆盖率由 `cargo xtask depcheck` 校验为 100%。

**3.1.3 依赖规则与强制手段**

| 规则 | 手段 | 失败处置 |
| --- | --- | --- |
| 无环、单向（E3） | `cargo xtask depcheck` 解析 cargo metadata + pnpm workspace 图，比对白名单文件 | 合并阻断 |
| 禁止反向依赖（L0/L1 不得依赖 L3/L4；L4 不得触 Core 内部） | crate 边界 + ESLint `import/no-restricted-paths` + dependency-cruiser | 合并阻断 |
| `apps/*` 不被任何库依赖 | depcheck 反向边检测 | 合并阻断 |
| 新增 crate / package 必须在 ADR 说明依赖位置；前端不得直读 terminal 内存 | PR 模板强制字段 + xtask 校验 ADR；跨边界禁 `unsafe` / 裸缓冲 | 合并阻断 |

### 3.2 构建与工具链

**3.2.1 锁定与可复现**

| 层 | 工具 | 锁定物 | 策略 |
| --- | --- | --- | --- |
| Rust | rustup | `rust-toolchain.toml`（channel + components + targets） | 固定 channel；每 minor 升级一次，升级必须跑通性能门禁 |
| 通用 | mise（开发）/ Nix flake（CI 与发布） | `.mise.toml`、`flake.lock` | CI 不使用开发者本机工具链；Nix 为发布环境唯一真源 |
| Node / TS | pnpm | `packageManager` 字段 + `pnpm-lock.yaml` | `--frozen-lockfile` 强制 |
| 依赖 | cargo | `Cargo.lock` 提交入库 | 发布构建禁用不稳定特性 |
| 产物 | cargo-dist + `cargo xtask` | `dist-workspace.toml`、子命令表 | 构建 / 签名 / 清单全部经 xtask 单一入口 |
`cargo xtask` 子命令表（禁止旁路脚本）：`depcheck` `fmt` `lint` `test` `bench` `perf-gate` `fuzz-smoke` `replay` `visual` `matrix` `sbom` `license` `dist` `sign` `manifest` `rollback`。

**3.2.2 交叉编译**

| 目标 | 方式 | 约束 |
| --- | --- | --- |
| linux-x64 / arm64（gnu + musl） | `cargo-zigbuild` 或容器镜像 | glibc 地板 = Ubuntu 22.04；AppImage 自带运行库 |
| windows-x64 | MSVC + `windows-rs` | 仅在 Windows runner 构建；mingw 产物不得发布 |
| macos-arm64 + x64 | 仅在 mac runner 构建，universal2 合并 | 公证与 hardened runtime 必过 |
| 禁止 | 跨 OS 伪造签名、在 Linux 上产出 macOS 公证产物 | — |

**3.2.3 CI 隔离签名**
1. 私钥仅存在于隔离 job 的临时密钥环，与构建 job 物理隔离（不同 runner、不同凭据域、不同网络出口）（E10）；CI 仅持短期 OIDC 凭据（ADR-0015 §D5）。
2. 流程：构建产物 → 计算哈希 → 隔离区**仅对哈希签名** → 发布 manifest 绑定「哈希 + 签名 + attestation」（与 ADR-0015 §D5 第 2 条一致）。
3. 私钥轮换周期 ≤12 个月，异常时吊销 ≤1h（对齐 R9）；签名 job 日志禁止出现密钥材料，macOS 公证使用短期 API Key。
4. **信任根分层与门限（ADR-0015 §D5）**：根密钥离线冷存储（FIPS 140-2 L3+ HSM + ≥2 把 YubiKey 5 FIPS），**门限 2-of-3**，仅用于签发 / 轮换发布子键与 TUF root，轮换 ≤24 个月；发布签名子键由云 HSM / KMS 托管且不可导出，**门限 2-of-3**（3 名 release manager 各持一次人工审批），轮换 ≤12 个月（与第 3 条一致）；插件 / 市场签名子键独立，轮换 ≤6 个月。新旧键重叠验证期 ≥30 天，TUF root 轮换新旧根双签。
5. **泄露应急时间线（不可协商，ADR-0015 §D5）**：T+0 冻结全部签名 job + 吊销 CI 临时凭据；≤1h 发布吊销清单与新 `timestamp` / `snapshot` 元数据、客户端 CRL 生效（R9）；≤24h 门限恢复新根 / 子键并发布紧急签名产物；≤72h 公开复盘 + 透明度日志补录。签名服务中断不判 nightly 失败（EXTERNAL 口径，§3.4.3），但阻断 stable 发布。

### 3.3 测试分层八件套

| # | 层 | 范围 | 工具 | 门禁阈值 | 触发 |
| --- | --- | --- | --- | --- | --- |
| L1 | 单元 | VT 解析 / reflow、能力令牌、布局纯函数、脱敏规则、风险分级 | `cargo test`、Vitest | 行覆盖 ≥80%（core/vt/store ≥85%）；关键不变式用属性测试 | 每 PR |
| L2 | 集成 | PTY 会话、存储重建、IPC 版本协商、插件宿主生命周期 | `cargo test --test`、临时 UDS / 命名管道 | 全绿；SQLite 索引可由 Session Log 完整重建且校验一致（DC-23） | 每 PR |
| L3 | 回放式 E2E | 录制真实会话字节流 + 输入脚本 → 断言最终网格与状态 | `tests/replay/` + `termai-headless` | 通过率 ≥99.5%；每个修复必须附回归语料 | 每 PR |
| L4 | 性能门禁 | 吞吐、延迟 P99、RSS、帧时、启动 | `termai-bench`(criterion) + 时序探针 | §5 全部门禁项；回归 >5% 阻断（B2 / §3.8.3） | PR 子集 + nightly 全量 |
| L5 | Fuzz | VT / PTY / IPC / 插件消息 | `cargo-fuzz` + libFuzzer，语料入库 | 24h 无 crash 且累计 ≥10⁸ 次执行（B5 / DC-37） | nightly + 周度长跑 |
| L6 | 兼容矩阵 | OS × 架构 × shell × 字体后端 × GPU 后端 × WebView 版本 | matrix runner + 场景脚本（cargo xtask matrix） | 4 shell（bash / zsh / fish / pwsh）× 3 字体后端（DirectWrite / CoreText / FreeType）= 12 组合的最低测试集 M12 全绿；偏差 100% 登记（§3.7、§8.2、ADR-0014） | nightly（承载机 = RM-B） |
| L7 | 视觉回归 | 固定字体 + 参考光栅器的像素比对 | Playwright（Web 层）+ 原生 golden | diff ≤0.1%，白名单外零变化（B3 / DC-14） | nightly |
| L8 | 泄漏检测 | 24h 长跑 RSS 斜率、fd / 句柄计数、GPU 资源 | `termai-bench --soak` + 进程采样 | 24h RSS 斜率 <1MB/h、漂移 <5%；sessiond 7×24 无泄漏 | 周度 + 发版前 |
补充：① 承载机分工（ADR-0014）：L4/L8 在 RM-A、L6 在 RM-B、L7 与帧时/延迟项在 RM-C，共用参考光栅器；跨机差异不得用阈值掩盖；② L4/L5/L8 输出同一 `bench-report.json` schema，本地与 CI 可直接对比；③ 新功能必须带单测，涉及 VT / IPC / 插件的必须带 fuzz 用例或回放语料（AGENTS §6）；④ 任何阈值放宽需附基准数据 + TSC 批准（AGENTS §5）。

### 3.4 CI 六件套与 PR 流水线

**3.4.1 六件套映射 + 设计门禁（§8.1，缺一不得合并）**

| # | 门禁 | 实现 | 阈值 |
| --- | --- | --- | --- |
| G1 | VT 兼容 | vttest / esctest / kitty 套件 + xterm 用例集 | 100% / 100% / 100% / ≥99%，差异登记 |
| G2 | 行为回放 | L3 | ≥99.5% |
| G3 | 视觉回归 | L7 | ≤0.1% |
| G4 | 性能门禁 | L4 | §5 门禁列全通过 + 回归 ≤5% |
| G5 | 依赖与许可 | cargo-deny / cargo-audit / npm audit / SBOM | GPL / AGPL 链接边界 = 0；高危 CVE = 0（无豁免） |
| G6 | 安全与 fuzz | L5 + 插件逃逸用例集 | 0 crash；逃逸用例 0 成功（AR-07、AR-08） |
| **G7** | **设计静态门禁** | `npm run design:check:static`（`tools/design-gates/check-design.mjs --static`，S1–S10，纯 Node 零依赖） | 全绿：折叠分区默认展开 ≤2、顶栏无「连接主机 / 新建工作区」、**图标控件** `aria-label` = 100%（口径含非 `<button>` 的 icon-only 交互元素：交互 `role` / `data-act` / `data-win` / `data-tip` / `tabindex` / `cursor:pointer`）、窗口控制为真实 `<button>`、主区唯一 view = 终端、rail 7 分区齐备、标尺外 spacing/radius/font-size/动效取值 = 0（字号按 **UI / 终端两条阶梯** 分别校验）、token 块零漂移、**硬编码色值 = 0 且 S8 默认阻断**、**键位三平台展开零冲突（S10：Action Registry 登记 mac / win / linux 三列，同平台零重复 chord，无 `Ctrl+Shift+Shift` 类无效组合，快捷键表 / 命令面板 / 菜单 / 文本提及均可反查登记）**（spec 02 A18/A21/A23/A25/A26/A31/A33/A36、UX-G11–UX-G14） |
| **G8** | **设计浏览器门禁** | `npm run design:check`（`tools/design-gates/browser.mjs`，Chrome headless + CDP，B1–B10） | 全绿：页面级零横向滚动（1600×1000 与 1280×800）、关软换行仍零横向滚动、tooltip 键盘可达、**侧栏面板 / 设置窗口打开时页面级纵向滚动 = 0（1600×1000 与 1280×800，内部滚动容器除外）**、设置窗口开/关/重开、主题跨窗口双向联动、tab「+」选择器键盘新增并激活、窗口控制 mac/win 两形态三态可还原；**键位行为与 Action Registry 一致（B10：用真实 CDP 按键事件验证 win `Ctrl+Shift+K` 开命令面板、裸 `Ctrl+K` 不开且留给终端、`Mod+Shift+L` 重分配为 `Ctrl+Alt+L`；mac `Cmd+K` 开、`Ctrl+K` 不开；AR-29 第 2/7 条）**；视觉回归在**动画冻结**下与基线比对（spec 02 A17/A19/A22/A28–A30/A32/A34、UX-G11/UX-G12/UX-G15–UX-G17） |

> **编号命名空间**：`UX-Gn` = 体验层门禁（spec 02），`Gn` = CI 层门禁（spec 07），引用时不得省略前缀。

**3.4.2 PR 流水线分级（反馈预算：中位 PR ≤15min）**

| 阶段 | 内容 | 预算 | 失败处置 |
| --- | --- | --- | --- |
| PR-S0 预检 | 许可头、Conventional Commits、AR/DC/ADR 关联、flag 黑名单枚举 | ≤1min | 阻断 |
| PR-S1 静态 | `cargo fmt --check`、`clippy -D warnings`、TS strict、token 零硬编码（DC-09） | ≤4min | 阻断 |
| PR-S2 快测 | L1 + L2 + L3 受影响子集 | ≤8min | 阻断 |
| PR-S3 门禁 | G1 / G4 子集 / G5 / `depcheck` | ≤10min | 阻断 |
| PR-S4 深检 | L4 全量、L5、L6、L7、L8、SBOM 归档 | 不占 PR 时延 | 阻断发版 |
| PR-S5 人工 | CODEOWNERS 双签；安全敏感改动需安全团队签 | SLA ≤24h | 阻断 |

> **编号消歧（AR-30 第 5 条）**：本节的 PR 流水线分级**统一写作 `PR-S0`…`PR-S5`**；§3.4.1 的**设计静态门禁保持 `S1`–`S10`**（`G7`）。两套编号不得再以裸 `S` 混用。

**3.4.3 通道流水线**：`main` 合并 → nightly（含 PR-S4 全量）→ `beta` 每 2 周切分支（依赖驱动变体见 OQ-E8）→ `stable` 月度。任一通道产物必须同源，可追溯到 commit + SBOM + 签名 manifest。

**nightly 中断口径（ADR-0014）**：nightly 缺席分两类。**FAIL-REPO**（main 上 PR-S0–PR-S4 任一阶段失败、测试失败、门禁红、产物损坏）**计入** A9 中断；**EXTERNAL**（签名 / 公证服务、HSM/KMS、上游 registry、云 runner 供给、网络出口或机房事故不可用）**不计入**，但须写 `nightly-exception.json`（时间 / 阶段 / 外部服务 / 证据链接 / 恢复时间 / 签署人），由 Release Owner（T5 当值）+ 1 名 maintainer **双人签署**；单人不得豁免，测试阶段失败时禁止使用 EXTERNAL，EXTERNAL 之夜不得推进 nightly 通道指针。连续 2 夜同一外部服务 EXTERNAL → P1 事故并 24h 内给出替代通道；任意原因连续 5 夜 → 发布工程复盘。**A9 核算**：滑动 30 天窗口内 FAIL-REPO = 0 且 EXTERNAL ≤6 夜、无连续 >2 夜 EXTERNAL，任一不满足则窗口重置计数。季度审计 EXTERNAL 占比 >20% 即视为 A9 不可信（走 Q2 审计）。

**CI 与签名成本口径（ADR-0014）**：`月成本 ≈ Σ_流水线（单次分钟数 × 月运行次数 × 该类 runner 单价）+ 签名/公证固定费 + 存储与出口`。单价口径：自托管物理 runner ≈ $0.04–0.08/核时；云托管 Linux 2 核 ≈ $0.008/min、Windows ≈ $0.016/min、macOS ≈ $0.08/min。v1 量级：36 组合矩阵 + L4/L6/L7 ≈ 660 min/夜；4 target fuzz 24h = 5,760 min/夜。**fuzz 与 soak 一律自托管，云托管 fuzz 机时 = 0**（否则单 fuzz 项即 >$13,000/月）。**月度 CI 硬上限 $3,000**（云托管 ≤$1,200 / 自托管运维 ≤$1,800），80% 告警、100% 起自动降采样非紧急流水线（视觉全量 → 隔夜、soak 周度 → 双周、E60 扩展套件 → 仅发版前）并公示；签名固定费量级 <$5,000/年。每次流水线输出 `ci-cost.json`（pipeline / minutes / runner_class / est_usd）。墙钟上限：PR 中位 ≤15min（A15）、nightly ≤6h。

**3.4.4 视觉回归方法学（动画冻结下渲染）**

1. **冻结动画**：所有截图渲染必须经 `--force-prefers-reduced-motion` 启动 Chrome，并在页面内叠加 CDP `Emulation.setEmulatedMedia(prefers-reduced-motion: reduce)`。原型唯一动画是终端光标的 `@keyframes caret`（200ms infinite alternate），冻结后由 reduced-motion 规则（`animation:none !important`、`--m-*:0s`）关闭。**未冻结时同一文件连渲 4 次可得到 3 个不同哈希**，视觉回归会在 CI 上随机失败。
2. **自检先于比对**：同一状态连渲两次，PNG SHA-256 必须相同；不相同直接判 `FAIL`（`ANIMATION NOT FROZEN`），不得继续与基线比对。
3. **静止窗口**：等待 `document.fonts.ready` + 双 `requestAnimationFrame` + ≥600ms 稳定（原型在 `fonts.ready` 与 60ms / 120ms 定时器中运行 `foldFit()` / `sbFit()`，截取中间态会得到另一个自洽但与基线不同的帧），并先渲一张 warm-up 丢弃，再取两张比对帧。
4. **容差**：默认口径为「单通道差 >±2 的像素数 = 0」（±2 仅吸收圆角与合成的亚像素舍入；实测噪声 218 px、最大通道差 2），亦可用 `--max-diff-pct=0.1` 采用 §5 的 ≤0.1% 口径；**任何失败必须打印差异 bbox**，禁止只报百分比。
5. **基线**：存于 `prototype/baseline/<platform>-<arch>-<state>.png` 并随代码入库；平台缺基线时该状态显式 `SKIP` 并打印生成命令（`npm run design:baseline`），不静默通过。基线更新必须与视觉变更同 PR，并由 T2 Shell & UX 复核（§3.1.2）。
6. **实现约束**：本阶段唯一实现物是单文件原型，浏览器层由 **Chrome headless + DevTools Protocol（Node 内建 WebSocket，零外部依赖）** 驱动；§3.3 L7 的 Playwright 待 L2 Web 面板落地后接入，门禁语义（冻结动画、双渲自检、基线比对、容差与 bbox）保持一致。

### 3.5 发布工程

**3.5.1 通道模型**

| 通道 | 触发 | 受众 | 稳定性承诺 | 回滚对象 |
| --- | --- | --- | --- | --- |
| nightly | `main` 合并 | 12 人 dogfood + 自选 | 无 | 仅撤回指针（中断 / 豁免口径见 §3.4.3、ADR-0014） |
| beta | 双周切点 | 志愿者 cohort ≤5% | 无 | 撤回指针 + flag kill-switch |
| stable | 月度 | 全量 | §5 / §8 全门禁 | ≤15min 全量回滚（B7） |

**3.5.2 flag 与灰度**
1. 三类 flag：`release`（新功能）/ `ops`（降级与 kill-switch）/ `experiment`（A/B）；**禁止**任何影响审批语义、脱敏、审计、GPL 依赖边界的 flag（C4 / C6 / C7 / C8），PR-S0 阶段枚举清单并校验黑名单为空。
2. 灰度阶梯：内部 dogfood → beta 1% → 5% → 25% → 100%，每级观察窗 ≥24h；观察指标 = crash-free session、L4 关键项、AI 危险命令率（DC-31）、盲确认率（R6）。
3. flag 生命周期 ≤2 个 minor，到期必须删除代码路径与 flag（CI 校验过期项 = 0）；服务端下发 flag 的能力不得用于远程开启数据上传（AR-12）或关闭确认（AR-06）。

**3.5.3 回滚设计（端到端 ≤15min）**
1. **撤回 channel 指针**：CDN manifest 指向上一 green 产物的哈希 + 签名，TTL ≤60s，完成 ≤5min。
2. **kill-switch**：`ops` flag 关闭新代码路径，服务端下发，P95 生效 ≤2min。
3. **已升级客户端**：update-check 间隔 ≤10min 且网关可下发强制检查；故从决策到「新下载与在册客户端回到上一 stable 行为」端到端 P99 ≤15min（B7）。
4. **不可降级的破坏性变更**（如存储格式）必须先向后兼容再发布：前向迁移器 + 回读兼容 ≥2 大版本（E5 / DC-23），禁止单向迁移上 stable。
5. 每季度执行一次**回滚演练**并登记实测耗时；演练未通过则下一次 stable 发版阻断（A8）。

### 3.6 崩溃与遥测

| 维度 | 设计 | 依据 |
| --- | --- | --- |
| 端点 | 自托管 Sentry 兼容端点（受我方控制面运营，仅接收崩溃与性能聚合） | 角色 10 §3、AR-12 |
| 默认状态 | 崩溃上报开关独立且默认关闭；Telemetry 默认关闭（opt-in） | AR-12、E9 |
| 上传前提 | 显式同意 + 客户端侧脱敏（正则 + 熵检测 + keychain 反向匹配）后上传；脱敏失败即拒绝发送并告知 | DC-33、C7 |
| 内容零采集 | 无命令文本、路径、代码、域名、环境变量值；仅 OS / 架构 / 版本 / 符号化崩溃栈 / 帧时直方图 / IPC 往返延迟 | AR-12 |
| 审计与最小集 | 出站请求可分类、可在 UI 回看；遥测关闭时上传字节 = 0；仅本地聚合计数 + 采样；session id 贯通原生与 WebView，W3C traceparent | AR-12、§8.2、角色 05 §3.9 |
| 用户导出 | 环形缓冲 + `:trace` 命令，仅用户显式导出才出设备 | 角色 05 §3.9 |
| 符号化 | 符号表在 CI 构建时上传自托管后端；发布产物剥离源码路径信息 | 隐私最小化 |

### 3.7 平台矩阵与降级策略

| OS | 版本地板 | 架构 | v1 状态 | PTY / 进程 | 主分发（次分发） | 一级 GPU 后端 |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | 10 1809（17763）+ / 11 | x64 | 一等公民（门禁） | ConPTY 唯一生产路径 + Job Object 进程树 | MSI（MSIX / portable ZIP / winget） | DX12（FL12_1） |
| Windows | 同上（参考机 = 11 24H2） | arm64 | 非 v1；P2 进入 | ConPTY（arm64） | （P2：MSI arm64 / ZIP） | DX12 |
| macOS | 13.6 Ventura+ | arm64 | 一等公民（门禁） | forkpty / openpty | notarized DMG universal2（.app ZIP / brew cask） | Metal 3 |
| macOS | 13.6 Ventura+ | x64 (Intel) | 支持但无性能门禁 | forkpty / openpty | notarized DMG universal2 | Metal 3 |
| Linux | glibc ≥2.35（Ubuntu 22.04 LTS）/ Fedora 38+ | x64 | 一等公民（门禁） | forkpty / openpty | AppImage（.deb / .rpm / Flatpak / tar.gz） | Vulkan 1.3 |
| Linux | 同上（参考机 = Ubuntu 24.04.1 LTS） | arm64 | 支持（headless 一等 / 桌面 Beta），无性能门禁 | forkpty / openpty | tar.gz + .deb（AppImage Beta） | Vulkan 1.3 |
| headless | Win 10 1809+ / macOS 13.6+ / glibc 2.35+ | x64 + arm64 | 一等公民 | 无 GPU 呈现 | 静态 tar.gz / zip | 无 |

**架构准入（ADR-0014）**：v1 门禁架构 = Windows x64 / Linux x64 / macOS arm64。macOS x64（Intel）v1 提供构建与发布、跑 L1–L3 与 L6 子集，不做性能门禁，P3 复核是否淘汰。**Linux arm64** v1 提供 headless（一等）与桌面 Beta 产物（跑 L1–L3、L5、L6 子集）；**升为门禁架构的 P2 门槛**：① Ubuntu 24.04 arm64 与 Fedora 40 arm64 上 L6 子集全绿；② 具备一台 arm64 RM-C 等效机（Snapdragon X Elite / Ampere Altra）；③ Vulkan 一级后端在两大发行版均达 §5 门禁。**Windows arm64 非 v1，进入 P2**；**P2 门槛**：① ConPTY arm64 可用且 Job Object 进程树语义与 x64 一致；② WebView2 arm64 evergreen 可装载；③ Snapdragon X Elite 参考机并入 RM-A / RM-C；④ L6 子集全绿。进入后标 Beta 一个 minor，只出 native arm64 产物、不提供 x64 模拟路径。未列入本表的 OS / 架构一律标注「不支持」，不用「也许能跑」措辞（AR-20）。

**Shell 与字体后端枚举（ADR-0014 §决策 3）**：4 shell = **bash**（macOS 系统 3.2 / Linux ≥5.1；Windows 侧走 MSYS2 / Git for Windows 原生 x64 ≥5.2，集成脚本必须 3.2 安全、禁 4.x-only 语法）、**zsh ≥5.8**、**fish ≥3.3**、**pwsh ≥7.2 LTS**（推荐 7.4）。3 字体后端 = **DirectWrite**（Windows：IDWriteFontCollection / IDWriteFontFallback 枚举与回退 + 度量）、**CoreText**（macOS：CTFontCreateForString cascade list + 度量）、**FreeType/fontconfig**（Linux：fontconfig 匹配 + FreeType 度量）。三者互斥、按 OS 原生提供；栅格化统一由 swash 生成 GPU atlas（DC-17），三者均不启用系统 subpixel AA（AR-14）。WSL 是 Transport，不计入 shell 矩阵；Windows PowerShell 5.1 不列入集成矩阵（OSC 633 门禁只对 pwsh ≥7.2）。

**12 组合与最低测试集 M12（ADR-0014）**：bash / zsh / fish / pwsh × DirectWrite / CoreText / FreeType = 12 组合**全部 v1 必修**，每组合跑 M12 全绿：① 冷启动到可输入（含集成脚本注入）② CJK 输入与三档回退命中 ③ emoji 彩色/单色 + ZWJ ④ IME 组合输入与候选窗定位 ⑤ reflow 80↔120 列 ⑥ 宽字符折行（双宽 + 组合字符 + 变体选择符）⑦ 连字开关 ⑧ OSC 133/633 命令边界与退出码 ⑨ OSC 7 / 0 / 2 ⑩ 复制/粘贴语义（bracketed paste）⑪ 集成失效降级（提示符启发式 + 低置信度标注）⑫ 能力查询（DA / kitty query、$TERM、terminfo）。4 个主场组合（bash×FreeType、zsh×CoreText、fish×FreeType、pwsh×DirectWrite）额外跑扩展套件 E60（≥60 用例：Sixel / kitty graphics、OSC 9/777、bidi、超长行与 10k 行回翻、多分屏、Kitty keyboard 探测、OSC 8 超链接等）。

**GPU 后端四级与降级（ADR-0014 §决策 4）**

| 等级 | 后端 | 能力 / 用户可见行为 |
| --- | --- | --- |
| T0 一级（门禁） | DX12（Windows）/ Metal（macOS）/ Vulkan 1.3（Linux） | 完整能力；**§5 门禁项只能在此档判定** |
| T1 二级 | Windows Vulkan；Linux OpenGL 4.5/EGL；macOS OpenGL 4.1 | 网格与外壳完整；关闭 VRR 与高刷优化；状态栏明示后端 |
| T2 软件光栅 | Windows WARP；Linux lavapipe；macOS term-render CPU 光栅 | 仅文本网格；**禁用** Sixel / kitty graphics / iTerm2 图形与模糊阴影；≤60Hz |
| T3 安全模式 | 无 GPU 呈现加速 | 不加载 L2 WebView（纯原生文本面板，AR-02）；禁用图形协议与动画；给出诊断导出入口 |

降级触发：能力探测缺必需特性 → 按 T0→T1→T2→T3 顺延选择首个可用后端；DeviceLost / 驱动 TDR → 重建 device + atlas（目标 ≤2s，会话不中断，sessiond 独立进程 AR-13），同会话 10 分钟内 3 次则降一级；T2 初始化或呈现失败 → T3。**凡运行在非 T0 后端，性能与兼容门禁一律判失败**；降级须产结构化事件并写本地审计，能力边界在 UI 明示（AR-20），配置键 `gpu.backend = auto | dx12 | vulkan | metal | gl | warp | lavapipe | software | safe`（手动值不可用时回退 auto 并提示）。**WebGPU 不在网格热路径**（AR-01），桌面 v1 不以 WebGPU 作为网格后端；WebView 后端不可用只影响 L2 面板。

**WebView 版本与偏差登记**：Windows 默认 evergreen、企业可 fixed（OQ-03）；macOS 用系统 WKWebView；Linux 最低 WebKitGTK 2.40+ 并维护碎片化清单（R3、R8）。WebView 缺失或不可信 → 纯原生文本面板（AR-02、OQ-02），不阻塞核心可用性。所有已知偏差登记差异登记表（附最小复现 + 豁免期限 + owner），未登记偏差视为门禁失败。

### 3.8 基准与性能预算的测量方法

**3.8.1 参考机与受控变量（ADR-0014）**：采用三档自托管物理参考机，§5 每一项门禁数字**只绑定其中一档的一级（T0）GPU 后端**；云 runner 结果标记 NON-GATING，不得顶替。

| 档位 | 定位 | 关键规格 | 承载的门禁项 |
| --- | --- | --- | --- |
| **RM-A 门禁基准机** | 计算类门禁唯一判定机 | AMD Ryzen 7 7700X（8C/16T）或等效（Cinebench R23 单核 ≥1900 / 多核 ≥14000）；32GB DDR5-5200；1TB PCIe 4.0 NVMe（≥6.5GB/s）；RTX 4060 8GB（驱动锁定）；2560×1440@165Hz；Win 11 24H2 / Ubuntu 24.04.1 LTS / macOS 14.5（Mac mini M2 Pro 32GB） | 冷启动、吞吐、空闲 RSS、24h RSS 斜率、插件宿主空载 |
| **RM-B 主流开发机 / 支持地板** | 声明最低支持规格；承载 L6 矩阵与地板阈值 | ≥4 物理核 / 8 线程（i5-1235U / R5 7530U 或等效，R23 单核 ≥1200）；16GB；NVMe ≥2GB/s；集成显卡；1920×1080@60Hz（DPI 100%/125%/150%）；Win 11 24H2 / Ubuntu 24.04 LTS / macOS 13.6（MBA M1 8GB） | 不判定 §5 门禁；判定地板阈值 F1–F6：冷启动 ≤300ms、key-to-photon P99 ≤33ms、1080p60 帧时 ≤16.7ms、吞吐 ≥250MB/s、空闲 RSS ≤160MB、安装包 <60MB |
| **RM-C 高刷 / 4K 机** | 显示与延迟类门禁唯一判定机 | 同 RM-A 的 CPU/内存/存储 + RTX 4070 12GB（或 RX 7800 XT）；macOS 侧 Mac Studio M2 Max；主 3840×2160@144Hz（门禁锁 120Hz）+ 副 2560×1440@240Hz；VRR / HDR 关闭 | 4K@120Hz 帧时、key-to-photon P99、网格对齐（100%/125%/150%/200% DPI）、视觉回归 golden |

受控变量：参考机为自托管物理机；测量期禁用自动更新 / 屏保 / 索引 / 休眠；Windows 固定「高性能」电源计划、Linux 用 performance governor、macOS 禁止 App Nap；环境变量与 shell 配置由 xtask 注入，预热 2 次丢弃。每台登记 `machine-fingerprint.json`（CPU + 微码 / 内存 / GPU + 驱动版本 / 显示器与刷新率 / OS build / 电源计划），任一字段变化即**重置基线并在 `docs/audit/` 公示**（对应 Q12）。等价替换规则：CPU 单核与多核相对基准机劣化 ≤5%，GPU 同 API 等级且驱动锁定，显示器像素与刷新率不低于门禁项要求。

**3.8.2 测量脚本与报告契约**：`cargo xtask bench --profile release --metric <all|startup|latency|throughput|rss|frame>` 输出版本化 `bench-report.json`：`{metric, value, unit, samples, runner, commit, toolchain, ts}`。本地与 CI 共用同一脚本与 schema，差异只允许来自硬件。

| 指标 | 方法 | 采样 |
| --- | --- | --- |
| 冷启动到可输入 | release 二进制 + 计时探针（首个输入被接受的时刻），取 P95 | ≥100 次 |
| key-to-photon | 内置时序探针（P50 / P99）+ 高速相机（≥1000fps）校验探针偏差 | ≥10⁵ 事件 |
| 4K@120Hz 帧时 | 合成基准（全 damage 网格）+ 丢帧率统计 | ≥10⁴ 帧 |
| 解析+渲染吞吐 | 1GB 语料 `cat` 无回压，计算 MB/s | ≥10 次中位数 |
| RSS | 1 万行 scrollback 稳定态采样（RSS 与峰值） | 稳定 60s 后 30 次 |
| 24h RSS 斜率 | 长跑线性回归斜率（MB/h） | 24h 连续 |
| 插件宿主空载 | 独立进程采样（仅启用插件时计入） | 30 次 |
| 安装包 | 压缩后产物度量 | 每次构建 |
| AI 网关附加延迟 / 诊断首 token | 服务端 SLO 埋点 + 在线埋点 P95 / P99 | 滚动 7 天 |

**3.8.3 回归判定（统一口径，B2）**
1. 同一指标在**同一参考机**取 N≥10 次有效运行的**中位数**为报告值，丢弃前 2 次预热；尾部指标（P99）样本数 ≥10⁵ 才参与判定。
2. 噪声门限：基线运行的 `MAD / 中位数 > 2%` 时先修测量环境，结论标记 `INCONCLUSIVE`（不判失败）。
3. **回归判定 = G4-PR / G4-REL 双口径（AR-27）**：**G4-PR（合并阻断）** = 单次有效运行中位数劣化 >5% 即阻断合并（与 HARNESS §8.1-4 / §2.2 B2 一致）；**G4-REL（发版阻断）** = 中位数劣化 >5% 且**连续 2 个夜间运行复现**，或发版前全套运行 1 次复现，即阻断发版。两道门均以第 1–2 条为前置：**测量本身必须先通过可复现性自检**（同 commit、同机器指纹连测两次，verdict 不得翻转），未通过者判 **INVALID**，不得与基线比对、不得据此阻断。
4. 门禁值与目标值分列输出（AR-19），达标余量写入技术债登记（§3.9）；失败项自动关联最近的性能相关 PR 与 flag 状态并输出归因建议。
5. **机器绑定（ADR-0014）**：判定只在 §3.8.1 的 RM-A / RM-C 上进行，且必须运行在 T0 一级 GPU 后端；云 runner 与降级后端（T1/T2/T3）上的数值一律标记 NON-GATING，不得作为门禁结论。参考机不可用时门禁项标 INCONCLUSIVE，不得用云 runner 顶替。

### 3.9 技术债与架构审计节奏

| 节奏 | 动作 | 产出 | 责任 |
| --- | --- | --- | --- |
| 每 PR | 债务项在 PR 描述登记，或声明新增债务 = 0 | 债条目 | 作者 + owner |
| 每版本 | 偿还 ≥1 项 P1 债；性能达标余量入登记 | `docs/audit/debt-<ver>.md` | T5 |
| 每季度 | **依赖图报告**：cargo-depgraph + pnpm 依赖图；环检测、越权边、超出 owner 的边、`apps/*` 反向边 | `docs/audit/deps-<yyyyQn>.md` | T5 + 各 owner |
| 每季度 | 架构审计：crate 边界 vs 团队边界漂移、CODEOWNERS 命中率、架构类变更的 ADR 覆盖率 | 同上 + 周会输入（R8） | 架构师 |
| 每半年 | 技术栈体检：vte 上游状态与替换评估（AR-18）、WebView 碎片化、GPU 后端分布 | 评估报告 + 可能的 ADR | T1 / T2 |
| 每 12 个月 | 许可与供应链复核（AR-21、R12）；治理复核（OQ-19） | 合规报告 | T5 + 法务 |
审计发现「未登记反向依赖」「无 owner 目录」「过期 flag」三类一律开 P1 债，并在下一版本前关闭。

### 3.10 ADR / RFC 治理

| 触发 | 流程 | 时限 | 批准 |
| --- | --- | --- | --- |
| 新增 / 删除模块、改变依赖方向 | RFC → ADR | 公示 ≥5 工作日 | TSC |
| 改变对外契约（termai-ipc、Local API、WIT、Session Log 格式） | RFC → ADR + capability 协商 | 公示 ≥5 工作日 | TSC + 2 maintainer（破坏性 WIT） |
| 改变 AR 结论、放宽预算 / 门禁、变更许可或数据边界 | RFC → ADR | **90 天公示** | TSC 2/3 |
| 引入新依赖类别（原生 dylib、网络库、GPL 邻接） | RFC + 安全 / 法务会签 | 公示 ≥5 工作日 | TSC + 法务 |
| 破坏性 WIT 变更 | RFC + codemod | N-2 minor 兼容窗口（DC-40） | 2 maintainer（E6） |
ADR 模板字段：背景 / 选项（含**被否决项与反方最强理由**）/ 决策 / 后果与代价 / 复议条件 / 关联 AR-DC / 生效日期。ADR 不可改写，被取代时在旧条目标注「被 ADR-xxxx 取代」（HARNESS §12）；全部归档 `docs/adr/`，CI 校验架构类改动是否携带 ADR 引用。

### 3.11 SBOM 与许可合规流水线

| 步骤 | 工具 | 门禁 |
| --- | --- | --- |
| Rust 依赖策略 | `cargo-deny`（bans / licenses / advisories / sources） | 未知许可 = 阻断；GPL / AGPL = 阻断（C8） |
| 漏洞 | `cargo-audit` + Renovate / Dependabot；TS 侧 `pnpm audit` | 高危 = 阻断（无豁免）；中危 ≥7 天未修 = 阻断发版 |
| SBOM | CycloneDX（Rust: `cargo-cyclonedx`；TS: `@cyclonedx/cyclonedx-npm`），合并为每产物单份 | 缺失 = 阻断发版 |
| 许可允许列表 | Apache-2.0 / MIT / BSD-2-3-Clause / ISC / Unicode-3.0 / Zlib 等（完整白 / 黑名单见 ADR-0015 §D4）；MPL-2.0 允许但禁止内联进我方文件（ADR-0015 §D3） | 出界 = 阻断，需 ADR |
| 产物溯源 | 签名 manifest + provenance attestation（SLSA 目标见 ADR-0015 §D6） | 缺失 = 阻断发版 |
| 模型供应链 | 权重哈希 + 通道签名 + 离线校验（DC-36） | 校验失败 = 拒绝加载 |
| 插件签名 | 签名校验 + 透明度日志 + 远程吊销 ≤1h（R9） | 失败 = 100% 拒装（P3 门禁） |
「链接边界」的正式定义与判定**以 ADR-0015 §D1 / §D2 为唯一口径（ADR-0015）**：边界三原则 —— **P1 同进程即入界**（静态 / 动态 / dlopen 不改变判定）、**P2 派生即继承**（复制 / 修改 / 骨架生成继承来源 SPDX）、**P3 未知即拒绝**（空 / `NOASSERTION` / `LicenseRef-*` / 无许可文本 = 拒绝）；判定三档 **A（允许）/ R（法务 + maintainer 双签，≤180 天）/ D（禁止，任何例外不得覆盖）**；完整场景表 **LB-01…LB-18** 见 ADR-0015 §D2，SPDX 白 / 黑名单与表达式求值见 §D4，弱 copyleft 准入见 §D3（MPL-2.0 允许但禁止内联进我方文件；LGPL-2.1/3.0 仅允许未修改的动态链接；EPL-2.0 / CDDL 默认禁止、逐案审批）。边界清单由 `cargo xtask license --boundary` 输出 `license-boundary.json`（逐单元 `{name, version, spdx, scope, edge_type, in_boundary, verdict}`）供审计与法务复核；**构建期独立进程工具与运行时 subprocess 位于边界外**（须满足 BT / T / AE 条件并登记），旧口径「build-dependencies 计入」不再沿用。

## 4. 接口与依赖

### 4.1 对内模块契约（本规格提供）

| 接口 | 消费者 | 契约要点 |
| --- | --- | --- |
| `cargo xtask <verb>` 子命令表 | 全部团队 | 唯一构建 / 门禁入口；增删走 ADR；输出 JSON schema 版本化 |
| `bench-report.json` schema | T1 / T2 / T4、发布经理 | 字段稳定；新增字段向后兼容 1 个 minor |
| 门禁报告格式（G1–G6） | PR 检查、发布门禁 | 每项含 status / threshold / actual / artifact 链接 |
| `depcheck` 白名单文件与 flag manifest | 全部团队、发布经理 | 依赖白名单是可审查单一文件；flag 记录名称 / 类型 / owner / 到期 minor / 灰度阶梯，黑名单校验（C4、C6） |
| CODEOWNERS | GitHub + 审计脚本 | 与 §3.1.2 表一一对应；xtask 校验无未覆盖目录 |
| SBOM + 签名 manifest | 安全、法务、企业客户 | 每产物一份，含哈希、签名、SBOM、attestation |

### 4.2 对外契约（不得破坏）

| 契约 | 兼容承诺 | 依据 |
| --- | --- | --- |
| Local API（JSON-RPC over UDS / named pipe，含 apiVersion） | 只维护一套契约；N-2 minor + 6 个月弃用 | DC-24、DC-40 |
| termai-ipc（二进制 + 版本 / 能力协商） | 破坏性变更走 capability 协商；兼容窗口 ≥2 minor | DC-22、AR-04 |
| Session Log 格式 + SQLite 索引 | 迁移器兼容 ≥2 大版本；索引可完整重建 | DC-23 |
| WIT 插件 ABI | N-2 minor + codemod + 2 maintainer | DC-40、AR-07 |
| 主题 schema（纯数据） | 版本化 + 可选签名；永久免费开放 | DC-12 |
| 发行产物与通道 | 三通道命名、manifest 格式、回滚语义稳定 | §3.5 |
| 遥测契约 | 内容零采集；出站可分类可回看；关闭时上传 0 字节 | AR-12 |

### 4.3 协作与相互依赖

| 对方 | 我承诺交付 | 对方必须给我 |
| --- | --- | --- |
| Core Kernel（04） | 门禁与性能测量脚本、参考机口径、回滚演练机制 | 帧预算分解、GPU 降级需求、fuzz 语料与差异登记表 |
| Shell & UX（05） | 视觉回归基线流程、token 零硬编码检查、安装包度量 | 原生 golden 与参考光栅器、Playwright 用例、a11y 自动化用例 |
| Agent & Context（07） | eval 合入门禁接线、脱敏失败拒绝发送的 CI 用例 | 离线 eval 集、危险命令率统计口径、工具清单与 RiskTier |
| Security（08） | fuzz 长跑平台、逃逸用例接线、SBOM、CVE SLA 管道 | 威胁模型、脱敏规则库、CVE 响应 SLA、红队用例集 |
| Platform / Ecosystem（09） | 插件签名校验与吊销管道、TTFHW 度量脚本 | SDK SemVer 策略、市场审核 SLA、TTFHW / 热重载指标 |
| 产品（01）与云平台（06） | 遥测最小集与 opt-in 流程；控制面 CI 静态检查（prompt 字段 = 0） | NSM 与验收口径、可用性测试窗口；服务端 SLO 埋点、AI 网关延迟结构 |

## 5. 验收标准（可量化）

| # | 验收项 | 阈值 | 依据 |
| --- | --- | --- | --- |
| A1 | HARNESS §5 全部门禁项 | 全部通过；回归 >5% 阻断 | §5、§8.1-4、AR-19 |
| A2 | G1–G6 六件套 | 全绿；缺一不得合并 | §8.1 |
| A3 | 行为回放通过率 | ≥99.5% | §8.1-2 |
| A4 | 视觉回归 diff | ≤0.1%，白名单外零变化 | §8.1-3、DC-14 |
| A5 | fuzz | 24h 无 crash 且 ≥10⁸ 次执行；历史 crash 100% 转回归 | DC-37、§8.1-6 |
| A6 | 兼容矩阵 | bash / zsh / fish / pwsh × DirectWrite / CoreText / FreeType = 12 组合的 M12 全绿；偏差 100% 登记 | §8.2、§3.7、ADR-0014 |
| A7 | 泄漏检测 | 24h RSS 斜率 <1MB/h；sessiond 7×24 漂移 <5% | §5、§8.2 |
| A8 | 回滚 | 任一 stable ≤15min 全量回滚；季度演练实测通过 | §8.2、B7 |
| A9 | nightly 稳定性 | 滑动 30 天窗口内 FAIL-REPO = 0、EXTERNAL ≤6 夜且无连续 >2 夜 EXTERNAL（§3.4.3、ADR-0014） | §8.2 |
| A10 | 依赖与许可 | GPL / AGPL 链接边界 = 0；每产物 SBOM 齐全 | §8.1-5、AR-21 |
| A11 | 遥测 | 关闭时上传 0 字节；默认配置抓包零用户未发起连接；100% 出站可在 UI 回看 | §8.2 |
| A12 | 可靠性 | crash-free session >99.9%；UI 崩溃 <2s 重连且屏幕一致 | §8.2 |
| A13 | 命令面板 | 功能覆盖率 100%（动作表与菜单差集为空） | DC-08 |
| A14 | 治理 | 每季度依赖图报告产出；架构类变更 100% 带 ADR 引用 | §3.9、§3.10 |
| A15 | 反馈时延 | 中位 PR ≤15min 得到 PR-S0–PR-S3 结论 | §3.4.2 |
| A16 | 插件边界 | 越权 100% 被拒并留审计；插件 UI 崩溃 0 次影响会话 | §8.2、DC-38 |
| **A17** | **设计静态门禁**（`npm run design:check:static`，S1–S10） | 全绿；标尺外取值 = 0（字号按 UI / 终端两条阶梯）；图标控件 `aria-label` = 100%；token 块零漂移；硬编码色值 = 0 且 S8 默认阻断；**键位三平台展开零冲突（S10）**；合并阻断 | spec 02 UX-G11–UX-G14、AR-22 / AR-23 / **AR-29 第 7 条** |
| **A18** | **设计浏览器门禁**（`npm run design:check`，B1–B10） | 全绿：零横向滚动、页面级纵向滚动 = 0（侧栏/设置窗口打开，1600×1000 与 1280×800）、tooltip 键盘可达、设置窗口开/关/重开、主题跨窗口双向联动、tab「+」键盘新增并激活、窗口控制两形态三态可还原、**键位行为与 Action Registry 一致（B10，AR-29 第 7 条）**、动画冻结视觉回归通过；合并阻断 | spec 02 UX-G11 / UX-G12 / UX-G15–UX-G17、AR-22 / AR-23 / **AR-29 第 7 条** |

## 6. 风险与缓解

| # | 风险 | 触发条件 | 缓解 | 关联 |
| --- | --- | --- | --- | --- |
| Q1 | 性能预算渐进侵蚀 | 冷启动 >150ms 或延迟 P99 >16ms 连续两版 | 门禁进 CI，回归即阻塞；达标余量记债 | R7 |
| Q2 | 门禁被橡皮章化（豁免泛滥） | 单季度豁免 >3 次，或同一门禁连续豁免 | 豁免需 TSC 批准 + 到期日；季度审计统计豁免率 | §5 |
| Q3 | 测试矩阵成本失控（12 组合 × 3 OS） | 夜间流水线 >6h 或排队 >1 天，或月度 CI >$3,000 | 分层抽样（M12 最低集 + 4 主场 E60）；fuzz / soak 自托管；成本超 80% 自动降采样（ADR-0014） | §3.3、§3.4.3 |
| Q4 | 签名 / 公证中断发布 | CI 证书或 Apple 服务不可用 | 备用证书 + 离线签名 runbook；EXTERNAL 豁免（双人签署 + 证据 + 不投递指针），连续 2 夜开 P1（ADR-0014） | R4、§3.4.3 |
| Q5 | 视觉回归误报腐蚀信任 | 白名单膨胀，或同机 diff 抖动 >0.1% | 固定字体 / 光栅器 / 参考机；白名单需 owner 双签；先修环境再判定 | R7 |
| Q6 | fuzz 语料与真实流量脱节 | 修复后同类 crash 复发 | 语料 = 回放语料 + 历史 crash + 合成；覆盖率看板 | DC-37 |
| Q7 | 团队 / 模块边界漂移 | 跨 crate 私改频繁、CODEOWNERS 命中率下降 | 季度依赖图 + 架构审计；未登记反向依赖开 P1 | R8 |
| Q8 | Linux WebView 碎片化破坏矩阵 | ≥2 发行版插件 UI 渲染异常 | WebView 设为可选依赖 + 原生降级面板；固定最低版本 | R3、R8 |
| Q9 | 许可 / 供应链事件 | 新依赖许可越界或出现高危 CVE | cargo-deny 白名单 + 月度依赖复核 + SBOM 归档 | R12 |
| Q10 | 遥测被功能需求侵蚀 | 出现「上传历史换同步」类诉求 | 控制面-only 写入不可协商清单；CI 静态禁止 prompt 与内容字段 | R11 |
| Q11 | 回滚演练缺失导致真实回滚超时 | 演练实测 >15min 或未按季度执行 | 演练进发版门禁清单（A8）；记录耗时趋势 | A8 |
| Q12 | 参考机漂移使基准不可比 | 参考机重装 / 硬件替换 / 混用云 runner | `machine-fingerprint.json` 指纹登记；替换需重设基线并公示；云 runner 仅 NON-GATING（ADR-0014） | §3.8.1、§3.8.3 |

## 7. Open Questions
> 下列为 HARNESS 未覆盖或未仲裁的空白。本规格不自行发明结论；编号为本规格独立的 `OQ-E*`。

| # | 问题 | 影响面 | 建议 | 需决策阶段 |
| --- | --- | --- | --- | --- |
| OQ-E1 | ~~参考机 SKU 与「同机可比」的定义~~ | 性能门禁的可复现性与跨版本可比性（§3.8），牵动全部 §5 门禁项 | **已裁决**：三档自托管参考机 RM-A（门禁机）/ RM-B（支持地板 + L6）/ RM-C（4K 与延迟）；门禁项一一映射、指纹化登记，云 runner 仅 NON-GATING（§3.8.1） | **已关闭（ADR-0014）** |
| OQ-E2 | ~~平台矩阵的枚举~~ | L6 门禁范围与测试成本；影响可支持平台声明 | **已裁决**：4 shell = bash（3.2/≥5.1，Windows 走 MSYS2 ≥5.2）/ zsh ≥5.8 / fish ≥3.3 / pwsh ≥7.2；3 字体后端 = DirectWrite / CoreText / FreeType；12 组合 M12 全绿。v1 门禁架构 = Win x64 / Linux x64 / macOS arm64；Linux arm64 v1 支持（headless 一等）P2 升门禁；**Windows arm64 非 v1，进入 P2**（门槛见 §3.7） | **已关闭（ADR-0014）** |
| OQ-E3 | ~~「nightly 连续 30 天不中断」的失败口径~~ | 门禁 A9 的可达成性；有被假绿灯掩盖真实失败的风险 | **已裁决**：FAIL-REPO 计入、EXTERNAL 不计入但需双人签署 + 证据 + 不投递指针；连续 2 夜同因 → P1、任意 5 夜 → 复盘；A9 = 30 天窗口 FAIL-REPO = 0 且 EXTERNAL ≤6 夜且无连续 >2 夜（§3.4.3） | **已关闭（ADR-0014）** |
| OQ-E4 | ~~**构建溯源强度**：是否强制 SLSA 级别与产物 attestation？~~ | 发布链安全、企业合规销售、CI 复杂度 | **已裁决（ADR-0015 §D6）**：P0–P2 强制 **SLSA Build L2** 地板（缺 provenance 即阻断发版）；P3 退出前达 **L3**；明确不以 L4 为目标 | **已关闭（ADR-0015）** |
| OQ-E5 | ~~**「链接边界」的操作定义**~~ | 门禁 G5 的判定口径，存在误伤与漏判双向风险 | **已裁决（ADR-0015 §D1 / §D2）**：三原则 P1 同进程即入界 / P2 派生即继承 / P3 未知即拒绝；判定三档 A / R / D；场景表 LB-01…LB-18；边界清单由 `cargo xtask license --boundary` 输出（§3.11） | **已关闭（ADR-0015）** |
| OQ-E6 | ~~**签名密钥托管与发布者身份**~~ | E10 与 §3.2.3 的可执行性、合规审计 | **已裁决（ADR-0015 §D5）**：根密钥离线冷存储 2-of-3、发布子键云 HSM / KMS 2-of-3 且仅签哈希、轮换 ≤24 / ≤12 / ≤6 个月、泄露应急 ≤1h / ≤24h / ≤72h（§3.2.3） | **已关闭（ADR-0015）** |
| OQ-E7 | ~~**崩溃 / 遥测后端的运营归属与保留期**~~ | AR-12 的落地、隐私合规、运营成本 | **已裁决（ADR-0015 §D8）**：自托管 Sentry 兼容端点；Cloud Platform 运营 / Security 为数据保护责任人 / Devex 拥有 schema；原始事件保留 30 天、聚合 issue 元数据 90 天、到期硬删除；企业可自托管或完全关闭 | **已关闭（ADR-0015）** |
| OQ-E8 | **发布节奏与「按依赖而非日历」的关系**：beta 双周 / stable 月度在功能未过门禁时是否无条件顺延（可无限期滑期）？ | 发布可预测性与团队节奏；牵动 A8 / A9 口径 | 日历为「最早不早于」，门禁为硬闸；滑期需公示原因，连续两次滑期触发复盘 | P1 |
| OQ-E9 | **旧平台维持成本上限**：Windows 10 官方支持结束后是否仍维持 1809 地板与对应矩阵？ | L6 成本、安全责任与 R4 类风险 | 设「地板复核」条款：OS 官方支持结束 + 6 个月后启动提升地板的 ADR | P1 |
| OQ-E10 | ~~CI 成本预算与 runner 供给~~ | 夜间全量矩阵（12 组合 × 3 OS）与 fuzz 24h 的实际可行性与成本 | **已裁决**：按「矩阵规模 × 机时单价 + 固定费」量级估算；月度 CI 硬上限 $3,000（云 ≤$1,200 / 自托管 ≤$1,800）；fuzz 与 soak 一律自托管（云托管 fuzz 机时 = 0）；80% 告警、100% 自动降采样；签名固定费 <$5,000/年（§3.4.3） | **已关闭（ADR-0014）** |

> 已关闭项：OQ-E1 / OQ-E2 / OQ-E3 / OQ-E10 由 ADR-0014 终裁并同步至 §3.4.3 / §3.7 / §3.8.1 / §3.8.3 / §5 / §6；OQ-E4 / OQ-E5 / OQ-E6 / OQ-E7 由 ADR-0015 终裁并同步至 §3.2.3 / §3.11。其余 OQ-E*（OQ-E8 / OQ-E9）维持待决。

> 变更记录（ADR-0015 / ADR-0014）：本次同步依据 ADR-0015（链接边界 / 供应链信任根）与 ADR-0014（平台矩阵 / 参考机）修订 §3.2.3（签名门限与信任根对齐 ADR-0015 §D5）、§3.11（「链接边界」改引 ADR-0015 §D1 / §D2，并更新许可允许列表与溯源指针），并关闭 §7 的 OQ-E4 / OQ-E5 / OQ-E6 / OQ-E7（ADR-0015）。

> 变更记录（AR-22 / AR-23、ADR-0016 / ADR-0017）：把设计/体验验收接入 CI 六件套——§2.3 新增 **B12**；§3.4.1 新增 **G7**（设计静态门禁 S1–S9）与 **G8**（设计浏览器门禁 B1–B8）；新增 **§3.4.4「视觉回归方法学（动画冻结下渲染）」**；§5 新增 **A17 / A18**。实现物：`tools/design-gates/`（零外部依赖：Node 标准库 + 系统 Chrome/CDP）、根 `package.json` 的 `design:check` / `design:check:static` / `design:selftest` / `design:baseline`、`.github/workflows/ci.yml` 的 `design` job。门禁条目与 spec 02 §5 的 A17–A35 / UX-G11–UX-G17 一一对应。

> 变更记录（AR-22 门禁收紧）：设计门禁债清零与口径补齐——§3.4.1 **G7** 的 S8 硬编码色值由 WARN 改为**默认阻断**（原型 28 处字面量全部收敛为语义 token / 展示文本实体），S3 口径扩展到非 `<button>` 的 icon-only 交互元素，字号标尺改为 **UI / 终端两条阶梯**（`tokens/base/type.json` 的 `ladder`）；**G8** 与 §5 **A17/A18** 同步新增浏览器断言 **B9**（侧栏面板 / 设置窗口打开时页面级纵向滚动 = 0，1600×1000 与 1280×800），§2.3 **B12** 的浏览器层范围由 B1–B8 改为 B1–B9。实现物：`tools/design-gates/`（S1–S9 / B1–B9）、`tools/tokens/`（双阶梯标尺）、`tokens/README.md`。

> 变更记录（AR-24…AR-29 跨文档对齐）：§3.8.3-3 由「连续 2 夜复现才阻断」改为 **G4-PR / G4-REL 双口径**（AR-27：单次 >5% 阻断合并；连续 2 夜复现或发版前全套阻断发版），并明确测量须先通过可复现性自检，否则判 INVALID。

> 变更记录（AR-24…AR-29 回填 / ADR-0018 追认）：§2.3 **B12** 与 §3.4.1 **G7** 的静态门禁范围由 S1–S9 扩展为 **S1–S10**（新增 **键位三平台展开零冲突**，AR-29 第 7 条），§5 **A17** 同步；RSS 口径（**核心进程组 = sessiond + 原生 UI**；WebView 就绪后增量 ≤60MB、插件宿主空载 ≤80MB 独立记账；必须报告总内存）与空闲 CPU ≤1% / 未聚焦出帧 = 0 帧/10s 门禁由 **AR-24** 确立（同步 `docs/spec/03` §2 / AC-04 与 kernel/03、kernel/06）；AR-27 的「可复现性自检 → verdict 不得翻转 → 否则 INVALID 且不得与基线比对」落入 §3.8.3-3；**ADR-0018** 追认 CBOR + POD 编码、crc32c 强制与 PtyBackend 九元面（kernel/07 OQ-ABI-01 / OQ-ABI-02 关闭，`docs/spec/03` §3.4 / §3.6 回填）。

> 变更记录（ADR-0013 / AR-21）：许可依据同步——§1.5、§2.1 C8、§3.9、§5 A10 中原以 AR-10 为现行依据的引用统一改为 AR-21（全栈开源、Apache-2.0 OR MIT；核心链接边界禁 GPL/AGPL/SSPL）。AR-10 的「核心单一许可」「云端闭源专有」部分已被 AR-21 取代，仅作为历史证据保留于 HARNESS §2，不再在本文作为现行依据引用。
