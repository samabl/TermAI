## 一、角色立场与判断依据

我的立场：UI 层只负责「把内核真相以最低延迟、最高保真地投射为像素与交互」，它不生产业务真相。三条判据：(1) 终端网格是像素热路径（逐帧重绘、毫秒级输入回路），不能与富交互 UI 共享技术选型；(2) AI-native 的增量价值几乎全在富 UI（对话、diff、表格、Markdown、审批流），这部分自研 ROI 极低；(3) 长期可维护性由**边界清晰度**决定，而非技术栈统一度。

由此终裁：**方案 A 的渲染内核 + 方案 B 的 UI 壳，受控合成**；否决 C（Electron 系）与 A 的"全量自研工具包"形态。

## 二、关键结论

**D1｜终裁「Native Grid + Web Shell」双层混合架构**
结论：Rust/wgpu 渲染终端网格与外壳 chrome（tab、分屏、状态栏、命令面板、浮层）；系统 WebView 承载 AI 面板、设置、插件视图。理由：热路径需 GPU 原生可控（damage 跟踪、字形图集、IME 光标）；富 UI 需成熟布局/文本/可访问性/DevTools。代价：两套渲染与 a11y 栈、seam 调试成本、Linux 依赖 WebKitGTK 的运行时碎片。

**D2｜否决 Electron/xterm.js 与"从零自研通用 UI 工具包"**
理由：前者是 Node 攻击面 + 启动内存开销 + 网格性能天花板；后者要重造文本编辑、IME、i18n、a11y、布局与 DevTools，是数年陷阱。代价：放弃 npm 插件生态红利；原生侧仍须自建精简原语（Box/Text/Icon/Scroll/Popover），但不做通用工具包。

**D3｜单一权威状态驻留 Core，UI 只是 projection**
理由：会话/配置/Agent/插件四类真相分散 = 必然的双向同步 bug。代价：UI 变更须经命令往返，前端只能做 UI 级乐观更新。

**D4｜布局是纯函数 `(tree, constraints) → rects`**
理由：可单测、可无头快照、可动画插值。代价：需在 Rust 实现约束求解子集，放弃 CSS Flex/Grid 的表达力。

**D5｜Design Token 是唯一视觉真源，生成双端产物**
理由：双栈不统一 token 必然视觉漂移。代价：CI 需 token 生成 + 视觉回归门禁。

**D6｜插件 UI「声明式优先 + 独立源 sandboxed iframe 兜底」**
理由：插件不得持有 Core 内存或 Node 能力，UI 与能力解耦才可审计。代价：声明式表达力有限，复杂插件降级为 iframe，牺牲部分一致性。

**D7｜IME 候选窗、字体 shaping/fallback、BiDi 必须在原生层完成**
理由：组合输入与候选窗定位跨 WebView 边界必然错位；网格字形须自控图集。代价：三平台 IME（TSF / NSTextInputClient / fcitx5+ibus）与 Harfbuzz 级 shaping 是最高成本项。

**D8｜测试以「确定性伪 PTY + 像素金标准」为地基**。代价：GPU/驱动差异要求参考光栅器与分平台阈值。

**D9｜可观测性默认本地、终端内容永不外传**。理由：缓冲区含密钥与生产数据，是最敏感 PII。代价：远程诊断弱化，依赖用户显式导出 trace。

**D10｜许可：Core 与插件 API 采用 Apache-2.0 开源，AI/团队/企业能力闭源**
理由：终端可信度与插件生态必须有开源 API，推理与协作才是付费点。代价：需维护开源治理与闭源边界，显式接受被 fork 的商业风险。

## 三、详细设计

### 3.1 渲染分层与合成（自底向上）

| 层 | 归属 | 内容 | 合成 |
|---|---|---|---|
| L0 网格 | Rust/wgpu | 单元格字形、SGR、光标、选区 | 主 render target，damage 增量 |
| L1 外壳 | Rust/wgpu | tab、分隔条、状态栏、命令面板 | 同 target 二次绘制 |
| L2 面板 | 系统 WebView | AI 对话、设置、diff、插件视图 | 独立 OS 子表面，Core 定位裁剪 |
| L3 动画 | Rust | 分屏插值、面板滑入、光标闪烁 | 统一时钟，禁止 WebView 驱动关键动画 |
| L4 浮层/IME | 平台原生 | 右键菜单、tooltip、候选窗 | 原生窗口，最高 z-order |

