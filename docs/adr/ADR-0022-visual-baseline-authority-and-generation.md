# ADR-0022：视觉回归基线的权威来源与生成机制

- 状态：Accepted
- 日期：M0 收口后（B4 门禁可用性修复轮）
- 决策者：Orchestrator（综合 DevEx(09) / 设计系统(03) / 前端(05) / 安全隐私(08) / CTO 工程(10) 立场；发起人要求「让 CI 的 B4 视觉回归门禁真正可用」）
- 关联：AGENTS 第 4 节（CI 六件套第 3 条视觉回归）、第 5 节（ADR 触发条件：新增产物类别与门禁实现机制）、第 7 节第 4 条（默认不上传，例外必须显式登记）；HARNESS 第 8.1 节第 3 条（视觉回归像素 diff ≤0.1%，非白名单区域零变化）、第 5 节（视觉回归 diff 门禁 ≤0.1%）、第 6.3 节（数据分级：路径与命令输出为 L2）；DC-14；docs/spec/07 第 3.4.1 节 G3、第 3.4.4 节（视觉回归方法学，尤其第 5 条「基线随代码入库、缺基线显式 SKIP」）；AR-11、AR-12；ADR-0014（平台矩阵与 runner 口径，云 runner 一律 NON-GATING）、ADR-0015（供应链与 action 准入）、ADR-0021（产物准入、D2 白名单、D5 K8）、ADR-0016 / AR-22（视觉语言）
- 影响实现：.github/workflows/ci.yml（新增 design-baseline 作业）；prototype/baseline/（入库的基线 PNG 与溯源清单）；tools/design-gates/check-design.mjs（复用既有 --update-baseline，不改语义）；tools/kernel-gates/check.mjs 的 K8（无需修改，保持 PASS）

## 背景与问题

1. DC-14 与 HARNESS 第 8.1 节第 3 条把视觉回归定为**合并阻断**门禁：像素 diff ≤0.1%，非白名单区域零变化；docs/spec/07 第 3.4.1 节记作 G3，实现为 design 作业的 B4。
2. B4 的判定语义是「runner 现场截图 vs prototype/baseline/&lt;platform&gt;-&lt;arch&gt;-&lt;state&gt;.png」，默认容差为**逐通道 ±2 且允许超差像素数 = 0**（tools/design-gates/browser.mjs）。
3. 入库的 7 个基线是**开发机**产物（win32-x64），而验证发生在 GitHub windows-latest runner 上。CI 实跑（run #35809183560）测得 7 个状态全部 MISMATCH，diff 0.044%–1.245%，diff bbox 覆盖整块卡片区域。
4. 该失败模式（大面积 bbox、百分比远小于 1%、文字与形状位置一致）指向**环境不一致**（字体光栅化、浏览器构建、DPI / deviceScaleFactor），而非视觉变更。同一提交在开发机上 design:check 通过、在 runner 上 B4 失败，这两件事可以同时为真，正说明参照物与验证环境不同源。
5. **根本约束**：像素基线是**环境指纹**，不是可移植资产。只有当「生成基线的环境」与「验证的环境」是同一 runner 镜像、同一浏览器构建、同一字体栈时，逐通道 ±2 的容差才有意义。把开发机基线当作跨环境真值，等于把门禁押在一个无法复现的参照物上。
6. 失败代价：design 作业长期红，CI 不能作为合并依据；若把 B4 降级为告警（方案 C），则 G3 / DC-14 事实上失效，并违反 AGENTS 第 5 节「放宽任何第 8 节门禁必须附基准数据 + TSC 批准」。
7. 现有约束：自 ADR-0021 起 ci.yml 允许 actions/upload-artifact，但 **D2 正向白名单只允许 target/release 下三个二进制 + SHA256SUMS.txt**，由 K8 机器校验（存在上传步骤 + retention-days + if-no-files-found: error + path 不命中 denylist + 未使用未准入 action）。要把 runner 生成的基线取回，必须扩展产物范围，这属于「新增产物类别」，按 AGENTS 第 5 节必须走 ADR。

## 可选方案与被否决项

