//! Append-only Session Log segments (kernel/04 section 3.2).
//!
//! WIRE LAYOUT (authoritative, see docs/plan/m0-spec-defects.md SD-03): explicit
//! little-endian, never repr(C).
//!   segment header, 64 bytes:
//!     0..8   magic "TMAILOG\0"        8..10  format_version u16
//!     10..12 min_reader_version u16   12..16 segment_id u32
//!     16..24 created_at_unix_ns u64   24..32 first_seq u64
//!     32..40 prev_segment_hash_prefix [u8;8]
//!     40..44 flags u32 (bit0 raw-ring, bit1 encrypted, bit2 sealed)
//!     44..48 header_crc32c u32 (covers bytes 0..44)
//!     48..64 reserved (zero)
//!   record frame:
//!     0..4 len u32 (payload only)  4..6 type u16  6..8 flags u16
//!     8..16 seq u64                16..24 ts_ns u64
//!     [payload] [crc32c u32 over bytes 4..24+len]
//!
//! Append-only: records are never updated in place, and a sealed segment refuses writes.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use termai_ipc::crc32c::Crc32c;

pub const SEGMENT_MAGIC: [u8; 8] = *b"TMAILOG\0";
pub const SEGMENT_HEADER_LEN: usize = 64;
pub const RECORD_HEADER_LEN: usize = 24;
/// Segment rolling threshold (kernel/04 section 3.2.4).
pub const SEGMENT_MAX_BYTES: usize = 8 << 20;

pub const SEG_FLAG_RAW_RING: u32 = 1 << 0;
pub const SEG_FLAG_ENCRYPTED: u32 = 1 << 1;
pub const SEG_FLAG_SEALED: u32 = 1 << 2;

pub const FORMAT_VERSION: u16 = 1;
pub const MIN_READER_VERSION: u16 = 1;

/// Record type ids. Published values are never reused (kernel/04 section 3.2.3).
pub mod ty {
    pub const PTY_OUT: u16 = 0x0001;
    pub const PTY_IN: u16 = 0x0002;
    pub const RESIZE: u16 = 0x0003;
    pub const CMD_START: u16 = 0x0010;
    pub const CMD_END: u16 = 0x0011;
    pub const CWD_CHANGE: u16 = 0x0012;
    pub const TITLE_CHANGE: u16 = 0x0013;
    pub const CONTEXT_EVENT: u16 = 0x0020;
    pub const STATE_CHANGE: u16 = 0x0030;
    pub const CHECKPOINT_REF: u16 = 0x0040;
    pub const LEASE_EVENT: u16 = 0x0050;
    pub const SUBSCRIPTION_DROP: u16 = 0x0060;
    pub const AUDIT_REF: u16 = 0x0070;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogError {
    Io(std::io::ErrorKind),
    BadMagic,
    UnsupportedVersion { found: u16, min_reader: u16 },
    HeaderCrcMismatch,
    RecordCrcMismatch { offset: u32 },
    Truncated { offset: u32 },
    UnknownRecordType(u16),
    Malformed(&'static str),
    Sealed,
    SegmentTooLarge,
    NoSegments,
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::Io(k) => write!(f, "log io error: {k:?}"),
            LogError::BadMagic => write!(f, "log segment magic mismatch"),
            LogError::UnsupportedVersion { found, min_reader } => {
                write!(f, "log version {found} needs reader >= {min_reader}")
            }
            LogError::HeaderCrcMismatch => write!(f, "log segment header crc mismatch"),
            LogError::RecordCrcMismatch { offset } => write!(f, "record crc mismatch at {offset}"),
            LogError::Truncated { offset } => write!(f, "record truncated at {offset}"),
            LogError::UnknownRecordType(t) => write!(f, "unknown record type {t:#06x}"),
            LogError::Malformed(why) => write!(f, "malformed record: {why}"),
            LogError::Sealed => write!(f, "segment is sealed"),
            LogError::SegmentTooLarge => write!(f, "segment exceeds the rolling threshold"),
            LogError::NoSegments => write!(f, "no log segments found"),
        }
    }
}

impl std::error::Error for LogError {}

impl From<std::io::Error> for LogError {
    fn from(e: std::io::Error) -> Self {
        LogError::Io(e.kind())
    }
}

/// Which producer wrote the input bytes (kernel/04 section 3.2.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Human = 0,
    Agent = 1,
    Plugin = 2,
}

impl Source {
    fn decode(v: u8) -> Result<Self, LogError> {
        match v {
            0 => Ok(Source::Human),
            1 => Ok(Source::Agent),
            2 => Ok(Source::Plugin),
            _ => Err(LogError::Malformed("source")),
        }
    }
}

/// Identifies where a record lives.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RecordId {
    pub segment_id: u32,
    pub seq: u64,
    pub offset: u32,
}

