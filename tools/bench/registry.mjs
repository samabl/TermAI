// tools/bench/registry.mjs
// Machine-readable registry of the "HARNESS section 5 metric -> measurement definition" mapping
// table that docs/spec/kernel/06-performance-methodology.md section 3.9 is required to maintain
// (AR-24 item 3 + the HARNESS section 5 "mapping requirement").
//
// tools/bench/check.mjs gate B1 re-parses HARNESS.md section 5 and gate B3 re-parses kernel/06
// section 3.9, then diffs both against this file. The registry is therefore a transcription that
// is verified against the documents on every run, not a second source of truth.
//
// Row count: H1..H19 = the 19 current HARNESS section 5 rows, in table order, no gaps, no
// duplicates. C1 is the out-of-table design-gate control (contrast ratio) and is deliberately NOT
// counted in the 19 (kernel/06 section 3.9 "coverage" note).

// family: OQ-PM-07 / AR-31 #9 grading (frame 5% / startup 3% / throughput 2% / RSS 1%).
//   - "exact"    = byte-equal / = 0 structural assertion (kernel/06 3.6 eps_self 0)
//   - "default"  = metrics OQ-PM-07 does not enumerate; kernel/06 3.1 base limit 2%
//   - "external" = not a machine gate at all (kernel/06 3.8)
// machine: which reference machine may judge it (ADR-0014 decision 1 / spec 07 3.8.1).
//   'none' means "no reference machine produces this number".
// governed: 'yes' | 'no' | 'partial' -- mirrors the last column of the kernel/06 3.9 table.
// gate: the numeric HARNESS section 5 release gate, or null when the row has no single machine
//       judgeable number here. gateOp mirrors the table's relational operator ('<=', '<', '=').
// harnessGateCell: the verbatim "release gate" cell text of the HARNESS section 5 row. Gate B1
//       compares it character-for-character with the parsed table cell.

