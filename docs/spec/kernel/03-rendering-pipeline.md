# 03 · 渲染管线与文本整形规格（Rendering Pipeline & Text Shaping）

> 定位：把 HARNESS §4.2（L0–L4 分层与合成规则）、AR-01/AR-14/AR-19/AR-20/AR-22/AR-23、DC-17/DC-20/DC-25 落成可编码、可 CI 校验的渲染与整形机制。本文只补机制，不新增 AR/DC 结论。
> 权威顺序：HARNESS.md > docs/spec/00-glossary.md > docs/spec/03-system-architecture.md > 本文；数值冲突一律以 HARNESS §5 为准。
> 数值口径（AR-19）：**门禁 = 达不到即阻塞发布**；**目标 = 挑战值**。本文引用数值直接取 §5 门禁列 / 目标列并标注列名；**本文新增的可测指标一律标【新增】，且集中在 §8 待裁决**，不与 §5 并列（AGENTS §5、AR-19）。
> **编号空间（AR-28 第 2 条）**：本分册内部问题编号统一为 **OQ-RND-NN**（如 OQ-RND-01）；其它分册用各自前缀（OQ-VT / OQ-PTY / OQ-SES / OQ-INP / OQ-PM / OQ-ABI）。

## 1. 范围与依据

| 维度 | 内容 |
| --- | --- |
| 范围内 | ① 管线阶段与数据流（PTY bytes → VT → Grid → damage → shaping/atlas → GPU draw → present）；② 网格数据结构与内存预算；③ damage region 算法；④ 字形 atlas 规格；⑤ 文本整形与 grapheme cluster→列映射；⑥ 帧调度与节能；⑦ GPU 后端降级 T0–T3；⑧ AR-23 第 6 条（软换行 = 视觉裁剪）在网格模型中的落点 |
| 范围外 | VT 序列语义全集与兼容矩阵（01 内核/VT 篇）；PTY/Transport（02）；Session Log/存储（04）；a11y 承诺边界与 IME 输入法实现（AR-20，由 01/02 篇承载）；token 取值（tokens codegen） |
| HARNESS 依据 | AR-01（混合分层、网格绝不走 Web、L2 不得覆盖 L4）；AR-02（WebView 可选 + 降级）；AR-03（AI 不进 PTY→像素热路径，内核不依赖 AI/网络/UI）；AR-14（灰度 AA、hinting 关闭、网格对齐 ≤0.5px、锐利/柔和两档、不承诺 subpixel）；AR-18（vte 在 trait 边界后，语义层自研）；AR-19（门禁/目标双列）；AR-20（UI 层 a11y、网格内 bidi 必走 Unicode Bidi 算法、诚实原则）；AR-22 §2（网格行高 1.25）、§4/§6（软换行、无横向滚动）；AR-23 §3（跨窗口状态 ≤1 帧）、§6（软换行关闭 = 视觉裁剪）；§4.2 合成规则 1–6；§5 门禁列；§8.1/§8.2 |
| 下层依据 | docs/spec/03 §3.2（L0–L4 与合成规则）、§3.4（termai-ipc）、§3.9（UI 是 projection）、§3.14 T1（输入热路径与「无 JSON/无 AI/无网络/无阻塞锁」约束）；docs/spec/02 C14/C16、§3.12、A20/A29/A34、UX-G15/UX-G17、UX-R12/UX-R17、UX-OQ-15；docs/spec/07 §3.4.4（动画冻结视觉回归）、§3.8（参考机与回归判定）、§2.3-B12、§3.4.2-G7/G8 |
| ADR 依据 | ADR-0001（混合架构与 L0–L4、AR-14 画质）；ADR-0012（VT 边界、双列口径、bidi/kitty keyboard 口径）；ADR-0014 决策 1/3/4（参考机、三字体后端、T0–T3 降级）；ADR-0016（视觉语言）；ADR-0017（终端为主体、软换行/裁剪） |
| 与 HARNESS 的差异 | 本文件曾发现 3 处 §5 口径空白（RSS 进程范围、缺少 CPU/功耗项、帧时与吞吐的测量定义）与 1 处判据空白（网格对齐 ≤0.5px 的判据）；**四者均已由 AR-24 裁决**，本文 §8 保留原始议题、反方理由与测量实现，裁决结果见各 OQ-RND 条目的「已决」标注与 §8.1 |

## 2. 关键结论（K）