| 方案 | 描述 | 判定 |
| --- | --- | --- |
| A | 新增**仅 workflow_dispatch** 的 design-baseline 作业，在 runner 上跑 npm run design:baseline，用 actions/upload-artifact 上传 prototype/baseline 下的 PNG 与一个只记录工具链、浏览器版本与提交 SHA 的溯源清单；人工下载后**通过 PR** 入库 | **采纳**（见决策） |
| B | workflow 内用 GITHUB_TOKEN + contents: write 自动把基线提交回 main | **否决**（见下） |
| C | 保持开发机基线，把 B4 降级为告警（不阻断合并） | **否决**（见下） |
| D1 | 在 RM-C 参考机（固定字体 + 参考光栅器）上生成并验证基线（ADR-0014、docs/spec/07 第 3.4.4 节） | **暂缓，作为复议方向**：这是 ADR-0014 的目标形态，但 RM-C 尚不存在，且 ADR-0014 明确「云 runner 一律 NON-GATING」；当前 CI 只能在云 runner 上判定，必须先让云 runner 自洽 |
| D2 | 在 CI 中固定 Chrome 版本（下载 Chrome for Testing，或引入 setup-chrome 类 action） | **否决**：新增 action 会直接让 K8 变红（ADR-0021 准入清单只有 checkout / setup-node / upload-artifact）；且只固定浏览器、不固定 runner 镜像的字体栈，根因仍在 |
| D3 | 提高容差，或为每个 runner 维护一套基线 | **否决**：前者是放宽 §8 门禁（AGENTS 第 5 节）；后者把「回归检测」退化为「与上次的自己比」，并让基线文件组合爆炸 |
| D4 | 每次运行现场渲染，只做「同 run 双渲自检」，不与任何入库基线比对 | **否决**：等价于永远绿灯，只能证明动画已冻结，不能检测视觉回归，G3 名存实亡 |
| D5 | 只入库基线的哈希清单，不入库 PNG | **否决**：哈希同样是环境指纹且无法诊断，丢失 diff bbox 证据；PNG 比对是 docs/spec/07 第 3.4.4 节的既定实现物 |

### 方案 B 的详细否决理由

1. **绕过 PR 与 CODEOWNERS 双签**：AGENTS 第 3 节 / docs/spec/07 的 E4 要求跨 owner 目录改动由两侧各一名 reviewer 批准（PR-S5 人工阶段）。prototype/ 的 owner 是 @termai/shell-ux（.github/CODEOWNERS），基线入库是视觉契约变更，必须由 T2 Shell & UX 复核（docs/spec/07 第 3.4.4 节第 5 条）。机器人直接写 main，等于把「谁能改门禁的参照物」从 PR 审查降级为一段 workflow YAML。
2. **自触发与循环风险**：GITHUB_TOKEN 推送到 main 默认**不会**触发 push 工作流（GitHub 的递归防护）。若要让自动提交真正触发后续 CI，必须改用长期 PAT 或 GitHub App，从而引入具仓库写权限的长期凭据，正是 AGENTS 第 7 节第 4 条「默认更保守、默认不上传」要避免的。
3. **门禁自证循环**：若生成基线与验证基线由同一条无人值守路径完成，则一次真实视觉回归也可能被「自动刷新基线」覆盖成绿灯，门禁失去阻断能力。
4. **分支保护冲突**：一旦按 E4 启用分支保护，机器人直推 main 会被拒绝，workflow 变成长期红；若为此放宽保护，则整体治理被削弱。
5. **结论**：**若无「独立分支 + 自动开 PR + T2 必审 + 禁止直推 main」的附加安全设计，方案 B 一律否决。**

### 方案 C 的详细否决理由

DC-14 与 docs/spec/07 第 3.4.1 节的 G3 明确把视觉回归列为**合并阻断**。AGENTS 第 5 节规定：任何 §8 门禁的放宽必须附基准数据与 TSC 批准。当前没有基准数据支持「0.1% 不可达」；恰恰相反，开发机基线在 runner 上的 diff 仅 0.044%–1.245%，说明差异来自环境而非产品。正确修法是**让基线与验证同源**，不是降级门禁。

## 决策

一句话：**视觉基线的唯一权威来源是「验证它的那个 runner 环境」；本仓库新增一个仅 workflow_dispatch 触发的 design-baseline 作业，在该 runner 上生成基线并上传，由人工经 PR 入库；机器人不得写 main；0.1% 阈值与逐通道 ±2 容差保持不变。**

### D1 基线的权威来源

- 基线 = 用与 design 作业**同一 runner 镜像、同一浏览器构建、同一字体栈**渲染出的 PNG。
- 基线的可采信范围**仅限** BASELINE-MANIFEST.json 记录的 runner 镜像与浏览器版本；跨版本比对不构成视觉回归证据。
- 生成基线的页面只能是**已入库的原型** prototype/termai-ui-terminal-first.html；禁止从真实终端会话或任何用户数据渲染基线（AR-11 / HARNESS 第 6.3 节）。

### D2 生成机制：仅 workflow_dispatch