export const SECTION5_MAPPING = [
  {
    id: 'H1',
    label: '冷启动到可输入',
    harnessGateCell: 'P95 ≤150ms',
    harnessTargetCell: '≤100ms',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 1 (spawn 前 → ready-sentinel 帧 present); machine binding §3.4 (RM-A)',
    carrier: 'G4 / L4；A-PM-04',
    governed: 'yes',
    family: 'startup',
    machine: 'RM-A',
    gate: 150,
    gateOp: '<=',
    target: 100,
    unit: 'ms',
    statistic: 'p95',
    apm: 'A-PM-04',
    notes: 'WARM profile gate (K-13); COLD-DISK is a non-gating observation.',
  },
  {
    id: 'H2',
    label: 'key-to-photon（本地）',
    harnessGateCell: 'P99 ≤16ms',
    harnessTargetCell: 'P99 ≤8ms（P50 ≤4ms）',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 2 + §3.3 (time chain t0–t9, camera-calibrated D and the conservative fallback); §3.4 (RM-C)',
    carrier: 'G4 / L4；A-PM-05',
    governed: 'yes',
    family: 'frame',
    machine: 'RM-C',
    gate: 16,
    gateOp: '<=',
    target: 8,
    unit: 'ms',
    statistic: 'p99',
    isTail: true,
    minSamplesPerRun: 100000,
    apm: 'A-PM-05',
    notes: 'tail metric: a Run needs >= 1e5 injections (K-03); without camera calibration only T_probe + D_max is reportable (K-14).',
  },
  {
    id: 'H3',
    label: 'PTY 层附加延迟（子指标，AR-30）',
    harnessGateCell: 'P99 ≤2ms',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.3 time chain segment t3 → t4 (sessiond writes PTY → echo readable); attribution sub-metric when end-to-end is over budget',
    carrier: 'G4 子指标；A-PM-05',
    governed: 'yes',
    family: 'frame',
    machine: 'RM-C',
    gate: 2,
    gateOp: '<=',
    target: null,
    unit: 'ms',
    statistic: 'p99',
    isTail: true,
    minSamplesPerRun: 100000,
    apm: 'A-PM-05',
    notes: 'AR-30 item 1 anchor = sessiond "input frame received -> PTY write returned" (no IPC, no render). kernel/06 §3.2 gives no separate Run definition for H3; this registry reads "tail" and the 1e5 sample floor from the shared injection stream of §3.3 -- flagged as a reading, not a spec quote.',
  },
  {
    id: 'H4',
    label: '4K @120Hz 网格帧时',
    harnessGateCell: '<8.3ms（丢帧 <0.1%）',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 3 (present timestamp difference); §3.4 (RM-C locked to 120Hz, VRR/HDR off)',
    carrier: 'G4；A-PM-06',
    governed: 'yes',
    family: 'frame',
    machine: 'RM-C',
    gate: 8.3,
    gateOp: '<',
    target: null,
    unit: 'ms',
    statistic: 'p99',
    isTail: false,
    minSamplesPerRun: 10000,
    apm: 'A-PM-06',
    notes: 'Run = >= 1e4 frames, so is_tail is false (the 1e5 floor is a key-to-photon rule, K-03). Dropped frames < 0.1% is a second, ratio-shaped assertion on the same run.',
  },
  {
    id: 'H5',
    label: '解析+渲染吞吐',
    harnessGateCell: '≥500MB/s',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 4 + §3.2 supplement 2 (headless = gate, end_to_end = non-gating attribution); K-09',
    carrier: 'G4；A-PM-07',
    governed: 'yes',
    family: 'throughput',
    machine: 'RM-A',
    gate: 500,
    gateOp: '>=',
    target: null,
    unit: 'MBps',
    statistic: 'median',
    apm: 'A-PM-07',
    notes: 'headless (sessiond-only) definition per AR-24 item 3; aggregated by byte-weighted harmonic mean; the UI-side final grid hash + dropped-frame assertion rides along.',
  },
  {
    id: 'H6',
    label: '空闲 RSS（1 万行）核心进程组 = sessiond + 原生 UI，不含 WebView 与插件宿主',
    harnessGateCell: '≤120MB',
    harnessTargetCell: '≤100MB（无 WebView 时）',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 5 + K-11 + §3.2 supplement 6 (rss.total must be reported alongside)',
    carrier: 'G4；A-PM-08',
    governed: 'yes',
    family: 'rss',
    machine: 'RM-A',
    gate: 120,
    gateOp: '<=',
    target: 100,
    unit: 'MiB',
    statistic: 'median',
    apm: 'A-PM-08',
    notes: 'AR-24 item 1: never report the core group alone; rss.total is mandatory (AR-20 honesty).',
  },
  {
    id: 'H7',
    label: 'AI 面板（WebView）就绪后的 RSS 增量（独立记账，不并入 120MB）',
    harnessGateCell: '≤60MB',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 7 (WebView-ready increment branch) + K-11 + §3.2 supplement 6',
    carrier: 'G4；A-PM-08',
    governed: 'yes',
    family: 'rss',
    machine: 'RM-A',
    gate: 60,
    gateOp: '<=',
    target: null,
    unit: 'MiB',
    statistic: 'median',
    apm: 'A-PM-08',
    notes: 'Separately accounted (AR-24 item 1 / DC-38); mixing it into the 120MB core baseline invalidates the report.',
  },
  {
    id: 'H8',
    label: '空闲 CPU（窗口聚焦、无输出）',
    harnessGateCell: '≤1%（RM-A，60s 均值）',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§5 A-PM-14 (criterion); §3.8 "power / efficiency" row (替代护栏 and the downgrade note)',
    carrier: 'G4；A-PM-14',
    governed: 'yes',
    family: 'default',
    machine: 'RM-A',
    gate: 1,
    gateOp: '<=',
    target: null,
    unit: 'pct',
    statistic: 'mean',
    apm: 'A-PM-14',
    notes: 'AR-24 item 2 added this as a gate. OQ-PM-07 does not enumerate it, so the MAD / eps_self family falls back to the kernel/06 3.1 base 2%. Power draw is a manual spot-check target and must not appear as a gate.',
  },
  {
    id: 'H9',
    label: '未聚焦 / 被遮挡时的出帧数',
    harnessGateCell: '= 0 帧/10s',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§5 A-PM-14 (criterion); §3.8 "power / efficiency" row',
    carrier: 'G4；A-PM-14',
    governed: 'yes',
    family: 'exact',
    machine: 'RM-A',
    gate: 0,
    gateOp: '=',
    target: null,
    unit: 'frames/10s',
    statistic: 'count',
    apm: 'A-PM-14',
    notes: 'Structural assertion (AR-24 item 2); a MEDIAN over a metric whose expected value is exactly 0 is degenerate, hence family "exact" (eps_self 0).',
  },
  {
    id: 'H10',
    label: '24h RSS 斜率',
    harnessGateCell: '<1MB/h',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 6 (Theil–Sen + 1h step detection) + K-12; §3.8 replacement guardrails',
    carrier: 'G4 / L8；A-PM-09',
    governed: 'yes',
    family: 'rss',
    machine: 'RM-A',
    gate: 1,
    gateOp: '<',
    target: null,
    unit: 'MB/h',
    statistic: 'theil-sen',
    apm: 'A-PM-09',
    notes: 'Cannot enter CI (24h exclusive machine time); §3.8 gives the PR-level proxy guardrails. Absolute residual MAD > 0.5MB is a second INCONCLUSIVE trigger in §3.2.',
  },
  {
    id: 'H11',
    label: '插件宿主空载',
    harnessGateCell: '≤80MB（仅启用插件时计入）',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 7 (plugin-host idle branch) + K-11 + §3.2 supplement 6',
    carrier: 'G4；A-PM-08',
    governed: 'yes',
    family: 'rss',
    machine: 'RM-A',
    gate: 80,
    gateOp: '<=',
    target: null,
    unit: 'MiB',
    statistic: 'median',
    apm: 'A-PM-08',
    notes: 'Counted only when plugins are enabled; independent accounting (DC-38).',
  },
  {
    id: 'H12',
    label: '输入字节等价（键盘 / 粘贴 / IME commit 全语料回放，逐字节比对）',
    harnessGateCell: '= 100%',
    harnessTargetCell: '—',
    owner: '语料 kernel/05；方法 kernel/06',
    measurementDefinition: 'method: §3.6 D0 byte-equality self-check (a scene rendered twice must produce an identical per-frame fingerprint; otherwise INVALID) + §3.2 supplement 5 ("freeze + self-check before comparison" is one contract); corpus: kernel/05 §5 IN-AC',
    carrier: 'G1 / G2',
    governed: 'partial',
    family: 'exact',
    machine: 'none',
    gate: 100,
    gateOp: '=',
    target: null,
    unit: 'pct',
    statistic: 'exact',
    apm: null,
    notes: 'Judgeable only in the L0 parser lane (AR-25 item 1). Gate B1/B3 record this row; the measurement itself belongs to kernel/05 and G1/G2. ADR-0029 D-6 defines governed="partial" as exactly this split (method from kernel/06, corpus and lane from kernel/05 + G1/G2), so this row is NOT one of the machine-free rows the value reader expects.',
  },
  {
    id: 'H13',
    label: '安装包',
    harnessGateCell: '<60MB',
    harnessTargetCell: '—',
    owner: 'kernel/06',
    measurementDefinition: '§3.2 row 8 (artifact bytes after compression / notarisation); artifact scope per §8 OQ-PM-01 (decided: AR-31 item 7)',
    carrier: '构建 job；A-PM-10',
    governed: 'yes',
    family: 'exact',
    machine: 'none',
    gate: 60,
    gateOp: '<',
    target: null,
    unit: 'MiB',
    statistic: 'exact',
    apm: 'A-PM-10',
    notes: 'Machine-independent build product: not a reference-machine gate, so evaluate() returns SKIP(NOT_MACHINE_GATE) for it. Per platform primary format (Windows MSI / macOS DMG universal2 / Linux AppImage); any one over budget is FAIL.',
  },
  {
    id: 'H14',
    label: 'AI 网关附加延迟',
    harnessGateCell: 'p95 <120ms / p99 <300ms',
    harnessTargetCell: '—',
    owner: '服务端 SLO（T6）',
    measurementDefinition: 'registered here only as a cannot-enter-CI item with a replacement guardrail: §3.8',
    carrier: '服务端埋点 + 滚动 7 天看板；非机器门禁',
    governed: 'no',
    family: 'external',
    machine: 'none',
    gate: null,
    gateOp: null,
    target: null,
    unit: 'ms',
    statistic: 'p95/p99',
    apm: null,
    notes: 'Server-side; not reproducible on a reference machine (kernel/06 §3.8).',
  },
  {
    id: 'H15',
    label: '诊断首 token',
    harnessGateCell: 'P95 <3s',
    harnessTargetCell: '—',
    owner: '服务端埋点（T6）',
    measurementDefinition: 'same as H14: §3.8',
    carrier: '在线埋点；非机器门禁',
    governed: 'no',
    family: 'external',
    machine: 'none',
    gate: null,
    gateOp: null,
    target: null,
    unit: 'ms',
    statistic: 'p95',
    apm: null,
    notes: 'Online instrumentation; not a machine gate.',
  },
  {
    id: 'H16',
    label: '控制面单 MAU 成本',
    harnessGateCell: '≤ $0.05/月',
    harnessTargetCell: '—',
    owner: '成本看板（T6/T5）',
    measurementDefinition: 'same as H14: §3.8; kept separate from the A-PM-12 CI-cost accounting',
    carrier: '月度成本看板；非机器门禁',
    governed: 'no',
    family: 'external',
    machine: 'none',
    gate: null,
    gateOp: null,
    target: null,
    unit: 'usd/mau/month',
    statistic: 'exact',
    apm: null,
    notes: 'Cost dashboard, not a build-time quantity.',
  },
  {
    id: 'H17',
    label: '视觉回归 diff',
    harnessGateCell: '≤0.1%',
    harnessTargetCell: '0',
    owner: 'tools/design-gates',
    measurementDefinition: 'methodology lives in spec 07 §3.4.4 (animation freeze + double-render self-check + baseline diff + diff bbox); this file only shares the common contract of §3.2 supplement 5. NOT under kernel/06 governance.',
    carrier: 'G3（spec 07 §3.4.1）、L7、RP-06、design:check',
    governed: 'no',
    family: 'exact',
    machine: 'none',
    gate: 0.1,
    gateOp: '<=',
    target: 0,
    unit: 'pct',
    statistic: 'ratio',
    apm: null,
    notes: 'Carrier is tools/design-gates; evaluate() returns SKIP(NOT_MACHINE_GATE).',
  },
  {
    id: 'H18',
    label: '硬编码色值',
    harnessGateCell: '= 0',
    harnessTargetCell: '—',
    owner: 'tools/tokens',
    measurementDefinition: 'tokens:check gate [5] + design static gate S8 (blocking by default); this file carries no measurement. NOT under kernel/06 governance.',
    carrier: 'G7（S8）、DC-09',
    governed: 'no',
    family: 'exact',
    machine: 'none',
    gate: 0,
    gateOp: '=',
    target: null,
    unit: 'count',
    statistic: 'exact',
    apm: null,
    notes: 'Carrier is tools/tokens; evaluate() returns SKIP(NOT_MACHINE_GATE).',
  },
  {
    id: 'H19',
    label: '网格对齐误差',
    harnessGateCell: '≤0.5px',
    harnessTargetCell: '—',
    owner: 'tools/design-gates（判据定义 kernel/03 RP-05）',
    measurementDefinition: 'RM-C golden-pixel measurement contract registered here: criterion = max abs(glyph_bitmap_origin - cell_box_origin) <= 0.5px at 100/125/150/200% DPI (AR-24 item 3 / kernel/03 OQ-RND-04); see §3.2 supplement 5 and §3.8. Gate judgement is NOT under kernel/06 governance.',
    carrier: 'G3 / G7；RP-05',
    governed: 'no',
    family: 'exact',
    machine: 'none',
    gate: 0.5,
    gateOp: '<=',
    target: null,
    unit: 'px',
    statistic: 'max-abs',
    apm: null,
    notes: 'AR-31 final supplement item 1: the measurement contract belongs to kernel/06, the gate verdict carrier belongs to tools/design-gates (G3 / RP-05).',
  },
];