| 编号 | 结论 | 理由 | 代价 |
| --- | --- | --- | --- |
| K-01 | **网格真相只存在于 sessiond**；UI 进程持有一份「按 rev 校验、可整体丢弃」的渲染镜像（mirror），镜像**不得回答任何真相查询** | AR-03/AR-13：内核须能脱离 UI 独立正确运行；spec/03 §3.9：UI 是 projection，禁止缓存网格真相。「禁止缓存真相」= 禁止以镜像为准回答 grid.*/copy/AI 上下文，而非禁止热路径存在连续内存 | 镜像有漂移风险 → 必须由 rev + 全量 snapshot 兜底（§3.3.3）；重复一份行数据的常量内存开销（≤2 MiB，见 §3.2.3） |
| K-02 | **管线跨进程切点在 damage → shaping**：sessiond 只做 VT/Grid/damage 并产出 GridDelta/ScrollOp，**不 shape、不碰字体/DPI/GPU**；UI 只做 shaping/atlas/draw/present，**不解析 VT** | AR-01 + DC-17：shaping 依赖字体发现、DPI、AA 预设与 GPU 能力（全在 UI）；sessiond 必须 headless 可用（apps/termai-headless 无 GPU 呈现，ADR-0014 决策 2） | 需定死 GridDelta 协议与镜像应用顺序（§3.1/§3.3）；任何字段缺失会造成「UI 需要回读真相」的往返；协议变更走 capability 协商 + 兼容 ≥2 minor（AR-04、DC-22） |
| K-03 | 网格采用**两形态**：冷存储 = line-arena 紧凑形态（**4B 文本键 + style-run 压缩**，满足 DC-20）；热路径 = 按需物化的 HotRow（8B/cell，属性已展开，无 run 查找） | DC-20 明确要求 line-arena + style-run；而逐 cell 查 run 会让 shaping 热路径每帧做区间查找。两形态各取所长 | 需一条物化路径与不变量断言（I4 差分测试，§3.2.1）；物化缓存需在被行失效时同步清理 |
| K-04 | **列宽（cell 列数）判定的唯一真源是 termai-vt 的 width 表**（UAX #11 + emoji presentation 规则）；termai-render **禁止自带第二套 wcwidth** | cluster↔列漂移是本管线最贵的 bug 类（roles/04 §六风险 2）；两份宽度表必然分叉 | termai-render 依赖 termai-vt 的宽度 API（仍在 DC-21 允许的 render → vt 边内）；宽度表升级需双端同步回归 |
| K-05 | **rustybuzz = 唯一 OpenType shaping 引擎；swash = 唯一栅格化/彩色字形引擎**；DirectWrite/CoreText/fontconfig+FreeType 只做字体发现、fallback 与**度量** | DC-17 + ADR-0014 决策 3（栅格化统一由 swash，三后端均不启用系统 subpixel AA）。单一 shaping 引擎避免字形 id 与 cluster 语义分叉 | 放弃系统栅格化的 hinting 与观感（AR-14 已接受）；需自维护合成 bold/oblique 与字体回退表情 |
| K-06 | **连字不改变列数**：一簇连字 glyph 占据其 cluster 的 cell 列数并在该范围内绘制；连字**跨越**软换行 / 裁剪 / 选区 / 光标边界时，在该边界处按 liga=off 重 shape 该段 | AR-14（ligature 默认开、可关）+ AR-23 §6 / spec02 C14（不改变网格列数与复制结果）。若允许连字自由跨 cell，光标与选区无法逐 cell 定位 | 边界处字形形态与「整行连字」略有差异（可被视觉回归捕获并入库）；需要 cluster 级重 shape 的确定性（RP-08/RP-10） |
| K-07 | **整屏滚动走「行槽位重映射」快路径**：每行绑定稳定的 GPU 槽位，滚动 = 旋转行→槽映射（O(1)），仅新暴露行进入 shaping/damage | §5 门禁「4K@120Hz 帧时 <8.3ms」下，整屏重 shape 不可能；roles/04 §3.2 亦要求 damage 部分重绘 | 行槽位与 damage 不一致会出画面错误 → 必须保留「全量重绘」回退开关与不变量断言（RV-04） |
| K-08 | **damage 只在 sessiond 内累积**，合并规则：脏行 ≥60% 视口或 scroll delta≠0 → 提升为全屏 damage；一帧只提交一次 GridDelta，编码期间到达的增量顺延到下一帧 | §4.2 规则 4（damage 增量重绘）+ §3.14 T1（无阻塞锁、仅 damage 增量）；分帧不完整提交会撕裂画面 | 60% 阈值是工程取值（非 §5 门禁）；阈值不当会造成慢帧 → 由 RP-02 的帧时直方图回归兜底 |
| K-09 | **未聚焦 / 被遮挡 = 零出帧**：FrameState::Occluded 下不做 GPU 提交、关闭光标闪烁；PTY 增量仍应用到镜像；恢复焦点做一次全 damage 重绘（≤1 帧） | HARNESS §4.2 规则 6「未聚焦窗口不主动出帧」、§3.2 规则 6。持续出帧是笔记本功耗与 RSS 斜率的主要噪声源 | 用户可能误判「卡死」（RV-06）；镜像仍在推进，恢复后需要一次全重绘（可测：RP-12） |
| K-10 | **软换行 / 裁剪是显示层映射（VisualRowMap），不是网格层行为**：切换开关**不产生任何 GridDelta、不改变 grid rev、不改变列数与复制字节** | AR-23 §6 + spec02 C14：网格由列数定义；AR-22 §4 的代价措辞（折行占用更多行）只有「显示层换行」解释才自洽 | 显示行数 ≠ 网格行数 → 滚动锚点、命中测试、a11y 坐标映射必须全部走 VRM（§3.8），实现面扩大 |
| K-11 | **Clip（软换行关）模式下每逻辑行只显示一行 = 光标所在行**（光标不在该行则显示头行），**不引入横向滚动、不做横向窗口平移** | 关闭态若固定显示头行，光标移入续行后不可见 → 无法输入；横向滚动被 AR-23 §6 明令禁止。此解是两者唯一的交集 | 处于光标所在逻辑行时看不到该行前段（可用复制 + 临时开启软换行补偿）；语义需 UX 确认（OQ-RND-05） |
| K-12 | atlas 主键含 **font / 设备像素 scale / size / weight / slant / AA 预设 / 合成标记 / glyph id**；**DPI 变化不做位图重采样，而是按新 scale 重建**；最多保留 2 组 scale 热页 | AR-14（不承诺 subpixel、锐利/柔和两档）+ 非整数 DPI 下位图缩放必然模糊；跨屏迁移（100%↔200%）若只留 1 组会持续抖动 | 内存 ×2 组 scale（由 §3.2.3 的 ≤24 MiB 硬上限兜底）；切 scale 首帧可能缺字形 → ASCII 预取（RP-11） |
| K-13 | **BiDi 限于行内视觉重排**：UBA 只在单个网格行内运行、绝不跨 WRAPPED 链与行边界；**复制、AI 上下文、插件镜像恒取逻辑序** | AR-20「网格内 bidi 必须走 Unicode Bidi 算法」+ AR-03（内核产出结构化上下文，不得受显示层影响） | RTL 混排行的视觉顺序与列顺序不一致 → 命中测试/选区/坐标映射需 bidi 映射表（§3.5.4）；方块/制表字符行需抑制重排 |
| K-14 | T0–T3 降级由 termai-gpu 单一决策点给出，termai-render 只消费能力集；**非 T0 一律 NON-GATING，性能与兼容门禁判失败** | ADR-0014 决策 4 铁律 1 与规则 4 | 低端/受限环境永远拿不到门禁结论（用户可见「图形能力受限」）——这是诚实原则（AR-20）的既定代价 |

## 3. 详细设计

### 3.1 管线阶段、输入输出与责任边界

| 阶段 | 执行者（crate/进程） | 输入 | 输出 | 线程/时机 | 失败定位 |
| --- | --- | --- | --- | --- | --- |
| S0 读取 | termai-pty → termai-session（sessiond） | PTY/Transport 字节 | 字节批次（≤64 KiB） | 专用 I/O 线程 | PTY 读错误码 + 会话状态机事件 |
| S1 VT 解析 | termai-vt（vte 在 trait 后，AR-18） | 字节批次 | Perform 调用序列（含 OSC 133/633、OSC 7、kitty/Sixel 语义层） | 与 S0 同线程 | 差异登记表 + 解析错误计数 |
| S2 网格应用 | termai-vt Grid | Perform 序列 | 行/单元格变更 + Damage 累积 | 同上 | 不变量断言 I1–I5（debug/fuzz/回放） |
| S3 damage 打包 | termai-session | Damage + ScrollOp | GridDelta（rev 单调）/ ScrollOp | 同在解析线程，结束后一次性入环 | delta 计数、全屏提升次数 |
| S4 传输 | termai-ipc 数据通道（共享内存环，AR-04/DC-22） | GridDelta / snapshot | UI 侧同样结构 | 生产者不阻塞 | credits / 溢出事件（RV-07） |
| S5 镜像应用 | termai-render（UI） | GridDelta / snapshot | 更新镜像行（紧凑→热行物化） | UI 主线程 | rev 缺口即请求 snapshot（§3.3.3） |
| S6 shaping | termai-render::shape | HotRow + 字体/DPI/AA 上下文 | RowGlyphs（cluster→cell→glyph） | UI 主线程（可分帧） | cluster↔列断言 + fit_squeezed 计数 |
| S7 atlas | termai-render::atlas + swash | AtlasKey | GlyphSlot（页、矩形、advance、bearing） | 同上 | miss 率、驱逐次数、重建代际 |
| S8 绘制/提交 | termai-gpu（wgpu；L0/L1/L3 同 target） | 顶点/槽位 + 调色板 | command buffer | 同上 | 阶段时间戳（帧时直方图） |
| S9 present | termai-gpu | 交换链图像 | 上屏 | vsync（PresentMode::Fifo） | present 时间戳 / 丢帧计数 |

