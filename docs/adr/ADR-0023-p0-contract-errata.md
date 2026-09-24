# ADR-0023｜P0 契约 errata 与字段集冻结（attach 族 msg_type / 字符串态上限 / GridSnapshot clusters）

| 项 | 内容 |
| --- | --- |
| **状态** | Accepted |
| **日期** | P0 立项（Wave 1 / W1-D） |
| **决策者** | 总负责人（Orchestrator），依据 AR-31 与 SD 登记 |
| **关联 AR** | AR-04、AR-13、AR-25、AR-26、AR-28 第 1 条、AR-29、AR-31 |
| **关联 DC** | DC-22、DC-23、DC-40 |
| **关联 HARNESS 章节** | §4.1、§4.3、§5、§7（P0 出口）、§8.1、§11.2 |
| **取代/关联 ADR** | ADR-0018（CBOR+POD 双档）、ADR-0020（线格式 errata）；承接 `docs/plan/m0-spec-defects.md` 的 SD-07、SD-08.1、SD-08.2 |
| **治理提示** | 本 ADR **不改变任何 AR/DC 结论**，只做「数值即契约」的补登记与实现对齐，属 ADR README §1「需要 ADR」的第 3 项（新增/冻结对外契约）。ADR README §4.3 要求「2 名 maintainer 批准 + 7 天公示」；当前仓库仅一个所有者、TSC 未成立（OQ-19），该程序**尚未可执行**——见 §5「负面后果」与 §6「复议条件」。 |

## 1. 背景与问题

P0 出口（HARNESS §7）要求 VT 一致性、IME/CJK、性能门禁与屏幕恢复四条全部可判定。P0 Wave 1 的契约盘点发现**三处「数值即契约」的缺口**，其共同特征是：**设计上已有结论或已被 AR-31 采纳，但缺少可落地的唯一数值**，导致实现者要么各写一套、要么把临时值长期化。三者都会让 P0 出口的门禁变成不可审计的橡皮章。

### 1.1 缺口一（SD-07）：attach 族没有 msg_type 数值

- `kernel/04` §3.4 定义了四类接入消息的**语义**：`ATTACH_REQ` / `ATTACH_ACK` / `TAIL_REPLAY` / `DETACH_NOTICE`（含字段与幂等语义：`resume_from = last_applied_seq`，服务端只发 `> resume_from`）。
- `kernel/07` §3.2 的 msg_type 表**没有**给这四类任何取值，而同节同时规定「数值即契约，永不复用」与「未登记即暴露视为契约事故」（§3.8）。
- M0 因此只能用**已登记数值**勉强表达一个 attach 子集（只读 attach = 空载荷 `GRID_SNAPSHOT(0x0180)`；取写权 = `LEASE_ACQUIRE(0x0304)`），但：
  - `mode: ReadOnly|Interactive`、`resume_from`、`limits{max_frame, credits}` 无处承载；
  - `TAIL_REPLAY` / `DETACH_NOTICE` **完全无法表达** → E-P0-4「screen 可恢复」的 tail replay 与多端 attach 不可实现；
  - 继续下去会诱发实现者自行取值，正是 §3.8 要禁止的事故。
- 已登记数值的现状：`0x0500` 段**完全空闲**；`LEASE_*` 占 `0x0304–0x0307`（`0x0303` 空缺）。

### 1.2 缺口二（SD-08.1）：字符串态载荷上限有两个并存的数字

- **AR-31 已采纳的分诊默认值**（`kernel/01` §8 OQ-VT-01，标注「已采纳默认值」）：**OSC·SOS·PM = 1 MiB；DCS·APC = 16 MiB**；超限即 `Overflow` + 计数，**且后续 Ground 字节必须照常上屏**（V-08 不变式）。
- **M0 的实际实现**取了更保守的 **OSC 64 KiB / DCS 1 MiB**，并经 `BackendCaps` 对外暴露。
- 两者并存违反「门禁数字唯一」的纪律。此时「更保守」**并不自动正确**：A2 公理要求的入场券是**兼容性**，而合法的图形分块（kitty graphics / iTerm2 inline image）与长提示符注入会**合法地超过 64 KiB**；拒绝它们属于「过保守即不合规」——与 SD-08.4（OSC 52 一律 deny）同一类错误。

### 1.3 缺口三（SD-08.2 / B-6）：GridSnapshot 会丢组合字符

