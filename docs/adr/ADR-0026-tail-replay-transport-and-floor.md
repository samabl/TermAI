# ADR-0026｜TAIL_REPLAY 的传输形态、floor 语义与错误码（WS-05b errata）

| 项 | 内容 |
| --- | --- |
| **状态** | Accepted |
| **日期** | P0 Wave 2（WS-05b 收口裁决） |
| **决策者** | 总负责人（Orchestrator） |
| **关联 AR** | AR-04（schema 版本化）、AR-13（会话真源）、AR-26（三层保留语义）、AR-30 |
| **关联 DC** | DC-22（termai-ipc）、DC-23（Log 真相） |
| **关联 HARNESS 章节** | §4.3、§7（P0 出口 E-P0-4）、§8.2（可靠性） |
| **关联 ADR / 登记** | **修订 ADR-0023 D1 的方向语义**；kernel/04 §3.4、kernel/07 §3.2/§3.6/§3.8；SD-18、SD-21 |

## 1. 背景与问题

WS-05b 把 `TAIL_REPLAY (0x0502)` 落地时暴露四处必须由裁决确定、而不是由实现者自选的问题：

1. **方向**：ADR-0023 D1 把 `0x0502` 定为 **S→C 单向**；但重连的客户端需要一个**显式请求载体**才能索取 tail（`ATTACH_ACK` 之后服务端主动推送只是一种时序，不是唯一场景）。
2. **floor 端点**：kernel/04 §3.4 的时序写 `TAIL_REPLAY(seq₀..head)`，同节又写「服务端只发 `> resume_from`」——**seq₀ 是否含端点自相矛盾**；且**排他 u64 无法表达「什么都没应用」**（seq 从 0 起，0 永远不在重放范围内）。
3. **窗口不可满足**：AR-26 规定原始字节只是 **P1 volatile ring**，tail 可能已滚出；此时缺**专用错误码**。
4. **事件子集与类型**：kernel/07 §3.6 把 `confidence` 定为 `f32`，而 Log / `termai-vt` 侧是 `u8`;且 Log 的 13 类记录里只有一部分有 §3.6 的 tag。

## 2. 可选方案