边界铁律：**S1–S4 不得出现 JSON/gRPC/AI/网络/阻塞锁**（§3.14 T1、AR-04）；**S6–S9 不得回读 sessiond 真相**（K-01）；L2（WebView）重绘不得触发 L0 全量重绘（§4.2 规则 4）。

### 3.2 网格数据结构与内存预算

**3.2.1 紧凑形态（sessiond 真相，DC-20）**

```rust
// crates/termai-vt/src/grid.rs
#[repr(C)] pub struct Cell { key: TextKey, attr: AttrId, flags: CellFlags } // 8 B：热行物化形态
pub struct TextKey(u32); // bit31|30 标签：0b00=单标量(U+21 位) | 0b10=cluster 索引 | 0b11=空白 | 0b01 保留
pub struct AttrId(u16);  // 唯一化属性集索引 → AttrTable（fg/bg/underline/hyperlink/…），DC-09 token 驱动
bitflags! { pub struct CellFlags: u16 { const WIDE_LEAD; const WIDE_TAIL; const CLUSTER_HEAD; const PROTECTED; } }
bitflags! { pub struct LineFlags: u16 { const WRAPPED; const HAS_CLUSTERS; const REVERSED_BIDI; } }

pub struct LineRecord {            // 冷存储：每行一段
    meta: LineMeta,                // id / used / runs / rev / flags / cluster_span
    cells: Box<[TextKey]>,         // 4 B/cell，仅文本键（属性不在此）
    runs: Box<[StyleRun]>,         // style-run 压缩：{ attr: AttrId, start: u16, len: u16 } = 6 B
    clusters: ClusterTable,        // 仅复杂 cluster 存在（HAS_CLUSTERS）
}
pub struct ClusterTable { text: String /* 原始 UTF-8，复制语义的唯一来源 */, recs: Box<[ClusterRec]> }
pub struct ClusterRec { col: u16, cols: u8, off: u32, len: u16, kind: ClusterKind }
pub enum ClusterKind { Combining, VariationSelector, ZwjSequence, RegionalIndicator, LigatureHint, BidiControl }
```

表示规则（可施工）：
1. **单标量 cell** 内联（含 CJK 与补充平面）；宽字符由 WIDE_LEAD + 后继 WIDE_TAIL（key=0b11 空白占位）表示，两列共享一个字符语义。
2. **组合字符 / 变体选择符 / ZWJ emoji / 区域指示符** 归并为一个 cluster，落在 base 的列范围（通常 1 或 2 列），原始标量序列保存在 ClusterTable.text（**逐字节保真，复制与 AI 上下文从这里取**）。
3. **硬换行（DECAWM）** 在网格层以 LineFlags::WRAPPED 链接同一逻辑行；这是网格层行为，与 §3.8 的显示层软换行**严格分离**。
4. used = 最后一个非空白列右边界，供复制修剪与 damage 收窄；cols 恒定（只有 resize 才变）。
5. **AttrTable 去重**：同一会话内不同属性集合通常 <64 个，AttrId 即索引；调色板变更（主题切换）只改 AttrTable → RGBA 映射，**不使 grid 失效**（视觉回归基线按主题分档，spec07 §3.4.4）。

不变量（debug 断言 + fuzz + 行为回放三处强制）：I1 每行 cell 数 = cols；I2 宽字符 lead/tail 成对且不跨行；I3 ClusterTable.text 与单标量 cell 按列序拼接 = VT 语义后的逻辑内容；I4 runs 展开后逐 cell 与参考实现一致；I5 切换 WrapMode 后 rev 不变、delta 计数 = 0。

**3.2.2 热行物化**

```rust
pub struct HotRow { pub line: LineId, pub rev: u32, pub cells: [Cell; MAX_COLS] } // attr 已展开，shaping 直接消费
pub trait Materializer { fn materialize(&self, line: LineId) -> &HotRow; }        // 行失效即重物化
```

物化只在行进入 damage、光标、选区或 a11y 查询时发生；物化结果随行 rev 失效。

**3.2.3 与 DC-20 / §5 RSS 门禁的对齐**

§5 门禁列：**空闲 RSS（1 万行）≤120MB**、**24h RSS 斜率 <1MB/h**（RM-A，稳定 60s 后采样；spec07 §3.8.2）。下表是**拆解到模块的内部子预算【新增】**——它把门禁数字变成「谁超了谁负责」的诊断工具，**不是新的 §5 门禁**；其口径已由 **AR-24 第 1 条**裁决（RSS = 核心进程组 = sessiond + 原生 UI），见 §8 OQ-RND-01（已决）。

| 归属 | 项 | 预算 | 算式/说明 |
| --- | --- | --- | --- |
| sessiond | line-arena 紧凑形态 | 11 MiB | 10k 行 × 200 列 × 4 B = 8.0 MiB；StyleRun ≈3/行 × 6 B ≈ 0.2 MiB；ClusterTable ≈2 MiB；行元数据 10k × 24 B ≈ 0.25 MiB |
| sessiond | VT 状态机 / 模式 / AttrTable / OSC 缓存 | 4 MiB | 固定 + 语料相关，上限由 fuzz 断言 |
| sessiond | GridDelta + 共享内存环 | 4 MiB | 容量上界见 OQ-A4（架构篇） |
| sessiond | 分配器碎片与增长余量 | 5 MiB | — |
| UI | 渲染镜像（热行 + 行映射 + VRM） | 2 MiB | ≤512 热行 × 200 列 × 8 B ≈ 0.8 MiB + 映射表 |
| UI | 字形 atlas（§3.4） | ≤24 MiB | 文本 R8 1024² × 4 页 = 4 MiB + 彩色 RGBA8 1024² × 1 页 = 4 MiB，**硬上限 24 MiB** |
| UI | 字形索引 / LRU / shaping scratch / glyph run cache | 8 MiB | 逐帧可回收 |
| UI | 顶点 / uniform / staging 双帧环 | 6 MiB | 4K 全 damage ≈6.5 万 quad × 16 B ≈ 1 MiB/帧 |
| UI | 字体数据（mmap 常驻）+ 调色板 | 8 MiB | 字体文件 mmap，常驻部分计入 |
| UI | L1 外壳（tab/分栏/状态栏/命令面板）状态 | 4 MiB | DC-25 受限控件集 |
| UI | 分配器碎片与增长余量 | 6 MiB | — |
| — | **合计** | **82 MiB** | 门禁余量 38 MiB（31.7%） |
| 不计入 | L2 WebView | 0（未加载） | roles/10 D-03：WebView **默认不加载**；就绪后增量 **≤60MB 独立记账**、不计入核心组门禁（**AR-24 第 1 条**），且必须同时报告总内存 |

### 3.3 damage region 算法

```rust
pub struct Damage {
    rows: RangeSetU16,          // 视口行区间集合（已合并、按 y 排序）
    scroll: Option<ScrollOp>,   // 整屏滚动快路径
    cursor: bool, selection: bool, overlay: OverlayDirty,
    full: bool,                 // 提升标记
}
pub struct ScrollOp { top: u16, bottom: u16, delta: i16 }   // delta<0 上滚（内容上移）
pub struct GridDelta { rev: u32, scroll: Option<ScrollOp>, damage: Damage,
                       rows: SmallVec<[RowPayload; 64]>, cursor: CursorState }
```

