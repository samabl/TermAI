# P0 验证手册（可执行命令 + 每条命令防的坑）

> 目的：把本会话的可运行操作知识集中到一页。每条命令给出：做什么 / 期望输出 / 它防的坑。
> 口径基线（第 112 轮）：kernel-gates 8 PASS ｜ conformance gating 64/64 ｜ bench:check 7 PASS（gating INCONCLUSIVE）｜ esctest 267 passed / 41 known-bug / 259 failed / 0 substitutions，且污染 0。

**第 114 轮已逐条实跑验证**：5 条门禁命令全部产出文档所述结果——① kernel-gates `8 PASS / 0 FAIL`；② selftest `result: PASS - every executed injection was caught; the gates are not always-green`；③ conformance `gating 64/64`、`G1: NOT_JUDGED`；④ bench:check `7 PASS / 0 FAIL`；⑤ ci-cost `state=UNDER_WARN`、`exit=0`。**手册本身是被验证过的，不是写下来就算的**（一份命令写错的手册比没有手册更糟）。

## 0. 三条铁律（各由一次事故换来）

1. 改动有效性先在协议层确认，再跑套件——套件用于计数，不用于定性。
2. 验证与提交分开执行；看到绿色的输出再 commit。合并两者等于把门禁降级成日志。
3. cargo build 必须看到 Compiling termai-… 才算重建；只看到 Finished 说明你测的是旧产物（本会话因此误判三次）。

## 1. 门禁（随时可跑）

```powershell
node tools/kernel-gates/check.mjs            # 期望 summary: 8 PASS / 0 FAIL / 0 SKIP
node tools/kernel-gates/check.mjs --selftest # 注入故障后仍能报红，证明门禁非恒绿
npm run conformance                          # 期望 gating 64/64  R_strict=1  R_gate=1；G1: NOT_JUDGED
npm run bench:check                          # 期望 7 PASS；gating INCONCLUSIVE / REFERENCE_MACHINE_UNAVAILABLE
node tools/ci-cost/check.mjs                 # 期望 state=UNDER_WARN，exit 0
```

坑：bench:check 的 gatingNumbersProduced: 0 不是失败——本机不是 RM-A/RM-C，§5 数字按 ADR-0014 一律 NON_GATING。不要在云 runner 上声称性能达标。

## 2. esctest（E-P0-1 的计数字段）

```powershell
python tools/conformance/upstream/esctest_adapter.py --esctest <esctest2 检出> --out target/conformance/<name> -- --expected-terminal xterm --xterm-checksum 336
```

期望：*** 267 tests passed, 41 known bugs, 259 TESTS FAILED *** 与 substitutions=0。

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
3. 契约层：node tools/kernel-gates/check.mjs（8 PASS）。
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
