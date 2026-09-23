# TermAI 架构决策记录（ADR）流程与索引

> 本目录是 HARNESS.md §2 仲裁决议（AR-01…AR-20）的**可追溯落地记录**。
> HARNESS 是唯一权威总纲；ADR 只做「为什么这样裁决、否决了什么、复议条件是什么、落在哪个模块」的展开，**不得与 AR / DC / §5 预算 / §8 门禁 / §10 Anti-features 冲突**。
> 冲突时一律以 HARNESS 为准；HARNESS §12 维护约定：新决议以 AR-21… 追加，修改已有 AR 必须新开 ADR 并在 AR 条目上标注「被 ADR-xxxx 取代」。

## 1. 何时需要 ADR

必须写 ADR 的情形（任一命中）：
1. 新增或变更 AR / DC 所约束的**跨模块边界**（进程拓扑、ABI、协议、存储真相源）。
2. 触及 HARNESS §0.4「不可协商清单」或 §10 Anti-features 的例外申请。
3. 引入新的**对外契约**（Local API、插件 WIT、云控制面接口、文件格式版本）。
4. 变更 HARNESS §5 任一**发布门禁**数值或新增门禁项（走 AR-19 口径：门禁 / 目标两列）。
5. 依赖方向变更（HARNESS §4.3），可能形成环或把第三方引入**核心链接边界**（AR-21；链接边界可执行定义见 ADR-0015）。
6. 采纳或替换第三方关键依赖（如 AR-18 的 vte、WebView 运行时、模型供应商 SDK）。

**不需要 ADR**：模块内部实现、纯重构、不影响契约的 bugfix、文档措辞。

## 2. ADR 模板（本目录所有文件遵循）

必须包含且顺序固定的一级/二级标题：
1. 头部元数据：状态 / 日期 / 决策者 / 关联（AR、DC、HARNESS 章节）。
2. 背景与问题：冲突点、量化约束、失败代价。
3. 可选方案：**至少 2 个**，含被否决项；被否决项必须写清否决理由。
4. 决策：一句话可执行结论 + 边界。
5. 理由：为什么它同时满足产品公理 A1/A2/A3 与预算。
6. 后果：正面 / 负面 / 需要接受的代价（负面不得省略）。
7. 反方记录与复议条件：保留被否决方的论点与**可观测的复议触发条件**。
8. 关联决策（DC-xx）与实现位置（crate / 模块 / 路径）。

## 3. 状态机

```
proposed ──accept──▶ accepted ──supersede──▶ superseded
    │                    │
    │                    └──deprecate──▶ deprecated
    └──reject──▶ rejected（保留文件，头部标注 rejected 与理由）
```

| 状态 | 含义 | 可实施 | 允许的下一步 |
| --- | --- | --- | --- |
| proposed | 已提交、待评审 | 否 | accepted / rejected |
| accepted | 生效，是实现依据 | 是 | superseded / deprecated |
| superseded | 被新 ADR 取代，指向新编号 | 否（新 ADR 生效） | 终态 |
| deprecated | 仍有效但已不推荐，过渡期 | 仅修复性改动 | superseded |
| rejected | 明确否决，保留反方论点 | 否 | 终态 |

**不可变原则**：accepted 之后正文不改写语义；更正只做「勘误（typo/链接）」并追加日期；语义变更必须新开 ADR 并把旧条目标为 superseded。

## 4. 变更与复议规则

1. **复议触发**：只有当 ADR 中记载的**可观测复议条件**被实测数据命中时，才允许重开讨论；主观「想改回去」不构成理由。
2. **证据要求**：复议提案必须附 CI 指标 / 基准 / 门禁报告或安全红队结论，指向 HARNESS §5 或 §8 的具体条目。
3. **权限**：涉及 AR / 不可协商清单的复议 → TSC 2/3 多数 + 90 天公示（HARNESS 头部效力条款）；其余 ADR → 2 名 maintainer 批准 + 7 天公示（对齐 DC-40）。
4. **同步义务**：ADR 生效后 24h 内同步对应 docs/spec 与可自动化的 CI 检查（HARNESS §12）。
5. **不可逆向**：任何复议不得放宽 AR-06（确认不可关闭）、AR-11（云端边界）、AR-21（GPL/AGPL/SSPL 禁入核心链接边界）与 secret 红线（§6.1）。

## 5. 与 HARNESS 的关系

| 维度 | HARNESS.md | docs/adr/ |
| --- | --- | --- |
| 效力 | 唯一权威，角色文档与 ADR 冲突均以其为准 | 从属，是 AR 的推理与落地展开 |
| 粒度 | 决议级（20 条 AR） | 决策级（一决策一文件，含被否决方案） |
| 变更 | 只增不改语义；新 AR-21… | 新 ADR + 旧 ADR superseded |
| 证据 | §8 门禁与 §5 预算 | 复议条件必须绑定到 §5/§8 条目 |
| 证据来源 | 汇总 10 份角色评审 | 引用角色原文保留的反方论点，不改写 |

