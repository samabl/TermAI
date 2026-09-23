//! Transport-agnostic byte link.
//!
//! M0 transport = stdio (a child process pipe), which is portable and testable on
//! every platform. The UDS / named-pipe transport required by DC-24 is a P1 swap
//! behind this same trait; the frame layer above does not change.

use std::collections::VecDeque;

pub trait Link: Send {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()>;
}

/// In-memory duplex link used by the protocol tests.
#[derive(Default, Debug)]
pub struct MemoryLink {
    inbound: VecDeque<u8>,
    outbound: Vec<u8>,
}

impl MemoryLink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_inbound(&mut self, data: &[u8]) {
        self.inbound.extend(data.iter().copied());
    }

    #[must_use]
    pub fn outbound(&self) -> &[u8] {
        &self.outbound
    }

    pub fn take_outbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbound)
    }
}

impl Link for MemoryLink {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.inbound.len());
        for slot in buf.iter_mut().take(n) {
            *slot = self.inbound.pop_front().unwrap_or(0);
        }
        Ok(n)
    }

    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.outbound.extend_from_slice(data);
        Ok(())
    }
}

/// stdio link: reads stdin, writes stdout. Binary safe; never text-transcodes.
pub struct StdioLink {
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
}

impl Default for StdioLink {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioLink {
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
        }
    }
}

impl Link for StdioLink {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::Read;
        self.stdin.lock().read(buf)
    }

    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = self.stdout.lock();
        out.write_all(data)?;
        out.flush()
    }
}