| # | 方案 | 结论 |
| --- | --- | --- |
| D6-A | `0x0502` **双向**：C→S 为请求（`\{from_seq, events: []\}` 或空载荷），S→C 为重放 | **采纳**（已实现并有端到端覆盖） |
| D6-B | 保持 S→C 单向，服务端在 `ATTACH_ACK` 后**无条件推送** tail | **否决**：无法表达「客户端只想续传某一段」；且把「订阅」与「补历史」压在同一次握手，重连边界不可控 |
| D7-A | floor **排他**；**无 floor 即拒绝**（`tail_replay_floor_missing`） | **采纳** |
| D7-B | floor 采用 watermark±1 回退，或把「无 floor」当 0 | **否决**：猜 watermark 会在客户端与服务端之间**静默错位**，正是 E-P0-4「屏幕一致」要防的 |
| D8-A | 复用已登记的 `AttachStateInvalid``+ DROP_NOTICE + 可操作 detail | **采纳**（不发明新码） |
| D8-B | 新增专用码 `ReplayWindowUnavailable` | **暂缓**：数值即契约，专用码可在下一个 minor 按 §3.7 增加；当前复用不损失可判定性 |
| D2-A | 线上保持 `f32`（§3.6 为 Context 事件 schema owner），Log 侧 `u8` 由投影点转换 | **采纳**，单位映射登记 **SD-21** 待 kernel/04 owner 确认 |
| D2-B | 把 §3.6 改成整数百分比 | **否决**：§3.6 已冻结且 `f32` 可无损承载未来更细的置信度 |

## 3. 决策

- **D1（方向，修订 ADR-0023 D1）**：`TAIL_REPLAY (0x0502)` **双向**。C→S 为请求（可只带 `from_seq`，`events` 取空），S→C 为同一 msg_type 的重放回复。ADR-0023 D1 的「S→C」按本条**扩充**为「S→C 回复 + C→S 请求」，数值不变、永不复用。
- **D2（floor）**：floor 是**排他**下界；只回 `seq > from_seq`。**无 floor（既无 `resume_from` 也无显式 `from_seq`）一律拒绝**，不回退到 watermark，也不当作 0。`from_seq` 已等于 head → 回**空事件集**（不是错误）。
- **D3（窗口不可满足）**：三类 gap 一律拒绝并给出可操作 detail：`BelowWindow` / `AheadOfHead` / `TailUnreadable`；**绝不回短重放**。
- **D4（错误码）**：复用 `AttachStateInvalid``+ DROP_NOTICE`；不新增码。
- **D5（事件子集）**：本轮只投影 **5 类**（StateChange→tag1、CmdEnd→tag3、CwdChange→tag4、TitleChange→tag5、Resize→tag6）。**未投影的必须写明理由**：PtyOut 属 P1 volatile raw ring（§3.6「仅 P0 事件」）；PtyIn/CheckpointRef/LeaseEvent/AuditRef 在 §3.6 **无对应 tag**，新增 tag 属 ADR；CmdStart 需要 Log 尚不承载的 `osc+confidence`。**新增 tag 必须先走 ADR**。
- **D6（confidence 类型）**：线上保持 `f32`（CBOR 0xFA）；Log 的 `u8` 由投影点显式转换，**单位映射待确认**（SD-21）。

## 4. 理由
1. **A1（默认安全）**：D2/D3 的共同点是「宁可拒绝，也不给一个看起来成功但缺事件的短重放」——错位的重放比明确失败危险得多，因为它让屏幕**静默**不一致。
2. **A2（正确性）**：排他 floor + 显式请求把「客户端已应用到哪」变成**客户端声明的契约**，而不是服务端猜测。
3. **可判定**：三类 gap、空事件集、幂等、分块顺序都可用纯 Rust/线上测试判定（WS-05b 已覆盖），不依赖参考机。
4. **不改数值**：本 ADR 只扩充方向语义与拒绝语义，**不动任何已发布 msg_type 数值**，符合 §3.7。

## 5. 后果
### 正面
- 重连路径第一次有了**明确失败面**：客户端知道「必须重取快照」，而不是拿到半截历史。
- `0x0502` 的数值契约保持稳定；双向使用不引入新数值。
### 负面（必须接受的代价）
1. **跨段重放未实现**（WS-05b 只读当前 segment）：旋转/归档后的 tail 只会得到 `BelowWindow` 拒绝。这是**诚实的缺口**，必须保留在登记里，不得被「已实现 TAIL_REPLAY」掩盖。
2. floor 无默认值 → 所有调用方必须显式声明应用进度；旧客户端若省略 `resume_from` 会被拒绝（这是**有意的**破坏性行为，需在 CLI/UI 层给出可操作提示）。
3. 事件子集只有 5 类 → 用 tail replay 恢复的信息不完整，**不能用它替代 GRID_SNAPSHOT**；UI 必须把两者配合使用。

## 6. 反方记录与复议条件
- **反方（宽松派）**：无 floor 时回退到 watermark 对用户更友好。
  **驳回**：友好与正确冲突时取正确；错位重放会让「屏幕一致」变成不可验证。**复议条件**：若实测显示重连失败率显著（>5%）且失败全因缺 floor，可复议增加"由服务端返回 watermark 供客户端显式确认"的两步握手，而**不是**服务端替客户端决定。
- **反方（专用码派）**：三类 gap 用同一个码会丢失可操作性。
  **驳回**：可操作性由 `detail` 承载（本次已给出 "request a full GRID_SNAPSHOT"），而**码是数值契约**，其扩张成本高于收益。**复议条件**：若 UI/CLI 需要按 gap 类型分支（而非读 detail），可按 minor 新增专用码并在 kernel/07 §3.8 登记。

## 7. 关联决策与实现位置
| 项 | 落点 |
| --- | --- |
| D1 双向 | `crates/termai-ipc/src/codec.rs`（TailReplay 编解码 + golden 向量）、`apps/sessiond/src/broker.rs` |
| D2/D3 floor 与 gap | `crates/termai-session/src/log.rs`（`ReplayGap` + `replay_window_check`）、broker 的拒绝分支 |
| D4 错误码 | kernel/07 §3.8 已登记的 `AttachStateInvalid`；不新增 |
| D5 事件子集 | broker 的「Log 记录 → §3.6 投影」；新增 tag 须先出 ADR |
| D6 类型 | `crates/termai-ipc/src/cbor.rs`（RFC 8949 binary32/64）；单位映射见 SD-21 |
| 规格同步 | kernel/04 §3.4 的 `TAIL_REPLAY` 标注「双向，floor 排他」；kernel/07 §3.2 补注 0x0502 双向 |