/// A logged record.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Record {
    /// Raw PTY output. This is the byte-exact truth (F0).
    PtyOut {
        pane: u16,
        bytes: Vec<u8>,
    },
    /// PTY input. Payload has NO plaintext bytes by default (audit + lease only).
    PtyIn {
        pane: u16,
        sha256: [u8; 32],
        len: u64,
        source: Source,
        lease_id: u64,
    },
    Resize {
        pane: u16,
        cols: u16,
        rows: u16,
        px_w: u16,
        px_h: u16,
    },
    CmdStart {
        pane: u16,
        cmd_id: u64,
        prompt_marker: u8,
        ts_ns: u64,
    },
    CmdEnd {
        pane: u16,
        cmd_id: u64,
        exit_code: i32,
        duration_ms: u64,
        cwd: String,
        confidence: u8,
    },
    CwdChange {
        pane: u16,
        cwd: String,
    },
    TitleChange {
        pane: u16,
        title: String,
    },
    ContextEvent {
        pane: u16,
        kind: u16,
        payload: Vec<u8>,
    },
    StateChange {
        from: u8,
        to: u8,
        reason: u8,
        exit_code: Option<i32>,
    },
    CheckpointRef {
        ckpt_id: u64,
        segment_id: u32,
        offset: u32,
        grid_digest: [u8; 32],
    },
    LeaseEvent {
        session: u128,
        from: u64,
        to: u64,
        action: u8,
        approver: u64,
    },
    SubscriptionDrop {
        sub_id: u64,
        class: u8,
        dropped_bytes: u64,
        policy: u8,
    },
    AuditRef {
        audit_seq: u64,
        prev_hash: [u8; 32],
        kind: u16,
    },
}

impl Record {
    #[must_use]
    pub const fn ty(&self) -> u16 {
        match self {
            Record::PtyOut { .. } => ty::PTY_OUT,
            Record::PtyIn { .. } => ty::PTY_IN,
            Record::Resize { .. } => ty::RESIZE,
            Record::CmdStart { .. } => ty::CMD_START,
            Record::CmdEnd { .. } => ty::CMD_END,
            Record::CwdChange { .. } => ty::CWD_CHANGE,
            Record::TitleChange { .. } => ty::TITLE_CHANGE,
            Record::ContextEvent { .. } => ty::CONTEXT_EVENT,
            Record::StateChange { .. } => ty::STATE_CHANGE,
            Record::CheckpointRef { .. } => ty::CHECKPOINT_REF,
            Record::LeaseEvent { .. } => ty::LEASE_EVENT,
            Record::SubscriptionDrop { .. } => ty::SUBSCRIPTION_DROP,
            Record::AuditRef { .. } => ty::AUDIT_REF,
        }
    }

    /// Retention class: true for the P1 volatile raw ring (default not persisted long term).
    #[must_use]
    pub const fn is_raw_ring(&self) -> bool {
        matches!(self, Record::PtyOut { .. })
    }

    fn encode_payload(&self) -> Vec<u8> {
        let mut e = Enc::default();
        match self {
            Record::PtyOut { pane, bytes } => {
                e.u16(*pane);
                e.raw(bytes);
            }
            Record::PtyIn {
                pane,
                sha256,
                len,
                source,
                lease_id,
            } => {
                e.u16(*pane);
                e.raw(sha256);
                e.u64(*len);
                e.u8(*source as u8);
                e.u64(*lease_id);
            }
            Record::Resize {
                pane,
                cols,
                rows,
                px_w,
                px_h,
            } => {
                e.u16(*pane);
                e.u16(*cols);
                e.u16(*rows);
                e.u16(*px_w);
                e.u16(*px_h);
            }
            Record::CmdStart {
                pane,
                cmd_id,
                prompt_marker,
                ts_ns,
            } => {
                e.u16(*pane);
                e.u64(*cmd_id);
                e.u8(*prompt_marker);
                e.u64(*ts_ns);
            }
            Record::CmdEnd {
                pane,
                cmd_id,
                exit_code,
                duration_ms,
                cwd,
                confidence,
            } => {
                e.u16(*pane);
                e.u64(*cmd_id);
                e.i32(*exit_code);
                e.u64(*duration_ms);
                e.str(cwd);
                e.u8(*confidence);
            }
            Record::CwdChange { pane, cwd } => {
                e.u16(*pane);
                e.str(cwd);
            }
            Record::TitleChange { pane, title } => {
                e.u16(*pane);
                e.str(title);
            }
            Record::ContextEvent {
                pane,
                kind,
                payload,
            } => {
                e.u16(*pane);
                e.u16(*kind);
                e.blob(payload);
            }
            Record::StateChange {
                from,
                to,
                reason,
                exit_code,
            } => {
                e.u8(*from);
                e.u8(*to);
                e.u8(*reason);
                match exit_code {
                    Some(c) => {
                        e.u8(1);
                        e.i32(*c);
                    }
                    None => {
                        e.u8(0);
                        e.i32(0);
                    }
                }
            }
            Record::CheckpointRef {
                ckpt_id,
                segment_id,
                offset,
                grid_digest,
            } => {
                e.u64(*ckpt_id);
                e.u32(*segment_id);
                e.u32(*offset);
                e.raw(grid_digest);
            }
            Record::LeaseEvent {
                session,
                from,
                to,
                action,
                approver,
            } => {
                e.u128(*session);
                e.u64(*from);
                e.u64(*to);
                e.u8(*action);
                e.u64(*approver);
            }
            Record::SubscriptionDrop {
                sub_id,
                class,
                dropped_bytes,
                policy,
            } => {
                e.u64(*sub_id);
                e.u8(*class);
                e.u64(*dropped_bytes);
                e.u8(*policy);
            }
            Record::AuditRef {
                audit_seq,
                prev_hash,
                kind,
            } => {
                e.u64(*audit_seq);
                e.raw(prev_hash);
                e.u16(*kind);
            }
        }
        e.buf
    }

