# P0 验证手册（可执行命令 + 每条命令防的坑）

> **逐节复核（第 204 轮，对照当时的代码与 CI）**：**§1 门禁清单 ✓ 完整（~~八条命令，含四项 selftest~~ **第 259 轮：十二条命令，含六项 selftest**）｜§2 esctest ✓ 准确（命令含必备的 `-- --expected-terminal xterm --xterm-checksum 336`；期望 267/41/259/0 与实测一致）｜§3 两个探针 ✓ 准确（含 `alt_decsc` 的期望输出）｜§4 分诊三件套 ✓ 准确（含三条坑注）｜§5 验收顺序 ◐ 已补（原缺「改动门禁本身」的情形，见该节第 3 步）｜§6 环境缺口 ✓ 准确**。**复核方式为逐节读取并与代码/CI 现状对照，不是重跑全部命令**——**§1 的命令在第 199 轮已整串实跑（全绿）。**

> 目的：把本会话的可运行操作知识集中到一页。每条命令给出：做什么 / 期望输出 / 它防的坑。
> 口径基线（第 112 轮）：kernel-gates 8 PASS ｜ conformance gating 64/64 ｜ bench:check ~~7 PASS~~ → **8 PASS（第 257 轮 ADR-0029 D-4 加 B9）**（gating INCONCLUSIVE）｜ esctest 267 passed / 41 known-bug / 259 failed / 0 substitutions，且污染 0。

**第 114 轮已逐条实跑验证**：5 条门禁命令全部产出文档所述结果——① kernel-gates `8 PASS / 0 FAIL`；② selftest `result: PASS - every executed injection was caught; the gates are not always-green`；③ conformance `gating 64/64`、`G1: NOT_JUDGED`；④ bench:check ~~`7 PASS / 0 FAIL`~~ → `8 PASS / 0 FAIL`（第 257 轮加 B9）；⑤ ci-cost `state=UNDER_WARN`、`exit=0`。**手册本身是被验证过的，不是写下来就算的**（一份命令写错的手册比没有手册更糟）。

## 0. 四条铁律（各由一次事故换来）

1. 改动有效性先在协议层确认，再跑套件——套件用于计数，不用于定性。
2. 验证与提交分开执行；看到绿色的输出再 commit。合并两者等于把门禁降级成日志。
3. cargo build 必须看到 Compiling termai-… 才算重建；只看到 Finished 说明你测的是旧产物（本会话因此误判三次）。
4. **绝不在仓库内建探针包，也绝不对仓库内的 manifest 跑 cargo**（第 260 轮事故）：在仓库根下 `cargo new target/<probe>` 会让 cargo 把它**自动加进根 `Cargo.toml` 的 `members`**，并把**整棵依赖树写进根 `Cargo.lock`**——本轮实测 **`Cargo.lock` +2574 行**。而 **ADR-0027 明文禁止这些 crate 在准入前进入 workspace 依赖图**，K3/K4 也会随之改变行为。**做法**：探针放到**仓库外**（如 `$env:TEMP\<probe>`），并在其 `Cargo.toml` 末尾加一个空的 `[workspace]` 使其自成 workspace；每次 cargo 调用后跑一次 `git diff --stat -- Cargo.toml Cargo.lock`，**必须为空**。**判据**：`git status` 里出现根 `Cargo.toml`/`Cargo.lock` 的改动，就是这条被违反了。

## 1. 门禁（随时可跑）

