# ADR-0015：AR-21 链接边界的 CI 可执行定义、SPDX 白名单与供应链信任根

- 状态：Accepted
- 日期：2025-01-15
- 决策者：项目发起人授权的「开源许可工程 / 法务 + 供应链安全」联合评审（经 Orchestrator 确认）
- 关联：AR-21（链接边界授权与 GPL/AGPL/SSPL 红线）、AR-10（许可与商业形态部分被 AR-21 取代，分歧记录保留）、ADR-0013（全栈开源许可范围）、DC-34（审计链）、DC-36（模型供应链）、DC-38（插件宿主）；HARNESS §0.4、§6.1、§6.3、§8.1-5、§11 OQ-27/OQ-28；docs/spec/05 §3.1.3/§3.7/§3.8/§3.11、docs/spec/07 §3.2.3/§3.11（OQ-E4/E5/E6/E7）
- 实现位置：仓库根 `deny.toml`；`third-party/policy.toml`（规范白/黑名单）、`third-party/runtime-tools.toml`、`third-party/build-tools.toml`、`third-party/boundary-exceptions.toml`、`third-party/assets.toml`；`crates/termai-xtask/src/license.rs`（`cargo xtask license --boundary`）；CI job `G5`（§8.1-5）；发布 manifest 与 provenance 流水线

## 背景与问题

AR-21 裁定「核心链接边界内禁止 GPL/AGPL/SSPL」，并把「链接边界的 CI 可执行定义」与「MPL-2.0 / EPL-2.0 等弱 copyleft 准入」明确委托给本 ADR（AR-21 第 3 条）。同时 OQ-28（合规部分）在签名密钥托管与门限、SLSA 等级、审计保留期与外锚、崩溃上报端点归属与保留期、遥测与审计默认值上均无唯一结论。

docs/spec/07 §3.11 目前只有一句操作定义：「编译 / 链接进发布产物的单元及其 build-dependencies 与静态链接库计入；dev-dependencies、测试用例、仅文档工具不计入」。该定义在 CI 实际落地时会撞上以下无法回避的问题：

1. **proc-macro** 在 Cargo 中声明于 `[dependencies]`，但只在编译期被 rustc 加载、输出令牌并入我方源码 —— 按包名扫描会误判，按运行时依赖扫描会漏判。
2. **代码生成器与编译器插件** 的输出是否继承生成器的 GPL/AGPL？GCC（RLE）、LLVM（Apache-2.0 WITH LLVM-exception）、Bison（输出例外）与无输出例外的 GPL 生成器必须给出同一判定口径。
3. **静态链接、动态链接、dlopen** 是否同一判定？是否存在以 dlopen 或 shim 包装规避静态链接检查的空间？
4. **vendored C 源码、`-sys` crate 经 `cc` 编译、被内联/复制粘贴的 GPL 函数或常量表**如何处置？
5. **运行时 subprocess 调用 GPL/AGPL 可执行文件**（git、ffmpeg、ripgrep）是否构成「链接」？这是本项目最常被问、也最容易被两个极端（一律禁止 / 一律豁免）答错的一点。
6. **WASM 沙箱插件与 trusted subprocess 插件**调用 GPL 工具、以及插件包内**捆绑** GPL 二进制，是否越界？
7. **SPDX 表达式**的 OR / AND / WITH / -or-later 如何求值？未知或缺失许可如何处理？
8. **弱 copyleft**：MPL-2.0 / EPL-2.0 / CDDL / LGPL-2.1 / LGPL-3.0 各自能否进入核心链接边界、进入哪个位置？

失败代价是双向的：判定过宽 → 误伤可用生态依赖、逼迫自研、拖慢 P0 交付；判定过窄 → 把 GPL/AGPL 引进发布产物，直接击穿 AR-21「保护下游与企业采用」的承诺与 §0.4 不可协商清单、§8.1-5 门禁，且**事后无法通过重新打包补救**（下游已获得 GPL 派生二进制）。因此判定必须**默认拒绝、机器可执行、可审计、可复议**。

OQ-28 的另一半同样缺可执行结论：密钥托管是「HSM 还是 CI secret」、门限是「几之几」、轮换周期、泄露后的时间线；SLSA 是「尽量高」还是具体等级与达成路径；审计保留期与外锚、崩溃端点归属与保留期、遥测与审计的默认值口径。这些是供应链信任根，不能停留在建议语气。

## 可选方案（至少 2 个，含被否决项）