    fn decode_payload(t: u16, buf: &[u8]) -> Result<Self, LogError> {
        let mut d = Dec { buf, pos: 0 };
        let rec = match t {
            ty::PTY_OUT => {
                let pane = d.u16()?;
                Record::PtyOut {
                    pane,
                    bytes: d.rest().to_vec(),
                }
            }
            ty::PTY_IN => Record::PtyIn {
                pane: d.u16()?,
                sha256: d.arr32()?,
                len: d.u64()?,
                source: Source::decode(d.u8()?)?,
                lease_id: d.u64()?,
            },
            ty::RESIZE => Record::Resize {
                pane: d.u16()?,
                cols: d.u16()?,
                rows: d.u16()?,
                px_w: d.u16()?,
                px_h: d.u16()?,
            },
            ty::CMD_START => Record::CmdStart {
                pane: d.u16()?,
                cmd_id: d.u64()?,
                prompt_marker: d.u8()?,
                ts_ns: d.u64()?,
            },
            ty::CMD_END => Record::CmdEnd {
                pane: d.u16()?,
                cmd_id: d.u64()?,
                exit_code: d.i32()?,
                duration_ms: d.u64()?,
                cwd: d.string()?,
                confidence: d.u8()?,
            },
            ty::CWD_CHANGE => Record::CwdChange {
                pane: d.u16()?,
                cwd: d.string()?,
            },
            ty::TITLE_CHANGE => Record::TitleChange {
                pane: d.u16()?,
                title: d.string()?,
            },
            ty::CONTEXT_EVENT => Record::ContextEvent {
                pane: d.u16()?,
                kind: d.u16()?,
                payload: d.blob()?,
            },
            ty::STATE_CHANGE => {
                let from = d.u8()?;
                let to = d.u8()?;
                let reason = d.u8()?;
                let has = d.u8()?;
                let code = d.i32()?;
                Record::StateChange {
                    from,
                    to,
                    reason,
                    exit_code: if has == 1 { Some(code) } else { None },
                }
            }
            ty::CHECKPOINT_REF => Record::CheckpointRef {
                ckpt_id: d.u64()?,
                segment_id: d.u32()?,
                offset: d.u32()?,
                grid_digest: d.arr32()?,
            },
            ty::LEASE_EVENT => Record::LeaseEvent {
                session: d.u128()?,
                from: d.u64()?,
                to: d.u64()?,
                action: d.u8()?,
                approver: d.u64()?,
            },
            ty::SUBSCRIPTION_DROP => Record::SubscriptionDrop {
                sub_id: d.u64()?,
                class: d.u8()?,
                dropped_bytes: d.u64()?,
                policy: d.u8()?,
            },
            ty::AUDIT_REF => Record::AuditRef {
                audit_seq: d.u64()?,
                prev_hash: d.arr32()?,
                kind: d.u16()?,
            },
            other => return Err(LogError::UnknownRecordType(other)),
        };
        Ok(rec)
    }
}

#[derive(Default)]
struct Enc {
    buf: Vec<u8>,
}

impl Enc {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u128(&mut self, v: u128) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn raw(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }
    fn blob(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.raw(v);
    }
    fn str(&mut self, v: &str) {
        let b = v.as_bytes();
        self.u32(b.len() as u32);
        self.raw(b);
    }
}

struct Dec<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Dec<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], LogError> {
        let end = self.pos.checked_add(n).ok_or(LogError::Malformed("len"))?;
        let s = self
            .buf
            .get(self.pos..end)
            .ok_or(LogError::Malformed("short payload"))?;
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, LogError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, LogError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, LogError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn u64(&mut self) -> Result<u64, LogError> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_le_bytes(a))
    }
    fn u128(&mut self) -> Result<u128, LogError> {
        let s = self.take(16)?;
        let mut a = [0u8; 16];
        a.copy_from_slice(s);
        Ok(u128::from_le_bytes(a))
    }
    fn i32(&mut self) -> Result<i32, LogError> {
        let s = self.take(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn arr32(&mut self) -> Result<[u8; 32], LogError> {
        let s = self.take(32)?;
        let mut a = [0u8; 32];
        a.copy_from_slice(s);
        Ok(a)
    }
    fn blob(&mut self) -> Result<Vec<u8>, LogError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn string(&mut self) -> Result<String, LogError> {
        let n = self.u32()? as usize;
        let s = self.take(n)?;
        String::from_utf8(s.to_vec()).map_err(|_| LogError::Malformed("utf8"))
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }
}

/// 64-byte segment header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SegmentHeader {
    pub format_version: u16,
    pub min_reader_version: u16,
    pub segment_id: u32,
    pub created_at_unix_ns: u64,
    pub first_seq: u64,
    pub prev_segment_hash_prefix: [u8; 8],
    pub flags: u32,
}

