//! Deterministic fuzz smoke for the IPC frame layer and the CBOR decoder (A12).
//!
//! Dependency-free by design: the PRNG is a local xorshift64* with fixed seeds, so
//! two runs of the same binary produce byte-identical corpora and byte-identical
//! results (no system time, no OS entropy, no `rand`). This is a smoke target that
//! proves "no panic on hostile input"; it is not a replacement for the 24h fuzz
//! campaign required by the merge gate (AGENTS.md section 4.6).
//!
//! Behaviours pinned by this file (read from `frame.rs` / `cbor.rs`):
//!   * `frame::decode` is zero-copy and never allocates: it returns `Ok(None)`
//!     (NeedMore) when the buffer is short, and rejects an oversize `len` before
//!     the "is the payload here?" branch.
//!   * `cbor::decode` (re-exported as `codec::from_bytes`) returns a typed
//!     `CborError` instead of panicking.

use termai_ipc::cbor::{self, CborError};
use termai_ipc::frame::{
    self, flag, DecodeCfg, FrameHeader, IpcError, FRAME_HEADER_LEN, MAX_FRAME_LEN, POD_MAX_LEN,
};

/// Iterations per random corpus (>= 20_000 as required).
const ITERS: usize = 25_000;
/// Longest random input handed to either decoder.
const MAX_INPUT: usize = 64;

/// xorshift64* (Marsaglia / Vigna). Fixed seeds only: this test must be replayable.
struct XorShift64Star(u64);

