# tools/design-gates — 设计/体验验收的可执行合并门禁

> 上游契约：`docs/spec/02-ux-and-design-system.md` §5（A17–A35 / UX-G11–UX-G17）。
> 决策依据：**AR-22**（视觉语言 v2 与信息密度）、**AR-23**（终端主体 / 单一侧栏 / 设置独立窗口 / 新建终端选择器 / 平台化窗口控制）、**AR-29**（输入与剪贴板默认口径；第 7 条三平台键位展开与 S10）、**ADR-0016 / ADR-0017**。
> 依赖：**零外部依赖**。Node 标准库 + 系统已安装的 Chrome（浏览器层）。不执行 `npm install` 任何包。
> 唯一实现物：`prototype/termai-ui-terminal-first.html`（纯设计阶段无产品代码）。

## 1. 怎么跑

```bash
npm run design:check            # 静态层 S1–S10 + 浏览器层 B1–B10（无 Chrome 时 B 层显式 SKIP）
npm run design:check:static     # 只跑静态层（任何环境、无需浏览器、<1s）
npm run design:check -- --strict        # 兼容参数：S8（硬编码色值）已默认阻断，--strict 等价于 no-op
npm run design:selftest         # 在临时副本上注入必然违规，证明门禁不是恒绿
npm run design:baseline         # 在“动画冻结”下重渲全部状态并写入基线（仅变更视觉时执行）
npm run design:check -- --max-diff-pct=0.1   # 视觉回归改用 ≤0.1% 容差（默认是“超差通道 >±2 的像素 = 0”）
```

退出码：任一断言 `FAIL` → 退出码 1（阻断合并）。`WARN` / `SKIP` 不阻断。

`npm run design:check` 与 `npm run design:selftest` 的输出都是统一的 `[Sx]/[Bx] PASS|FAIL|WARN|SKIP 标题 - 数值` 行，末尾给出 `summary:` 与 `result:`；失败时再列 `blocking failures:`。

## 2. 失败时怎么定位

| 断言 | 失败时看什么 | 典型修复 |
| --- | --- | --- |
| S1 | `default-expanded N / M` 与展开分区的序号 | 把多出来的 `<details class="fold">` 改成 `data-open="false"` 并去掉 `open` 属性 |
| S2 | 打印命中的顶栏容器与禁用文案 | 删除顶栏「连接主机 / 新建工作区」；新建入口只留在 tab「+」选择器（`#addTabBtn` / `#newTermPop`） |
| S3 | 缺 `aria-label` 的图标控件 selector 与命中原因（`role` / `data-act` / `data-tip` / `tabindex` / `cursor:pointer`） | 给该控件补 `aria-label`（与 `data-tip` 同源的 i18n key）；若它本该是按钮，改成真实 `<button type="button">` |
| S4 | 非 `<button>` 或缺 `aria-label` / 被 `aria-hidden` 的 `[data-win]` | 用真实 `<button>` 承载窗口控制并命名 |
| S5 | `.views` 内 `.view` 数量与 active 数 | 保证主区任何状态只有 `#viewTerm` 一个 view，不留视图切换器 |
| S6 | 7 个 rail 分区的 `data-sec` 列表与缺标签项 | 分区固定为 Workspaces / Sessions / Agent / Snippets / Themes / Plugins / Audit Log，每项 `aria-label` + `title`/`data-tip` |
| S7 | `SPACING/RADIUS/FONT_SIZE/MOTION <prop>: <value>` | 用 `var(--sp-*)` / `var(--r-*)` / `var(--fs-*)` / `var(--m-*)`，或把新值写进 `tokens/base/*.json` 后 `npm run tokens:build` |
| S8 | 每种颜色的 `#XXXXXX xN` 清单 | 对应 CSS 改成 token 变量（`tokens/base/palette.json` + `npm run tokens:build`）；展示文本里的十六进制用 `&#35;` 实体（见 §5） |
| S9 | `differs from generated CSS at byte N` | `node tools/tokens/build.mjs` 重新生成 token 块（不要在原型里手改） |
| S10 | 冲突的平台 + 双方 action id、`Ctrl+Shift+Shift` 无效组合、或「键位登记没有三平台声明」的命令面板/菜单/快捷键表条目 | 在 `#actionRegistry` 里为该 action 补 `mac` / `win` / `linux`（mac = Cmd 系；win/linux = Ctrl+Shift 整体；`Mod+Shift+X` 在 Win/Linux 重分配为 `Ctrl+Alt+X`），或对平台独占动作加 `"platformExclusive"` 并只声明该平台 |
| B1/B2 | 视口尺寸 + `doc.scrollWidth/clientWidth` + 溢出元素清单 | 去掉导致横向溢出的固定宽度；终端网格 `overflow-x` 只能是 `hidden`（软换行关 = 视觉裁剪，永不横向滚动） |
| B3 | `trigger` / `visible` / `escHides` | 检查 `[data-tip]` 元素的 `focusin` tooltip 与 Esc 关闭链路 |
| B4 | 每个状态的 `hash` / `diff` / `bbox` | 先看 `ANIMATION NOT FROZEN`（哈希不稳定）；再看 `MISMATCH` 的 bbox 定位像素变化区域；确认是有意变更后 `npm run design:baseline` |
| B5–B8 | 逐字段的布尔值（`openHidden` / `afterSettingsLight` / `tabsAfter` / `states`） | 按 AR-23 第 3/3/4/5 条修复对应交互 |
| B9 | 视口 + `doc/body.scrollHeight` / `clientHeight` + 溢出元素清单 | 页面级纵向滚动必须为 0（侧栏 / 设置窗口打开时）；把溢出放进 `overflow-y:auto/hidden` 的内部滚动容器（`.panel-scroll` / `.ai-scroll` / `.term-body`），不要撑成页面级滚动 |
| B10 | 失败时逐条给出 `FAILED: ...` 的语义化检查名 + 两平台的 `palette` / `theme` 观测量 | 键盘处理没有走 `#actionRegistry`：检查 win/linux 的 Mod 是否整体展开成 `Ctrl+Shift`（裸 `Ctrl+K` 必须留给终端）、`Mod+Shift+X` 是否重分配为 `Ctrl+Alt+X`、mac 是否用 `Cmd`；行为与登记不一致时改键盘处理，不要改断言 |