```powershell
node tools/kernel-gates/check.mjs            # 期望 summary: 8 PASS / 0 FAIL / 0 SKIP
node tools/kernel-gates/check.mjs --selftest # 注入故障后仍能报红，证明门禁非恒绿
npm run conformance                          # 期望 gating 64/64  R_strict=1  R_gate=1；G1: NOT_JUDGED
npm run bench:check                          # 期望 8 PASS（第 257 轮加 B9）；gating INCONCLUSIVE / REFERENCE_MACHINE_UNAVAILABLE；值区块打印「0 of 3 reported (H17, H18, H19)」（未给 --report 时）
node tools/ci-cost/check.mjs                 # 期望 state=UNDER_WARN，exit 0
node tools/ci-cost/check.mjs --selftest      # 期望 3 分支如文档所述（2 注入 + 1 对照）
node tools/conformance/selftest.mjs          # 期望 PASS（注入 1 条坏期望被捕获 + 对照成立）
npm run conformance:verify                   # 期望 PASS (63 curated cases match the table)
node tools/conformance/verify-selftest.mjs   # 期望 PASS（注入 1 条生成器/语料漂移被捕获 + 对照成立）
npm run conformance:suites                   # 期望 PASS（静态能力声明：esctest 声明级别 1 + color-query 静态排除，均有 ADR + kernel/01 引用）
node tools/conformance/check-suites.mjs --selftest  # 期望 PASS（6/6 注入被捕获 + 对照成立）
npm run bench:selftest                       # 期望 PASS - every injection was caught（~~第 256 轮：65/65~~ → 第 257 轮：74/74）
```

坑：bench:check 的 **⚠ 第 201 轮更正**：**「0 不是失败」这一条在本机**目前**有两种原因，而手册此前只说了其中一种**——**手册的原话把 0 解释为「本机不是 RM-A/RM-C，§5 数字按 ADR-0014 一律 NON_GATING」（这是**设计意图**）；**但第 188/189 轮核实：`check.mjs:672` 的 `gatingNumbersProduced` 是**字面常量 0**，**没有任何代码从结果计算它**——**因此今天的 0 是**常量**，不是「算出来发现没有 gating 数字」。** **对读者的实际影响**：**不要因为这一行而以为工具「测量过并正确地拒绝给出门禁数字」**——**它目前没有测量**。**✅ 第 255 轮：D-6 第 ④ 步已做出来**——`gatingNumbersProduced` 现由 `tools/bench/values.mjs` 从读取到的、声明 `gating:true` 的 metric 计算（注入该行会让 `B7` FAIL，见 `bench:selftest` 63/63）；**但「本工具没有测量」仍成立**：机器无关行的值由 `--report` **读入**，不是它测出来的。 gatingNumbersProduced: 0 不是失败——本机不是 RM-A/RM-C，§5 数字按 ADR-0014 一律 NON_GATING。不要在云 runner 上声称性能达标。

## 2. esctest（E-P0-1 的计数字段）

```powershell
python tools/conformance/upstream/esctest_adapter.py --esctest <esctest2 检出> --out target/conformance/<name> -- --expected-terminal xterm --xterm-checksum 336 --max-vt-level 1
```

期望（**级别 1**，第 257 轮 ADR-0030 起必须显式声明级别）：*** 99 tests passed, 378 known bugs, 90 TESTS FAILED *** 与 substitutions=0。

**第 257 轮（ADR-0030）：esctest 的数字必须连「级别 + eligible 分母」一起报，缺级别的数字不得引用。**
规则：声明的 VT 级别 = xterm DA1 在该级别的 expected 集合**全部为已实现能力**的最高级别（当前 = **1**）；**禁止**按失败数选级别。
本机实测（同一检出、同一命令，只换 `--max-vt-level`）：

| level | passed | known-bug | failed | eligible = passed+failed |
| --- | --- | --- | --- | --- |
| **1（当前声明）** | 99 | 378 | **90** | 189 |
| 2 | 105 | 369 | 93 | 198 |
| 3 | 111 | 334 | 122 | 233 |
| 4 | 266 | 43 | 258 | 524 |
| 5（旧口径/默认） | 267 | 41 | **259** | 526 |

低级别下的 known-bug 含大量**「因级别不足未运行」**的用例——报告必须单列为 `excluded_by_vt_level`，**不得**叙述成已知缺陷。**E-P0-1 的当前口径 = level 1：90 失败 / 189 eligible，另有 378 条按级别排除；两个数字必须并列**（这是换尺子，不是改善）。