算法（伪代码，全部在 sessiond 内、单线程执行）：

```text
on cell_write(y, x):        damage.rows.insert(y)
on scroll(top, bottom, d):  damage.scroll = coalesce_scroll(prev, ScrollOp{top, bottom, d})
on scrollback_push():       damage.rows.insert_all(0..viewport_rows)
merge():
    if damage.rows.covered() > 0.60 * viewport_rows:        damage.full = true
    if scroll.is_some_and(|s| s.span() == viewport_rows):    damage.full = true
    damage.rows.coalesce()                                   // 环形位图 + 首/末脏行游标，O(rows)
emit():                                                      // 每帧一次；编码期到达的增量顺延
    rev += 1; ring.push(GridDelta{..}); damage = Damage::default()
```

1. **变更累积**：只记行号与 cursor/selection/overlay 标志，不复制行内容（行内容在 emit 时按 damage 逐行取，避免重复拷贝）。
2. **区域合并**：视口用位图 + 首/末脏行游标；跨 scrollback 的变更只置 scrollback_dirty（不回传行号）。
3. **整屏滚动特殊路径**：ScrollOp 不携带行内容；UI 侧执行「行槽位重映射」（K-07）：slot_of[y] 旋转 + 新暴露行请求 payload（RowPayload），旧槽位不重 shape。
4. **与帧预算的关系**：damage 是帧预算的**输入**而非结果。基线：仅光标脏 → 单 cell；单行脏 → 1 行 shaping；≥60% → 全 damage 基准用例（这正是 §5「4K@120Hz 网格帧时 <8.3ms」的判定负载，ADR-0014 决策 1）。
5. **失败定位**：bench-report.json 增列【新增】damage_rows_hist、full_promotions、scroll_fastpath_hits/misses；帧时超限时先看 full_promotions 是否异常升高。
6. **镜像漂移兜底**：GridDelta 带单调 rev；UI 发现 rev 缺口 → 丢弃镜像并发起 GridSnapshot 全量替换（K-01 的一致性兜底，不依赖增量重放）。

### 3.4 字形 atlas 规格

```rust
pub struct AtlasKey {                 // 主键（全部字段参与 hash）
    font: FontId,                     // 逻辑字体（含 fallback 命中结果）
    scale_q8: u16,                    // 设备像素 / 逻辑像素 × 256（DPI 变化即换键）
    size_q6: u16, weight: u16, slant: u8,
    aa: AaPreset,                     // Sharp | Soft（AR-14 两档；均为灰度 AA）
    synth: SynthFlags,                // 合成 bold / oblique
    glyph: u32,                       // 字体内 glyph id（非 codepoint：连字/变体各自成键）
}
pub struct Atlas { pages: Vec<Page>, index: HashMap<AtlasKey, GlyphSlot>, lru: LruList, budget: usize }
pub struct GlyphSlot { page: u8, rect: Rect, advance_q6: i16, bearing: (i16, i16), gen: u16, flags: SlotFlags }
pub trait Rasterizer {                // swash 实现
    fn rasterize(&mut self, k: &AtlasKey) -> GlyphBitmap;   // 灰度 R8；彩色走 RGBA8 页
    fn is_color(&self, k: &AtlasKey) -> bool;               // COLRv1 / CBDT
}
```

| 维度 | 规格 | 可验证 |
| --- | --- | --- |
| 页规格 | 文本页 R8Unorm 1024×1024（1 MiB）；彩色页 RGBA8 1024×1024（4 MiB）；AR-14 的灰度 AA 让文本页只用 1 通道（相对 RGBA 省 4×） | 页数/字节在 bench 与 debug UI 可读 |
| 上限与分页 | atlas.max_bytes = 24 MiB【新增】；文本页 ≤4、彩色页 ≤1；超出即驱逐，**绝不增长以保 RSS 门禁** | RP-04（内部子预算断言）+ RP-10 |
| LRU | **页级 LRU**（非 glyph 级：页级驱逐天然批量、避免索引碎片）；当前帧与上一帧引用的页 pin 住不驱逐（双帧环） | RP-10：把上限压到 2 MiB 重跑 golden，帧哈希必须完全相同 |
| 失效与重建 | 触发：字体文件/回退链变化、size/weight/AA 预设变更、scale 变更、device lost、atlas.rebuild 命令。失效 = gen += 1，**旧 gen 的 slot 一律视为 miss**，绝不用错代际字形 | RP-10/RP-14；gen 与 miss 率进指标 |
| DPI 变化 | **不做位图重采样**；按新 scale_q8 建新键与页；保留**最多 2 组 scale 热页**（100%↔200% 往返不抖动） | RP-11：切换 ≤1 帧内完成 ASCII 预取，全量重建 ≤300ms【新增】 |
| 跨屏迁移 | 窗口跨显示器：scale_q8 变化 → 新 scale 页组 + 旧组保留为第 2 组；第 3 组出现时按 LRU 整组回收；预取范围 = ASCII 可打印 95 字形 + 当前视口已用 cluster | RP-11；内存不超上限 |

### 3.5 文本整形规格

**3.5.1 cluster → cell 列的映射算法**

```text
shape_row(row):
  1. 列宽真源 = termai-vt::width::measure(scalars)        // K-04，termai-render 不得自带 wcwidth
  2. 切分：按 LineRecord.clusters + 单标量 cell 恢复 UAX #29 extended grapheme cluster 序列
  3. 列锚定：cluster[i].col = 前序 cluster 列宽累加；宽字符占 2 列（WIDE_LEAD / WIDE_TAIL）
  4. itemize：按 (script, attr, fallback font) 切 run     // 同一 run 内才可做 OpenType layout
  5. rustybuzz shape(run, features) → [GlyphInfo{glyph, cluster, advance, offset}]   // 唯一 shaping 引擎
  6. 逐 cluster 落位：glyph 原点 = cell_box 内居中；超宽/超高 glyph 等比缩放 fit（记 fit_squeezed）
  7. liga/calt 开时跨 cell 的连字以 cluster range 记录（SlotFlags::SPAN(n)）；遇软换行段边界 /
     裁剪边界 / 选区边缘 / 光标位置，则在该边界处对两段分别以 liga=off 重 shape（K-06）
```

| 情形 | 算法 | 判据（语料） |
| --- | --- | --- |
| 宽字符（CJK / Emoji Presentation） | 占 2 列，tail 无文本；写入 tail 半格 → 按 VT 语义清除整个字符 | GraphemeBreakTest + EastAsianWidth + 真实 CJK 语料 |
| 组合字符（Mn/Mc/Me、VS15/16） | 归入 base cluster，列宽 = base 列宽；不新增列 | UAX #29 + 变体选择符用例（ADR-0014 M12-⑥） |
| Emoji ZWJ 序列（家庭 emoji） | 整串 = 1 cluster，列宽取 base 的 2；彩色页绘制；选中/复制取整串 | emoji-test.txt 全量 + E60 |
| 区域指示符（国旗） | 两标量合 1 cluster，列宽 2 | emoji-test.txt |
| 连字（JetBrains Mono） | 不改变列数；边界处按 K-06 重 shape | ADR-0014 M12-⑦ 连字开关各一次 |
| 字形超宽（窄 cell 宽字形、Nerd Font 图标） | 等比缩放 fit 到 cell box；fit_squeezed 计数可观测（不溢出、不遮挡邻格） | 网格对齐 RP-05 |