| 方案 | 描述 | 否决 / 采纳理由 |
| --- | --- | --- |
| A 纯 cargo-deny 许可白名单 | 只按包名与许可证字符串匹配，不区分运行时 / 构建期 / 链接方式 | **否决**。无法覆盖 proc-macro、vendored C、`cc` 编译、dlopen、subprocess、插件捆绑，存在系统性漏判 |
| B 只做法务人工评审 | 每个依赖人工签署 | **否决**。不可持续、无回归防护、无法在 15 分钟 PR 反馈预算内执行（spec 07 §3.4.2） |
| C 一切非宽松许可全禁（含 MPL/LGPL） | 最大保守 | **否决（过度保守）**。MPL-2.0 为文件级 copyleft（MPL §3.3 明确允许 Larger Work 组合），未修改的动态 LGPL 链接在许可文本与产业实践中风险可控；全面禁止会排除关键依赖并逼迫自研，与 A2（延迟/正确性优先）冲突 |
| D 分层可执行边界 + 机器分类器 + 例外审批 + 弱 copyleft 分级准入（本 ADR） | 用「是否与第一方代码同进程 / 同产物」定义边界；构建期与运行时进程外工具走独立白名单；一切未知默认拒绝 | **采纳** |
| E 只要 fork/exec 就全量豁免 | 把 subprocess 一律视为 out-of-boundary | **否决（作为无限豁免）**。会被「shim 包装」「必需 GPL 组件」「随产物捆绑 GPL 二进制」绕过；必须叠加可替换性、不修改、不捆绑与归因条件（T1–T6） |
| F 只做运行时扫描（对发布产物跑扫描器） | 只检最终二进制 | **否决（作为唯一手段）**。发现太晚、无法归因到源、且 dlopen 与动态下载可绕过；仅保留为**第二道**校验 |

## 决策

### D0 唯一结论（一句话）

**链接边界 = 「与 TermAI 第一方代码在任一发布产物中同进程执行，或被静态 / 动态 / 内联 / 派生并入该产物」的全部第三方单元及其派生代码。边界内：GPL / AGPL / SSPL / BUSL 及一切 field-of-use 限制许可 = 禁止；MPL-2.0 = 允许；LGPL-2.1/3.0 = 仅允许未修改的动态链接；EPL-2.0 / CDDL = 默认禁止、逐案审批。构建期独立进程工具与运行时 subprocess 在满足 BT / T / AE 条件时位于边界外。一切未知或缺失许可 = 拒绝（fail closed）。**

### D1 术语与边界定义（可执行）

**术语**
- **发布产物（Artifact）**：任一向外分发的可执行文件、库、容器镜像、SDK / 协议包、WASM 组件、CLI。包括客户端、sessiond、termai-agent、plugin-host、云端服务镜像、插件 SDK 与协议包。
- **第一方代码（First-party）**：以 `Apache-2.0 OR MIT` 发布、由本仓库维护的代码。
- **链接边界（Link Boundary）**：第三方单元 C 属于边界内，当且仅当满足以下任一：
  - **(B1) 同进程执行**：C 的代码在运行时与第一方代码共享地址空间（静态链接、动态链接、dlopen / 运行时加载均计入）。
  - **(B2) 静态并入**：C 的代码被并入发布产物（Rust 默认静态链接、`.a`、`-static`、vendored C/C++/asm、`-sys` crate 经 `cc` 编译的原生源码、内联或复制粘贴的代码）。
  - **(B3) 派生并入**：C 的派生代码（由 C 复制、修改，或由其骨架 / 模板逐字生成）被编译进发布产物。
- **边界外（Out-of-Boundary）**：构建期独立进程工具、运行时 subprocess、插件（WASM / trusted subprocess）、OS 系统库。**边界外不等于无义务**：仍受 D2 的 BT / T / AE 条件、SPDX 登记与归因约束。

**三条不可协商判定原则**
- **P1 同进程即入界**：凡与第一方代码共享地址空间者一律按并入处理，**链接方式（静态 / 动态 / dlopen）不改变判定**。
- **P2 派生即继承**：复制、修改、骨架生成所得的代码继承来源 SPDX；GPL / AGPL / SSPL 派生 = 禁止。
- **P3 未知即拒绝**：空白、`NOASSERTION`、`LicenseRef-*`、无许可证文本、来源不明 = 拒绝；不得以「找不到许可证」或「作者说是 MIT」放行。

**边界清单产物**：`cargo xtask license --boundary` 输出机器可读的 `license-boundary.json`，逐单元给出 `{name, version, spdx, scope, edge_type, in_boundary, verdict}`，作为 G5 门禁与法务审计的唯一口径。

### D2 判定表（允许 / 需审批 / 禁止，CI 可直接实现）

判定三档：**允许 A**（自动放行）／**需审批 R**（法务 + maintainer 双签，进 `boundary-exceptions.toml`，有效期 ≤180 天）／**禁止 D**（任何例外均不得覆盖）。

