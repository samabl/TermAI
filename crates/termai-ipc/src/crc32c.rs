//! CRC-32C (Castagnoli), mandatory on every frame (ADR-0018 / kernel/07 section 3.1).
//!
//! Implemented in-crate on purpose: no new link-boundary dependency is introduced
//! before it passes the ADR-0015 admission table.

const POLY_REFLECTED: u32 = 0x82F6_3B78;

const fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ POLY_REFLECTED
            } else {
                crc >> 1
            };
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = make_table();

/// Incremental CRC-32C.
#[derive(Clone, Copy, Debug)]
pub struct Crc32c {
    state: u32,
}

impl Default for Crc32c {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32c {
    #[must_use]
    pub const fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut state = self.state;
        for b in data {
            let idx = ((state ^ u32::from(*b)) & 0xFF) as usize;
            state = (state >> 8) ^ TABLE[idx];
        }
        self.state = state;
    }

    #[must_use]
    pub const fn finish(self) -> u32 {
        !self.state
    }
}

/// One-shot CRC-32C.
#[must_use]
pub fn crc32c(data: &[u8]) -> u32 {
    let mut c = Crc32c::new();
    c.update(data);
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_check_vector() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn known_vectors() {
        assert_eq!(crc32c(b""), 0x0000_0000);
        assert_eq!(crc32c(b"a"), 0xC1D0_4330);
        assert_eq!(crc32c(b"abc"), 0x364B_3FB7);
    }

    #[test]
    fn incremental_matches_one_shot() {
        let data = b"termai-ipc-frame-crc";
        let mut c = Crc32c::new();
        c.update(&data[..7]);
        c.update(&data[7..]);
        assert_eq!(c.finish(), crc32c(data));
    }
}