- `kernel/03` §3.3 的设计**已经**给出答案：`LineRecord` 携带 `clusters: ClusterTable`（仅复杂 cluster 存在，由 `HAS_CLUSTERS` 标志位指示），「组合字符 / 变体选择符 / ZWJ emoji / 区域指示符归并为一个 cluster，**原始标量序列保存在 ClusterTable.text（逐字节保真，复制与 AI 上下文从这里取）**」。
- M0 的实现把 `Cell.ch` 做成单个 `char`，组合标记只能留在内存侧表，因而 **snapshot / golden / digest 都会丢**。这直接触及 `kernel/01` K-08、V-10 与 UX-G17「软换行开关下复制逐字节相同」——**对外契约级缺口**，不是已知限制可覆盖的范围。
- 同时 B-6 要求 GridSnapshot/GridDelta/ScrollOp 的**字段集**在 P0 冻结；M0 只做了 **临时** v1 冻结（OQ-RND-07 仍开放）。字段集不冻结，`kernel/03`（UI 镜像）、`kernel/04`（attach）、插件只读镜像三方就无法并行开码。

## 2. 可选方案

### 2.1 缺口一：attach 族数值

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | 在 `kernel/07` §3.2 新增 **`0x05xx` 会话接入区段**，分配 `0x0500 AttachRequest` / `0x0501 AttachAck` / `0x0502 TailReplay` / `0x0503 DetachNotice`；`LEASE_*` 维持 `0x0304–0x0307` 不动 | 见 §3 D1 |
| B（否决） | 继续用「空载荷 `GRID_SNAPSHOT` + `LEASE_*`」表达 attach | **否决**：无字段承载 `mode/resume_from/limits`；`TAIL_REPLAY` 与 `DETACH_NOTICE` 永远无法实现，E-P0-4 不可达；且把「请求」与「数据」压在同一数值上，会让 CAP 校验点（`kernel/07` §3.4 CAP-1…CAP-7）无法区分「订阅」与「写入」意图 |
| C（否决） | 复用 `0x00xx` 控制段的空位 | **否决**：该段语义是**连接级**控制（Hello/Ping/GoAway/Resync），会话接入属**会话域**；混段会让审计分类与错误码收口变模糊，且 0x00xx 空位需与外部分册对账，成本高于新开区段 |

### 2.2 缺口二：字符串态上限

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | 冻结为 AR-31 已采纳值：**OSC·SOS·PM 1 MiB / DCS·APC 16 MiB**，保留 `Overflow` + 计数 + 「后续 Ground 照常上屏」不变式 | 见 §3 D2 |
| B（否决） | 保留 M0 的 64 KiB / 1 MiB | **否决**：与 AR-31 已采纳值冲突；64 KiB 会拒绝合法的图形分块与长提示符，把「安全默认」变成**兼容性事故**；且该数字未经过真实语料验证，只是实现期随手取的保守值 |
| C（否决） | 两个数字并存，由 `BackendCaps` 协商 | **否决**：V-08 是 §8.1 的合并阻断门禁，门禁不能有「视协商而定的两个阈值」；协商只能用于**能力**，不能用于**门禁真值** |

### 2.3 缺口三：clusters 与字段集

| 方案 | 内容 | 结论 |
| --- | --- | --- |
| **A（采纳）** | 依据 `kernel/03` §3.3 冻结字段集 v1：`LineRecord` 携带 `clusters: ClusterTable`；golden 与 digest 必须覆盖 ClusterTable；把 OQ-RND-07 的临时冻结转正式 | 见 §3 D3 |
| B（否决） | 把组合标记塞进 `Cell.ch`（改成字符串/小数组） | **否决**：破坏定长 cell 与列宽纯函数语义（`kernel/03` K-04「列宽真源唯一」）；无法表达 ZWJ 序列与区域指示符的**标量序列**，复制仍不保真 |
| C（否决） | 维持现状，登记为「已知限制」 | **否决**：K-08 / V-10 / UX-G17 是**对外承诺**；「已知限制」不能豁免复制保真，只能豁免个别 cluster 的**列宽**回归（RV-01），两者性质不同 |
| D（否决） | 另立第二套宽度表以支持 cluster | **否决**：`kernel/03` K-04 明令列宽真源唯一（在 01），双表必然漂移 |

## 3. 决策（可执行结论）

### D1｜attach 族 msg_type 分配（解 SD-07）

在 `kernel/07` §3.2 新增区段，**数值即契约、永不复用**：

| msg_type | 名称 | 方向 | 编码 | 语义 owner |
| --- | --- | --- | --- | --- |
| **0x0500** | `AttachRequest` | C→S | CBOR | `kernel/04` §3.4 |
| **0x0501** | `AttachAck` | S→C | CBOR | `kernel/04` §3.4 |
| **0x0502** | `TailReplay` | S→C | CBOR | `kernel/04` §3.4 |
| **0x0503** | `DetachNotice` | C→S | POD | `kernel/04` §3.4 |