| ID | 场景 | 入界 | 判定 | 必满足条件 | CI 判定规则 |
| --- | --- | --- | --- | --- | --- |
| LB-01 | 运行时依赖（Rust `[dependencies]` 非 proc-macro；npm `dependencies`） | 是 | 按 D4：白名单 = A；弱 copyleft = 按 D3；黑名单 = D | — | cargo-deny licenses + `xtask license --boundary` |
| LB-02 | dev / 测试依赖（`[dev-dependencies]`、devDependencies、benches / examples / 测试夹具、纯文档工具） | 否 | **A（含 GPL / AGPL）** | 不得进入发布产物的运行时依赖闭包 | `cargo tree -e normal --no-dev` 反向依赖检查；`xtask license --scope=release` |
| LB-03 | build-dependency / build.rs / 外部构建 CLI（git、make、cargo 插件） | 否 | GPL / AGPL / SSPL：**A**，须满足 BT1–BT5；任一不满足 → **R** | BT1–BT5 | `xtask license --build-tools` + `third-party/build-tools.toml` |
| LB-04 | proc-macro（`proc-macro = true`） | 否 | 同 LB-03（rustc 独立进程加载、输出令牌并入我方源码） | BT1–BT5，且 **BT3 强制** | 生成物 SPDX 头扫描 + build-tools.toml |
| LB-05 | 静态链接（Rust 默认、`.a`、`-static`、`cc` / `-sys` 编译 C/C++、vendored 源码） | 是 | 同 LB-01 | — | vendor 清单 + build.rs `cc::Build` 检测 + cargo-deny |
| LB-06 | 进程内动态链接（`-l`、`.so` / `.dll` / `.dylib`） | 是 | 同 LB-05；LGPL 见 D3-W5 | LGPL 须未修改且可替换 | 二进制依赖扫描（ldd / otool / dumpbin）+ deny.toml |
| LB-07 | dlopen / 运行时加载（libloading、LoadLibrary、dlopen） | 是 | 同 LB-05；**禁止以 dlopen 规避静态链接判定** | — | 源码静态扫描 + 运行时依赖清单 |
| LB-08 | 内联 / 复制源码（含单函数、常量表、生成骨架） | 是 | 继承来源 SPDX；GPL / AGPL / SSPL = **D** | — | reuse / scancode 许可证头 + 结构性相似度检查 |
| LB-09 | 代码生成器输出（纯变换，或生成器含输出例外：GCC RLE、LLVM exception、Bison） | 否 | **A**，输出视为第一方 | BT3：生成文件带 `SPDX-License-Identifier: Apache-2.0 OR MIT` 与 generated-by 行 | 生成物头扫描 |
| LB-10 | 代码生成器输出（复制生成器骨架 / 无输出例外 / 注入生成器许可证头） | 是 | 继承生成器 SPDX；GPL / AGPL / SSPL = **D** | — | 生成物出现外部 SPDX 头或骨架指纹 → 阻断 |
| LB-11 | 编译器 / 链接器 / 构建加速器（rustc、LLVM、GCC 插件、sccache、ld） | 否 | **A（构建工具）** | BT2、BT4、BT5；产物内不得保留其运行时 | build-tools.toml |
| LB-12 | 运行时 subprocess 调用 **GPL** 可执行（git / ffmpeg / ripgrep） | 否 | **A**（须 T1–T6） | T1–T6 | runtime-tools.toml + 静态检查「无链接、无 dlopen」 |
| LB-13 | 运行时 subprocess 调用 **AGPL** 可执行 | 否 | 客户端本地：**A**（T1–T6）；**我方云端对外提供的网络功能**：**R** | T1–T6，且 `scope=server` 有审批票据 | runtime-tools.toml 的 `scope` 字段 |
| LB-14 | subprocess 调用 SSPL / BUSL / Commons-Clause / Elastic 可执行 | 否 | 用户自带、本地调用：**A**（T1–T6）；**我方分发或用于向我方对外服务**：**R**（默认禁止） | T1–T6 | runtime-tools.toml + 分发清单 |
| LB-15 | WASM 沙箱插件经宿主 spawn 调用 GPL 工具 | 否 | **A**（T1–T6）；插件包**捆绑** GPL 二进制：**A 但需市场审核**（source offer + `bundled-copyleft` 标记） | T1–T6 + manifest 字段 | 插件 manifest + 市场审核流水线 |
| LB-16 | trusted subprocess 插件（git / docker / kubectl / 云 CLI）调用 GPL 工具 | 否 | **A**（T1–T6） | 同 LB-15 | 同上 |
| LB-17 | OS 系统库（glibc / musl / 平台框架） | 否 | **A**（GPL 系统库例外 / LGPL 动态链接） | 未修改；不随我方产物再分发（AppImage 自带运行库属例外，须登记 SPDX 与 source offer） | 系统库白名单 |
| LB-18 | 反规避：shim 包装 / vendor 改名 / git submodule / 二进制 blob | 是 | 按**真实** SPDX 判定，命名与目录不改变结论 | 无源码 blob → **R** + SPDX 证明 | AE1–AE5 检查（见 D10） |

**构建期隔离条件 BT（LB-03 / LB-04 / LB-09 / LB-11 必须全部满足）**
- **BT1**：仅出现在构建期依赖集合（`[build-dependencies]` / dev-dependencies / 外部 CLI），**不进入发布产物的运行时依赖闭包**。
- **BT2**：以**独立进程**调用（cargo 构建脚本、`Command`），不得链接、不得 dlopen。
- **BT3**：其输出**不得逐字复制生成器自身的源码、骨架或许可证头**；生成物只能由我方输入机械变换得到。
- **BT4**：移除该工具后仍可从源码构建出等价产物（无隐藏运行时依赖）。
- **BT5**：登记于 `third-party/build-tools.toml`（名称、SPDX、版本、用途、是否声明输出例外）。

**运行时 subprocess 条件 T（LB-12…LB-16 必须全部满足）**
- **T1 进程隔离**：仅通过 argv + stdin/stdout/stderr + 退出码 + 用户可见文件通信；不得链接、不得 dlopen、不得共享内存或共享地址空间。
- **T2 不再分发**：不随我方产物捆绑被调用可执行文件；用户 / OS 提供，或作为**独立**下载产物并附其许可证与 source offer。
- **T3 可替换**：存在文档化的接口边界，可替换为功能等价的非 GPL 实现；缺失该工具时核心功能**降级而非崩溃**。
- **T4 不修改**：不修改被调用工具的源码；确需 patch 时，patch 按原许可公开且用户可替换。
- **T5 登记**：登记于 `third-party/runtime-tools.toml`（名称、SPDX、用途、`scope=client|server`、是否必需）。
- **T6 归因**：在 THIRD-PARTY-NOTICES 列出；不得暗示背书。