impl SegmentHeader {
    #[must_use]
    pub fn encode(&self) -> [u8; SEGMENT_HEADER_LEN] {
        let mut b = [0u8; SEGMENT_HEADER_LEN];
        b[0..8].copy_from_slice(&SEGMENT_MAGIC);
        b[8..10].copy_from_slice(&self.format_version.to_le_bytes());
        b[10..12].copy_from_slice(&self.min_reader_version.to_le_bytes());
        b[12..16].copy_from_slice(&self.segment_id.to_le_bytes());
        b[16..24].copy_from_slice(&self.created_at_unix_ns.to_le_bytes());
        b[24..32].copy_from_slice(&self.first_seq.to_le_bytes());
        b[32..40].copy_from_slice(&self.prev_segment_hash_prefix);
        b[40..44].copy_from_slice(&self.flags.to_le_bytes());
        let mut c = Crc32c::new();
        c.update(&b[0..44]);
        b[44..48].copy_from_slice(&c.finish().to_le_bytes());
        b
    }

    pub fn decode(b: &[u8; SEGMENT_HEADER_LEN]) -> Result<Self, LogError> {
        if b[0..8] != SEGMENT_MAGIC {
            return Err(LogError::BadMagic);
        }
        let mut c = Crc32c::new();
        c.update(&b[0..44]);
        let want = u32::from_le_bytes([b[44], b[45], b[46], b[47]]);
        if c.finish() != want {
            return Err(LogError::HeaderCrcMismatch);
        }
        let mut a8 = [0u8; 8];
        a8.copy_from_slice(&b[32..40]);
        let h = SegmentHeader {
            format_version: u16::from_le_bytes([b[8], b[9]]),
            min_reader_version: u16::from_le_bytes([b[10], b[11]]),
            segment_id: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            created_at_unix_ns: u64::from_le_bytes([
                b[16], b[17], b[18], b[19], b[20], b[21], b[22], b[23],
            ]),
            first_seq: u64::from_le_bytes([b[24], b[25], b[26], b[27], b[28], b[29], b[30], b[31]]),
            prev_segment_hash_prefix: a8,
            flags: u32::from_le_bytes([b[40], b[41], b[42], b[43]]),
        };
        h.validate()?;
        Ok(h)
    }

    pub fn validate(&self) -> Result<(), LogError> {
        if self.format_version < self.min_reader_version {
            return Err(LogError::Malformed("version ordering"));
        }
        if self.format_version > FORMAT_VERSION + 1 {
            return Err(LogError::UnsupportedVersion {
                found: self.format_version,
                min_reader: self.min_reader_version,
            });
        }
        Ok(())
    }

    #[must_use]
    pub const fn is_sealed(&self) -> bool {
        self.flags & SEG_FLAG_SEALED != 0
    }

    #[must_use]
    pub const fn is_raw_ring(&self) -> bool {
        self.flags & SEG_FLAG_RAW_RING != 0
    }
}

/// fsync policy (kernel/04 section 3.2.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlushMode {
    Buffered,
    /// fdatasync: used for CmdEnd / StateChange / LeaseEvent / CheckpointRef.
    FsyncData,
    FsyncFull,
}

/// Rolling policy (kernel/04 section 3.2.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RotationPolicy {
    pub max_bytes: usize,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            max_bytes: SEGMENT_MAX_BYTES,
        }
    }
}

/// Segment file name.
#[must_use]
pub fn segment_path(dir: &Path, segment_id: u32) -> PathBuf {
    dir.join(format!("segment-{segment_id:08}.tmalog"))
}