## 3. 两层结构：哪些是静态、哪些需要浏览器

### 静态层（S 层，S1–S10）——纯 Node，任何环境都可跑

解析单个 HTML：token 标尺从 `tokens/base/*.json` 读取（**不硬编码**），token 漂移复用 `tools/tokens/lib.mjs` 的 `generate()` / `extractPrototypeBlock()`，硬编码色值扫描与 `tools/tokens/check.mjs [5]` 同口径。

| 断言 | 断言什么 | 规格锚点 |
| --- | --- | --- |
| S1 | `details.fold` 默认展开数 ≤ 2 | A21 / UX-G11 / AR-22 §6 |
| S2 | 顶栏（`.toolbar` / `.titlebar`）不存在「连接主机 / 新建工作区」 | A31 / UX-G14 / AR-23 §4 |
| S3 | 所有**图标控件**（任意标签）都有 `aria-label`：图标类（`.icon-btn` / `.rail-btn` / `.ttab-add` / `.send` / `.wc-dot` / `.wc-btn` / `.t-x`），以及**无可见文字**但带交互信号（交互 `role` / `data-act` / `data-win` / `data-tip` / `tabindex` / 命中 `cursor:pointer` 规则）的元素。带可见文字的控件不计入（其可访问名来自文本） | A18 / UX-G12 / AR-22 §3 |
| S4 | 所有 `[data-win]` 是真实 `<button>`、有 `aria-label`、未被 `aria-hidden` | A33 / UX-G16 / AR-23 §5 |
| S5 | 主区 `.views` 内恰好 1 个 `.view`，且为终端、处于 active、无 `data-view` 切换器 | A25 / UX-G14 / AR-23 §1 |
| S6 | rail 恰好 7 个 `role=tab` 分区，集合固定，每项有 `aria-label` + `title`/`data-tip` | A26 / UX-G14 / AR-23 §2 |
| S7 | CSS 中 spacing / radius / font-size（UI 与终端两条阶梯的并集）/ 动效时长全部落在 token 标尺内 | A23 / UX-G13 / AR-22 §1 |
| S8 | token 块之外硬编码十六进制色值数量（**默认阻断**；债务已清零，`--strict` 为兼容 no-op） | AR-22「硬编码色值 = 0」/ UX-G2（spec 02） |
| S9 | 原型内联 token 块与 `tokens/` codegen 逐字节一致 | DC-09 / AR-22 / UX-G2（spec 02） |
| S10 | 键位展开零冲突：Action Registry 的每个动作显式声明 macOS / Windows / Linux 三平台展开；同平台无重复绑定；无 `Ctrl+Shift+Shift` 之类无效组合；所有键位登记（快捷键表 / 命令面板 / 菜单 / 文本提及）都能反查到登记 | AR-29 第 7 条 / C5（spec 02 §3.5） |