> 关键法律论点：subprocess 调用是**独立程序之间的通行做法**，不构成衍生作品或链接（区别于静态/动态并入）。T1–T4 的作用是把「独立程序」这一主张固定为可验证事实，避免被认定为「紧密结合的单一程序」。**T5–T6** 保证可审计与可归因。

### D3 弱 copyleft 准入（MPL-2.0 / EPL-2.0 / CDDL / LGPL）

| 许可 | 边界内准入 | 允许位置 | 条件 | 法务保留点 |
| --- | --- | --- | --- | --- |
| **MPL-2.0** | **允许** | 任意位置（含静态链接） | W1 未修改文件仅保留许可证头与 NOTICE；W2 修改过的文件保持 MPL-2.0 并公开该文件源码（file-level）；W3 不得把 MPL 代码内联进我方文件；W4 登记来源与版本 | MPL §3.2「Covered Software」在生成代码 / 宏展开场景的边界未在判例中检验；每 12 个月随 spec 07 §3.10 复核 |
| **LGPL-2.1 / LGPL-3.0** | **允许（仅动态链接）** | 仅进程内**动态**链接的独立共享库，且库未修改、用户可替换 | W5 禁止静态链接与 vendored；W6 禁止修改库源码；W7 **Rust / npm 包默认禁止**（默认静态链接），除非证明其仅绑定一个动态系统库；W8 随附替换 / 再链接说明（LGPL-3.0 反规避条款 §3） | LGPL-3.0 的「用户可替换」需实证（relink 路径可测）；W7 会排除绝大多数 LGPL crate，这是有意为之的保守选择 |
| **EPL-2.0** | **默认禁止；仅逐案审批（R）** | 出边界（dev / build 工具、独立进程）；或未修改的进程内动态系统库，逐案 | W9 法务逐案审批 + 登记 + ≤180 天有效期 | EPL §3.2 的源代码义务与「Commercial Distribution」定义、模块边界缺乏判例支撑 |
| **CDDL-1.0 / 1.1** | **默认禁止；仅逐案审批（R）** | 同 EPL-2.0 | W10 同 W9；不得与我方 Apache-2.0 分支组合 | 专利与终止条款组合风险需逐案确认；CDDL-1.0 与 GPL 不兼容，会阻塞下游以 GPL 组合使用 |

### D4 SPDX 白名单 / 黑名单与表达式求值

**允许白名单（Tier A，边界内免审批；规范清单在 `third-party/policy.toml`，本节为可读镜像）**
- 宽松许可：`Apache-2.0`、`MIT`、`MIT-0`、`BSD-2-Clause`、`BSD-3-Clause`、`BSD-3-Clause-Clear`、`0BSD`、`ISC`、`Zlib`、`libpng-2.0`、`Unicode-3.0`、`Unicode-DFS-2016`、`CC0-1.0`、`Unlicense`、`BlueOak-1.0.0`、`Python-2.0`、`PSF-2.0`
- 带例外：`Apache-2.0 WITH LLVM-exception`
- 非代码资产（仅当 `assets.toml` 声明为字体 / 图标 / 文档，不入代码依赖）：`OFL-1.1`（字体）、`CC-BY-4.0`（文档与图片）
- **名称澄清（易错点）**：SPDX 的 `BSL-1.0` 是 **Boost Software License 1.0（宽松，允许）**，**不是** Business Source License。Business Source License 的 SPDX 标识是 **`BUSL-1.1`（禁止）**。CI 必须按精确 SPDX ID 匹配，禁止子串匹配。

**禁止黑名单（Tier D，边界内一律禁止，不可例外）**
- 强 copyleft：`GPL-2.0-only`、`GPL-2.0-or-later`、`GPL-3.0-only`、`GPL-3.0-or-later`、`AGPL-3.0-only`、`AGPL-3.0-or-later`
- 服务端限制 / field-of-use：`SSPL-1.0`、`BUSL-1.1`、`Commons-Clause`、`Elastic-2.0`、`PolyForm-*`、`Prosperity-3.0.0`、`Hippocratic-2.1`、`JSON`
- 其他 copyleft / 非自由：`GFDL-*`、`CPAL-*`、`OSL-*`、`EUPL-*`、`MPL-1.0/1.1`、`QPL-1.0`、`Sleepycat`、`Watcom-1.0`、`Sybase`、`Aladdin`、`JRL`、`JPL`、`xinetd`
- 内容许可误用：`CC-BY-NC-*`、`CC-BY-ND-*`、`CC-BY-SA-*`（禁止作为代码或其依赖；SA 用于非代码资产需 **R**）
- **未知即拒绝**：空、`NOASSERTION`、`LicenseRef-*`、无 LICENSE 文本、许可证与元数据不一致 → **D**
- 说明：`JSON` 许可证含「不得用于 Evil」式使用领域限制，非 OSI 认可、非自由，禁止。

