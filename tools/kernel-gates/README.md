# tools/kernel-gates —— 合并阻断门禁 K1–K8

**这是什么**：AGENTS §4「合并前必须通过的门禁」在本仓库的实现。**K1–K8 全绿才可合并**（`kernel:check` 是两个 CI job 的第一步）。

```powershell
node tools/kernel-gates/check.mjs              # 期望 summary: 8 PASS / 0 FAIL / 0 SKIP
node tools/kernel-gates/check.mjs --selftest   # 注入故障并证明门禁不是恒绿（当前 30 条注入全部被捕获）
node tools/kernel-gates/check.mjs --json       # 机器可读
node tools/kernel-gates/check.mjs --root=<d>   # 对另一棵树跑（selftest 用它构造临时工作区）
```

## 八道门禁

| 门禁 | 检查 | 依据 |
| --- | --- | --- |
| **K1** | `cargo fmt --all -- --check` | — |
| **K2** | `cargo clippy --workspace --all-targets -- -D warnings` | — |
| **K3** | `cargo test --workspace` | — |
| **K4** | 依赖形态：允许的 crate/app 边（**ADR-0019** D1）+ **GPL/AGPL/SSPL 黑名单**（AR-21）+ **AR-03：内核不得依赖网络/AI/UI 库** | ADR-0019、AR-21、**AR-03** |
| **K5** | 许可：每个 `package.license` 为 `Apache-2.0 OR MIT`（或 `license.workspace = true`） | AR-21 |
| **K6** | 规格缺陷登记表完整：M0（SD-01..08）与 P0（SD-09..**23**） | AGENTS §5 |
| **K7** | `.github/CODEOWNERS` 为每个受跟踪的顶层目录给出规则 | spec 07 §3.1.2 |
| **K8** | 工作流策略：构建产物留存（**ADR-0021**）+ **每个 check 步骤必须配对 selftest** | ADR-0021、**第 141 轮新增** |

## 两处由本会话加入的判据（新增门禁请照此办理）

- **K4 的 AR-03 判据**（第 162 轮）：`KERNEL_CRATES` 六个内核 crate 对 `NETWORK_AI_UI_TOKENS` 逐个比对。**`termai-render` 有意不在 `KERNEL_CRATES` 内**——**ADR-0024／ADR-0027 允许它引入 wgpu／winit／rustybuzz／swash**。
- **K8 的配对判据**（第 141 轮）：~~六行~~ ~~八行（第 258 轮；第 141 轮时为六行）~~ **十行（第 265 轮；第 141 轮时为六行）**显式清单（`GATE_PAIRS`）。**用显式清单而不是从步骤名推断**，因为步骤名不统一（`tokens:check`、`ci-cost check`、`conformance L0`）——**第 140 轮按名字推断曾误报 13 条**。

## 改门禁时的硬要求

**改动任何判据（新增、改判据、改阈值）都必须**同时**加一条注入与一条对照，并跑 `--selftest` 确认它被捕获**——见 `docs/plan/p0-verification-runbook.md` §5 第 3 步。

**理由是本仓库三次实测**：`ci-cost` 的上限分支曾经**从未执行过**；`K4` 的 AR-03 分支曾因 **TDZ** 成为**假绿**（判据一旦命中就抛错、不命中就永远不报）；`B7` 的第三条判据由**字面常量**承担因而恒真。**三次都不是「有人忘了规则」，而是「检查写得比它守的东西晚」。**

**`--selftest` 的 30 条注入覆盖 K1、K4、K5、K6、K7、K8**；**K2／K3 是薄包装**（跑一条 cargo 命令、看退出码），**其失败路径与 K1 的注入走的是同一条管道**——**因此未单独注入，这是有意保留的取舍，不是遗漏**（第 164 轮的判断）。