必须带 --xterm-checksum 336：缺它时 esctest 对每格校验和取反，得出 110/117 一类的伪失败（第 39 轮踩过）。
单条复现（3 秒，替代 60 秒全量）：--include <Class.test> 必须放在 -- 之后（适配器自身不认它）。
AR-27：任何被引用的数字都要跑两次并逐字段一致（本会话已对 267/259 做过，失败集合逐字节相同）。

## 3. 两个协议探针（定性用）

A. 直驱二进制（确认改动是否真的进了被测产物）：
```powershell
$s = "FEED <hex>`nQUIT`n"; $s | & .\target\debug\termai-vt-conformance.exe --server
```
B. 用终端自身读状态（无需新增访问器，DECRQM 查模式）：
```powershell
# CUP(2,3) ; DECSC ; DECSET(47) ; DECRQM(47) ; CUP(6,7) ; DECSC ; DECRESET(47) ; DECRQM(47) ; DECRC ; DSR
$seq = '1b5b333b3248'+'1b37'+'1b5b3f343768'+'1b5b3f34372470'+'1b5b373b3648'+'1b37'+'1b5b3f34376c'+'1b5b3f34372470'+'1b38'+'1b5b366e'
$s = "FEED $seq`nQUIT`n"; $s | & .\target\debug\termai-vt-conformance.exe --server
# 期望（含 alt_decsc 修复）：x1b[?47;1$y 然后 x1b[?47;2$y 然后 x1b[3;2R
```

## 4. 分诊三件套（E-P0-1 台账的机器产物）

```powershell
node tools/conformance/failing-index.mjs <log> <testsDir> docs/audit/esctest-failing-index.md
node tools/conformance/classify-esctest.mjs
node tools/conformance/triage-single.mjs <log> <ClassPrefix 或 ALL> <outRoot>
```

坑：分诊必须到单条用例——按类隔离对类内污染是盲的（第 82 轮）。
坑：--include 是正则；类名里的点号会匹配任意字符。
坑：空输出要当无效，不是 0 失败（第 81 轮把自己的过滤写法错当成工具无输出）。

## 5. 改动的验收顺序（每次照做）

1. 协议层：用探针确认行为变了。
2. 单元/工作区：cargo test；cargo fmt；clippy -D warnings。
3. 契约层：node tools/kernel-gates/check.mjs（8 PASS）。**⚠ 若你的改动**改动了门禁本身**（新增判据、改判据、改阈值），还必须跑该门禁的 selftest，并**为改动的那条分支加一条注入 + 一条对照**——`kernel-gates --selftest`（~~23/23~~ **~~24/24~~ → ~~26/26（第 258 轮加注入 + 对照）~~ **28/28（第 259 轮）**）、`bench:selftest`（~~65/65~~ **74/74（第 257 轮）**）、`conformance selftest`、`ci-cost --selftest` 是四个现成范例。理由是本会话三次抓到的同一件事**：**一个「通过」的门禁不等于一个「能失败」的门禁**——`ci-cost` 的上限分支从未执行、`K4` 的 AR-03 检查因 TDZ 假绿、~~`B7` 的第三条判据由字面常量承担~~ **（第 255 轮已改为由读取值承担）**。**第 5 步管的是「被测对象」的对照，本句管的是「门禁自身」的对照，两者不可互替。**
4. 计数层：跑 esctest 并与改动前对比；变差就回滚（不留无收益改动）。
5. 锁死：给收益写回归测试，且含负例对照（否则会退化成恒绿）。
6. 记账：更新登记表与出口总表（改数字必须同时改总表——第 98 轮的漂移就是这么来的）。

## 6. 已知的环境缺口（不是排期问题）