/// List segments in ascending id order.
pub fn list_segments(dir: &Path) -> Result<Vec<(u32, PathBuf)>, LogError> {
    let mut out: BTreeMap<u32, PathBuf> = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix("segment-") {
            if let Some(id) = rest.strip_suffix(".tmalog") {
                if let Ok(id) = id.parse::<u32>() {
                    out.insert(id, entry.path());
                }
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// BLAKE3 prefix used to chain segments (kernel/04 section 3.2.1).
#[must_use]
pub fn chain_prefix(segment_bytes: &[u8]) -> [u8; 8] {
    let h = blake3::hash(segment_bytes);
    let mut p = [0u8; 8];
    p.copy_from_slice(&h.as_bytes()[0..8]);
    p
}

/// sha256 for PtyIn records.
#[must_use]
pub fn sha256_of(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut a = [0u8; 32];
    a.copy_from_slice(&out);
    a
}

/// Append-only segment writer.
pub struct SegmentWriter {
    path: PathBuf,
    file: File,
    header: SegmentHeader,
    seq: u64,
    offset: usize,
    sealed: bool,
    dirty: usize,
}

impl std::fmt::Debug for SegmentWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SegmentWriter")
            .field("path", &self.path)
            .field("segment_id", &self.header.segment_id)
            .field("seq", &self.seq)
            .field("offset", &self.offset)
            .field("sealed", &self.sealed)
            .finish()
    }
}

impl SegmentWriter {
    /// Create a fresh segment.
    pub fn create(
        dir: &Path,
        segment_id: u32,
        first_seq: u64,
        prev_segment_hash_prefix: [u8; 8],
        created_at_unix_ns: u64,
        flags: u32,
    ) -> Result<Self, LogError> {
        std::fs::create_dir_all(dir)?;
        let path = segment_path(dir, segment_id);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        let header = SegmentHeader {
            format_version: FORMAT_VERSION,
            min_reader_version: MIN_READER_VERSION,
            segment_id,
            created_at_unix_ns,
            first_seq,
            prev_segment_hash_prefix,
            flags,
        };
        file.write_all(&header.encode())?;
        file.flush()?;
        Ok(Self {
            path,
            file,
            header,
            seq: first_seq,
            offset: SEGMENT_HEADER_LEN,
            sealed: false,
            dirty: 0,
        })
    }

    #[must_use]
    pub const fn segment_id(&self) -> u32 {
        self.header.segment_id
    }

    #[must_use]
    pub const fn bytes_written(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub const fn seq(&self) -> u64 {
        self.seq
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// True when the segment has reached the rolling threshold.
    #[must_use]
    pub fn needs_rotation(&self, policy: &RotationPolicy) -> bool {
        self.offset >= policy.max_bytes
    }

    pub fn seal(&mut self) -> Result<(), LogError> {
        self.header.flags |= SEG_FLAG_SEALED;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&self.header.encode())?;
        self.file.sync_data()?;
        self.sealed = true;
        Ok(())
    }

    /// Chain prefix of everything written so far (used as the next segment's prev prefix).
    pub fn chain_prefix(&mut self) -> Result<[u8; 8], LogError> {
        let mut f = File::open(&self.path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(chain_prefix(&buf))
    }

    /// Append one record. Persist-then-publish is the caller's duty.
    pub fn append(&mut self, rec: &Record, ts_ns: u64) -> Result<RecordId, LogError> {
        if self.sealed {
            return Err(LogError::Sealed);
        }
        let payload = rec.encode_payload();
        if payload.len() > SEGMENT_MAX_BYTES {
            return Err(LogError::SegmentTooLarge);
        }
        let mut hb = [0u8; RECORD_HEADER_LEN];
        hb[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        hb[4..6].copy_from_slice(&rec.ty().to_le_bytes());
        hb[6..8].copy_from_slice(&0u16.to_le_bytes());
        hb[8..16].copy_from_slice(&self.seq.to_le_bytes());
        hb[16..24].copy_from_slice(&ts_ns.to_le_bytes());
        let mut c = Crc32c::new();
        c.update(&hb[4..24]);
        c.update(&payload);
        let crc = c.finish();

        let at = self.offset;
        self.file.write_all(&hb)?;
        self.file.write_all(&payload)?;
        self.file.write_all(&crc.to_le_bytes())?;
        self.offset += RECORD_HEADER_LEN + payload.len() + 4;
        self.dirty += RECORD_HEADER_LEN + payload.len() + 4;
        let id = RecordId {
            segment_id: self.header.segment_id,
            seq: self.seq,
            offset: at as u32,
        };
        self.seq += 1;
        Ok(id)
    }

    pub fn flush(&mut self, mode: FlushMode) -> Result<(), LogError> {
        match mode {
            FlushMode::Buffered => {}
            FlushMode::FsyncData => {
                self.file.sync_data()?;
                self.dirty = 0;
            }
            FlushMode::FsyncFull => {
                self.file.sync_all()?;
                self.dirty = 0;
            }
        }
        Ok(())
    }

    /// Seal the current segment and open the next one. Returns the new segment id.
    pub fn rotate(
        &mut self,
        dir: &Path,
        policy: &RotationPolicy,
        now_ns: u64,
    ) -> Result<u32, LogError> {
        let prefix = self.chain_prefix()?;
        self.seal()?;
        let next_id = self.header.segment_id + 1;
        let next = SegmentWriter::create(
            dir,
            next_id,
            self.seq,
            prefix,
            now_ns,
            self.header.flags & !SEG_FLAG_SEALED,
        )?;
        let _ = policy;
        *self = next;
        Ok(next_id)
    }
}

/// One record as read back.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LoggedRecord {
    pub id: RecordId,
    pub ts_ns: u64,
    pub record: Record,
}

/// Result of reading a segment.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SegmentRead {
    pub header: SegmentHeader,
    pub records: Vec<LoggedRecord>,
    /// Trailing bytes were unusable. NEVER silent: the caller must surface it.
    pub tail_truncated: bool,
    /// Offset of the first CRC mismatch, when tail damage was detected.
    pub crc_mismatch_at: Option<u32>,
    pub last_valid_seq: Option<u64>,
}

/// Read a whole segment, stopping at the first damaged record.
pub fn read_segment(path: &Path) -> Result<SegmentRead, LogError> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < SEGMENT_HEADER_LEN {
        return Err(LogError::Truncated { offset: 0 });
    }
    let mut hb = [0u8; SEGMENT_HEADER_LEN];
    hb.copy_from_slice(&bytes[0..SEGMENT_HEADER_LEN]);
    let header = SegmentHeader::decode(&hb)?;

    let mut records = Vec::new();
    let mut pos = SEGMENT_HEADER_LEN;
    let mut tail_truncated = false;
    let mut crc_mismatch_at = None;

    while pos < bytes.len() {
        if bytes.len() - pos < RECORD_HEADER_LEN {
            tail_truncated = true;
            break;
        }
        let len = u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let t = u16::from_le_bytes([bytes[pos + 4], bytes[pos + 5]]);
        let seq = u64::from_le_bytes([
            bytes[pos + 8],
            bytes[pos + 9],
            bytes[pos + 10],
            bytes[pos + 11],
            bytes[pos + 12],
            bytes[pos + 13],
            bytes[pos + 14],
            bytes[pos + 15],
        ]);
        let ts_ns = u64::from_le_bytes([
            bytes[pos + 16],
            bytes[pos + 17],
            bytes[pos + 18],
            bytes[pos + 19],
            bytes[pos + 20],
            bytes[pos + 21],
            bytes[pos + 22],
            bytes[pos + 23],
        ]);
        if len > SEGMENT_MAX_BYTES {
            return Err(LogError::Malformed("record length exceeds segment cap"));
        }
        let total = RECORD_HEADER_LEN + len + 4;
        if bytes.len() - pos < total {
            tail_truncated = true;
            break;
        }
        let payload = &bytes[pos + RECORD_HEADER_LEN..pos + RECORD_HEADER_LEN + len];
        let want = u32::from_le_bytes([
            bytes[pos + RECORD_HEADER_LEN + len],
            bytes[pos + RECORD_HEADER_LEN + len + 1],
            bytes[pos + RECORD_HEADER_LEN + len + 2],
            bytes[pos + RECORD_HEADER_LEN + len + 3],
        ]);
        let mut c = Crc32c::new();
        c.update(&bytes[pos + 4..pos + RECORD_HEADER_LEN]);
        c.update(payload);
        if c.finish() != want {
            tail_truncated = true;
            crc_mismatch_at = Some(pos as u32);
            break;
        }
        let record = Record::decode_payload(t, payload)?;
        records.push(LoggedRecord {
            id: RecordId {
                segment_id: header.segment_id,
                seq,
                offset: pos as u32,
            },
            ts_ns,
            record,
        });
        pos += total;
    }

    let last_valid_seq = records.last().map(|r| r.id.seq);
    Ok(SegmentRead {
        header,
        records,
        tail_truncated,
        crc_mismatch_at,
        last_valid_seq,
    })
}

/// Why a requested tail-replay window cannot be satisfied.
///
/// AR-26 fixes the retention semantics: P0 metadata is durable but may be archived
/// (kernel/04 section 3.2.4 drops raw bytes past the segment budget) and the P1 raw byte
/// ring is volatile. A client asking for a range the Log can no longer serve must be told
/// so: a short replay that looks successful would fake screen consistency, which is the
/// opposite of E-P0-4 "screen 可恢复". \`from_seq\` is EXCLUSIVE; \`head\` is the
/// SegmentWriter's next free seq, i.e. the exclusive upper bound of appended records.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReplayGap {
    /// Records after \`from_seq\` up to the oldest readable record are gone.
    BelowWindow { first_available: u64 },
    /// The client claims to have applied seqs the server never produced.
    AheadOfHead { head: u64 },
    /// The segment tail is damaged, so a complete replay cannot be produced.
    TailUnreadable { last_valid: Option<u64>, head: u64 },
}