**3.5.2 rustybuzz 与 swash 的使用边界**

| 引擎 | 职责 | 明确不做 |
| --- | --- | --- |
| rustybuzz | OpenType layout（GSUB/GPOS）、连字、mark 定位、cluster 级 glyph 序列 | 栅格化、bidi 重排、字体发现、列宽判定 |
| swash | 轮廓提取与缩放、灰度栅格化、合成 bold/oblique、彩色字形（COLRv1/CBDT） | OpenType layout（**不用** swash 的 shape 引擎，避免双 shaping 语义） |
| 字体后端（DirectWrite / CoreText / fontconfig+FreeType） | 字体发现、cascade 回退、**度量**（advance / 行高 / 基线），ADR-0014 决策 3 | 栅格化（统一 swash）、系统 subpixel AA（AR-14） |

**3.5.3 行高与网格对齐（AR-14、AR-22 §2）**

网格行高固定 **1.25 × font size**（AR-22 §2）；cell 宽 = 等宽字体的 advance（"0" 的 advance，来自字体后端度量）；glyph 在 cell box 内居中。**网格对齐误差 ≤0.5px**（§5 门禁列）的判据定义见 §8 OQ-RND-04，默认按「所有已绘制 glyph 位图原点相对 cell 原点的偏差最大值」判定。

**3.5.4 BiDi 的受限范围（AR-20）**

```rust
pub struct BidiMap { runs: SmallVec<[BidiRun; 8]> }   // 行内视觉重排区间（列区间 + 嵌入层级）
pub struct BidiRun { cells: Range<u16>, level: u8 }
```

1. 作用域 = **单个网格行**；WRAPPED 链的每一行独立跑 UBA（不跨行重排，K-13）。
2. **抑制条件**：行内含制表/方块元素（U+2500–U+257F、U+2580–U+259F）或该行被应用标记为「列敏感」→ 不做视觉重排，记 bidi_suppressed 计数。
3. 视觉重排只影响绘制顺序；**选区、命中测试、复制、a11y 朗读、插件镜像一律走逻辑序**（用 BidiMap 反查视觉→逻辑）。
4. v1 不承诺：跨行 bidi、输入行以外的双向编辑、字符镜像（AR-20 诚实原则；E60 只验读取正确性）。

### 3.6 帧调度与节能

```rust
pub enum FrameState { Active, Idle, Occluded, Suspended }
pub struct Scheduler { state: FrameState, dirty: DirtyMask, pending: Option<GridDelta>, last_present: Instant }
```

| 规则 | 机制 | 验证 |
| --- | --- | --- |
| vsync | PresentMode::Fifo（120Hz → 8.33ms 节拍）；不用 Immediate/Mailbox（撕裂/能耗，见 §7 V-09） | RP-02 帧时与丢帧 |
| 合帧 | 每 vsync 至多 1 帧；编码期到达的 delta 进 pending 顺延；**不部分提交** | frames_encoded / frames_deferred 计数 |
| Idle（聚焦静止） | 仅光标闪烁；damage = 光标 cell；reduced-motion 下闪烁 = 0（§8.2 可访问性） | RP-12 |
| Occluded（未聚焦/被遮挡/最小化） | **零 GPU 提交**、停闪烁；delta 仍应用到镜像；焦点恢复做一次全 damage 重绘 | RP-12：未聚焦或被遮挡出帧 = **0 帧/10s**（AR-24 第 2 条） |
| Suspended（锁屏/会话挂起） | 停止消费共享内存环（credits = 0）；溢出策略 = **丢弃 delta + 请求全 snapshot**（AR-04 要求显式声明，不得静默丢弃） | RV-07；重连后 grid 哈希一致 |
| 空闲 CPU（**AR-24 第 2 条，已生效门禁**） | RM-A/T0，聚焦可见 + 静止 60s 预热后 5×60s 采样；cpu% = ΔCPU_time / Δwall（单核归一）；判据 **≤1%**（60s 均值）；**未聚焦或被遮挡时出帧 = 0 帧/10s** | RP-12；已决 AR-24 第 2 条 |

### 3.7 GPU 后端降级矩阵（与 ADR-0014 决策 4 一致）

| 等级 | 后端 | termai-render/gpu 行为 | 用户可见行为 | 门禁 |
| --- | --- | --- | --- | --- |
| **T0** | DX12 / Metal / Vulkan 1.3 | 全功能：多页 atlas（R8+RGBA）、连字、≥120Hz、Sixel/kitty graphics、模糊/阴影 | 无提示（正常） | 全部 §5 门禁**只在此判定** |
| **T1** | Vulkan（Win）/ OpenGL 4.5、EGL（Linux）/ OpenGL 4.1（macOS） | 网格与外壳完整；关 VRR/高刷优化与部分合成效果；atlas 格式走降级路径 | 状态栏后端徽章 + 「图形能力受限」，可点开 gpu-capability-report.json | **失败**（NON-GATING） |
| **T2** | WARP（Win）/ lavapipe（Linux）/ 自带 CPU 光栅（macOS：swash 栅格 + CPU 合成 + blit） | **禁用** Sixel/kitty/iTerm2 图形、模糊与阴影；帧率上限 60Hz；damage 增量重绘保留 | 明示「无 Sixel / 无图片协议」；图片位置显示占位提示 | **失败** |
| **T3** | 无 GPU 呈现加速 | 不加载 L2 WebView（纯原生文本面板，AR-02）；禁用全部图形协议与动画；启动给诊断包导出入口 | 明示「已进入安全模式」+ 诊断导出 | **失败** |

触发与恢复（ADR-0014 决策 4 唯一口径）：特性探测缺项 → T0→T1→T2→T3 顺延首个可用后端；DeviceLost/TDR → 重建 device + **重建 atlas（gen+1）目标 ≤2s**、会话不中断（sessiond 独立进程，AR-13）；同会话 10 分钟内 3 次 → 降一级 + 结构化事件 + 本地审计；T2 初始化/呈现失败 → T3。配置键 gpu.backend = auto|dx12|vulkan|metal|gl|warp|lavapipe|software|safe，手动值不可用时回退 auto 并提示，不静默忽略。WebGPU 不在网格热路径（AR-01）。

### 3.8 AR-23 第 6 条：软换行 = 视觉裁剪在网格模型中的落点

**结论落点：折行/裁剪是显示层映射（termai-render 的 VisualRowMap），网格层（termai-vt）不感知该开关。**

```rust
pub enum WrapMode { Fold /* 默认，AR-22 §4 */, Clip /* 软换行关，AR-23 §6 */ }
pub struct ScrollAnchor { pub logical_line: LineId, pub row_in_line: u16, pub mode: WrapMode }
pub struct VisualRowMap { rows: Vec<VisualRow>, total: u32, anchor: ScrollAnchor }
pub struct VisualRow { pub line: LineId, pub seg: u16, pub kind: VRowKind }
pub enum VRowKind { Head { tail_clipped: bool }, Cont, CursorSeg }
```