**边界**：
1. `0x0504–0x05FF` 划为**保留**，不得分配；新分配走新 ADR。
2. `LEASE_ACQUIRE/GRANT/REVOKE/TRANSFER = 0x0304–0x0307` 与 `CAP_DENIED = 0x0302` 维持不变，attach 流程继续复用它们表达写权。
3. 字段以 `kernel/04` §3.4 为准；本 ADR 只分配数值与编码档位，不改字段语义。
4. 编码档位遵循 ADR-0018：结构性/低频消息走 **CBOR 演进档**，定长热路径走 **POD（≤4 KiB）**。
5. 未知 `0x05xx` 数值的处置沿用 `kernel/07` §3.2：落在**已知区段** → 回 `Error{UnsupportedMsg}` 且不断连。
>
> **被 ADR-0026 扩充**：`0x0502 AttachAck/TailReplay` 的方向语义由「S→C」扩充为「**S→C 回复 + C→S 请求**」，并确定 floor 排他、无 floor 即拒绝、窗口不可满足时复用 `AttachStateInvalid`。**数值不变**，§3.7 的「永不复用」不受影响。

### D2｜字符串态载荷上限冻结（解 SD-08.1）

冻结为 **AR-31 已采纳值**，并作为 `kernel/01` OQ-VT-01 的唯一数值：

| 字符串态 | 上限 | 超限行为 |
| --- | --- | --- |
| OSC | **1 MiB** | `Overflow` + 计数；**后续 Ground 字节照常上屏** |
| SOS / PM | **1 MiB** | 同上 |
| DCS / APC | **16 MiB** | 同上 |

**边界**：
1. `BackendCaps.osc_len_limit / dcs_len_limit` 必须返回上表值，**不得**继续返回 M0 的保守值。
2. V-08 不变式不变：字符串态必有终止条件，被忽略序列之后的 Ground 字节必须照常进入 `print()/execute()`。
3. §5 空闲 RSS ≤120MB 的上界需按新上限**重估并登记**（返工范围已由 `00-p0-triage.md` §3 明示）。

### D3｜GridSnapshot/GridDelta 字段集 v1 正式冻结（解 SD-08.2 / B-6）

1. 字段集**以 `kernel/03` §3.3 的结构为准**并转为正式冻结：`GridSnapshot` 携带 `lines: [LineRecord]`，复杂 cluster 经 `clusters: ClusterTable` 表达（`HAS_CLUSTERS` 标志位），`ClusterTable.text` 保存**逐字节原始标量序列**；单标量 cell 维持紧凑表示。
2. **golden 与 digest 必须覆盖 ClusterTable**：复制路径、AI 上下文与 `TERMAI-GRID 1` 快照不得再丢组合字符。golden 格式如需变更，按版本化处理（`TERMAI-GRID 1` 保持兼容读取，新增 cluster 区块并递增 minor）。
3. OQ-RND-07 从「临时冻结 v1」转**正式冻结**；破坏性变更仍走 capability 协商 + 兼容 ≥2 minor（AR-04 / DC-40）。
4. 个别 cluster 的**列宽**回归仍可按 RV-01 走差异登记（附最小复现 + 豁免期限 + owner），但**复制保真不适用豁免**。

## 4. 理由

1. **A1（终端是信任基础设施）**：D1 让 attach 的 `mode/resume_from/limits` 有正式载体，写权仍由 `LEASE_*` 与 CAP 闸门把关；「不可判定即拒绝」不会被新段破坏。
2. **A2（延迟、正确性、可脚本化）**：D2 把「过保守」纠正为「与已采纳值一致」，恢复对合法大载荷的兼容；D3 恢复复制与快照的字节保真——两者都是入场券式的正确性。
3. **A3（状态结构化）**：D1 与 D3 都是「把状态变成 AI 可读写的结构化对象」的必要条件：没有 tail replay 就没有可恢复的结构化会话；没有 ClusterTable 就没有可保真的上下文。
4. **可判定性**：三项都指向同一个目标——**让 P0 门禁有唯一真值**。数值即契约的意义就在这里：两个数字并存时，门禁判定的是「实现者选择」，不是「系统能力」。

## 5. 后果

### 正面
- E-P0-4 的 attach 全族（tail replay / detach notice / 多端 attach）从「不可实现」变为可实现。
- V-08 与 §5 RSS 上界有了唯一数字，G1 可判定。
- B-6 字段集冻结，`kernel/03`（UI 镜像）、`kernel/04`（attach）、插件只读镜像三方可以并行开码；OQ-RND-07 可关闭。
- 组合字符的复制保真从「契约缺口」回到「实现对齐设计」。