**需审批（Tier C）**：`EPL-2.0`、`CDDL-1.0/1.1`、`OpenSSL`、`Apache-1.0/1.1`、`BSD-4-Clause`、`MPL-1.1`、`NCSA`、`Zlib-acknowledgement`、其他 OSI 认可但含附加条款的许可。审批规则见 D10。

**SPDX 表达式求值（CI 必须逐字实现）**
1. `-only` / 无后缀：按该版本判定。
2. `-or-later`：若基础许可在黑名单 → 仍为黑名单（GPL-2.0-or-later 不得当作「可选 MIT」）。
3. `A OR B`：至少一支允许即**允许**，但 CI 必须记录 **elected 分支**，并断言构建**未行使**被禁分支（例如无对 GPL 选项的链接）。若可选分支含弱 copyleft，elected 分支须落入白名单或 D3 的允许档。
4. `A AND B`：**全部**分支允许才允许。
5. `A WITH E`：A 与 E 都必须在白名单（例外白名单：LLVM-exception、GCC-exception-*、Bison-exception-2.2、Classpath-exception-2.0、Font-exception-2.0）。
6. 括号与运算符优先级按 SPDX 规范；解析失败 → 拒绝（不得回退为「按最宽松分支猜测」）。

### D5 签名密钥托管与门限（OQ-28 / spec 07 OQ-E6）

**信任根分两层，任何单人都不能独立签名。**
1. **根密钥（TUF root / 委派）**：**离线冷存储**，FIPS 140-2 Level 3 及以上 HSM 或等效离线设备 + 至少 2 把 YubiKey 5 FIPS；**门限 2-of-3**（3 名 custodian，异地分持 Shamir 恢复份额）；**仅**用于签发 / 轮换发布子键与 TUF root；轮换 ≤24 个月；每次仪式**双人在场 + 录像 + 签名记录**，季度做恢复演练。
2. **发布签名子键（按通道 / 产品）**：**云 HSM / KMS 托管，密钥不可导出**（Azure Managed HSM 或 AWS CloudHSM）；**门限 2-of-3**（3 名 release manager 各持一次人工审批，branch protection 强制）；CI 仅持**短期 OIDC 凭据**，且**只对哈希签名**（对齐 spec 07 §3.2.3 与 E10，构建 job 与签名 job 物理隔离）；轮换 ≤12 个月。
3. **插件 / 市场签名子键**：独立子键；因分发面最广，轮换 **≤6 个月**；可用但不可导出。
4. **轮换与过渡**：新旧键**重叠验证期 ≥30 天**；TUF root 轮换必须**新旧根双签**（防止静默替换与降级）。
5. **泄露应急时间线（不可协商）**：
   - **T+0**：冻结全部签名 job，吊销 CI 临时凭据，封存现场；
   - **≤1h**：发布吊销清单 + 新 `timestamp` / `snapshot` 元数据，客户端 CRL 生效（对齐 R9「吊销 p95 ≤1h」）；
   - **≤24h**：用门限恢复新根 / 子键并发布紧急签名产物；
   - **≤72h**：公开复盘 + 透明度日志（Rekor 思路）补录；受影响版本公告与自动更新强制检查。
6. **硬禁止**：私钥进入 CI secret、仓库、日志或构建产物（gitleaks + 密钥材料扫描为门禁）；单人可签；无审计的签名行为。签名服务中断**不判 nightly 失败**（OQ-E3 EXTERNAL 口径），但**阻断 stable 发布**。

### D6 SLSA / provenance 等级目标与达成路径（spec 07 OQ-E4）

**目标等级（唯一结论）**
- **P0–P2：SLSA Build L2 为强制地板** —— 任何发布产物缺少可验证 provenance 即阻断发版。
- **P3 退出前：达到 SLSA Build L3**（硬化构建平台 + 由构建服务签发的不可伪造 provenance + 发布路径中无用户可控构建步骤）。
- **明确不以 L4 为目标**：L4 要求封闭（hermetic）与双独立方可复现构建，对当前 12 人规模的项目成本与收益严重失衡；保留「≥1 个目标 bit-for-bit 可复现」为 L4 前置的 stretch 目标，达标余量记为工程债而非承诺。

**达成路径（按 Phase）**
1. **P0**：GitHub Actions OIDC + `slsa-github-generator` 可复用工作流；Sigstore keyless（Fulcio 签发 + Rekor 记录）→ L2；每产物生成哈希清单并与 SBOM 绑定。
2. **P1**：构建硬化 —— 临时 runner、出网拒绝清单、`--locked`、工具链与依赖锁定（`rust-toolchain.toml` + `.mise.toml`）、干净检出、禁缓存投毒；构建仅允许来自 `main` / tag。
3. **P2**：构建域与签名域**信任隔离**（E10）；增加**独立验证 job**，用 `slsa-verifier` 在发布前校验 provenance，失败即 fail closed。
4. **P3**：隔离构建服务（无用户可控构建步骤、按构建定义签发 provenance）→ L3；接入两方复核。

