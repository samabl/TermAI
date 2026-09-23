# ADR-0001：混合渲染架构：Native Grid + Web Shell

- 状态：Accepted
- 日期：2025-01-01
- 决策者：Orchestrator（综合 10 角色评审）
- 关联：AR-01、AR-02、AR-14；DC-09、DC-13、DC-14、DC-15、DC-17、DC-25；HARNESS §4.2、§5、§10

## 背景与问题
1. 终端字符网格位于 PTY→像素热路径：HARNESS §5 门禁 key-to-photon P99 ≤16ms、4K@120Hz 帧时 <8.3ms、解析+渲染吞吐 ≥500MB/s。
2. 网格层必须承载字形 shaping、SGR、光标、选区、Sixel/Kitty 图形协议、grapheme cluster → 列映射，并对 IME 候选窗做像素级定位。
3. 同时，AI 面板 / diff / 设置 / 插件视图是富交互界面：表单、列表、可折叠卡片、虚拟滚动。自研这套控件集的 ROI 极低（DC-25：自绘受限控件集 ≤20 个）。
4. 争议：01 主张全原生 UI 工具链；02/03/04/05/10 主张混合；全体共同否决 Electron + xterm.js。

## 可选方案（至少 2 个，含被否决项）
| 方案 | 主张方 | 否决 / 采纳理由 |
| --- | --- | --- |
| A 全原生 UI 工具链 | 01 | 否决。富交互 UI 与插件 UI 的自研成本与迭代速度不可接受；DC-25 已把自绘控件集锁死在 ≤20 个 |
| B 全 WebView（Electron + xterm.js） | — | 共同否决。网格进入 Web 热路径，key-to-photon 与 IME/图形协议不可控，与 §0.4 硬边界直接冲突 |
| C Native Grid + Web Shell 混合 | 02/03/04/05/10 | **采纳**（AR-01） |

## 决策
1. **L0 网格 / L1 外壳 / L3 动画**：Rust + wgpu 自绘；damage 增量重绘；同一 render target 二次绘制。**绝不走 Web**。
2. **L2 面板**：系统 WebView 承载 AI 对话、设置、diff、插件视图；独立 OS 子表面，由 Core 定位裁剪；WebView 崩溃不影响会话。
3. **L4 浮层 / IME**：平台原生，最高 z-order；**WebView 不得覆盖** IME 候选窗与右键菜单（浏览器栈另有沙箱约束）。
4. 跨层拖拽由 Core 接管指针，不让 WebView 参与命中判定。
5. **WebView 是可选运行时依赖**（AR-02）：缺失或不可信时降级为纯原生文本面板，功能子集可用；Linux 以 AppImage 自带运行时为备选；Windows 默认 WebView2 evergreen，企业版提供 fixed-version。
6. **画质**（AR-14）：默认灰度抗锯齿 + hinting 关闭 + 严格网格对齐（误差 ≤0.5px），提供「锐利/柔和」两档；不承诺 ClearType 式 subpixel AA。

## 理由
1. 像素热路径由 Rust 直提 GPU，才能同时满足 §5 的延迟与吞吐门禁，并对 IME/图形协议做确定性控制。
2. 富交互 UI 交给 WebView 换来插件 UI 的迭代速度；AR-02 用「可选依赖 + 降级」把终端的可用性从系统组件上解耦。
3. 与 HARNESS §4.2 分层表一一对应，层间规则可被 CI 与代码评审静态检查（如禁止 web-shell 依赖 term-render）。

## 后果（正面 / 负面 / 需要接受的代价）
- 正面：热路径性能与 IME 可控；L2 崩溃隔离；插件 UI 生态迭代快；窗口体量小于 Electron。
- 负面：双 UI 栈带来**永久一致性成本**与主题/token 对齐成本；UI 迭代比纯 Web 慢；可招聘人才面窄；Linux WebKitGTK 碎片化（风险 R8）。
- 需要接受的代价：
  1. token 必须同时 codegen 到 Rust 与 CSS，CI 强制零硬编码色值（DC-09）与视觉回归 diff ≤0.1%（DC-14）。
  2. 原生/Web seam 的定位与 IME 错位风险由 CJK 回归矩阵与发版门禁兜底（风险 R2）。
  3. subpixel AA 缺失写入已知问题清单，Windows 用户观感损失被显式接受。

## 反方记录与复议条件
- **反方（01，保留原文）**：双 UI 栈带来永久一致性成本、UI 迭代慢、人才面窄；主张全原生以换取长期可组合性。
- **复议触发条件**：若任一目标平台 WebView **无法达到 P99 ≤16ms 且 IME 可修正**，启动「面板原生化分期评估」，按层逐步回收 L2。
- 复议证据要求：HARNESS §5 key-to-photon 门禁实测报告 + IME 矩阵失败用例，提交 TSC。

## 关联决策（DC-xx）与实现位置
| 决策 | 内容 | 实现位置 |
| --- | --- | --- |
| DC-17 | wgpu + rustybuzz/swash，多页字形 atlas | term-render（→ termai-vt、termai-gpu） |
| DC-25 | 自绘受限控件集 ≤20 | ui-native |
| DC-09 / DC-13 / DC-14 | token 唯一真相源 / 对比度门禁 / 视觉回归 | tokens/*.json → codegen |
| DC-15 | 核心 100% Rust，TS 仅在 Web 面板与插件 SDK | apps/*、web-shell、plugin-ui-sdk |
| AR-01 硬边界 | 网格与关键动画不走 Web；L2 不覆盖 L4 | shell-bridge（指针与裁剪归属） |
