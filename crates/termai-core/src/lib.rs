//! `termai-core` — **叶子契约 crate**（kernel/07 §3.1、kernel/05 §4.1）。
//!
//! 硬边界（AGENTS §3 / AR-03，由依赖图在编译期兜底）：
//! 本 crate **不得**依赖 vt / pty / ipc / session / agent / ui / 网络。
//! 只有类型、纯函数与无 I/O 的编码器。
#![forbid(unsafe_code)]

pub mod capability;
pub mod error;
pub mod grid;
pub mod input;
pub mod risk;
pub mod time;

/// termai-ipc 协议版本（`0xMMmm`，kernel/07 §3.1）。
pub const PROTO_MAJOR: u16 = 0;
pub const PROTO_MINOR: u16 = 1;
/// `0xMMmm` 打包。
pub const PROTO_VERSION: u16 = (PROTO_MAJOR << 8) | PROTO_MINOR;

/// 会话标识（PTY Session，DC-03：与 Agent Session / Workspace **禁止混称**）。
///
/// 宽度取 **u128**：kernel/04 §4 规定 SessionId 为 **ULID**（128 位、字典序即时间序），
/// 这与 Log 的段内排序、attach 恢复与 audit 关联直接相关。本 M0 修正了先前的 u64 取值（见 SD-05）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SessionId(pub u128);

/// 窗格标识（DC-03：Workspace > Tab > Pane(≤8)）。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct PaneId(pub u16);

impl core::fmt::Display for SessionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "s{:032x}", self.0)
    }
}
