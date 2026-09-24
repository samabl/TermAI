# tools/ci-cost —— CI 成本读数与上限门禁

**检查什么**：`ci-cost.json`（本目录之外、仓库根）的**结构完整性**、**算术一致性**，以及**估算月成本是否触及/超过上限**。

```powershell
node tools/ci-cost/check.mjs              # 期望：ci-cost: OK - schema complete, arithmetic consistent, under the cap（exit 0）
node tools/ci-cost/check.mjs --selftest   # 期望：ci-cost selftest: PASS - 3 branch(es) behaved as documented
node tools/ci-cost/check.mjs --json       # 仅输出机器可读 JSON
```

## 判定分支（`--selftest` 覆盖的三条）

| 分支 | 触发 | 期望 |
| --- | --- | --- |
| 算术 | 声明的总额与逐条求和不符 | **报错** |
| 上限 | 估算触及或超过 `caps_usd_per_month` | **报错**（`AT_OR_OVER_CAP`） |
| 对照 | 真实清单 | **通过**——**排除「校验器一律拒绝」** |

**这三条是第 133 轮补上的**：在此之前，上限分支**从未被执行过**——一个从未走过的失败路径，不算「已知能用」，只算「已知能编译」（参见计划 §6.3 规则 10）。

## 读这份数字前必须知道的

**`ci-cost.json` 里的分钟数/运行次数是**量级假设**，不是实测**——清单用 `assumptions` 字段记录这一点，请连同 `$comment` 一起读。

**`not_included` 列出的项目没有被计入成本**（该字段就是为此存在）。**引用总额时不要越过它**——否则会把「未计入」读成「已覆盖」。

**依据**：ADR-0014 决策 6、spec 06 `A-PM-12`、E-P0-3；接入 CI 的两个 job（Windows / linux-x64），与 `kernel-gates`、`bench`、`conformance` 并列。

**为什么有这份 README**：同目录的兄弟工具（`tools/bench/README.md`、`tools/design-gates/README.md`）都有——而本工具的数字**恰恰是最容易被当真的一类**（一个货币金额），**所以它更需要写明哪部分不是实测**（第 205 轮补）。
