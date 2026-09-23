//! 跨分册错误码登记（kernel/07 §3.8）。**数值/字符串即契约，已发布者永不复用**。

/// `AuthzError` → `TERMAI-E-AUTHZ-{...}`（kernel/07 §3.8）。
pub mod authz {
    pub const MISSING: &str = "TERMAI-E-AUTHZ-MISSING";
    pub const EXPIRED: &str = "TERMAI-E-AUTHZ-EXPIRED";
    pub const REVOKED: &str = "TERMAI-E-AUTHZ-REVOKED";
    pub const SCOPE: &str = "TERMAI-E-AUTHZ-SCOPE";
    pub const TAINT: &str = "TERMAI-E-AUTHZ-TAINT";
    pub const APPROVAL: &str = "TERMAI-E-AUTHZ-APPROVAL";
    pub const POLICY: &str = "TERMAI-E-AUTHZ-POLICY";
    pub const MALFORMED: &str = "TERMAI-E-AUTHZ-MALFORMED";
}

/// `IpcError`（kernel/07 §3.1/§3.2）。帧层错误断连，消息层错误不断连。
pub mod ipc {
    pub const RESERVED_NONZERO: &str = "ReservedNonZero";
    pub const FRAME_TOO_LARGE: &str = "FrameTooLarge";
    pub const CRC_MISMATCH: &str = "CrcMismatch";
    pub const ENCODING_MISMATCH: &str = "EncodingMismatch";
    pub const CORRUPT: &str = "Corrupt";
    pub const NEED_MORE: &str = "NeedMore";
    pub const UNSUPPORTED_MSG: &str = "UnsupportedMsg";
    pub const RESERVED_MSG_TYPE: &str = "ReservedMsgType";
}

/// 握手失败 `reason`（kernel/07 §3.3）。
pub mod handshake {
    pub const VER_UNSUPPORTED: &str = "VerUnsupported";
    pub const CAP_UNKNOWN: &str = "CapUnknown";
    pub const PEER_DENIED: &str = "PeerDenied";
    pub const HANDSHAKE_REPLAY: &str = "HandshakeReplay";
    pub const HANDSHAKE_TIMEOUT: &str = "HandshakeTimeout";
}

/// Local API capability 拒绝码（kernel/07 §3.8）。
pub const JSONRPC_CAP_DENIED: i32 = -32030;

/// CLI 退出码（kernel/07 §3.3 表）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CliExit {
    Ok = 0,
    /// 握手超时，可重试（指数退避 ≤3 次）。
    HandshakeFailure = 2,
    /// 权限拒绝：peer 凭据不匹配 / nonce 重放。
    PermissionDenied = 3,
    /// 版本不兼容：禁止自行降到 proto_min 以下重试。
    VersionIncompatible = 4,
}

impl CliExit {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(authz::MISSING, "TERMAI-E-AUTHZ-MISSING");
        assert_eq!(CliExit::VersionIncompatible.code(), 4);
        assert_eq!(JSONRPC_CAP_DENIED, -32030);
    }
}