```text
build_vrm(anchor, mode, viewport_rows):
  // 逻辑行 = 由 LineFlags::WRAPPED 串起来的网格行链
  Fold: 每个逻辑行占 ceil(display_cols / cols) 个显示行（= 链长）
        续行绘制极轻行首标记（纯装饰：不进选区、不进复制、不被 AI 读取，spec02 C14）
  Clip: 每个逻辑行只占 1 个显示行
        若光标在该逻辑行 → 显示光标所在段（CursorSeg，K-11）；否则显示头段（Head{tail_clipped:true}）
        被隐藏的续行 **仍在网格中**：不删除、不改列数、不参与横向滚动
  两种模式都不产生横向滚动（AR-23 §6），都不改变 grid rev
```

复制语义（逐字节一致的落点，spec02 C14 / A20 / A34、UX-G17、UX-R12）：

```text
copy(range) 在 sessiond 侧、按网格逻辑行生成：
  for each logical_line in range:
     segs = wrapped_chain(logical_line)        // 网格层硬换行链，与 WrapMode 无关
     text = concat(seg.text_bytes)             // 不插入换行 / 空格 / 续行标记
     非末段按 used 修剪尾随空白；末段 trim_end（与既有终端复制语义一致）
  宽字符取 lead（cluster 原串），WIDE_TAIL 不重复贡献字符
  断言：copy_hash == 录制会话的期望字节哈希（RP-09）
```

三条可证伪的断言（CI 化，RP-08 / RP-09）：
1. set_wrap_mode(Fold ↔ Clip) 前后：**GridDelta 计数 = 0**、grid rev 不变、行 cells 逐字节不变。
2. 同一会话、同一选区，Fold 与 Clip 两态复制结果**逐字节相同**（spec02 A20/A34）。
3. Clip 态终端**不出现横向滚动条**（spec02 A34、UX-G17），且 ScrollAnchor 不产生横向偏移（K-11 不做横向平移）。

## 4. 接口与依赖

| 方向 | 对象 | 我需要谁给什么 | 我向谁承诺什么 |
| --- | --- | --- | --- |
| 上游 | termai-vt（01 内核/VT 篇） | LineRecord/Cell/ClusterTable 与不变量 I1–I5；**唯一列宽表**（K-04）；LineFlags::WRAPPED 语义；GridDelta/Damage/ScrollOp 结构 | 只读消费；不改 width 表；cluster 语义变更走 capability 协商 |
| 上游 | termai-ipc / termai-session（架构篇） | 共享内存数据通道、versioned GridSnapshot/GridDelta/ScrollOp、credits 与**显式溢出策略** | 生产者不阻塞；消费者按 rev 校验，缺口即请求全 snapshot；L2 重绘不触发 L0 全量 |
| 上游 | termai-core + tokens（DC-09） | 单一权威配置（font family/size、ligature、wrap mode、AA 预设、gpu.backend）与单调 revision；调色板（token → RGBA） | 配置变更 ≤1 帧内生效（AR-23 §3 跨窗口联动）；零硬编码色值（§5 门禁列）；主题切换不使 grid 失效 |
| 上游 | platform 层 | 窗口焦点/遮挡/最小化、显示器与 DPI scale、reduced-motion 标志 | 状态机（§3.6）只在 Active 出帧 |
| 下游 | ui-native / shell-bridge | FrameStats（阶段时间戳、miss 率、驱逐次数、present 计数）、命中测试 pixel↔(line,col)、BidiMap 反查 | 单元格几何、行高 1.25、对齐 ≤0.5px、软换行/裁剪的视觉映射与逻辑行映射 |
| 下游 | a11y（AR-20） | 逻辑序文本抽取 + 坐标映射（不经 VRM 语义） | raw 输出朗读可用；不承诺语义化 TUI |
| 下游 | plugin-host（AR-07/DC-39） | 声明式装饰输入：cell range + style token + z-order 枚举 | 装饰**永不**改变 grid 真相、不得越出声明 range、不得帧内任意绘制 |
| 下游 | 测试/CI（spec07） | 确定性伪 PTY、录制语料、参考光栅器、动画冻结 golden | 帧哈希可复现；bench-report.json 增列渲染指标【新增】 |
| 禁止 | — | 任何 AI SDK / 网络句柄 / JSON 热路径（AR-03、AR-04、§3.14 T1） | 本 crate 只依赖 termai-vt 与 termai-gpu（DC-21） |

## 5. 可验证验收

| ID | 判据 | 测量方法 / 语料 / 工具 | CI 落点 |
| --- | --- | --- | --- |
| RP-01 | key-to-photon（本地）**门禁 P99 ≤16ms**，目标 P99 ≤8ms / P50 ≤4ms | RM-C，T0；内置时序探针 ≥10⁵ 事件 + ≥1000fps 高速相机校验探针偏差 | cargo xtask bench --metric latency；nightly 全量 + PR 子集 |
| RP-02 | 4K@120Hz 网格帧时 **门禁 <8.3ms**、丢帧 <0.1% | RM-C（4K 锁 120Hz），全 damage 合成基准 ≥10⁴ 帧；帧时取 present 时间戳差（口径 OQ-RND-03） | bench --metric frame；nightly |
| RP-03 | 解析+渲染吞吐 **门禁 ≥500MB/s**（**headless（sessiond-only）口径**，AR-24 第 3 条） | RM-A，1GB 语料 cat 无回压，10 次中位数；headless（sessiond-only = 解析 + grid + damage，不含 shaping/present）+ UI 侧最终 grid 哈希与丢帧比对（测量定义见 kernel/06 §3.2） | 同 RP-02 |
| RP-04 | 空闲 RSS（1 万行）**≤120MB**；24h 斜率 **<1MB/h**；并断言 §3.2.3 各子预算【新增】 | RM-A，T0；稳定 60s 后 30 次采样（spec07 §3.8.2）；子预算以构建期计数导出 | 周度 + 发版前（L4/L8） |
| RP-05 | 网格对齐误差 **≤0.5px**（100/125/150/200% DPI） | RM-C；渲染用例断言 glyph 位图原点偏差（判据定义 OQ-RND-04） | PR |
| RP-06 | 视觉回归 diff **≤0.1%**，非白名单零变化 | 动画冻结 + prefers-reduced-motion（spec07 §3.4.4）；分平台 golden | design:check / nightly |
| RP-07 | cluster↔列映射 100% 一致 | Unicode GraphemeBreakTest + emoji-test.txt + EastAsianWidth + 多语言真实语料（≥10⁵ 行）；逐格快照比对参考实现 | PR（新增用例即门禁） |
| RP-08 | 切换 WrapMode 产生 **0 个 GridDelta**、rev 不变；开/关两态复制逐字节相同 | 录制会话回放 + 断言（spec02 A20/A34、UX-G17） | PR（阻断合并） |
| RP-09 | 复制逐字节一致（CJK 双宽、组合字符、ZWJ、WRAPPED 链、跨行选区） | 录制语料 → copy_hash == 期望字节哈希 | PR |
| RP-10 | atlas 驱逐正确性：上限压到 2 MiB 重跑 golden，帧哈希与默认预算**完全相同** | 确定性栅格化 + 强制驱逐用例 | PR |
| RP-11 | DPI 切换 / 跨屏迁移【新增】：切换 ≤1 帧内完成 ASCII 预取，全量重建 ≤300ms，热页 ≤2 组，内存不超上限 | RM-C，拖动窗口跨 100%↔200% 显示器；计数器断言 | nightly |
| RP-12 | 空闲 CPU **门禁 ≤1%**（单核归一，5×60s，60s 均值）；**未聚焦或被遮挡出帧 = 0 帧/10s**（AR-24 第 2 条，已生效门禁） | RM-A，T0，bench --metric power + 进程采样 | nightly |
| RP-13 | 降级矩阵逐项：T0 全功能；T1/T2/T3 能力边界与提示一致；**非 T0 一律 FAIL/NON-GATING** | 特性探测注入 + 强制 gpu.backend 各值；能力报告快照 | nightly（矩阵作业） |
| RP-14 | DeviceLost：≤2s 重建 device + atlas，会话不中断；10 分钟 3 次 → 降一级并写审计 | 注入 device lost（测试驱动） | nightly |
| RP-15 | 插件声明式装饰：装饰前后 grid 哈希一致、不越出声明 cell range、z-order 在枚举内 | 恶意装饰用例（越界/覆盖/超大 z） | PR + 安全套件 |
| RP-16 | bidi：行内视觉顺序与 UBA 参考一致；复制/AI 抽取恒为逻辑序；方块字符行抑制生效 | UBA 参考实现 + RTL 语料 + 抑制计数（E60 扩展套件） | nightly |

