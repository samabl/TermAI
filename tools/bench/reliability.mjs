// tools/bench/reliability.mjs
// Machine-readable registry of the HARNESS section 8.2 reliability timing measurement
// (sessiond rebuild P95 / P99) that docs/spec/kernel/06-performance-methodology.md section 3.10
// is required to carry (ADR-0029 decision D-4, closing docs/plan/p0-open-decisions.md D-4).
//
// This registry is DELIBERATELY INDEPENDENT of registry.mjs. Section 3.10 states the reason in the
// contract itself: SECTION5_MAPPING is the verbatim transcription of HARNESS section 5's nineteen
// rows and gates B1/B3 re-derive it from the documents on every run, so adding a reliability row
// to it would be a false transcription. The integrity rules below therefore define their own口径 --
// this file must NOT import or reuse the section-5 registry口径 (kernel/06 3.10, "the machine
// guarantee of registration separation").
//
// Gate B9 (tools/bench/check.mjs) re-derives the two thresholds (P95 2000 ms / P99 5000 ms) from
// the section 3.10 text on every run and diffs this file against the document, exactly as B1/B3
// do for section 5.
//
// Row count: R1 = P95, R2 = P99. Two rows, no gaps, no duplicates. The contract requires both to
// be reported ("两行都要报，不得只报 P95").
//
// family:  'frame'     -- kernel/06 3.10 fixes the tail family grade (OQ-PM-07 5% / eps_self 5%).
// machine: 'RM-A'      -- kernel/06 3.10 binds the rows exactly like section 5 (rebuild is CPU/IO
//                        timing); no RM-A -> NON_GATING / INCONCLUSIVE, never a gate.
// owner:   'kernel/06' -- section 3.10 owns the measurement definition.
// carrier: HARNESS section 8.2 reliability topic + AR-26 item 4.
// origin:  'reading'   -- section 3.2 gives no separate Run definition for this row; section 3.10
//                        derives it by the same tail rule and marks it a reading, not a quote.

export const RELIABILITY_MAPPING = [
  {
    id: 'R1',
    label: 'sessiond 重建 P95',
    metric: 'reliability.sessiond_rebuild.p95',
    gate: 2000,
    gateOp: '<=',
    unit: 'ms',
    statistic: 'p95',
    family: 'frame',
    machine: 'RM-A',
    owner: 'kernel/06',
    carrier: 'HARNESS §8.2 可靠性；AR-26 第 4 条',
    where: 'kernel/06 §3.10「门禁」行（P95 ≤ 2000ms）',
    measurementDefinition: 'kernel/06 §3.10「Run 定义」：一次 sessiond 重建（客户端发出重连 / attach 请求 → 收到可用的 GRID_SNAPSHOT 或覆盖窗口的 TAIL_REPLAY 且网格可交互）的服务端可观测段；机器绑定 §3.10「机器绑定」（RM-A）；自检 §3.10「自检」（§3.6 的 D0 场景冻结 + D1 双跑一致，eps_self 5%）先于门禁与基线比较，assertReproducible() 同样适用',
    origin: 'reading',
    notes: '§3.2 未给本行 Run 定义，本条按同一尾部规则推导并标 origin:reading（kernel/06 §3.10「Run 定义」）。无 RM-A → NON_GATING / INCONCLUSIVE，不得成为门禁。',
  },
  {
    id: 'R2',
    label: 'sessiond 重建 P99',
    metric: 'reliability.sessiond_rebuild.p99',
    gate: 5000,
    gateOp: '<=',
    unit: 'ms',
    statistic: 'p99',
    family: 'frame',
    machine: 'RM-A',
    owner: 'kernel/06',
    carrier: 'HARNESS §8.2 可靠性；AR-26 第 4 条',
    where: 'kernel/06 §3.10「门禁」行（P99 ≤ 5000ms）',
    measurementDefinition: 'same Run definition as R1（kernel/06 §3.10「Run 定义」）；尾部指标：Run 内样本数 ≥ 1e4 才报 P99，否则 INVALID(INSUFFICIENT_SAMPLES)（§3.1 第 7 步 / §3.10）；机器绑定 §3.10「机器绑定」（RM-A）；自检 §3.10「自检」',
    isTail: true,
    minSamplesPerRun: 10000,
    origin: 'reading',
    notes: '尾部指标：Run 内样本数 ≥1e4 才报 P99，否则 INVALID(INSUFFICIENT_SAMPLES)（kernel/06 §3.10「Run 定义」）。两行都要报，不得只报 P95。',
  },
];