### 负面（必须接受的代价）
1. **M0 的 attach 子集要迁移**：空载荷 `GRID_SNAPSHOT` 表达 attach 的临时做法作废，线上协议测试与 `sessiond` 的 attach 路径需改到 `ATTACH_REQ`。
2. **字符串上限放宽 = 内存上界上升**：OSC 从 64 KiB → 1 MiB、DCS 从 1 MiB → 16 MiB，§5 的空闲 RSS ≤120MB 需要重新评估；`corpus/unterminated/` 与 fuzz 种子需重建。
3. **golden/digest 格式变化**：涉及 golden 往返不动点、哈希稳定性与已有 `.trec` 语料的兼容。
4. **治理程序未走完**：ADR README §4.3 的「2 名 maintainer + 7 天公示」在本仓库当前治理状态下**不可执行**（单所有者、TSC 未成立）。本 ADR 按 M0 的 errata 先例（ADR-0020）标记 Accepted 以便实现不被阻塞，但该缺口已登记，必须在 P0 出口前闭合（见 p0-delivery-plan §4 治理缺口）。

## 6. 反方记录与复议条件

- **反方（保守派，对 D2）**：不确定时应取更保守的安全默认值（AGENTS §7.4），1 MiB/16 MiB 提高了解析器的内存与 DoS 面。
  **驳回理由**：该值并非不确定——AR-31 已裁决为「已采纳默认值」；且 DoS 面由「必有终止条件 + 上限 + 计数」这一**结构不变式**控制，而不是由把阈值人为调低控制。把 64 KiB 当安全默认，代价是拒绝合法图形分块，属过保守导致的不合规。
  **复议条件**：若 fuzz 或真实语料证明 1 MiB/16 MiB 可在允许时间内造成 §5 RSS 门禁失败，或出现无法用「终止 + 上限 + 计数」缓解的 DoS 用例，可发起复议并附最小复现与 RSS 报告。

- **反方（务实派，对 D1）**：M0 的临时表达已经能跑通只读 attach 与取写权，新开区段是额外工作量。
  **驳回理由**：临时表达无法承载 `mode/resume_from/limits`，也无法表达 tail replay；继续沿用会把「不可实现」伪装成「已实现」，正是 M0 报告要求避免的过度声称。
  **复议条件**：若在 P0 出口前证明 tail replay 对 E-P0-4 非必需（即屏幕恢复可仅靠 snapshot + lease 达成），可复议把 0x0502 降为 P1；但 0x0500/0x0501 的分配不回退。

- **反方（性能派，对 D3）**：ClusterTable 会让快照变大、clone 成本上升。
  **驳回理由**：`kernel/03` §3.3 已用「仅复杂 cluster 存在（`HAS_CLUSTERS`）+ 单标量紧凑表示」控制了开销；保真度是 K-08/V-10 的硬承诺。
  **复议条件**：若 RP-03（吞吐 ≥500MB/s）或 RP-01（帧时）因 ClusterTable 引入 >5% 回归且无法用紧凑编码消除，可复议编码方式（不得复议保真承诺本身）。

## 7. 关联决策与实现位置

| 项 | 落点 |
| --- | --- |
| D1 数值 | `kernel/07` §3.2 新增 `0x05xx` 段；实现：`crates/termai-ipc`（msg_type 表 + 帧封装）、`apps/sessiond`（attach 状态机） |
| D1 语义 | `kernel/04` §3.4（字段与幂等，不改） |
| D2 常量与行为 | `kernel/01` §8 OQ-VT-01 标「已冻结（ADR-0023 D2）」；实现：`crates/termai-vt`（上限常量 + `Overflow` 计数 + `BackendCaps`） |
| D3 字段集 | `kernel/03` §3.3；实现：`crates/termai-core`（core-dto 字段集）、`crates/termai-vt`（产出 ClusterTable + golden/digest） |
| 规格缺陷登记 | `docs/plan/m0-spec-defects.md` 的 SD-07 / SD-08.1 / SD-08.2 处置列更新为「ADR-0023 D1/D2/D3」 |
| HARNESS 登记 | §11.2 追加 **CR-14**（三项契约 errata 的裁决与同步义务） |
| 后续 ADR 义务 | WS-03 的 `termai-render`/`termai-gpu`、WS-05 的 attach 实现若新增 crate，须各自出 ADR 说明依赖位置（AGENTS §3），并同步 K4 白名单与 K7 CODEOWNERS |

**同步义务（HARNESS §12：ADR 生效后 24h 内同步）**：本 ADR 生效后须完成 ① `kernel/07` §3.2 表；② `kernel/01` §8 OQ-VT-01；③ `kernel/03` §3.3 与 OQ-RND-07；④ `docs/adr/README.md` 索引；⑤ HARNESS §11.2 CR-14。