**attestation 存储与校验**
- **格式**：in-toto attestation（`https://slsa.dev/provenance/v1`），DSSE 信封；容器镜像用 OCI referrers / `cosign attach attestation`，文件产物用 `.intoto.jsonl` 随发布件。
- **透明性**：每个 attestation 在 Rekor（Sigstore）登记，Sigstore bundle 随产物归档。
- **校验**：CI 在**发布前**执行 `slsa-verifier verify-artifact --provenance ... --source-uri ... --builder-id ...`；消费者侧提供 `termai verify` 与文档化命令复验。发布 manifest 绑定 `{artifact hash, provenance hash, signature, sbom hash}` 四元组。
- **双信任根**：Sigstore keyless 之外，另有我方 HSM 手签 manifest 作为独立第二根；Sigstore 不可用时以 HSM 签名作为地板，但不得降级为「无 provenance」。
- **保留**：attestation / SBOM / bundle 保留**至少与产物同寿命 + 10 年**（合规审计）。

### D7 审计保留期默认与外锚（spec 05 OQ-S1）

- **默认保留期：本地 90 天**（append-only 段按龄整段删除）。
- **用户可控范围：7–3650 天**；企业策略可强制延长（如 ≥1 年）或缩短至下限。
- **保留下限（不可低于）**：红线记录 —— L2/L3 审批、出站拒绝、密钥使用、插件能力授予、吊销事件 —— **≥30 天**；审计本身**不可关闭**（DC-34）。
- **删除语义**：按龄删除整段并生成**删除收据**（记录 seq 区间 + 时间 + 原因）；全量本地擦除需显式二次确认并留收据；企业策略可禁止擦除。
- **外锚（Merkle checkpoint 外部锚定）**：
  1. **用户 / 企业自持（默认，尤其企业）**：周期性把 Merkle root 推送到**企业自有** SIEM、WORM 存储、S3 Object Lock 或 SFTP；**不经我方云**（AR-11）。
  2. **本地导出（个人默认可用）**：导出带签名的 Merkle checkpoint 链（NDJSON + root），用户自持。
  3. **第三方时间戳 / 公共透明度日志（opt-in）**：RFC 3161 TSA 或 Rekor，**仅提交 Merkle root 哈希**（L0 公开级，不含任何内容），因此不违反 AR-11。第三方须登记为子处理者（元数据面）。
- **锚定节奏**：每 24h 或每 1000 条记录，先到先锚；锚记录含 `{seq_range, root, prev_root, ts, signature}`。
- **外锚默认值**：消费级 **opt-in 关闭**；企业 MDM 策略**默认开启**。
- **诚实声明（不变）**：哈希链 + 外锚提供 **tamper-evidence，而非 tamper-proof**；不得对外宣称「任何人都无法篡改」（spec 05 §3.7）。

### D8 崩溃上报端点归属与保留期（spec 07 OQ-E7）

- **端点**：**自托管 Sentry 兼容**（Sentry Self-Hosted / GlitchTip），独立域名与命名空间，与控制面业务数据库物理隔离。
- **运营归属**：**Cloud Platform（角色 06 / 团队 T5 的管道）负责运营**；**Security（08）为数据保护责任人并拥有服务端脱敏规则集**；**Devex（T5）拥有 schema 与 CI 校验**。三方责任在 spec 05 §4.1 明确。
- **区域与自托管**：区域随租户驻留（OQ-15，租户级）；企业版可自托管或**完全关闭**。
- **保留期**：**原始事件默认 30 天**；**聚合 / 符号化 issue 元数据默认 90 天**；到期**硬删除，无冷归档**。企业策略只能下调，不能上调（上调需 DPIA 更新）。
- **脱敏责任（双层，均强制）**：客户端侧脱敏为**第一道且主责**（DC-33，失败即丢弃事件，fail closed）；服务端 `before_send` 清洗为**第二道**，规则集由 Security 拥有、CI 校验 schema。
- **内容禁止**：与 AR-12 一致 —— 无命令文本、路径、代码、域名、环境变量值、secret；仅 OS / 架构 / 版本 / 符号化崩溃栈 / 帧时直方图 / IPC 往返延迟。`session_id` 为可轮换假名。
- **访问控制**：默认仅见 issue；访问原始事件需 break-glass 双人审批并审计。
- **DSN**：按通道 / 项目独立，泄露可单独吊销。
- **子处理者**：登记于数据流清单 + DPA；不得用于训练。

### D9 遥测与审计默认值（AR-12 落地口径）

| 项 | 默认 | 是否独立开关 | 边界 |
| --- | --- | --- | --- |
| 遥测 | **关闭（opt-in）** | 是 | 内容零采集（schema 级字段数 = 0，CI 强制）；关闭时上传 = 0 字节；仅本地聚合计数 + 采样 |
| 崩溃上报 | **关闭** | **是（与遥测互不连带）** | 独立 DSN；关闭时请求 = 0；开启仍走双层脱敏 |
| 审计 | **开启，但仅本地** | 否（不可关闭，DC-34） | 不含内容（args_digest）；默认不出设备；仅用户 / 企业 SIEM 导出 |
| 出站清单 | **100% 可见** | 否（不可关闭） | 每次出站可分类、可审计、可在 UI 回看与撤销 opt-in（AR-12） |
| 首次运行 | **零弹窗、无同意墙** | — | 同意须非阻断、可就地拒绝、可随时撤回（AR-15、OQ-S5） |