### 浏览器层（B 层，B1–B10）——Chrome headless + CDP（Node 内建 WebSocket）

不依赖 puppeteer / playwright。启动参数固定包含 `--force-prefers-reduced-motion`，并在页面内再加 `Emulation.setEmulatedMedia(prefers-reduced-motion: reduce)`，确保唯一动画（终端光标 `@keyframes caret`，200ms）被冻结。

| 断言 | 断言什么 | 规格锚点 |
| --- | --- | --- |
| B1 | 1600×1000 与 1280×800 下 `documentElement.scrollWidth == clientWidth`，且不存在 `overflow-x:visible` 的横向溢出元素 | A17 / UX-G11 |
| B2 | `#wrap=0`（软换行关 = 视觉裁剪）下仍无横向滚动，且 `#term` 的 `overflow-x` 不是 `scroll/auto` | A34 / UX-G17 / AR-23 §6 |
| B3 | 焦点进入 `[data-tip]` 图标按钮后 `.tip` 可见（`is-on` + 非 `display:none` + opacity>0.5），Esc 关闭 | A19 / UX-G12 / AR-22 §3 |
| B4 | 动画冻结下渲染，与 `prototype/baseline/` 基线比对（先做“同状态连渲两次哈希相同”自检） | A17 / G3 / spec 07 方法学 |
| B5 | 设置窗口可开 / 可关 / 可从 `#rail [data-act=settings]` 重开 | A28 / UX-G15 / AR-23 §3 |
| B6 | 主题双向联动：设置窗口改 → 主窗口反映；主窗口改 → 设置窗口分区反映 | A29 / UX-G15 / AR-23 §3 |
| B7 | tab「+」选择器键盘路径：打开 → 输入 → ↑↓ → Enter 新增并激活 tab；Esc 无副作用 | A30 / UX-G15 / AR-23 §4 |
| B8 | mac / win 两形态可见控制组正确，且最大化 / 最小化 / 关闭三态均可还原 | A32 / UX-G16 / AR-23 §5 |
| B9 | 侧栏面板打开与设置窗口打开时**页面级纵向滚动 = 0**（1600×1000 与 1280×800；侧栏内部滚动容器不算） | A22 / UX-G11 / AR-22 §6 |
| B10 | **键位行为与 Action Registry 一致**：用真实 CDP 按键事件（`Input.dispatchKeyEvent` + 修饰键位掩码）验证 —— win 形态 `Ctrl+Shift+K` 开命令面板、裸 `Ctrl+K` **不**开（留给终端）、`Mod+Shift+L` 展开为 `Ctrl+Alt+L` 且 `Ctrl+Shift+L` 无副作用；mac 形态 `Cmd+K` 开、`Ctrl+K` 不开 | AR-29 第 2/7 条 / C5（spec 02 §3.5） |

## 4. 视觉回归方法学（必须遵守）

原型唯一的动画是终端光标 `@keyframes caret`（200ms infinite alternate）。**未冻结动画时同一文件连渲会得到不同哈希**，视觉回归会在 CI 上随机失败。因此：