规则：L2 不得覆盖 L4；跨 L0/L2 的拖拽由 Core 接管指针，WebView 只上报命中。

### 3.2 状态管理与数据流

权威层在 Core：Session/Grid、Config、Agent Session、Plugin Registry。传输走 IDL-first（JSON Schema 生成 Rust+TS 类型，高频网格走二进制 snapshot + delta）。前端 store 只存 UI 临时态（展开、滚动、草稿、hover），细粒度 selector 订阅，禁止缓存网格真相。AI 流式 token 按 ≥16ms 合帧后入 store，避免逐 token 触发渲染。配置走 patch → Core 校验合并广播 → UI 重投影，严格单向。

### 3.3 布局引擎与分屏树

节点 `Leaf(paneId) | Split(dir, ratio, a, b)`，插入时同向折叠，稳定 ID 支持拖拽 reparent；浮层窗格独立于树，用单层 z-order 数组。求解为纯函数，动画在前后 rects 间插值；树与 ratio 按 workspace 持久化。

### 3.4 组件化与设计系统

原生侧只做原语（Box/Text/Icon/Scroll/Popover/Focus 环），不做表格与富文本。Web 侧 React + TS strict + headless 原语 + CSS 变量，禁止运行时 CSS-in-JS。Token 包为无依赖叶子，生成 `tokens.rs`/`tokens.css`。组件必须声明 a11y 语义（role、焦点序、键盘可达），双栈各自映射平台 a11y API。

### 3.5 插件 UI 隔离与沙箱

Tier1 声明式：插件提交 view schema（JSON），Host 渲染，零代码执行。Tier2：每插件独立 origin iframe（`sandbox="allow-scripts"`、`default-src 'none'`、默认断网），经 Core 中转 JSON-RPC，能力白名单（fs.read/exec/network 需用户授权）。插件进程隔离 + 资源配额 + 崩溃隔离，绝不获得 Node 或原生句柄。

### 3.6 打包 / 签名 / 更新 / 安装

| 平台 | 分发 | 签名 | 运行时 |
|---|---|---|---|
| Windows | MSI（MSIX 可选），用户态免管理员 | Azure Trusted Signing / EV | WebView2 引导 |
| macOS | notarized dmg / .app，universal2 | Developer ID + hardened runtime | 系统 WKWebView |
| Linux | AppImage + deb/rpm + Flatpak | 仓库签名 | WebKitGTK（版本风险） |

更新链：签名强校验 + 差分 + 灰度 + 原子安装与回滚，更新器独立进程。首启 shell 集成（OSC 133/7）可选注入且可一键回退，另提供 portable 模式。

### 3.7 i18n 与字体

字体：自建字形缓存与多 DPI 图集、Harfbuzz shaping、连字开关、彩色 emoji、Nerd Font fallback 链；度量来自配置，UI 与网格共用同一度量源。i18n：单一 FTL/ICU 目录编译为 Rust 与 TS 双产物，预留 RTL 镜像；终端 BiDi 明确划为受限范围（先保读取正确性）。

### 3.8 测试策略

| 层级 | 手段 | 门禁 |
|---|---|---|
| 单测 | vttest/esctest 一致性、属性测试、fuzz | 合并阻断 |
| 组件 | 原生 golden + Vitest/RTL | 合并阻断 |
| E2E | 确定性伪 PTY + Playwright + 原生驱动 | 每夜 |
| 视觉 | 固定字体与参考光栅器像素比对 | 每夜，差异像素 <0.1% |
| 性能 | 输入延迟/帧时/吞吐/启动基准 | 回归 >5% 阻断 |

### 3.9 可观测性

原生与 WebView 共享 session id，W3C traceparent 贯通；环形缓冲 + `:trace` 命令导出。崩溃上报默认关闭、显式 opt-in，终端内容脱敏过滤；帧时直方图、慢帧归因、IPC 往返延迟本地聚合后可选上报。

### 3.10 模块清单与依赖方向

```
tokens(无依赖叶子)
term-render(L0) → ui-native(L1) → shell-bridge(L2) → web-shell(L3) → plugin-ui-sdk(L4)
core-dto(IDL 生成，双端共享，无反向依赖)
```

只允许 L0→L1→L2→L3→L4 单向；禁止 L0/L1 依赖 L3/L4，禁止 L4 触碰 Core 内部，禁止任何前端层直读 terminal 内存。由 crate 边界 + ESLint 依赖规则 + CI 依赖图检查强制。