/// Decide whether \`(from_seq, head)\` can be replayed COMPLETELY from an already-read
/// segment. Returns the gap instead of a partial event list, so a caller cannot
/// accidentally serve a lying short replay (kernel/04 section 3.4 + AR-26).
pub fn replay_window_check(read: &SegmentRead, from_seq: u64, head: u64) -> Result<(), ReplayGap> {
    if from_seq > head {
        return Err(ReplayGap::AheadOfHead { head });
    }
    let lo = from_seq.saturating_add(1);
    if lo >= head {
        return Ok(()); // nothing is being requested
    }
    let first = read.records.first().map(|r| r.id.seq);
    let last = read.records.last().map(|r| r.id.seq);
    match (first, last) {
        // Seq values are handed out consecutively, so the readable set is a contiguous
        // range: completeness means it covers every seq in (from_seq, head).
        (Some(f), Some(l)) if f <= lo && l >= head - 1 => Ok(()),
        (Some(f), _) if f > lo => Err(ReplayGap::BelowWindow { first_available: f }),
        _ => Err(ReplayGap::TailUnreadable {
            last_valid: last,
            head,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "termai-log-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn all_records() -> Vec<Record> {
        vec![
            Record::PtyOut {
                pane: 0,
                bytes: vec![0x1b, b'[', b'3', b'1', b'm', b'h', b'i', 0x00, 0xff],
            },
            Record::PtyIn {
                pane: 0,
                sha256: sha256_of(b"ls"),
                len: 3,
                source: Source::Agent,
                lease_id: 9,
            },
            Record::Resize {
                pane: 0,
                cols: 120,
                rows: 40,
                px_w: 0,
                px_h: 0,
            },
            Record::CmdStart {
                pane: 0,
                cmd_id: 1,
                prompt_marker: b'A',
                ts_ns: 10,
            },
            Record::CmdEnd {
                pane: 0,
                cmd_id: 1,
                exit_code: 127,
                duration_ms: 42,
                cwd: "/tmp/x".to_string(),
                confidence: 2,
            },
            Record::CwdChange {
                pane: 0,
                cwd: "/home/u".to_string(),
            },
            Record::TitleChange {
                pane: 0,
                title: "term".to_string(),
            },
            Record::ContextEvent {
                pane: 0,
                kind: 7,
                payload: vec![1, 2, 3],
            },
            Record::StateChange {
                from: 1,
                to: 2,
                reason: 0,
                exit_code: None,
            },
            Record::CheckpointRef {
                ckpt_id: 5,
                segment_id: 1,
                offset: 64,
                grid_digest: [9u8; 32],
            },
            Record::LeaseEvent {
                session: 0xDEAD_BEEF,
                from: 1,
                to: 2,
                action: 3,
                approver: 4,
            },
            Record::SubscriptionDrop {
                sub_id: 11,
                class: 2,
                dropped_bytes: 4096,
                policy: 1,
            },
            Record::AuditRef {
                audit_seq: 77,
                prev_hash: [1u8; 32],
                kind: 9,
            },
        ]
    }

    // ---- tail replay window (AR-26 / kernel/04 section 3.4) ----

    fn seg_read(first_seq: u64, count: u64) -> SegmentRead {
        let records = (0..count)
            .map(|i| LoggedRecord {
                id: RecordId {
                    segment_id: 0,
                    seq: first_seq + i,
                    offset: 0,
                },
                ts_ns: i,
                record: Record::Resize {
                    pane: 0,
                    cols: 80,
                    rows: 24,
                    px_w: 0,
                    px_h: 0,
                },
            })
            .collect::<Vec<_>>();
        SegmentRead {
            header: SegmentHeader {
                format_version: FORMAT_VERSION,
                min_reader_version: MIN_READER_VERSION,
                segment_id: 0,
                created_at_unix_ns: 0,
                first_seq,
                prev_segment_hash_prefix: [0u8; 8],
                flags: 0,
            },
            last_valid_seq: records.last().map(|r| r.id.seq),
            records,
            tail_truncated: false,
            crc_mismatch_at: None,
        }
    }

    #[test]
    fn replay_window_accepts_exactly_the_readable_range() {
        let read = seg_read(5, 4); // seq 5..=8, head is 9.
        assert_eq!(replay_window_check(&read, 4, 9), Ok(()));
        assert_eq!(replay_window_check(&read, 5, 9), Ok(()));
        // At the head (or past the last record) there is simply nothing to replay.
        assert_eq!(replay_window_check(&read, 8, 9), Ok(()));
        assert_eq!(replay_window_check(&read, 9, 9), Ok(()));
        // An empty Log with no records ever appended is also satisfied.
        assert_eq!(replay_window_check(&seg_read(0, 0), 0, 0), Ok(()));
    }

    #[test]
    fn replay_window_refuses_a_floor_below_the_oldest_readable_record() {
        let read = seg_read(5, 4);
        assert_eq!(
            replay_window_check(&read, 0, 9),
            Err(ReplayGap::BelowWindow { first_available: 5 }),
            "the client needs seq 1..4, which are gone"
        );
        assert_eq!(
            replay_window_check(&read, 3, 9),
            Err(ReplayGap::BelowWindow { first_available: 5 })
        );
        // An empty Log cannot answer a non-empty request either.
        assert_eq!(
            replay_window_check(&seg_read(0, 0), 0, 3),
            Err(ReplayGap::TailUnreadable {
                last_valid: None,
                head: 3
            })
        );
    }

    #[test]
    fn replay_window_refuses_a_resume_point_ahead_of_the_server_head() {
        let read = seg_read(0, 3); // seq 0..=2, head is 3.
        assert_eq!(
            replay_window_check(&read, 4, 3),
            Err(ReplayGap::AheadOfHead { head: 3 }),
            "the client claims seqs the server never produced"
        );
    }

    #[test]
    fn replay_window_refuses_a_damaged_tail_that_the_request_needs() {
        let mut read = seg_read(0, 3); // readable 0..=2
        read.tail_truncated = true;
        read.last_valid_seq = Some(2);
        // The request reaches into the unreadable region (head 6, readable up to 2).
        assert_eq!(
            replay_window_check(&read, 0, 6),
            Err(ReplayGap::TailUnreadable {
                last_valid: Some(2),
                head: 6
            })
        );
        // A client already at the head needs nothing from the damaged region.
        assert_eq!(replay_window_check(&read, 5, 6), Ok(()));
    }

    #[test]
    fn every_record_type_round_trips_byte_exactly() {
        let recs = all_records();
        let mut seen = std::collections::BTreeSet::new();
        assert_eq!(recs.len(), 13, "all 13 record types must be covered");
        for r in &recs {
            let payload = r.encode_payload();
            let back = Record::decode_payload(r.ty(), &payload).unwrap();
            assert_eq!(&back, r, "roundtrip failed for type {:#06x}", r.ty());
            assert!(seen.insert(r.ty()), "duplicate type {:#06x}", r.ty());
        }
    }

    #[test]
    fn header_is_64_bytes_and_crc_guarded() {
        let h = SegmentHeader {
            format_version: FORMAT_VERSION,
            min_reader_version: MIN_READER_VERSION,
            segment_id: 3,
            created_at_unix_ns: 42,
            first_seq: 0,
            prev_segment_hash_prefix: [7u8; 8],
            flags: 0,
        };
        let b = h.encode();
        assert_eq!(b.len(), SEGMENT_HEADER_LEN);
        assert_eq!(SegmentHeader::decode(&b).unwrap(), h);
        let mut bad = b;
        bad[12] ^= 0xFF;
        assert_eq!(
            SegmentHeader::decode(&bad),
            Err(LogError::HeaderCrcMismatch)
        );
        let mut magic = b;
        magic[0] = b'X';
        assert_eq!(SegmentHeader::decode(&magic), Err(LogError::BadMagic));
    }

    #[test]
    fn write_then_read_preserves_order_and_bytes() {
        let dir = tmp_dir("roundtrip");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, SEG_FLAG_RAW_RING).unwrap();
        let recs = all_records();
        for (i, r) in recs.iter().enumerate() {
            let id = w.append(r, 100 + i as u64).unwrap();
            assert_eq!(id.seq, i as u64);
        }
        w.flush(FlushMode::FsyncData).unwrap();
        let read = read_segment(w.path()).unwrap();
        assert_eq!(read.records.len(), recs.len());
        assert!(!read.tail_truncated);
        assert_eq!(read.last_valid_seq, Some(12));
        for (a, b) in read.records.iter().zip(recs.iter()) {
            assert_eq!(&a.record, b);
        }
        assert!(read.header.is_raw_ring());
    }

    #[test]
    fn torn_tail_is_reported_not_silently_dropped() {
        let dir = tmp_dir("torn");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"first".to_vec(),
            },
            1,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"second".to_vec(),
            },
            2,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        let path = w.path().to_path_buf();
        drop(w);

        let full = std::fs::read(&path).unwrap();
        // Truncate inside the second record.
        std::fs::write(&path, &full[..full.len() - 3]).unwrap();
        let read = read_segment(&path).unwrap();
        assert_eq!(read.records.len(), 1);
        assert!(read.tail_truncated);
        assert_eq!(read.last_valid_seq, Some(0));
    }

    #[test]
    fn corrupted_record_is_detected_by_crc() {
        let dir = tmp_dir("crc");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"aaaa".to_vec(),
            },
            1,
        )
        .unwrap();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: b"bbbb".to_vec(),
            },
            2,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        let path = w.path().to_path_buf();
        drop(w);

        let mut bytes = std::fs::read(&path).unwrap();
        let n = bytes.len();
        bytes[n - 8] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();
        let read = read_segment(&path).unwrap();
        assert_eq!(read.records.len(), 1);
        assert!(read.tail_truncated);
        assert!(read.crc_mismatch_at.is_some());
    }

    #[test]
    fn sealed_segment_refuses_writes() {
        let dir = tmp_dir("sealed");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        w.append(
            &Record::TitleChange {
                pane: 0,
                title: "t".into(),
            },
            1,
        )
        .unwrap();
        w.seal().unwrap();
        assert!(w.is_sealed());
        assert_eq!(
            w.append(
                &Record::TitleChange {
                    pane: 0,
                    title: "u".into()
                },
                2
            ),
            Err(LogError::Sealed)
        );
        // The sealed flag round-trips through the header.
        let read = read_segment(w.path()).unwrap();
        assert!(read.header.is_sealed());
    }

    #[test]
    fn rotation_chains_segments_and_continues_seq() {
        let dir = tmp_dir("rotate");
        let policy = RotationPolicy { max_bytes: 200 };
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, 0).unwrap();
        for i in 0..20u8 {
            w.append(
                &Record::PtyOut {
                    pane: 0,
                    bytes: vec![i; 32],
                },
                u64::from(i),
            )
            .unwrap();
            if w.needs_rotation(&policy) {
                w.rotate(&dir, &policy, 1000).unwrap();
            }
        }
        w.flush(FlushMode::FsyncFull).unwrap();
        let segs = list_segments(&dir).unwrap();
        assert!(
            segs.len() >= 2,
            "expected rotation, got {} segment(s)",
            segs.len()
        );
        let mut seqs = Vec::new();
        for (_, p) in &segs {
            let r = read_segment(p).unwrap();
            for rec in r.records {
                seqs.push(rec.id.seq);
            }
        }
        let expected: Vec<u64> = (0..20).collect();
        assert_eq!(seqs, expected, "seq must be monotonic across segments");
    }

    #[test]
    fn pty_out_bytes_are_byte_exact_f0() {
        let dir = tmp_dir("f0");
        let mut w = SegmentWriter::create(&dir, 0, 0, [0u8; 8], 1, SEG_FLAG_RAW_RING).unwrap();
        let payload: Vec<u8> = (0u8..=255).collect();
        w.append(
            &Record::PtyOut {
                pane: 0,
                bytes: payload.clone(),
            },
            1,
        )
        .unwrap();
        w.flush(FlushMode::FsyncFull).unwrap();
        let read = read_segment(w.path()).unwrap();
        match &read.records[0].record {
            Record::PtyOut { bytes, .. } => assert_eq!(bytes, &payload),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn unknown_record_type_is_rejected() {
        assert_eq!(
            Record::decode_payload(0x00FF, &[]),
            Err(LogError::UnknownRecordType(0x00FF))
        );
    }
}