1. 所有截图渲染加 `--force-prefers-reduced-motion`（浏览器层还叠加 CDP 的 reduced-motion 覆盖）；
2. 比对前先做自检：**同一状态连渲两次，PNG SHA-256 必须相同**，不同则直接 `FAIL` 并报 `ANIMATION NOT FROZEN`；
3. 截图前等待 `document.fonts.ready` + 双 `requestAnimationFrame` + 600ms 静止（原型会在 `fonts.ready` 与 60/120ms 定时器里跑 `foldFit()/sbFit()`，只截其中间态会得到另一个自洽但与基线不同的帧）；
4. 先渲一张 warm-up（丢弃），再取两张比对帧；
5. 比对容差：**默认“单通道差 >±2 的像素数 = 0”**（±2 只吸收圆角/合成的亚像素舍入，实测噪声 218 px、最大通道差 2），可用 `--max-diff-pct=0.1` 放宽到 spec 的 ≤0.1%；失败时**始终打印差异 bbox**。

### 基线文件与平台

基线位于 `prototype/baseline/`，文件名为 `<platform>-<arch>-<state>.png`（本仓库已提交 `win32-x64-*`）。字体光栅随 OS/字体版本变化，**基线按平台存**：在某个平台没有基线时，B4 会 `SKIP` 并显式打印原因与生成命令，**不静默跳过**；其它平台一旦提交自己的基线即自动变为阻断。

当前基线状态（7 个）：`dark-workspaces-1600x1000`、`light-workspaces-1600x1000`、`dark-agent-1600x1000`、`dark-collapsed-1600x1000`、`dark-settings-1600x1000`、`dark-nowrap-1600x1000`、`dark-workspaces-1280x800`。

## 5. S8 硬编码色值：已从 WARN 升级为**默认阻断**

AR-22 的目标是「硬编码色值 = 0」。原型原有的 **28 处债务（20 种颜色）** 已全部清零：终端画布的深色语义收敛为
`term-line / term-strong / term-metric / term-tab-* / term-danger / term-accent* / term-fg-bright / term-sel-ink`，
窗口控制红绿灯收敛为 `wc-close` / `wc-min` / `wc-max`，彩色实心底前景为 `ink-on-color`、深色分段选中底为 `seg-on`。
3 处位于**展示文本**（主题列表的 `azure #4A9EFF` / `azure #245BC9`）：其 `#` 用 HTML 实体 `&#35;` 书写，
渲染完全不变，但源码不再含 CSS 色值字面量（详见 `tokens/README.md` §10）。

因此 S8：

- **默认 `FAIL` 并列出完整清单**（阻断合并）；`--strict` 仍被接受，但已是等价 no-op（默认即严格）；
- 本地可用 `npm run design:check:static` 秒级复验；
- `tokens:check [5]` 对**其它**历史原型（`termai-ui-netcatty-style.html` / `termai-ui-prototype.html`）仍只打印债务清单、
  不阻断——它们不是本阶段实现物，S8 只约束 `prototype/termai-ui-terminal-first.html`。

后续若原型又出现字面量，按 §2 的修复列处理：加语义 token（`tokens/base/palette.json`）→ `npm run tokens:build`。

## 6. Chrome 安装（浏览器层前置）

浏览器层按以下顺序查找 Chrome：`CHROME_PATH` → `CHROME_BIN` → `GOOGLE_CHROME_BIN` → 平台默认路径 → `PATH`（`google-chrome` / `chromium` / `chrome.exe` / `msedge.exe`）。找不到时整层 `SKIP` 并打印原因。

```bash
# Ubuntu / Debian（GitHub Actions ubuntu-latest 已预装 google-chrome，通常无需安装）
sudo apt-get update && sudo apt-get install -y google-chrome-stable
# 或仅需要 Chromium：
sudo apt-get install -y chromium-browser
export CHROME_PATH=$(command -v google-chrome)

# macOS
brew install --cask google-chrome
export CHROME_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'

# Windows
# 安装 Google Chrome 后无需配置；如需覆盖：
$env:CHROME_PATH='C:\Program Files\Google\Chrome\Application\chrome.exe'
```

CI（`.github/workflows/ci.yml` 的 `design` job）默认信任 ubuntu runner 预装的 `google-chrome`；若镜像变更，按上面第一条安装并导出 `CHROME_PATH` 即可。

## 7. `--selftest` 证明门禁不是恒绿

