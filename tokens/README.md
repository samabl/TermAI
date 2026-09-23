# TermAI Design Tokens（唯一真相源 · codegen · CI 校验）

> 依据：**HARNESS AR-22**（视觉语言 v2 的全部取值与标尺）、**DC-09**（token 为唯一真相源）、
> **DC-12/DC-13**（主题纯数据 / 对比度门禁）、**ADR-0016**（视觉语言 v2）、`docs/spec/02-ux-and-design-system.md` §3.11（token 七组）。
> 本目录是全部视觉取值的**唯一真相源**；仓库内其它位置的色值/字号/间距/圆角/动效都是**生成产物**。

## 1. 唯一真相源是什么

    tokens/base/*.json         七组基础 token（= 基座 :root = 深色，AR-23 第 7 条默认主题向下回退深色）
    tokens/themes/*.json       主题**覆盖文件**（只写与基座不同的键，不是重复定义）
    tokens/tokens.schema.json  手写 JSON Schema（校验上面两类源文件）
    tokens/contrast-pairs.json 需要在两套主题下校验的对比度配对

其它一切都可以删掉重生成：

    node tools/tokens/build.mjs              # 生成产物（dist / Rust / 原型内联 token 块）
    node tools/tokens/check.mjs              # 校验（schema / 标尺 / 对比度 / 漂移 / 硬编码色值债务）
    node tools/tokens/check.mjs --selftest   # 自证：注入必然失败的用例，断言校验器确实报错

**禁止手改生成产物**（`packages/tokens/dist/*`、`crates/termai-tokens/*`、原型中的
`@tokens:begin ... @tokens:end` 区块）。改了也会被 `tokens:check` 的漂移门禁拦下。

## 2. 七组与目录

| 组 | 文件 | 内容 | 数量 |
| --- | --- | --- | --- |
| palette | `tokens/base/palette.json` | 画布/表面/边框/正文/accent/壁纸/对象身份色/终端与 ANSI/选区/终端 chrome/窗口控制 | 66 |
| ai-semantic | `tokens/base/ai-semantic.json` | 风险四级色阶 + 未知风险 + AI 代码片三色 | 9 |
| type | `tokens/base/type.json` | 字体栈、**UI 字号阶梯**、**终端字号阶梯**（每个 fontSize 用 `ladder` 声明） | 15 |
| space | `tokens/base/space.json` | 间距标尺 + 结构高度 + 窗口外距 | 14 |
| radius | `tokens/base/radius.json` | 输入/卡片/弹层/药丸四档圆角 | 4 |
| elevation | `tokens/base/elevation.json` | elev-0/1/2 + 浮动窗口投影 | 4 |
| motion | `tokens/base/motion.json` | 120/180/240ms + 两条缓动曲线 | 5 |
| **合计** | | | **117** |

> 注：`palette` 的 semantic 层覆盖 AR-22 固定的七个键（画布/卡片/浮起/边框/正文/次要 + accent），
> 并按 §3.11 包含 `terminal.ansi`（本原型为 `--a-*` + `--term-*`）与选区语义；另含
> **终端 chrome 语义层**（`term-line/term-strong/term-metric/term-tab-*/term-danger/term-accent*/term-fg-bright/term-sel-ink`）与
> **窗口控制语义**（`wc-close/wc-min/wc-max`），它们都是把原型 token 块外的硬编码色值收敛而来的（见 §10）。

## 3. 命名约定（键 → 四端一致）

token 键一律 **kebab-case**，且与原型现有 CSS 自定义属性**一一对应**（本次是重构而非改设计，
渲染结果必须像素不变）：

| token 键（JSON） | CSS 变量 | TypeScript | Rust |
| --- | --- | --- | --- |
| `canvas` | `--canvas` | `canvas` | `dark::CANVAS` / `light::CANVAS` |
| `text-2` | `--text-2` | `text2` | `TEXT_2` |
| `r-card` | `--r-card` | `rCard` | `R_CARD` |
| `m-fast` | `--m-fast` | `mFast` | `M_FAST` |
| `card-head-h` | `--card-head-h` | `cardHeadH` | `CARD_HEAD_H` |

规则：JSON 键 = CSS 变量名去掉前缀 `--`；TS 用 camelCase；Rust 用 SCREAMING_SNAKE_CASE。
`tokens.ts` 同时导出 `vars`（camelCase → CSS 变量名）与 `cssVar()` 便于组件引用。

## 4. 如何新增 / 修改一个 token

1. 在对应 `tokens/base/<group>.json` 里新增一条 `{ "type", "value", "description" }`；
   键必须是 kebab-case，`type` 取自 schema 的枚举。**`fontSize` 必须额外声明 `"ladder": "ui" | "terminal"`**（见 §10）。
2. 若该键在浅色下取值不同：在 `tokens/themes/light.json` 的 `overrides` 里补一条（**只写不同的键**）。
   深色是基座，`themes/dark.json` 的 `overrides` 为空是刻意的。
3. `node tools/tokens/build.mjs` 重新生成全部产物。
4. `node tools/tokens/check.mjs` 必须通过（标尺 / 对比度 / 漂移都会查）。
5. 源文件与生成产物**一起提交**（产物入库，CI 幂等校验）。

硬性约束：

- 间距（`sp-*`）∈ {4,8,12,16,24,32,48}；圆角（`r-*`）∈ {8,10,14,999}；
  动效时长（`m-*`）∈ {120,180,240}ms；图标尺寸 ∈ {16,20,24}；字号必须落在**自己 `ladder` 对应的阶梯**内（见 §10）。
- 基座必须覆盖原型 token 块里声明过的**每一个**自定义属性（当前 117 个）。

## 5. 主题覆盖机制

解析模型：`resolved(theme) = base + themes[theme].overrides`。

- **基座 = 深色**：`:root` 输出 `resolved(dark)`（基座的全部键 + 深色覆盖）。
- **浅色 = 覆盖**：`[data-theme="light"]` 只输出与基座不同的键，以及 `color-scheme: light`。
- 每个主题文件的 `colorScheme` 字段决定该主题块输出的 `color-scheme` 值。
- 终端 chrome（`--term-*` / `--a-*`）刻意**不随主题变化**（终端在两套主题下都是深色），
  因此它们只在基座定义；对比度配对也按深色终端底校验。同理，窗口控制红绿灯（`--wc-*`）是平台色，
  `--ink-on-color`（彩色实心底上的纯白前景）与 `--seg-on`（仅深色主题使用的分段控件选中底）
  也只在基座定义、浅色下沿用基座取值——这是原型既有的渲染行为，不是遗漏。
- **新增官方主题**：加 `tokens/themes/<id>.json`（id 为 kebab-case，`colorScheme` 为 dark|light），
  build 自动产出新的解析结果与 Rust 常量。**第三方主题**用 `packages/tokens/dist/theme.schema.json`：
  纯数据、禁止脚本/CSS 注入（DC-12），`overrides` 的键被限制为官方 token 集合。

## 6. build / check 是什么

`tools/tokens/build.mjs`（零外部依赖，只用 Node 标准库）：

- 读 `tokens/base/*.json` + `tokens/themes/*.json`，在内存合并解析；
- 生成 `packages/tokens/dist/{tokens.css,tokens.ts,tokens.json,theme.schema.json}`、
  `crates/termai-tokens/{Cargo.toml,src/lib.rs}`；
- 用**同一段 CSS** 覆盖 `prototype/termai-ui-terminal-first.html` 中 `@tokens:begin/end` 之间的内联 token 块
  （原型保持单文件、零外部资源、`file://` 可开）。

`tools/tokens/check.mjs`（任一失败即 exit 1 并打印可读报告）：

1. **schema**：`tokens/base/*.json` 与 `tokens/themes/*.json` 是否符合 `tokens.schema.json`；
2. **标尺**（AR-22）：间距 / 圆角 / 动效时长 / 字号（UI 与终端**两条**阶梯，按 token 的 `ladder` 分别校验）/ 图标尺寸；
3. **对比度**（WCAG 2.x 真实公式）：`contrast-pairs.json` 的每一对在 **dark + light** 两主题下解析出实际生效颜色
   （支持 rgba 合成与 基座+覆盖 合并），正文类 ≥4.5:1、UI/大字类 ≥3.0:1；输出每主题最小比值与最紧的 5 对；
4. **漂移**：内存重跑生成逻辑，与磁盘产物**逐字节**比较；再把原型内联 token 块与生成 CSS 比较；
5. **自证**（`--selftest`）：注入 space=13px、对比度压到 ~2:1、产物改一字节、原型 token 改一字节，
   断言校验器确实报错——防止“永远返回绿”。
6. **附带（只警告）**：扫描 `prototype/*.html` token 块之外的十六进制色值，作为「零硬编码色值」的债务清单。
   `termai-ui-terminal-first.html`（本阶段唯一实现物）已为 **0**；`termai-ui-netcatty-style.html` / `termai-ui-prototype.html`
   是历史原型，仍打印债务但不阻断（S8 只针对实现物，见 §10）。

## 7. CI 如何挂钩

CI 只需两步（check 内已含生成漂移校验）：

    node tools/tokens/build.mjs
    node tools/tokens/check.mjs        # 非 0 即阻断

等价的“重生成零 diff”写法：

    node tools/tokens/build.mjs && git diff --exit-code -- packages/tokens crates/termai-tokens prototype/termai-ui-terminal-first.html

对应 `docs/spec/07` 的 **S1 静态门禁**（`clippy -D warnings` / TS strict / token 零硬编码，≤4min，阻断）。
`--selftest` 建议作为独立 CI 步骤（证明门禁不是空转）。

## 8. 生成产物清单

| 产物 | 消费者 |
| --- | --- |
| `packages/tokens/dist/tokens.css` | web-shell / 原型（同段 CSS 内联进原型） |
| `packages/tokens/dist/tokens.ts` | TS 前端（`as const` + `TokenName`/`ThemeName` 类型） |
| `packages/tokens/dist/tokens.json` | 工具链 / 主题商店 / 文档站（两套主题解析后的扁平结果） |
| `packages/tokens/dist/theme.schema.json` | 第三方主题文件校验（DC-12） |
| `crates/termai-tokens/src/lib.rs` | 核心 Rust（`Theme` 枚举 + 逐 token 常量 + 查表） |
| `crates/termai-tokens/Cargo.toml` | 叶子 crate，零依赖（ADR-0015） |
| 原型的 `@tokens:begin/end` 区块 | 单文件原型（AR-23 参考实现） |

> 入库提示：仓库根 `.gitignore` 有一条通用的 `dist/` 规则，会连带忽略 `packages/tokens/dist/`。
> 为满足 DC-09「产物入库」，`packages/tokens/.gitignore` 用 `!dist/` / `!dist/**` 复写该规则
> （仅作用于本目录，**不修改根 .gitignore**）。

## 9. 像素不变（视觉回归）验证

本次是**重构而非改设计**，原型渲染必须像素不变。验证方法（与运行环境无关）：

    chrome --headless=new --hide-scrollbars --force-device-scale-factor=1 --force-prefers-reduced-motion --virtual-time-budget=3500 --user-data-dir=<唯一目录> --window-size=1600,1000 --screenshot=<out.png> file:///<abs>/prototype/termai-ui-terminal-first.html
    # 浅色：URL 追加 #theme=light

原型里唯一的动画是终端光标 `@keyframes caret`（200ms infinite alternate），
普通截图会随机停在某一相位，因此**逐字节哈希需重跑几次命中该相位**；
除光标像素外，其余 1600×1000 像素在两套主题下都与基线逐像素一致。

## 10. 取舍与已知债务（诚实记录）

- **字号两条阶梯**：每个 `fontSize` token 用 `"ladder"` 声明所属阶梯，`tokens:check` 只校验它落在
  **自己那条**阶梯内（而不是所有字号共用一个并集）：
  - **UI 阶梯** 12 / 13 / 14 / 15 / 16 / 18 / 20 —— `docs/spec/02` §3.11 的 12/13/14/16/18/20，
    **保留 15**：区块标题的层级是已认可原型（AR-23 参考实现）的既定设计；
  - **终端阶梯** 12 / 12.5 / 14 / 16 / 18 / 20 —— 终端字号独立可调，允许 12.5 这类半档；
    设置窗口提供 12/12.5/14，运行时经 `--term-fs` 生效。
  两条阶梯的并集 `{12,12.5,13,14,15,16,18,20}` 与原“并集阶梯”完全一致，因此本次只补元数据与允许集合，
  **不改变任何 CSS 取值**。把原型真正收敛到规格单阶梯需要一次设计变更（新 ADR）。
  `tokens:check --selftest` 注入 `term-fs = 13px`：13 在 UI 阶梯内、不在终端阶梯内，必须被 `RULER_FONTSIZE` 拦下。
- **图标尺寸**没有对应的 CSS 自定义属性（原型用 `data-sz` 属性），故标尺校验扫描原型的 `data-sz`。
- **reduced-motion**：原型 `@media (prefers-reduced-motion: reduce){ :root{--m-*:0s} }`
  是媒体查询内的即时退化覆盖，不属于 token 声明块，仍由原型自行维护。
- **硬编码色值债务：已清零（28 → 0）**。`prototype/termai-ui-terminal-first.html` token 块之外的
  28 处十六进制字面量（20 种）全部收敛为语义 token：
  - 终端 chrome 语义层：`term-line / term-strong / term-metric / term-tab-ink / term-tab-on /
    term-tab-line / term-danger / term-accent / term-accent-fg / term-accent-ink / term-fg-bright / term-sel-ink`；
  - 窗口控制红绿灯：`wc-close / wc-min / wc-max`；
  - 彩色实心底前景 `ink-on-color`（`#FFF`）与仅深色使用的分段选中底 `seg-on`（`#262B34`）；
  - 与既有 token 取值相同的两处直接复用：`.rev` 的前景用 `--term-bg`、`::selection` 之外的选区用 `--term-sel-ink`。
  其中 3 处（`.sel` 的白色前景）与 2 处**展示文本**（主题列表的 `azure #4A9EFF` / `azure #245BC9`）：
  文本里的 `#` 改写为 HTML 实体 `&#35;`，渲染完全一致（像素不变），但源码不再含 CSS 色值字面量。
  `design:check` 的 **S8 已从 WARN 升级为默认阻断**；`tokens:check [5]` 对**其它**两个历史原型
  （`termai-ui-netcatty-style.html` / `termai-ui-prototype.html`）仍只打印债务清单、不阻断（它们不是本阶段实现物）。
- **对比度配对**按组件真实落点选取（chip 在 `--term-bg2` 终端 chrome 上、pill 在 `--raised` 上、
  全局选区在 `--surface`/`--canvas` 上）。若把 `--term-mut` 用在浅色 `--surface` 上会失败——
  那属于组件用法问题，不是 token 取值问题。