## 6. 索引

| 编号 | 标题 | 状态 | 依据 AR | 关键约束（一句话） |
| --- | --- | --- | --- | --- |
| ADR-0001 | 混合渲染架构：Native Grid + Web Shell | Accepted | AR-01、AR-02、AR-14 | 字符网格与关键动画绝不走 WebView；WebView 为可选依赖，缺失降级纯原生面板 |
| ADR-0002 | AI 运行时与内核的边界三分法 | Accepted | AR-03、AR-17 | 产品层一等组件 / 进程层独立进程 / 契约层只承诺结构化 Log + 能力协议 |
| ADR-0003 | 结构化上下文与 Session Log 存储模型 | Accepted | AR-04、DC-23 | append-only Session Log 为唯一真相，SQLite 仅可重建派生索引，热路径禁 JSON |
| ADR-0004 | 单 Agent 主循环架构（否决多 Agent 编排） | Accepted | AR-05 | 默认单主循环 + 受控子 Agent 委派；编排框架默认关闭 |
| ADR-0005 | 风险分级、审批与撤销模型 | Accepted | AR-06 | L0–L3 + U 分级；L2/L3 确认不可配置关闭；撤销只承诺工作区文件与 git |
| ADR-0006 | 插件 ABI、运行时与 PTY 边界 | Accepted | AR-07、AR-08 | 唯一 ABI = WIT + WASM Component；宿主独立进程；插件永不写 PTY/输出流 |
| ADR-0007 | 配置体系、Local API 与脚本化契约 | Accepted | AR-09、DC-24 | TOML + drop-in + 层级合并，不内嵌配置语言；Local API 只维护一套 JSON-RPC 契约 |
| ADR-0008 | 许可、商业模式与云边界 | Superseded by ADR-0013 | AR-10（被 ADR-0013 取代）、AR-11、AR-12 | 历史记录：原判「核心 Apache-2.0 + 云服务闭源 + 拒绝 AGPL/BSL」，该许可与商业形态部分已被 ADR-0013 取代；**其云边界结论仍有效**（PTY/文件/输出/prompt/密钥永不出设备） |
| ADR-0009 | 多路复用与持久会话模型 | Accepted | AR-13 | sessiond 为会话唯一真源；tmux 兼容不替代；屏幕可恢复、进程不可恢复 |
| ADR-0010 | 目标用户与市场范围 | Accepted | AR-15 | Primary = SRE/平台 + 高级工程师；不做教学模式与欢迎向导 |
| ADR-0011 | AI 交互范式与输出视觉语法 | Accepted | AR-16、AR-17 | 入口顺序固定；输出必须走结构化 schema；诊断优先于命令生成 |
| ADR-0012 | VT 引擎策略、性能预算口径与 a11y 承诺边界 | Accepted | AR-18、AR-19、AR-20 | vte 置于 trait 边界后 + clean-room 评估；门禁/目标两列；a11y 只承诺 UI 层 |
| ADR-0013 | 许可与开源范围终裁（全栈开源，Apache-2.0 OR MIT） | Accepted | AR-21（取代 AR-10 的许可与商业形态部分） | 客户端/服务端/SDK/协议/工具链统一 Apache-2.0 OR MIT；**无闭源云端模块**；收入 = 托管/支持/合规与企业服务；云边界与遥测默认不变；取代 ADR-0008 |
| ADR-0014 | 平台支持矩阵与参考机 | Accepted | AR-19、DC-16/DC-17、§8 | 三档自托管参考机（RM-A 门禁 / RM-B 支持地板 / RM-C 高刷 4K），云 runner 一律 NON-GATING；v1 门禁架构 = Win x64 / Linux x64 / macOS arm64（**Windows arm64 非 v1、P2 进入**）；4 shell × 3 字体后端 12 组合全 v1 必修；GPU 四级降级（非 T0 后端门禁判失败）；nightly 分 FAIL-REPO（计入）/ EXTERNAL（双人签署、不计入）；CI 月度硬上限 $3,000 |
| ADR-0015 | 链接边界、许可白名单与供应链 | Accepted | AR-21、DC-36/DC-37、§6.1 | 边界三原则（同进程即入界 / 派生即继承 / 未知即拒绝）+ 判定表 LB-01…LB-18（A/R/D 三档）；SPDX 白/黑名单与 OR/AND/WITH 求值；**MPL-2.0 允许（禁内联）、LGPL 仅未修改动态链接、EPL/CDDL 逐案审批**；subprocess 调 GPL 允许、调 AGPL 视方向审批；签名离线根 2-of-3 + 子键 2-of-3、仅签哈希；**P0–P2 SLSA Build L2 地板、P3 达 L3、不追 L4**；审计 90 天、外锚企业自持；崩溃端点自托管（原始 30 天 / 聚合 90 天） |
| ADR-0016 | 产品视觉语言 v2（Netcatty 取向）与信息密度规则 | Accepted | AR-22（取代 DC-10） | macOS 风格浮动圆角窗 + azure 蓝 + 卡片化 + 浅色一等主题；间距/圆角/图标/emoji 与四级按钮为硬约束 |
| ADR-0017 | 终端为主体、单一侧栏、设置独立窗口、新建终端即标签选择器 | Accepted | AR-23（修订 AR-16、AR-22 第 7 条） | 删除视图切换；rail 固定在左；设置独立窗口；软换行关 = 视觉裁剪；默认主题跟随系统 |
| ADR-0018 | 内核对外契约变更追认（PtyBackend 九元面 / IPC CBOR+POD / crc32c 强制） | Accepted | AR-28 第 1 条、AR-25、DC-22、DC-40 | 九元 PtyBackend；POD 4KiB 热路径档 + CBOR 演进档；IPC 与共享内存同级 crc32c |
| ADR-0019 | 内核 crate 布局与依赖准入 | Accepted | AGENTS 第三/五节、DC-21、AR-21、ADR-0015 | core/ipc/vt/pty/session 单向无环，apps 不被依赖；准入 vte/unicode-width/blake3/sha2/libc/windows-sys；portable-pty 拒绝、russh 未准入 |
| ADR-0020 | 实现期线格式与契约 errata（SD-01 至 SD-05） | Accepted | AR-04、AR-25 第 2 条、AR-28 第 1 条、DC-22/DC-23/DC-40 | 线格式以显式小端为唯一权威（禁止 repr(C) 直转）；Hello 增 required；SessionId = u128 ULID；叶子契约用 &mut dyn 且不加 Send |
| ADR-0021 | CI 构建产物输出与 GitHub Action 准入 | Accepted | AGENTS 第 4/5 节、HARNESS 第 8.1/6.3 节、AR-11、AR-12、AR-21、ADR-0014、ADR-0015 | 准入 actions/upload-artifact 作为唯一新增 action；产物内容白名单（仅二进制 + SHA256SUMS）；保留 14 天；由 K8 机器校验「凡构建必出产物」 |
| ADR-0022 | 视觉回归基线的权威来源与生成机制 | Accepted | AR-11、AR-12、DC-14 | 像素基线是环境指纹，必须由与验证同一 runner 镜像/浏览器生成；新增仅 workflow_dispatch 的 design-baseline 作业，产物范围 = prototype/baseline 的 PNG + 溯源清单；机器人不写 main，基线经人工 PR 入库 |
| ADR-0023 | P0 契约 errata 与字段集冻结 | Accepted | AR-04、AR-13、AR-25、AR-26、AR-28 第 1 条、AR-29、AR-31 | attach 族新增 0x05xx 段（AttachRequest/AttachAck/TailReplay/DetachNotice）；字符串态上限冻结为 OSC·SOS·PM 1 MiB / DCS·APC 16 MiB；GridSnapshot/GridDelta 字段集正式冻结并含 clusters: ClusterTable（golden/digest 必须覆盖） |
| ADR-0024 | 渲染管线第一刀：termai-render 的依赖位置与零新增依赖切片 | Accepted | AR-01、AR-03、AR-19、AR-23 §6、AR-24、DC-17、DC-21 | 新增 crates/termai-render，本切片仅依赖 termai-core（镜像 + rev 自愈 + damage 帧输入，零像素）；软换行/裁剪归本 crate 且必须零 GridDelta；第三方依赖本次**零准入**（wgpu/winit/rustybuzz/swash 待后续 ADR 附 cargo-deny） |
| ADR-0025 | GridSnapshot / RowPayload 的逐行 LineFlags 与 golden/digest 兼容 | Accepted | AR-04、AR-19、AR-23 §6、DC-23、DC-40、ADR-0023 D3、ADR-0024 D2 | 增 `row_flags` / `RowPayload.flags` 与 `LINE_WRAPPED`；canonical_bytes 纳入逐行 flags，`GRID_DTO_MINOR` 1→2（解 SD-13，是 VRM/RP-08 的前置）；golden 保持 `TERMAI-GRID 1` 可解析，缺字段视为全 0；零新增依赖 |
| ADR-0026 | TAIL_REPLAY 的传输形态、floor 语义与错误码（WS-05b errata） | Accepted | AR-04、AR-13、AR-26、AR-30、DC-22、DC-23、ADR-0023 D1 | `0x0502` 扩充为**双向**（C→S 请求 / S→C 重放）；floor **排他**且**无 floor 即拒绝**（不回退 watermark）；窗口不可满足（BelowWindow/AheadOfHead/TailUnreadable）一律**拒绝 + DROP_NOTICE**，绝不给短重放；复用 `AttachStateInvalid` 不新增码；事件子集只投影 5 类，新增 tag 须先出 ADR；线上 `confidence` 保持 f32（单位映射见 SD-21） |
| ADR-0027 | 渲染/字体的第三方依赖准入（已采证；公告按 ADR-0031 时间盒例外） | **Accepted** | AR-01、AR-03、AR-14、AR-21、AR-24、DC-17、DC-21 | wgpu / winit / rustybuzz / swash / fontdb 及传递依赖的**候选清单 + 判定档位 + 验收程序**；**SPDX 列留空**（本环境无法取得许可证元数据）；表填满前任何 crate **不得进入 workspace 依赖**（ADR-0015 P3「未知即拒绝」）；同时登记 `termai-gpu` 新 crate 与 `termai-render → vt/gpu` 两条边 |
| ADR-0028 | §5 测量与门禁的实现位置：以 `tools/bench` 为准 | Accepted | AR-19、AR-27、DC-23、ADR-0014 | ADR-0014 声明的 `crates/termai-xtask` 不存在；实际存在、且已被 B1–B8 逐字校验并已自证能失败（`bench:selftest`）的是 Node `tools/bench`。把实现与校验分属两处会立刻产生两套「有效测量」的定义。本 ADR 取代 ADR-0014 的两处实现位置引用。 |
| ADR-0029 | P0 判定域、能力声明与 §8.2 测量归属 | Accepted | AR-18、AR-19、AR-20、AR-24.3、AR-25、AR-26.4、AR-30、AR-31 | esctest（kernel/01 **V-02**）与 xterm 语料（**V-04**）是**两个**套件：前者 `R_strict=1, X=0`、后者 ≥99%+差异——**D-1 的前提错误已更正，读法 B 被否决**（它构成 §8 放宽、需 TSC）；OSC 4/10/11/12 明文声明 v1 不实现 + `S_cap` 静态能力前置；DA/DA2/DECID 必须实现且与钉定 oracle 逐字节一致（声称集 ⊆ 实现集）；§8.2 可靠性测量入 kernel/06 §3.10 + 独立 `RELIABILITY_MAPPING`；`B7` 只约束**机器绑定**的门禁数字（H13 澄清） |
| ADR-0031 | 渲染字体栈的 unmaintained 阻塞：rustybuzz / ttf-parser（时间盒例外） | **Accepted** | AR-01、AR-03、AR-18、AR-21、DC-17、DC-21 | ADR-0027 D3 采证发现 `advisories` 失败：`rustybuzz` RUSTSEC-2026-0206、`ttf-parser` RUSTSEC-2026-0192，均 unmaintained 且无安全升级，而 DC-17/K-05 指定 `rustybuzz` 为唯一 shaping 引擎。给出「时间盒例外（需到期 + 替换触发 + SBOM 标注）」与「立刻替换（改 DC-17/K-05）」两条路及其代价；**因属安全默认值放宽，状态 Proposed，决定权在 owner/TSC** |
| ADR-0030 | D-3 的可实施形态：VT 级别声明、DA1/DA2 应答与 DECID 的 8-bit C1 边界 | Accepted | AR-18、AR-20、AR-25、AR-31；kernel/01 V-02 / OQ-VT-14 | 读钉定 esctest2 源码发现 ADR-0029 D-3 的「逐字节一致」不可实施：DA 断言是**集合包含 + 范围**，级别 5 的 expected 要求声称 selective erase/locator/color/rectangular editing 等本实现没有的能力（违反 AR-20）。**规则化级别**（claimed = DA1 expected 全部已实现的最高级别，当前 = **1**），esctest 必须显式 `--max-vt-level` 且报告必须并列「级别 + eligible 分母」；DA1=`CSI ? 1;2 c`、DA2=`CSI > 0;314;0 c`（Pv 自报）、DECID 依 OQ-VT-14 仅非 UTF-8 模式识别。 |

## 7. 命名与文件约定

- 文件名：`ADR-<四位编号>-<kebab-slug>.md`（编号单调递增，永不复用）。
- 头部「关联」必须同时给出 AR 编号与 HARNESS 章节，便于 CI 校验追溯完整性。
- 本目录文件为**决策证据**：被取代的 ADR 保留原文，仅在头部状态改为 superseded 并追加指向新 ADR 的链接。