## 6. 风险与降级

| ID | 触发条件 | 降级行为 | 用户可见后果 |
| --- | --- | --- | --- |
| RV-01 | cluster↔列映射回归失败（宽字符/ZWJ/组合字符） | 该 cluster 退化为「按 base 字符宽度」渲染 + 登记差异（最小复现 + 豁免期限 + owner，HARNESS §8.1 要求） | 个别 emoji/组合字符可能 1 列偏移，写入已知问题清单 |
| RV-02 | GPU 特性缺失 / DeviceLost / TDR | 按 §3.7 逐级降级（T1→T2→T3），重建 atlas | 状态栏后端徽章 + 能力受限提示；T2 起无图形协议；T3 安全模式 |
| RV-03 | atlas 抖动（跨屏 DPI 频繁切换、大字号 + 彩色 emoji） | 双 scale 热页 + 页级 LRU；**禁止帧内驱逐本帧引用页** | 偶发单帧字形补齐延迟（不丢帧、不闪烁错字） |
| RV-04 | 滚动快路径与 damage 不一致（画面错位） | 运行时开关 scroll_fastpath=false → 回退全量重绘 + 结构化事件 | 滚动瞬间帧时上升，画面不出现错位 |
| RV-05 | Clip 模式被误读为「内容丢失」（spec02 UX-R17） | 复制仍取完整逻辑行；状态栏明示裁剪语义；快捷键临时切 Fold | 屏幕上看不到行尾，但复制与 AI 上下文完整 |
| RV-06 | 零出帧被误判为「卡死」 | 焦点恢复立即全 damage 重绘（≤1 帧）；镜像持续更新 | 切回窗口首帧即恢复；不丢内容 |
| RV-07 | 共享内存环溢出（UI 长时间不消费） | 丢弃 delta + 请求全 snapshot 重同步（AR-04 显式溢出策略） | 短暂重绘一次，无内容丢失（真相在 sessiond） |
| RV-08 | 字体热加载 / 回退链变化 | gen += 1 全量失效 + 单帧全 damage | 字体变化后首帧整体重绘一次 |

## 7. 被否决的方案与反方意见

| 被否决方案 | 反方最强理由 | 我方反驳 | 复议条件 |
| --- | --- | --- | --- |
| V-01 网格与 shaping 放 UI/WebView（单栈） | 迭代快、单栈无 seam、可复用 Web 远程客户端 | 违反 AR-01 硬边界；像素经 DOM/IPC 后 P99 与 IME 不可控 | ADR-0001 既有条件：任一平台 WebView 无法达到「P99 ≤16ms 且 IME 可修正」则启动面板原生化评估 |
| V-02 shaping 放 sessiond，向 UI 下行 glyph run | UI 变哑终端、多端 attach 像素级一致、无重复 shaping | 违反 K-02：内核须 headless 且不得依赖字体/DPI/GPU；glyph id 与字体版本绑定会把字体问题引入内核 | 若多端 attach 要求像素级一致，或 UI 侧 shaping 无法进帧预算（RP-02 连续两版失败） |
| V-03 启用 subpixel AA 提升 Windows 观感 | 系统终端观感明显更好，用户第一眼就能感知 | AR-14 明令不承诺：与透明背景、缩放、非整数 DPI 根本冲突 | 须走 AR 变更（TSC 2/3 + 90 天公示）；代价：atlas 内存 ×4、跨平台画质不一致、对齐判据重写 |
| V-04 软换行关闭时提供横向滚动 | 日志查看器语义更实用；「看不到行尾」是真实痛点（UX-R17） | AR-23 §6 明令不做横向滚动；横向视口会把终端网格变成日志查看器 | 复议需 UX-R17 触发率数据 + AR-23 变更；代价：网格语义与复制语义同时复杂化 |
| V-05 每 cell 直接存完整属性、不做 style-run | 实现简单，无 run 索引与展开的错误面 | 违反 DC-20；10k 行预算下内存放大约 1.5×，直接威胁 §5 RSS 门禁 | 若 StyleRun 的差分测试（I4）连续两个版本出现缺陷 |
| V-06 软换行开关触发 reflow（把显示结果写回网格） | 屏幕与内容一致、单屏信息量统一 | 违反 AR-23 §6 / spec02 C14「不改变字符内容、网格列数与复制结果」；破坏 A20 逐字节复制 | 无（与 AR-23 直接冲突）；仅允许「临时开启软换行查看」这一只读补偿 |
| V-07 用系统字体后端栅格化（DirectWrite/CoreText hinting） | 小字号清晰度更好、系统一致性更高 | ADR-0014 决策 3 明确栅格化统一 swash；AR-14 已放弃 hinting 与 subpixel | 需 AR-14 变更；代价：三平台画质分叉、视觉回归基线 ×3、对齐 ≤0.5px 难保 |
| V-08 每帧全量重绘（不做 damage） | 无 damage 一致性 bug，实现与调试成本最低 | 直接违反 §5 帧时门禁与 §4.2 规则 6 | 若 damage 一致性缺陷导致两个连续版本出现画面错误 |
| V-09 用 Mailbox/Immediate 换取更低延迟 | 省一整帧（约 8.3ms），对 P99 ≤8ms 目标有决定性帮助 | 撕裂与能耗代价；且门禁用 present 时间戳判定，撕裂会被视觉回归与高速相机捕获 | 若 Fifo 下 P99 目标连续两个发布周期不可达，且撕裂用例零失败 |
| V-10 允许超宽 glyph 溢出 cell（不缩放） | 字形保真、不做有损压缩 | 会遮挡邻格，破坏网格对齐 ≤0.5px 与逐格命中测试 | 若 fit_squeezed 在真实语料命中率 >1% 且用户可感知为缺陷 |