| 缺什么 | 影响到 |
| --- | --- |
| C 编译器（no acceptable cc found in PATH） | vttest 无法构建 → E-P0-1 的另一半 |
| Xvfb + 钉定 xterm 的 oracle 环境 | G1 真实语料（AR-31 要求至少 20%） |
| 网络（仅非公网可达） | 渲染依赖的 SPDX 证据 → E-P0-2 无法准入 |
| RM-A/B/C 参考机 | E-P0-3 的判定（接线已完成） |
| 仓库设置 / 发起人 | C1 TSC、C2 CODEOWNERS 双签、C3 分支保护、C5 真实 CI 运行 |
| **Chrome / Edge（`CHROME_PATH`）** | **`design:check` 的浏览器层（B4 视觉回归）：本机无浏览器时该门禁**判 FAIL**（`maxDiffPct=0` 无法比对），**而 CI 的 design job 会安装浏览器**。**第 250 轮实跑：`design:check` 得 `19 PASS / 1 FAIL`，唯一阻断项是 B4**——**因此「全绿」指的是**本机能跑的那些门禁**，设计门禁与 vttest 同属**环境受限** | **⚠ 第 251 轮补精确化**：**本机跑不了的只是 `design:check` 的**浏览器层**——**它的 `--selftest` 在本机**可以跑，且**通过**（**`injected faults caught: 19/19`**，**注入施加在临时副本上、原型源未被改动**）。**因此「环境受限」指的是**核查**，不是**自检**。**（同一轮实测：`tokens:selftest` 亦通过——**至此六个 selftest 在本机全部跑通**：kernel ~~23/23~~ **~~24/24（第 255 轮）~~ → ~~26/26（第 258 轮）~~ **28/28（第 259 轮）**、bench **65/65（第 256 轮）**、ci-cost 3/3、conformance、tokens、design 19/19。）** **⚠ 第 252 轮再补：该门禁**半数可在本机跑****——**`design:check --static` 实测 `10 PASS / 0 FAIL / 10 SKIP`，result PASS，exit 0**（**10 个 SKIP 就是浏览器层的那些，工具明确标注「not executed: --static / --no-browser requested」**）。**因此准确的画像是**：**静态层（10 关）本机通过；浏览器层（10 关）本机无法执行——`--static` 下 SKIP，完整检查下因 B4 无法比对而 FAIL。** **而 CI 的 design job 正是分两步跑**（先 static layer、再 full）——**本机与 CI 的差别因此只在第二步。**
## 7. 约定检查（文档自身的规则，可运行）

```powershell
node tools/audit/lint-notes.mjs            # 列出「宣布了更正、却没有删除线」的候选行（默认扫 docs/audit/debt-p0.md）
node tools/audit/lint-notes.mjs --selftest # 自证：植入的违规必须被找到，两个对照必须干净
node tools/audit/check-claims.mjs         # 核对「§6.3 有 N 条纪律」这类**计数声明**与实际的条数是否一致（不一致则 exit 1）
node tools/audit/check-claims.mjs --selftest # 自证：一致的计数必须干净、不一致的必须被报出
```

**它查的是什么**：**计划 §6.3 规则 12 的删除线约定**——**一行的结论变了时，被取代的原句必须划掉，而不是只在后面追加**。**本会话四次实例**（第 147/148 轮的旧断言留在行首、第 218 轮我自己的追加、第 220 轮扫出 A15／A16）。

**何时跑（第 230 轮补）**：**凡改动了 `docs/audit/debt-p0.md` 的表格行，改完就跑一次**——**它一条命令、结果稳定**（**第 230 轮实跑：4 条候选，与第 222 轮完全相同，且 `--selftest` PASS**），**因此它的成本不是「要不要跑」，而是「不跑就没查」**。**若候选数**增加**，说明新写的更正在行首留下了旧断言**（**本会话四次实例：第 147／148／218／220 轮**）。

**⚠ 它是**候选清单**，不是门禁**：**无论有无候选都 exit 0**。**第 220 轮实测 36 行得 5 条候选，其中 2 条是判断问题**（**数字被整体替换、旧文本已不存在**的；**缺陷登记表的标题是原始发现、结论列承载更正**的）。**因此它只负责把人指到该看的行上，判定留给人**——**第 140 轮的教训：按模式推断违规会造假警报，而总在喊的检查会教人忽略它。**