> 口径澄清：**审计 ≠ 遥测**。审计是本地信任功能（DC-34），默认开启但永不出设备；遥测与崩溃上报是出站行为，默认关闭。不得以「审计默认开」推导遥测可默认开。

### D10 CI 落地（机器可读策略与门禁）

**规范文件（单一真相，人读文档引用之）**
- `third-party/policy.toml`：白名单 / 黑名单 / Tier C / 例外白名单 / SPDX 求值开关。
- `deny.toml`：cargo-deny 的 `[licenses]`（allow + exceptions）、`[bans]`、`[advisories]`、`[sources]`（仅 crates.io / 官方 registry；git 依赖须 pin commit + 审批）。
- `third-party/runtime-tools.toml` / `build-tools.toml` / `boundary-exceptions.toml` / `assets.toml`。
- `cargo xtask license --boundary` 输出 `license-boundary.json`；与 `sbom.cdx.json`、`provenance.intoto.jsonl` 一并归档。

**反规避检查 AE（必须实现）**
- **AE1**：包名 / 目录改名不改变判定（按来源与真实 SPDX 判定）。
- **AE2**：无源码二进制 blob（`.a` / `.so` / `.bin`）→ 需审批 + SPDX 证明。
- **AE3**：git submodule、`vendor/`、`third_party/` 内的 C/C++/asm 按 LB-05 静态链接判定。
- **AE4**：`links = "..."` + build.rs 中的 `cc` / `bindgen` 一律按静态链接判定。
- **AE5**：不得以「仅构建期」为名把 GPL 库编译进运行时产物（用 BT1 反向依赖检查证伪）。

**G5 门禁子项（缺一阻断合并 / 发版）**
- **G5-a**：边界内黑名单 SPDX 命中数 = **0**。
- **G5-b**：白名单外且无有效例外（未过期、双签）的单元数 = **0**。
- **G5-c**：分类器在**golden 语料**（覆盖 LB-01…LB-18 的合成 fixture）上判定正确率 = **100%**（防分类器回归）。
- **G5-d**：每个发布产物必须同时具备 SBOM + provenance attestation（D6），否则**阻断发版**。
- **G5-e**：反规避检查（AE1–AE5）命中且未登记者 = **0**。

**例外审批**：需 **法务 + maintainer 双签**，写入 `boundary-exceptions.toml`（含 owner、ticket、到期日 ≤180 天）；到期自动失效并重新评审。**GPL / AGPL / SSPL / BUSL / Commons-Clause / Elastic-2.0 不得进入任何例外**（AR-21 红线）。

## 理由

1. **与产品公理一致**：A1「终端是信任基础设施」要求供应链默认拒绝、可审计；A3 的 AI-native 前提是发布产物可信。可执行边界让「信任」成为 CI 事实而非声明。
2. **同时满足法务稳健与工程可行**：B1/B2/B3 用「同进程 / 并入 / 派生」这一被广泛接受的版权法切入点定义边界，避免了对「静态 vs 动态」的教条依赖；MPL-2.0 与动态 LGPL 的准入释放了生态，同时用文件级 / 可替换条件守住 copyleft 义务的可履行性。
3. **subprocess 的判定是产业与 FSF 立场的交集**：独立进程之间的 argv / stdio 通信属通行做法，不构成衍生作品；T1–T4 把「独立程序」从主张变成可验证事实，避免了「shim 包装」与「必需 GPL 组件」两种实际会击穿承诺的绕过。
4. **默认拒绝可自动化**：P3（未知即拒绝）+ fail-closed 解析，使 G5 不需要在 CI 里做法律判断，只需做确定性求值；疑难一律进人审，且人审也不能突破黑名单。
5. **供应链信任根的分层门限**：单点密钥是 R9 供应链投毒的唯一剩余高危路径（spec 05 SEC-RK-03 残余）。离线根 + 云 HSM 子键 + 2-of-3 + ≤1h 吊销，把「维护者密钥被盗」的爆炸半径压到一个通道 / 一个子键。
6. **SLSA L3 是可达的最优解**：L2 是地板（无 provenance 不发版），L3 在 P3 前达成；拒绝 L4 是承认小团队的现实约束，而不是用「尽量高」掩盖不做决定。
7. **保留期与默认值的选取可被验证**：审计 90 天 / 崩溃原始 30 天 / 遥测默认关，均给出可测量的门禁（G5-d、SEC-AC-07、日志与抓包断言）。
8. **与既有仲裁自洽**：不改变 AR-11（云边界）、AR-12（默认关闭）、AR-06（不可关闭）、§5 预算与 §8.1 门禁强度；本 ADR 只把 AR-21 的授权转化为可执行规则，并为 OQ-27 / OQ-28 提供唯一结论。

## 后果（正面 / 负面 / 需要接受的代价）

**正面**
- G5 从「GPL/AGPL 链接边界 = 0」的一句口号，变为可回归、可审计、可解释到具体依赖与版本的判定；新增依赖的许可风险在 PR 阶段拦截。
- 弱 copyleft 有了明确准入，避免「一律禁止」导致的生态排除与自研浪费。
- 供应链信任根（密钥、SLSA、审计外锚、崩溃端点）全部有负责人、周期与门禁。
- OQ-27 / OQ-28 可关闭；spec 05 / spec 07 的对应章节可写实。