// Independent integrity口径: contiguous R1..R2, no duplicates, exactly two rows, exactly one p95
// and one p99, and every row carries owner / carrier / measurement definition (kernel/06 3.10).
// The optional rows argument exists so gate B9's selftest can inject duplicates / gaps; it does
// NOT read the section-5 registry口径.
export function reliabilityIntegrity(rows) {
  const list = rows || RELIABILITY_MAPPING;
  const errors = [];
  const expectedIds = ['R1', 'R2'];
  const expectedStatistics = ['p95', 'p99'];
  const seen = {};
  for (const r of list) {
    if (seen[r.id]) errors.push({ code: 'RELIABILITY_DUPLICATE', id: r.id, message: 'reliability id ' + r.id + ' appears more than once' });
    seen[r.id] = true;
    if (expectedIds.indexOf(r.id) < 0) errors.push({ code: 'RELIABILITY_UNEXPECTED_ID', id: r.id, message: 'reliability id ' + r.id + ' is outside R1..R2' });
    if (!r.owner) errors.push({ code: 'RELIABILITY_MISSING_OWNER', id: r.id, message: r.id + ' has no owner' });
    if (!r.carrier) errors.push({ code: 'RELIABILITY_MISSING_CARRIER', id: r.id, message: r.id + ' has no gate carrier' });
    if (!r.measurementDefinition) errors.push({ code: 'RELIABILITY_MISSING_DEFINITION', id: r.id, message: r.id + ' has no measurement definition' });
    if (!r.where) errors.push({ code: 'RELIABILITY_MISSING_WHERE', id: r.id, message: r.id + ' has no kernel/06 section 3.10 anchor' });
    if (!r.metric || String(r.metric).indexOf('reliability.') !== 0) errors.push({ code: 'RELIABILITY_BAD_METRIC', id: r.id, message: r.id + ' has metric=' + String(r.metric) + '; section 3.10 requires a reliability. prefix' });
    if (!(typeof r.gate === 'number' && r.gate > 0)) errors.push({ code: 'RELIABILITY_BAD_GATE', id: r.id, message: r.id + ' has gate=' + String(r.gate) + '; a positive numeric threshold is required' });
    if (['<=', '<', '=', '>=', '>'].indexOf(r.gateOp) < 0) errors.push({ code: 'RELIABILITY_BAD_GATE_OP', id: r.id, message: r.id + ' has gateOp=' + String(r.gateOp) });
    if (!r.unit) errors.push({ code: 'RELIABILITY_MISSING_UNIT', id: r.id, message: r.id + ' has no unit' });
    if (!r.family) errors.push({ code: 'RELIABILITY_MISSING_FAMILY', id: r.id, message: r.id + ' has no metric family' });
    if (!r.machine) errors.push({ code: 'RELIABILITY_MISSING_MACHINE', id: r.id, message: r.id + ' has no machine binding' });
  }
  const missing = expectedIds.filter(function (id) { return !seen[id]; });
  for (const id of missing) errors.push({ code: 'RELIABILITY_GAP', id: id, message: 'reliability id ' + id + ' is missing; section 3.10 requires both P95 and P99' });
  if (list.length !== 2) errors.push({ code: 'RELIABILITY_ROW_COUNT', id: 'ALL', message: 'the reliability registry has ' + list.length + ' row(s); section 3.10 requires exactly 2 (p95 and p99)' });
  const stats = list.map(function (r) { return r.statistic; }).sort();
  if (stats.join(',') !== expectedStatistics.join(',')) errors.push({ code: 'RELIABILITY_STATISTIC_SET', id: 'ALL', message: 'statistics are [' + stats.join(', ') + ']; section 3.10 requires exactly p95 and p99' });
  return { ok: errors.length === 0, errors: errors, ids: list.map(function (r) { return r.id; }), expectedIds: expectedIds };
}

export function findReliability(id) {
  for (const r of RELIABILITY_MAPPING) if (r.id === id) return r;
  return null;
}