`npm run design:selftest` 在 `os.tmpdir()` 下的**临时副本**上注入必然违规（绝不修改 `prototype/` 原文件，结束后清理），逐条断言被对应断言捕获；任一条未被捕获则 selftest 自身失败并退出 1。当前注入（19 条，全部执行即 19/19）：

| 注入 | 期望被 |
| --- | --- |
| 把某个折叠分区设为默认展开 | S1 |
| 顶栏注入「连接主机」 | S2 |
| 删掉一个 `.icon-btn` 的 `aria-label` | S3 |
| 注入 icon-only 且带 `data-act` 的**非 button** 元素（无 `aria-label`） | S3 |
| 删掉一个窗口控制的 `aria-label` | S4 |
| 主区注入第二个 `.view` | S5 |
| 删掉一个 rail 分区的 `aria-label` | S6 |
| 把 spacing 改成 `13px` | S7 |
| 同时注入 `border-radius:11px` / `font-size:13.5px` / `transition … 133ms` | S7 |
| token 块外注入 `#ABCDEF`（默认即阻断） | S8 |
| token 块改一个字节 | S9 |
| 把 `quit` 的 Windows 展开改成 `Ctrl+Shift+K`（与 `palette` 撞键） | S10 |
| 把 `quit` 的 Windows 展开改成 `Ctrl+Shift+Shift+K`（机械展开无效组合） | S10 |
| 删掉 `quit` 的 `linux` 字段且不加 `platformExclusive` | S10 |
| 命令面板某条 `.kb` 改成未登记的 `Mod+Shift+Z` | S10 |
| 给终端网格强加 `overflow-x:scroll` | B1/B2 |
| 用 `display:none` 藏掉 tooltip | B3 |
| 注入 3000px 块并解除 body 的纵向裁剪 | B9 |
| 把键盘处理的平台展开退化成旧规则（win 下 `Mod` 仍按裸 Ctrl：Shift 一律拒绝，`Ctrl+K` 也能开面板） | B10 |

没有 Chrome 的环境：静态注入照常执行，浏览器注入打印 `SKIP` 并说明原因（不是“未捕获”，而是“无法执行”），脚本退出 0；在装有 Chrome 的机器上会执行完整 19 条。

## 8. 与 token 门禁的分工

- `npm run tokens:check`：token 自身（schema / 标尺 / 对比度 / codegen 漂移 / hex 债）。
- `npm run design:check`：**原型产物的契约**（布局、a11y、标尺外取值、视觉回归）。
- S7 只校验 CSS 里的**字面量**；`var(--sp-*)` 引用的合法性由 `tokens:check` 的标尺门禁负责，两者不重复实现。
- S9 不重新实现 codegen，直接复用 `tools/tokens/lib.mjs`。

## 9. S10 / B10 键位展开（AR-29 第 2/7 条）

### 真相源（谁说了算）

S10 的**唯一权威真相源**是原型里的结构化 **Action Registry**：

```html
<script type="application/json" id="actionRegistry">
[ { "id": "palette", "display": "Mod+K", "mac": "Cmd+K", "win": "Ctrl+Shift+K", "linux": "Ctrl+Shift+K" } ]
</script>
```

- `id`：稳定 action id（与命令面板 `data-run` / 统一动作表 `data-act` / 快捷键表行 `data-action` 对齐）；
- `display`：UI 中出现的平台无关写法（`Mod+...`，或 `?` 这类无修饰键）；
- `mac` / `win` / `linux`：**显式**三平台展开，缺一即失败；
- `platformExclusive`（可选）：`"mac" | "win" | "linux"`，仅该平台字段必需且其它平台字段不得出现 —— 这就是「平台独占」豁免；
- 该块放在主 `<script>` **之后**，不渲染、也不进入 `scanTags()` 的标记扫描区，因此**不产生像素差异**（B4 仍为 0）。

### 判定规则