**负面 / 必须接受的代价**
- 维护 `policy.toml` + 三个工具清单 + golden 语料，是持续的工程与法务负担。
- MPL-2.0 与 LGPL 的准入要求「不内联」「不静态链接」「可替换」，会限制部分依赖的引入方式，个别场景需要自研替代或走审批。
- SLSA L3 需要专用构建服务与独立验证 job，CI 成本上升；P0–P2 期间 provenance 与 SBOM 是发布硬门禁，可能延缓首次发布。
- 崩溃上报保留期偏保守（原始 30 天），跨 stable→beta→nightly 的长周期 triage 可能不够，需要运维额外采样或延长（若延长须更新 DPIA）。
- 第三方 RFC 3161 时间戳作为可选外锚会引入一个新的（元数据面）子处理者。

**需要接受的代价 / 明确不做**
- 不承诺 L4、不承诺「不可篡改审计」，不用「尽量高」这类不可验收措辞。
- 不为下游 GPL 组合需求让步（CDDL/GPL 组合仍禁止）。
- 不因成本原因在发布路径保留「无 provenance」的降级（可停发，不可无 provenance 发）。

## 反方记录与复议条件

- **反方（社区 / 极客，主张方案 C 全禁弱 copyleft）**：MPL / LGPL 的准入增加合规负担，且一旦引入就难以移除。**接受其成本，否决其结论**：文件级与动态链接的可履行性可被 CI 验证，全面禁止的生态代价高于收益。
  - **复议条件（可观测）**：12 个月内出现 ≥1 起因 MPL-2.0 或 LGPL 引发的法务事件 / 下架请求 / 下游采用阻断 → 收紧对应许可档位（需新 ADR）。
- **反方（工程效率）**：T3（可替换）与 T4（不修改）对 subprocess 工具提出额外工程要求，拖慢集成。
  - **复议条件**：若某必需工具无法满足 T3，走 **R 审批**并在 `runtime-tools.toml` 记录技术债与替代计划；不得直接放宽 T3 为「不要求可替换」。
- **反方（成本 / 平台）**：SLSA L3 需要隔离 builder 与双信任根，CI 成本与复杂度上升。
  - **复议条件**：若 L3 成本超出 spec 07 OQ-E10 的 CI 月度预算上限 → 可停在 L2 并**公开声明**，但**不得降为无 provenance**（地板不变）。
- **反方（隐私）**：第三方 RFC 3161 / Rekor 外锚引入新子处理者。
  - **复议条件**：若用户 / 法务反对该子处理者 → 仅保留「企业自持」与「本地导出」两种外锚，第三方时间戳默认永久关闭。
- **反方（运维）**：崩溃原始事件保留 30 天不足以覆盖跨通道 triage 周期。
  - **复议条件**：若 triage 数据（未解决 issue 的年龄分布）证明 30 天不足 → 可延长至 45 / 60 天，须同步更新 DPIA 与数据流清单。
- **不可放宽 / 不可复议项**：GPL / AGPL / SSPL 及 field-of-use 许可的边界内禁止；P3 未知即拒绝；签名门限与 ≤1h 吊销；审计不可关闭；遥测 / 崩溃默认关闭；AR-11 云边界。

## 关联决策（DC-xx）与实现位置

| 决策 / 条目 | 内容 | 实现位置 |
| --- | --- | --- |
| AR-21 第 3 条 | 链接边界 CI 可执行定义 + 弱 copyleft 准入 | 本 ADR D1–D4、`third-party/policy.toml`、`cargo xtask license --boundary` |
| HARNESS §8.1-5 | 依赖与许可门禁 G5 | CI job `G5`（G5-a…G5-e），消费 `license-boundary.json` |
| DC-34 | 审计 append-only + 本地哈希链 + SIEM 导出 | spec 05 §3.7；D7 保留期与外锚 |
| DC-36 | 模型视为供应链：权重哈希 + 通道签名 + 离线校验 | spec 05 §3.8；与 D5 子键共用信任根 |
| DC-38 | 插件宿主独立进程 + 签名 + 权限清单 | spec 05 §3.8；D2 的 LB-15/LB-16 |
| SEC-R-17…SEC-R-21 | 链接边界、SPDX、签名/溯源、审计保留、崩溃端点 | docs/spec/05-security-privacy-compliance.md |
| 签名流水线 | 隔离签名 + 仅签哈希 | docs/spec/07 §3.2.3；D5 |
| provenance / SBOM 流水线 | CycloneDX + in-toto + Rekor | docs/spec/07 §3.11；D6 |

- **取代关系**：不取代任何 ADR；**扩展** ADR-0013 第 3 条的授权范围，并使 docs/spec/07 §3.11「链接边界操作定义」的临时口径升级为本 ADR 的正式定义。
- **受影响文档**：`docs/spec/05-security-privacy-compliance.md`（本章程已同步，标注 (ADR-0015)）、`docs/spec/07-engineering-quality-and-release.md` §3.11（后续由 T5 同步）、`docs/adr/README.md` §6 索引（**由 Orchestrator 追加 ADR-0015 行**，本 ADR 作者无权限修改）。
- **CI 落点**：`deny.toml`、`third-party/*.toml`、`crates/termai-xtask/src/license.rs`、`.github/workflows/`（G5、签名隔离 job、provenance 验证 job）。