## 8. Open Questions

> OQ-RND-01 至 OQ-RND-04 原为**与 HARNESS 的差异/口径空白**，现均由 **AR-24** 裁决（保留编号以便追溯）；其余为本规格待裁决项。变更任何预算/门禁数值仍须按 AGENTS §5 走 RFC → ADR。

| ID | 问题 | 影响面 | 建议值 | 决策阶段 |
| --- | --- | --- | --- | --- |
| **OQ-RND-01**（**已决：AR-24 第 1 条**） | §5 门禁「空闲 RSS（1 万行）≤120MB」的进程范围与 WebView 是否计入 | 门禁判定、§3.2.3 全套子预算、WebView「默认不加载」的取舍 | **已决：核心进程组 = sessiond + 原生 UI ≤120MB；WebView 就绪后增量 ≤60MB、插件宿主空载 ≤80MB 独立记账、不计入核心组；且必须同时报告总内存（不允许只报核心组）**（AR-20 诚实原则）。§3.2.3 子预算按此口径 | **已决（AR-24）** |
| **OQ-RND-02**（**已决：AR-24 第 2 条**） | §5 缺少 CPU / 出帧门禁 | 功耗承诺、CI 指标集、低端机（RM-B）门槛 | **已决并生效为 §5 门禁：空闲 CPU ≤1%（聚焦、无输出、60s 均值，单核归一）；未聚焦或被遮挡时出帧 = 0 帧/10s**。功耗降级为**人工抽测目标（不进 CI）**，禁止文档中出现无测量方法的功耗承诺。测量与 §3.6 / RP-12 对齐 | **已决（AR-24）** |
| **OQ-RND-03**（**已决：AR-24 第 3 条**） | §5 帧时与吞吐的测量定义 | RP-02 / RP-03 可复现性、CI 判定、跨版本可比性 | **已决：帧时 = 连续两次 present 时间戳差**（GPU timestamp query 交叉校验，负载 = 全 damage 4K 网格）；**吞吐 = headless（sessiond-only）口径**（解析 + grid + damage，不含 shaping/present），UI 侧只断言最终 grid 哈希一致 + 丢帧 <0.1%；测量定义统一落在 kernel/06 §3.2 | **已决（AR-24）** |
| **OQ-RND-04**（**已决：AR-24 第 3 条**） | §5「网格对齐误差 ≤0.5px」的判据 | RP-05 可判定性、AR-14 两档画质验收 | **已决：max over drawn glyphs of abs(glyph_bitmap_origin − cell_box_origin) ≤ 0.5px**，在 100/125/150/200% DPI 分别判定（RM-C，golden 像素测量）；「柔和」档放宽为 glyph 边缘质心判据（两档各自留基线） | **已决（AR-24）** |
| OQ-RND-05 | Clip 模式的光标可见性语义（K-11「每逻辑行只显示光标所在段」）需 UX 签署；spec02 §3.12 只定义「右边界裁剪」与「复制完整」，未定义光标进入续行后的显示 | 输入可用性、spec02 §3.12 措辞、命中测试 | 建议采纳 K-11（一行/逻辑行 + 光标段优先），并在状态栏给「该行已裁剪」轻提示；替代方案（固定头行 + 横向平移）已被 AR-23 §6 排除 | P1（与 UX/设计系统联合裁决） |
| OQ-RND-06 | atlas 内存硬上限默认值（建议 24 MiB）与彩色 emoji 页是否允许出现在门禁跑（4 MiB/页 的 RGBA 页占比大） | §5 RSS 门禁余量、RP-04/RP-10/RP-11 | 建议 24 MiB + 彩色页 1 张；门禁跑启用彩色页（否则 emoji 视觉回归与 ADR-0014 M12-③ 无法覆盖） | P1 |
| OQ-RND-07 | GridSnapshot/GridDelta/ScrollOp 的 IDL 归属与字段集（core-dto）——本文件只给最小字段集 | termai-ipc 兼容窗口、headless/多端 attach、插件只读镜像 | 建议字段集即 §3.3 结构（rev 单调 + 显式 damage + 显式 scroll），由 core-dto 生成 Rust/TS 双端；破坏性变更走 capability | P1（与架构篇/IPC owner 联合） |
| OQ-RND-08 | bidi 范围：v1 是否开启行内视觉重排，还是只做「逻辑序 + 标记」 | E60 扩展套件、命中测试/选区、AR-20 边界 | 建议按 K-13 开启行内重排（方块/制表行抑制），跨行重排明确不做；若 E60 资源不足可先只做标记（P2 再开重排） | P2（ADR-0014 E60 时间线） |
| OQ-RND-09 | 跨屏迁移保留 2 组 scale 热页的内存代价 vs 抖动收益是否划算（大字号 × 2 scale × 彩色页可能撞上限） | RP-11、OQ-RND-06 上限、RV-03 | 建议保留 2 组 + 预算内 LRU 整组回收；若 RP-11 显示 2 组仍不足，改为「按显示器预取 ASCII 95 字形」的轻量策略 | P2 |

**8.1 与 HARNESS 的差异标注（不绕过，交 Orchestrator 裁决）**

1. **OQ-RND-01（已决 AR-24 第 1 条）**：§5 未定义 RSS 门禁的进程范围与 WebView 计入规则 → 已裁决为**核心进程组（sessiond + 原生 UI）≤120MB**，WebView 就绪后增量 ≤60MB、插件宿主空载 ≤80MB 独立记账，且必须报告总内存；§3.2.3 内部子预算按此口径复核，不再按「UI + sessiond 合计」口径重算。
2. **OQ-RND-02（已决 AR-24 第 2 条）**：§5 缺少 CPU/功耗门禁 → 已裁决：**空闲 CPU ≤1%** 与「**未聚焦/被遮挡出帧 = 0 帧/10s**」升为 §5 门禁（测量见 §3.6 / RP-12）；功耗降级为人工抽测目标，不进 CI。
3. **OQ-RND-03 / OQ-RND-04（已决 AR-24 第 3 条）**：§5 的两项门禁（4K 帧时、网格对齐 ≤0.5px）缺测量定义 → 已裁决为本文件 §3.5.3 / §3.6 与 kernel/06 §3.2 的判据（帧时 = present 时间戳差；吞吐 = headless（sessiond-only）；网格对齐 = glyph 位图原点 vs cell 原点），**门禁数值不变**（AR-19 两列口径不变）。

**自查记录**：八节齐全且顺序固定；引用编号均存在于 HARNESS（AR-01/02/03/04/14/18/19/20/22/23、DC-09/13/14/15/16/17/20/21/22/25、§4.2/§5/§8.1/§8.2）、docs/spec/02/03/07 与 ADR-0001/0012/0014/0016/0017；引用 §5 处均注明门禁列/目标列；4 条新增/空白项集中在 §8，未与 §5 并列；每条结论含可施工项（结构/算法/trait/状态迁移）与可验证项（语料/工具/判据/CI 落点）。