- 新增作业 design-baseline，条件为 if: github.event_name == 'workflow_dispatch'。
- **不进 push / pull_request 常规路径**：常规 push 不生成、不上传任何截图，避免产物存储成本，也避免每次 push 都公开一份 UI 截图。作业级 if 保证常规触发下**没有任何步骤被执行、没有任何上传发生**。
- 作业序：actions/checkout → actions/setup-node(22) → 定位 Chrome/Edge 并导出 CHROME_PATH 与浏览器版本 → npm run design:baseline → 写溯源清单 → actions/upload-artifact。
- 不新增 action：仍只用 ADR-0021 准入清单内的三个；K8 的 ADMITTED_ACTIONS 不变。
- 作业保留在 ci.yml 内（而非拆成新 workflow）：K8 只扫描 ci.yml，放在同一文件可让新增上传步骤继续处在机器校验之下。

### D3 产物范围（本作业专属的封闭白名单）

**仅允许两项**：

1. prototype/baseline/ 下本 runner 标签的视觉基线 PNG（windows-latest 对应 win32-x64-*.png），并以负模式 !prototype/baseline/*.current.png 显式排除调试期的比对产物。
2. prototype/baseline/BASELINE-MANIFEST.json：溯源清单，字段固定为 schema / adr / generator / commit / short_commit / run_id / repository / platform_tag / runner_os / runner_image / runner_image_version / node / rustc / browser_name / browser_version / baseline_files。

**明确禁止**（与 ADR-0021 D2 一致，本 ADR 不放宽）：日志、门禁报告 JSON、会话日志（.termai/）、测试临时文件、真实终端内容，以及任何含绝对路径的字段（HARNESS 第 6.3 节把路径与命令输出列为 L2）。清单只记录浏览器**文件名与版本号**，不记录其绝对路径。

### D4 与 ADR-0021 D2 白名单的关系

- ADR-0021 D2 是**按作业列举的正向白名单**：windows-build 作业仅允许三个二进制 + SHA256SUMS.txt。本 ADR 为 design-baseline 作业新增**并列且不重叠**的第二条白名单。
- 两条都是封闭列举，不构成「产物禁令放开」：ADR-0021 D2 的禁止项（日志、门禁报告、会话日志、测试临时文件、含路径者）**继续全面生效**。
- 数据分级依据：基线截图的内容是我们自己**已公开的原型页面**（L0 公开），不含 PTY 流、文件内容、命令输出或密钥（AR-11 / HARNESS 第 6.3 节）。ADR-0021 反方保留的「未签名产物可能被误认官方发布」不适用于截图：截图不是可执行产物，且 artifact 名与 manifest 明确标注为视觉基线、非发布物。
- 复议边界：ADR-0021 的复议条件（P1 引入签名 / SLSA 证明后重审二进制产物策略）覆盖 windows-build 作业；本 ADR 的复议条件独立（见下）。

### D5 K8 如何保持绿

1. 新增上传步骤同样设置 retention-days: 14 与 if-no-files-found: error（与 ADR-0021 D3 一致）。
2. path 只用 prototype/baseline/win32-x64-*.png（负模式排除调试产物）与 BASELINE-MANIFEST.json，**不命中** K8 denylist（target/debug、.termai、日志通配、target 通配、全仓通配、path: .）。
3. uses 仍只有 actions/checkout / actions/setup-node / actions/upload-artifact，全部在 ADR-0021 准入清单内。
4. K8 不解析 if，因此「仅手动触发」由 workflow 语义与本 ADR 保证；K8 的机器判定保持 PASS。

### D6 人工入环节点（职责）

1. 由总负责人（或 T5 DevEx 当值）在 main 上执行 gh workflow run ci.yml，触发 design-baseline。
2. 下载产物，取出 prototype/baseline 下的 PNG 与 BASELINE-MANIFEST.json。
3. 通过 **PR** 入库，PR 关联本 ADR 与 DC-14，由 T2 Shell & UX 复核（docs/spec/07 第 3.4.4 节第 5 条）。
4. 机器人不写 main；本机制只提供「生成与取回」，不提供「写入」。

### D7 验证方式（诚实边界）

- 本 ADR 落地时 **B4 仍为 FAIL**：runner 基线尚未生成，本 ADR 只交付机制，不主张门禁已修复。
- 转绿判据：下一次 CI 的 design 作业中 **B4 = PASS**，且报告打印 7 个状态的 hash 与 diff=0（或全部落在逐通道 ±2 内且超差像素数 = 0）。
- 若转绿后 runner 镜像滚动导致 B4 再次失败，先比对 BASELINE-MANIFEST.json 的 runner_image / browser_version：不一致则按本机制重新生成基线；一致则视为真实视觉回归，按 G3 阻断。

## 理由

1. 同时满足产品公理：A1「默认安全」——生成的产物只有我们自己已公开的 UI 截图与元数据，无用户数据、无环境绝对路径；A2「正确性」——让门禁参照物与验证环境同源，是像素比对成立的物理前提；A3 不受影响。
2. 不触碰不可协商清单：无新增 secret、无云端数据外发（AR-11），不引入新依赖类别（AR-21 / ADR-0015）。
3. 满足 AGENTS 第 5 节的 ADR 触发条件（新增产物类别 / 门禁实现机制），且**不放宽**任何 §8 门禁数值。
4. 人工 PR 入环保留 E4 双签与 DC-14 的阻断语义；自动化只覆盖「生成 + 取回」，不延伸到「写 main」。

## 后果

**正面**

- B4 第一次具备可成立的参照物：基线与验证同 runner、同浏览器、同字体栈。
- 基线来源可追溯：BASELINE-MANIFEST.json 记录镜像、浏览器版本与提交 SHA，失败时可区分「环境漂移」与「真实回归」。
- G3 / DC-14 的阻断语义保持不变，0.1% 阈值与逐通道 ±2 容差不被削弱。
- 常规 push 不新增上传，产物存储与公开暴露面最小。

**负面（必须接受的代价）**

1. **人工步骤不可消除**：基线刷新需要触发 + 下载 + PR，存在时延；这是换取 E4 双签与门禁自证安全的代价。
2. **依赖 runner 镜像稳定性**：GitHub 托管的 windows-latest 会滚动更新镜像与预装浏览器；每次滚动都可能使基线失效，需要重新生成。这是周期性维护成本，且当前唯一可行的云 runner 形态（ADR-0014：云 runner 一律 NON-GATING，故这里也不得声称它是参考机）。
3. 基线 PNG 入库会增加仓库体积（每个状态一张全屏 PNG），并随每次基线刷新在历史中累积。
4. design-baseline 作业不参与常规 CI，因此机制故障（例如浏览器缺失）**不会在 push 时暴露**，只有人工触发才发现；缓解是作业内对「找不到浏览器」「未写出 PNG」直接 exit 1。
5. 公开产物对全世界可下载（ADR-0021 已登记的同一条事实）；本项目 Apache-2.0 OR MIT，接受该暴露，但日志与报告仍在禁令内。

## 反方记录与复议条件

- **保留（09 DevEx / 自动化视角，支持方案 B）**：人工步骤是 toil，runner 每次滚动都要有人再做一遍；机器人提交可把「基线与环境同步」变成零延迟。**否决理由**：绕过 E4 与 PR 审查、引入自触发循环与门禁自证风险（见方案 B 详细否决）。**复议条件**：TSC 成立且分支保护已按 E4 生效之后，允许实现「独立分支 + 自动开 PR + T2 必审 + 禁止直推 main」的自动化；直推 main 永不复议通过。
- **保留（03 / 05 设计视角，支持方案 C 的动机）**：B4 的环境敏感度会造成高假阳性，浪费维护者时间。**否决理由**：DC-14 / G3 是合并阻断项；假阳性的根因是参照物不同源，本 ADR 已消除。**复议条件**：在同源基线建立后，若仍出现连续 2 次「manifest 显示环境未变却只差亚像素」的失败，可重议容差口径，但必须附基准数据并经 TSC 批准（AGENTS 第 5 节）。
- **保留（08 安全视角）**：公开产物面扩大。**否决理由**：本次新增的是已公开原型页面的截图与不含路径的版本元数据，分级为 L0；日志与报告仍在禁令内。**复议条件**：若基线生成被扩展为包含任何真实会话 / 用户数据（本 ADR 明确禁止），本 ADR 立即失效并重新决策。
- **被取代 / 复议方向（D1）**：ADR-0014 的 RM-C 参考机 + 参考光栅器是目标形态；当 RM-C 可用时，design-baseline 与 design 的视觉比对应迁移到 RM-C，本 ADR 的「云 runner 自洽」定位随之复议。

## 关联决策与实现位置

| 项 | 内容 |
| --- | --- |
| 关联决策 | DC-14（视觉回归为 CI 门禁，diff ≤0.1%）、DC-09（生成物入库）、AR-11 / AR-12（数据边界与日志纪律）、E4（跨 owner 双签）、G3（docs/spec/07 第 3.4.1 节） |
| 实现位置 | .github/workflows/ci.yml，作业 design-baseline（仅 workflow_dispatch） |
| 基线目录 | prototype/baseline/（PNG 随代码入库；BASELINE-MANIFEST.json 为溯源清单） |
| 生成工具 | tools/design-gates/check-design.mjs --update-baseline（即 npm run design:baseline；复用既有实现，不改语义） |
| 机器门禁 | tools/kernel-gates/check.mjs 的 K8（无需修改，保持 PASS） |
| 复核责任 | T2 Shell & UX（基线视觉复核）+ T5 DevEx（运行与产物） |