1. **登记完整性**：非独占动作必须有非空 `mac` + `win` + `linux`；`platformExclusive` 动作必须只声明该平台，多声明其它平台也算失败。
2. **零冲突**：把每个平台字段解析成规范化 chord（修饰键排序 + 键名小写），同一平台内两个动作命中同一 chord 即 `FAIL`（action id 去重）。
3. **无效组合**：重复修饰键（`Ctrl+Shift+Shift+K`）、只给修饰键没有键、展开里仍写 `Mod`、macOS 用 `Ctrl`、Win/Linux 用 `Cmd`/`Meta` 一律 `FAIL`。
4. **登记不被绕过**：反查以下**展示位**，每个都必须在 Registry 中有三平台声明，且 `display` 与 Registry 完全一致：
   - 设置窗口快捷键表的行首 `<td>`（该行用 `data-action` 标注动作 id）；
   - 命令面板 `.pal-item[data-run]` 内的 `.kb`；
   - 菜单 `.pop-item[data-act]` 内的 `.pi-tag`（作用域单选等只展示徽标的 `data-scope` 项不算键位）；
   - 所有 `Mod+...` 文本提及（`data-tip` / toast / 新建终端选择器数据）。修饰键-only 的 `Mod+Shift` 不算声明。
5. **反向覆盖**：Registry 里声明了、但原型里任何地方都不出现的 `display` 也失败（防止死登记）。

### 门禁暴露并修正的既有漂移

- 命令面板 `diagnose`（`ai.diagnose.last_error`）原先显示 `Mod+I`，与冻结的 Agent 分区键（`sidebar.section.agent` = `Mod+I`，spec 02 §3.5）重复。修正为 `—`（不再声明独立绑定，Diagnosis Card 经 Agent 分区 `Mod+I` 抵达），符合 AR-29「冲突以改绑解决」。
- 「更多」菜单的「分屏」项 `data-act="split"`（垂直分屏）原先挂 `Mod+Shift+\`（水平分屏的键）。修正为 `Mod+\`，与 Registry 的 `split` 一致。

### Win/Linux 重分配规则

- `Mod+X` → `Cmd+X` / `Ctrl+Shift+X`；
- `Mod+Shift+X` → `Cmd+Shift+X`（mac）/ `Ctrl+Alt+X`（win & linux，AR-29「优先 Ctrl+Alt 系」）——**绝不**机械展开为 `Ctrl+Shift+Shift+X`。

### 行为层（B10）：登记数据与运行时一致

S10 只证明登记数据自洽，不能阻止「注册表全绿而按键行为不符」。原型键盘处理因此改为**同一真相源**：

| 位置（原型 JS） | 职责 |
| --- | --- |
| `actionRegistry()` | 惰性解析文件末尾的 `id="actionRegistry"` JSON 块（该块在主脚本之后，首次按键时读取） |
| `keyPlatform()` | 当前平台 → Registry 字段名：显式 `#plat=mac\|win` 优先；auto 时 UA 再分辨 Linux |
| `parseChordSpec()` / `chordMatches()` | 把字段展开解析成修饰键 + 键名并**精确匹配**（含 Shift），不另写映射表 |
| `actionForEvent()` | 遍历 Registry，返回命中的 action id |
| `runKeyAction()` | action id → 行为（对话式动作走统一动作表 `runAction()`；`handoff` 由输入行自身监听处理，避免重复触发） |

因此 win/linux 下裸 `Ctrl+K` 不会命中 `Ctrl+Shift+K`（裸 Ctrl 留给终端应用），`Mod+Shift` 类动作直接使用 Registry 里已经重分配好的 `Ctrl+Alt` 字段。B10 用真实 `Input.dispatchKeyEvent` 按键事件验证上述行为；selftest 注入「win 下 Mod 仍按裸 Ctrl」的旧规则证明 B10 能捕获。

### 已知边界（不静默）

- 运行时行为已随 AR-29 落地：键盘处理（`actionForEvent()` / `chordMatches()` / `runKeyAction()`）按 `keyPlatform()` 选 `#actionRegistry` 的 `mac` / `win` / `linux` 字段匹配，**不持有第二份映射表**。S10 仍只校验登记数据；「登记数据与实际按键行为不一致」由浏览器层 **B10** 用真实按键事件捕捉（selftest 会注入旧规则 `metaKey || ctrlKey` 且拒绝 Shift，证明 B10 能捕获）。
- `platformExclusive`（平台独占）分支已实现并有对应断言，但当前原型尚无需要它的动作。