## 四、与其他角色的接口与依赖

| 角色 | 必须给我 | 我承诺 |
|---|---|---|
| Core/Terminal | damage 快照、输入注入、选区与超链接语义、配色解析 | 渲染保真矩阵、帧预算、IME 正确性 |
| AI/Agent | 流式事件协议、结构化 block（markdown/diff/table/terminal 嵌入）、审批请求 | block renderer 注册表与稳定扩展点；拒绝裸 HTML |
| Plugin | manifest schema、能力模型 | 沙箱宿主、消息总线、view slot、配额执行 |
| Config | typed schema、迁移、热加载 | UI 只发 patch，绝不直写 |
| Platform/Security | 签名、更新器、CSP 与沙箱红队 | 无越权路径、SBOM |
| Product/Orchestrator | 确认开源边界、a11y 与本地化目标等级 | 可量化验收达标 |

**预期冲突点**：Core/平台方可能主张"零 WebView"纯净主义（Ghostty 路线）——我坚持 AI 面板值得引入 WebView；Security 会强烈反对 WebView 攻击面——我接受残余风险并给出沙箱与 CSP 契约；AI 方会要求直接渲染模型原始 HTML——我以结构化 block 拒绝；Perf 方可能要求网格也走 Web 以求统一——我直接否决。

## 五、被否决的方案与反方意见

| 方案 | 否决理由 | 反方最强论点（保留） |
|---|---|---|
| C Electron + xterm.js | Node 攻击面、启动内存、网格天花板 | 开发最快、npm 生态、跨端一致；若首版只验证 AI 交互，C 是最优 MVP |
| A 全量自研（含原生工具包） | 重造编辑/IME/a11y/布局，数年投入 | 完全可控、无 WebView 攻击面、启动最轻；Warp/Ghostty 已证明长期上限更高 |
| B 全量 Tauri（网格也走 Web） | DOM 渲染网格存在性能天花板 | 单栈、插件天然沙箱；除网格外我完全采纳 B |
| WASM 内嵌 UI（egui/iced） | 生态与文本编辑能力不足 | 纯 Rust、无 JS；可作原生原语的内部实现选项，但不成对外契约 |

## 六、风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| IME 候选窗与网格合成错位 | 高 | 原型先行、三平台 IME 专项、候选窗全原生 |
| Native/WebView seam（z-order、DPI、焦点、闪烁） | 高 | 单窗口多表面合成标准件 + 自动化 seam 测试 |
| Linux WebKitGTK 碎片 | 中高 | 固定最低版本，评估 AppImage 自带运行时 |
| GPU 驱动差异 | 中 | 能力探测 + 软件光栅 fallback + 降级矩阵 |
| 插件沙箱逃逸 | 中 | 独立源 + CSP + 能力白名单 + 持续红队 |
| 双栈技能分裂 | 中 | 边界契约 + IDL 生成 + 单一 token 源 |
| 更新链投毒 | 低中 | 签名强校验 + 灰度 + 可回滚 |

## 七、可验证验收标准

1. 输入延迟 p99 < 8ms（按键→字形上屏，参考机）。
2. 4K 全 damage 网格 @120Hz 帧时 < 8.3ms，丢帧率 < 0.1%。
3. 冷启动到首个提示符 < 150ms；AI 面板激活首绘 < 300ms。
4. 4 分屏空闲 RSS < 120MB；安装包 < 60MB。
5. vttest + 选定 esctest 集合 100% 通过；fuzz 24h 零崩溃。
6. 三平台视觉金标准差异像素 < 0.1%，无非预期 diff。
7. 插件 iframe 逃逸用例 0 成功；CSP 违规拦截率 100%。
8. 核心流程达 WCAG 2.2 AA 与平台 a11y 审计。
9. crash-free session > 99.9%；IPC 往返 p99 < 1ms。
10. 性能基准回归 > 5% 阻断合并。

## 八、待决议问题

1. WebView2 采用 evergreen（安全）还是 fixed version（确定性）？影响体积与一致性。
2. 是否允许用户自定义原生 chrome 主题（CSS 注入）？牵涉 token 边界。
3. 插件 Tier2 是否首版开放，还是仅声明式？
4. 终端 BiDi/复杂脚本的正式支持范围与时间点。
5. 跨平台 a11y 语义统一映射由谁定义与维护。
6. 开源治理：CLA、插件签名与市场审核的归属。
