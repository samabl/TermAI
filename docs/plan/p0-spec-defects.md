# TermAI P0 交付期规格缺陷登记（SD-09 起）

> **性质**：本文件是实现期**发现**，不是设计权威。M0 期的登记见 [m0-spec-defects.md](m0-spec-defects.md)（SD-01…SD-08）。
> **纪律**（AGENTS §5）：修正既有结论必须走 ADR；本文件只做**登记**，处置落在代码注释与本表，需要的追认动作单列。
> **依据**：HARNESS §11.2（CR-14 / CR-15 / CR-16）、§5、§8.1；**AR-24 第 3 条**（kernel/06 是测量口径的唯一权威）；ADR-0014（平台矩阵与参考机）、ADR-0023（P0 契约 errata）。

## SD-09｜spec 07 §3.8.2 的扁平 legacy 对象与 kernel/06 §3.7 的结构化字段同名冲突

- **证据**：spec 07 §3.8.2 期望一个扁平对象 `{metric,value,unit,samples,runner,commit,toolchain,ts}`；而 kernel/06 §3.7 的 bench-report 顶层 `commit` / `toolchain` / `runner` **都是对象**。把扁平字段并入顶层会与**同一节**的 schema 直接冲突。
- **本 P0 处置**：以 **kernel/06 §3.7 为唯一 schema 权威**（AR-24.3）；9 个 legacy 字段按 §3.7 原文落在 **metric 级 8 个 + 顶层 `commit`**；spec 07 §3.8.2 描述的扁平形态作为**加法字段 `flatProjection`** 承载（§3.7 允许加法字段向后兼容 1 minor），并由门禁 B2 每次运行打印该歧义。
- **需要的动作**：spec 07 §3.8.2 补注「扁平对象是 legacy 摘要形态，权威 schema 见 kernel/06 §3.7」；已登记 **HARNESS CR-16**。

## SD-10｜kernel/06 §4 的 FailKind 缺「绝对门禁越界」成员

- **证据**：§3.1 的 `FAIL(v, gate)` 需要表达「未超回归阈值但**越过绝对门禁**」（例如空闲 RSS >120MB、帧时 ≥8.3ms），而 §4 的 `FailKind` 枚举没有对应成员。
- **本 P0 处置**：`tools/bench` 以扩展项 **`GATE_BREACH`** 实现，标 `origin:'extension'`，**不冒充 spec 成员**。
- **需要的动作**：kernel/06 §4 增列该成员（分册修订，不改 AR/DC）。

## SD-11｜kernel/06 §3.2 未给 H3（PTY-LAT-1）的 Run 定义

- **证据**：AR-30 第 1 条把 **PTY-LAT-1 升为 §5 门禁**（P99 ≤2ms），但 kernel/06 §3.2 的测量定义只写到 H2；H3 的 Run / 样本量口径缺失。
- **本 P0 处置**：`tools/bench` 按「与 H2 共用注入流」读作 `isTail` / ≥1e5，标 `origin:'reading'`，并在 registry 与 README 显式登记为**读数而非引用**。
- **需要的动作**：kernel/06 owner 确认或改判；若改判，H3 的判定实现必须同步（否则 H3 会以错误口径进入门禁）。

## SD-12｜非 spec 原文的机器分类原因码

- **证据**：`CLOUD_RUNNER` / `FLOOR_MACHINE_ONLY` / `NOT_MACHINE_GATE` 三个原因码不在 kernel/06 正文，是由 **ADR-0014 决策 1 / 铁律 5** 与 **AR-31 第 8 条**推导出来的。
- **本 P0 处置**：标 `origin:'extension'` 并在 `tools/bench/README.md` 给出推导链。
- **需要的动作**：若 kernel/06 owner 认可，并入 §6 / §3.4 的枚举登记；否则应改为 spec 已有的码。

## 处置总表

| 编号 | 落点 | 类型 | 本 P0 处置 | 需要动作 |
| --- | --- | --- | --- | --- |
| SD-09 | spec 07 §3.8.2 与 kernel/06 §3.7 | schema 冲突 | kernel/06 §3.7 为权威；扁平形态入加法字段 `flatProjection` | spec 07 §3.8.2 补注；**CR-16 已登记** |
| SD-10 | kernel/06 §4 | 枚举缺成员 | `GATE_BREACH` 扩展项 + `origin:'extension'` | kernel/06 §4 增列 |
| SD-11 | kernel/06 §3.2 | 定义缺失 | 按与 H2 共用注入流读作 `isTail`/≥1e5，标为**读数** | kernel/06 owner 确认或改判 |
| SD-12 | kernel/06 §3.4/§6 | 原因码越界 | 标 `origin:'extension'` 并写明推导链 | owner 认可后并入枚举 |

## 与前序登记的关系

- SD-01…SD-08 的处置见 [m0-spec-defects.md](m0-spec-defects.md)；本文件**不重复**其内容。
- SD-07 / SD-08.1 / SD-08.2 已由 **ADR-0023** 处置（见该 ADR 与 m0-spec-defects 处置总表）。