impl XorShift64Star {
    const fn new(seed: u64) -> Self {
        // The all-zero state is a fixed point; substitute a fixed non-zero seed.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform-ish value in `0..upper_exclusive`.
    fn next_range(&mut self, upper_exclusive: usize) -> usize {
        (self.next_u64() % upper_exclusive as u64) as usize
    }

    fn fill(&mut self, out: &mut [u8]) {
        for b in out {
            *b = self.next_u64() as u8;
        }
    }
}

const SEED_A: u64 = 0x0000_0000_0000_0001;
const SEED_B: u64 = 0xDEAD_BEEF_CAFE_F00D;
const SEED_C: u64 = 0x0123_4567_89AB_CDEF;

/// Determinism guard: the corpus must be reproducible from the fixed seeds alone.
/// If this drifts, the "same binary twice yields the same result" contract is broken.
#[test]
fn prng_stream_is_frozen() {
    let mut rng = XorShift64Star::new(SEED_A);
    assert_eq!(rng.next_u64(), 0x47E4_CE4B_896C_DD1D);
    assert_eq!(rng.next_u64(), 0xABCF_A6A8_E079_651D);
    assert_eq!(rng.next_u64(), 0xB9D1_0D8F_EB73_1F57);
}

/// Build a 24-byte header. `rsv` is forced to 0 so the length/CRC branches are
/// reached instead of the earlier `ReservedNonZero` short-circuit.
fn header_bytes(len: u32, flags: u16, corr_id: u64, crc32c: u32) -> [u8; FRAME_HEADER_LEN] {
    FrameHeader {
        len,
        ver: 0x0100,
        msg_type: 0x0100,
        flags,
        rsv: 0,
        corr_id,
        crc32c,
    }
    .encode()
}

/// Case A: arbitrary bytes never make the frame decoder panic.
#[test]
fn frame_decode_random_bytes_never_panics() {
    let mut rng = XorShift64Star::new(SEED_A);
    let mut buf = [0u8; MAX_INPUT];
    for i in 0..ITERS {
        let len = rng.next_range(MAX_INPUT + 1);
        rng.fill(&mut buf[..len]);
        if let Ok(Some(f)) = frame::decode(&buf[..len], DecodeCfg::verifying()) {
            assert_eq!(
                f.payload.len(),
                f.header.len as usize,
                "iteration {i}: payload length must equal header.len"
            );
        }
    }
}

/// Case B: exactly 24 random header bytes with an extreme `len`; the length guard
/// must fire before any allocation, and a short buffer must defer (never OOM).
#[test]
fn frame_decode_extreme_len_is_rejected_without_allocation() {
    let mut rng = XorShift64Star::new(SEED_B);

    // (1) len > MAX_FRAME_LEN: rejected by the length guard, before the
    //     "is the payload here yet?" check and before any allocation.
    for &len in &[
        MAX_FRAME_LEN as u32 + 1,
        1u32 << 24,
        i32::MAX as u32,
        u32::MAX,
    ] {
        let h = header_bytes(
            len,
            (rng.next_u32() as u16) & !flag::RESERVED_MASK,
            rng.next_u64(),
            rng.next_u32(),
        );
        assert_eq!(
            frame::decode(&h, DecodeCfg::verifying()),
            Err(IpcError::FrameTooLarge),
            "len={len} must be rejected as FrameTooLarge"
        );
    }

    // (2) len == 0: a full 24-byte header is already "complete", so the length guard
    //     lets it through and the CRC gate fires. Store a CRC that cannot match.
    let mut zero_len = header_bytes(0, flag::ENC, rng.next_u64(), 0);
    let crc = frame::frame_crc(&zero_len[0..20], &[]);
    zero_len[20..24].copy_from_slice(&(crc ^ 0xFFFF_FFFF).to_le_bytes());
    assert_eq!(
        frame::decode(&zero_len, DecodeCfg::verifying()),
        Err(IpcError::CrcMismatch),
        "len=0 with a wrong CRC must be CrcMismatch, not an allocation"
    );

    // (3) 0 < len <= MAX_FRAME_LEN on a 24-byte buffer: the rest of the frame simply
    //     is not here yet, so decode defers with NeedMore (Ok(None)); no allocation.
    for &len in &[
        1u32,
        4096,
        POD_MAX_LEN as u32 + 1,
        1 << 20,
        MAX_FRAME_LEN as u32,
    ] {
        let flags = if len as usize > POD_MAX_LEN {
            flag::ENC
        } else {
            0
        };
        let h = header_bytes(len, flags, rng.next_u64(), rng.next_u32());
        assert_eq!(
            frame::decode(&h, DecodeCfg::verifying()),
            Ok(None),
            "len={len} with a 24-byte buffer must defer, not allocate"
        );
    }
}

/// Case C: arbitrary bytes never make the CBOR decoder panic; malformed input is a
/// typed `Err`.
///
/// Note: a blanket "must be Err" assertion would be false. A single random byte such
/// as 0x00, 0x40, 0xA0 or 0xF6 is a complete CBOR value, so the corpus legitimately
/// contains both outcomes. The guarantees asserted here are: no panic, the decoder
/// rejects the malformed majority, and it accepts known-good input (not always-red).
#[test]
fn cbor_decode_random_bytes_never_panics() {
    let mut rng = XorShift64Star::new(SEED_C);
    let mut buf = [0u8; MAX_INPUT];
    let mut rejected = 0usize;
    let mut accepted = 0usize;
    for _ in 0..ITERS {
        let len = rng.next_range(MAX_INPUT + 1);
        rng.fill(&mut buf[..len]);
        match cbor::decode(&buf[..len]) {
            Ok(_) => accepted += 1,
            Err(_) => rejected += 1,
        }
    }
    assert_eq!(accepted + rejected, ITERS);
    assert!(rejected > 0, "corpus produced no rejected input");
    assert!(
        accepted > 0,
        "cbor::decode rejected every random input; decoder or corpus is suspect"
    );

    // Deterministic malformed / valid anchors keep this case non-vacuous.
    assert_eq!(cbor::decode(&[]), Err(CborError::Truncated));
    assert_eq!(
        cbor::decode(&[0x5F, 0x40, 0xFF]),
        Err(CborError::UnsupportedAdditional(31))
    );
    assert_eq!(cbor::decode(&[0x00]), Ok(cbor::Value::U64(0)));
}

/// Case D (positive control): truncating a real frame must report NeedMore
/// (`Ok(None)`), so a decoder that regressed to "always Ok(Some)" or "always Err"
/// fails this suite.
#[test]
fn truncated_valid_frame_reports_need_more() {
    let wire = frame::encode(0x0100, flag::ENC, 7, b"payload").unwrap();

    for n in 1..=FRAME_HEADER_LEN {
        assert_eq!(
            frame::decode(&wire[..n], DecodeCfg::verifying()),
            Ok(None),
            "valid frame truncated to {n} bytes must report NeedMore"
        );
    }

    // The untruncated frame must parse, so the suite is not always-green.
    let f = frame::decode(&wire, DecodeCfg::verifying())
        .unwrap()
        .unwrap();
    assert_eq!(f.payload, b"payload");
    assert_eq!(f.header.len as usize, 7);

    // A complete but corrupt frame must not silently pass either.
    let mut corrupt = wire.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 0xFF;
    assert_eq!(
        frame::decode(&corrupt, DecodeCfg::verifying()),
        Err(IpcError::CrcMismatch)
    );
}
