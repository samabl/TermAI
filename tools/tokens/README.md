# tools/tokens —— 设计令牌校验（零外部依赖）

**检查什么**：`tokens/` 下的设计令牌源与生成块，跑五道门禁；**除第 5 道外，任一不过即 exit 1**。

```powershell
npm run tokens:check      # node tools/tokens/check.mjs
npm run tokens:selftest   # node tools/tokens/check.mjs --selftest（注入故障，证明门禁非恒绿）
npm run tokens:build      # node tools/tokens/build.mjs（重新生成 token 块——不要手改生成物）
```

## 五道门禁

| # | 门禁 | 是否阻断 |
| --- | --- | --- |
| 1 | schema | 阻断 |
| 2 | AR-22 rulers | 阻断 |
| 3 | **WCAG 对比度**（真实计算，`thresholds = { text: 4.5, ui: 3.0 }`） | 阻断 |
| 4 | codegen drift（生成块与源不一致） | 阻断 |
| 5 | **hex debt（硬编码色值）** | **仅警告，不阻断** |

## 与 HARNESS §5 的对应（**读这两行前先看「是否阻断」那列**）

| §5 行 | 由本工具承担的部分 | 现状 |
| --- | --- | --- |
| **H18 硬编码色值**（§5 为 `family: exact`） | 第 5 道门禁 | **只警告**——**因此「有扫描」≠「有等价强度的门禁」**（第 181 轮核实）。**若要它阻断，那是**收紧门禁**（不需要 TSC），但会改变 CI 行为，须单独记录** |
| **H19 双主题对比度** | 第 3 道门禁 | **门禁列（正文 ≥4.5:1）已强制**；**目标列（官方主题 ≥7:1）不强制——而按 §5 的「门禁 / 目标」两列口径，目标列本就不阻断，故此点正确** |

**注**：本文件表头写着「Fails (exit 1) on any gate」，**而第 5 道是警告**——**两者并存时的准确读法是「任一**未通过的门禁**」；把它读成「任一**发现**」会以为 hex 会阻断构建。**

## 生成物

**token 块由 `tokens:build` 生成**，**第 2 道门禁（AR-22 rulers）与第 4 道（drift）都盯着这一点**——**手改生成物会被 drift 门禁抓住**（`tools/design-gates/README.md` 的 S9 条目给出了正确的修法：改源后重新生成）。