// Out-of-table control: HARNESS section 5 has no contrast row, so C1 exists only for cross-checking
// the token gates and is excluded from the 19-row count (kernel/06 section 3.9).
export const OUT_OF_TABLE_CONTROLS = [
  {
    id: 'C1',
    label: '双主题对比度：正文 ≥4.5:1；官方主题目标 ≥7:1',
    owner: 'tools/tokens',
    carrier: 'tokens:check [3]（DC-13、AR-23 item 7）；G7 辅助',
    inSection5: false,
    governed: 'no',
  },
];

export const MAPPING_ROW_COUNT = SECTION5_MAPPING.length;          // must be 19
export const MAPPING_IDS = SECTION5_MAPPING.map(function (r) { return r.id; });
export const OUT_OF_TABLE_IDS = OUT_OF_TABLE_CONTROLS.map(function (r) { return r.id; });

export function findMapping(id) {
  for (const r of SECTION5_MAPPING) if (r.id === id) return r;
  return null;
}

// Integrity of the registry itself: contiguous H1..H19, no duplicates, no gaps, plus the
// out-of-table control that must NOT be counted. Gate B3 runs this and additionally diffs the
// registry against the kernel/06 section 3.9 table.
export function mappingIntegrity() {
  const errors = [];
  const expected = [];
  for (let i = 1; i <= 19; i++) expected.push('H' + i);
  const seen = {};
  for (const r of SECTION5_MAPPING) {
    if (seen[r.id]) errors.push({ code: 'MAPPING_DUPLICATE', id: r.id, message: 'mapping id ' + r.id + ' appears more than once' });
    seen[r.id] = true;
    if (expected.indexOf(r.id) < 0) errors.push({ code: 'MAPPING_UNEXPECTED_ID', id: r.id, message: 'mapping id ' + r.id + ' is outside H1..H19' });
    if (!r.owner) errors.push({ code: 'MAPPING_MISSING_OWNER', id: r.id, message: r.id + ' has no owner' });
    if (!r.carrier) errors.push({ code: 'MAPPING_MISSING_CARRIER', id: r.id, message: r.id + ' has no gate carrier' });
    if (!r.measurementDefinition) errors.push({ code: 'MAPPING_MISSING_DEFINITION', id: r.id, message: r.id + ' has no measurement definition' });
    if (['yes', 'no', 'partial'].indexOf(r.governed) < 0) errors.push({ code: 'MAPPING_BAD_GOVERNED', id: r.id, message: r.id + ' has governed=' + String(r.governed) });
  }
  const missing = expected.filter(function (id) { return !seen[id]; });
  for (const id of missing) errors.push({ code: 'MAPPING_GAP', id: id, message: 'mapping id ' + id + ' is missing (HARNESS section 5 coverage must be gapless)' });
  if (SECTION5_MAPPING.length !== 19) {
    errors.push({ code: 'MAPPING_ROW_COUNT', id: 'ALL', message: 'registry has ' + SECTION5_MAPPING.length + ' rows; HARNESS section 5 currently has 19' });
  }
  for (const c of OUT_OF_TABLE_CONTROLS) {
    if (seen[c.id]) errors.push({ code: 'MAPPING_CONTROL_COUNTED', id: c.id, message: 'out-of-table control ' + c.id + ' must not appear in the H1..H19 registry' });
  }
  return { ok: errors.length === 0, errors: errors, count: SECTION5_MAPPING.length, ids: MAPPING_IDS.slice(), expectedIds: expected };
}
