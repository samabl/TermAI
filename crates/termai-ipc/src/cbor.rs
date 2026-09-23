//! Minimal CBOR codec (RFC 8949 subset) for control / Context / audit frames.
//!
//! Implemented in-crate: no dependency enters the link boundary before it passes the
//! ADR-0015 admission table. Supported: null, bool, unsigned/negative int, byte
//! string, text string, array, map. Indefinite lengths are rejected: the kernel
//! contract requires definite lengths so that a frame can never be unterminated.

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    U64(u64),
    I64(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
}

impl Value {
    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::U64(v) => Some(*v),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m
                .iter()
                .find(|(k, _)| k.as_text() == Some(key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    /// Build a text-keyed map from a slice of pairs.
    #[must_use]
    pub fn map(entries: Vec<(&str, Value)>) -> Value {
        Value::Map(
            entries
                .into_iter()
                .map(|(k, v)| (Value::Text(k.to_string()), v))
                .collect(),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CborError {
    Truncated,
    UnsupportedAdditional(u8),
    UnsupportedMajor(u8),
    InvalidUtf8,
    TrailingBytes,
    DepthExceeded,
}

const MAX_DEPTH: u8 = 16;

fn write_head(out: &mut Vec<u8>, major: u8, value: u64) {
    let m = major << 5;
    if value < 24 {
        out.push(m | value as u8);
    } else if value <= u64::from(u8::MAX) {
        out.push(m | 24);
        out.push(value as u8);
    } else if value <= u64::from(u16::MAX) {
        out.push(m | 25);
        out.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value <= u64::from(u32::MAX) {
        out.push(m | 26);
        out.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

pub fn encode_into(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Null => out.push(0xF6),
        Value::Bool(false) => out.push(0xF4),
        Value::Bool(true) => out.push(0xF5),
        Value::U64(n) => write_head(out, 0, *n),
        Value::I64(n) => {
            if *n >= 0 {
                write_head(out, 0, *n as u64);
            } else {
                write_head(out, 1, (-1 - *n) as u64);
            }
        }
        Value::Bytes(b) => {
            write_head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            let bytes = s.as_bytes();
            write_head(out, 3, bytes.len() as u64);
            out.extend_from_slice(bytes);
        }
        Value::Array(items) => {
            write_head(out, 4, items.len() as u64);
            for i in items {
                encode_into(i, out);
            }
        }
        Value::Map(entries) => {
            write_head(out, 5, entries.len() as u64);
            for (k, val) in entries {
                encode_into(k, out);
                encode_into(val, out);
            }
        }
    }
}

#[must_use]
pub fn encode(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(v, &mut out);
    out
}

fn read_head(buf: &[u8], pos: &mut usize) -> Result<(u8, u64), CborError> {
    let first = *buf.get(*pos).ok_or(CborError::Truncated)?;
    *pos += 1;
    let major = first >> 5;
    let add = first & 0x1F;
    let value = match add {
        0..=23 => u64::from(add),
        24 => {
            let b = *buf.get(*pos).ok_or(CborError::Truncated)?;
            *pos += 1;
            u64::from(b)
        }
        25 => {
            let s = buf.get(*pos..*pos + 2).ok_or(CborError::Truncated)?;
            *pos += 2;
            u64::from(u16::from_be_bytes([s[0], s[1]]))
        }
        26 => {
            let s = buf.get(*pos..*pos + 4).ok_or(CborError::Truncated)?;
            *pos += 4;
            u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        }
        27 => {
            let s = buf.get(*pos..*pos + 8).ok_or(CborError::Truncated)?;
            *pos += 8;
            u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]])
        }
        other => return Err(CborError::UnsupportedAdditional(other)),
    };
    Ok((major, value))
}

fn decode_at(buf: &[u8], pos: &mut usize, depth: u8) -> Result<Value, CborError> {
    if depth > MAX_DEPTH {
        return Err(CborError::DepthExceeded);
    }
    let (major, value) = read_head(buf, pos)?;
    match major {
        0 => Ok(Value::U64(value)),
        1 => Ok(Value::I64(-1 - value as i64)),
        2 => {
            let n = value as usize;
            let s = buf.get(*pos..*pos + n).ok_or(CborError::Truncated)?;
            *pos += n;
            Ok(Value::Bytes(s.to_vec()))
        }
        3 => {
            let n = value as usize;
            let s = buf.get(*pos..*pos + n).ok_or(CborError::Truncated)?;
            *pos += n;
            let text = core::str::from_utf8(s).map_err(|_| CborError::InvalidUtf8)?;
            Ok(Value::Text(text.to_string()))
        }
        4 => {
            let n = value as usize;
            let mut items = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                items.push(decode_at(buf, pos, depth + 1)?);
            }
            Ok(Value::Array(items))
        }
        5 => {
            let n = value as usize;
            let mut entries = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let k = decode_at(buf, pos, depth + 1)?;
                let v = decode_at(buf, pos, depth + 1)?;
                entries.push((k, v));
            }
            Ok(Value::Map(entries))
        }
        6 => Err(CborError::UnsupportedMajor(6)),
        7 => match value {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 => Ok(Value::Null),
            _ => Err(CborError::UnsupportedMajor(7)),
        },
        other => Err(CborError::UnsupportedMajor(other)),
    }
}

/// Decode exactly one value; trailing bytes are an error.
pub fn decode(buf: &[u8]) -> Result<Value, CborError> {
    let mut pos = 0usize;
    let v = decode_at(buf, &mut pos, 0)?;
    if pos != buf.len() {
        return Err(CborError::TrailingBytes);
    }
    Ok(v)
}

/// Decode one value and report how many bytes it consumed.
pub fn decode_prefix(buf: &[u8]) -> Result<(Value, usize), CborError> {
    let mut pos = 0usize;
    let v = decode_at(buf, &mut pos, 0)?;
    Ok((v, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: Value) {
        let bytes = encode(&v);
        assert_eq!(decode(&bytes), Ok(v));
    }

    #[test]
    fn primitives_roundtrip() {
        roundtrip(Value::Null);
        roundtrip(Value::Bool(true));
        roundtrip(Value::Bool(false));
        roundtrip(Value::U64(0));
        roundtrip(Value::U64(23));
        roundtrip(Value::U64(24));
        roundtrip(Value::U64(256));
        roundtrip(Value::U64(70_000));
        roundtrip(Value::U64(u64::MAX));
        roundtrip(Value::I64(-1));
        roundtrip(Value::I64(-1000));
    }

    #[test]
    fn containers_roundtrip() {
        roundtrip(Value::Bytes(b"\x00\x01\xff".to_vec()));
        roundtrip(Value::Text("hello".to_string()));
        roundtrip(Value::Array(vec![Value::U64(1), Value::Text("a".into())]));
        roundtrip(Value::map(vec![
            ("ver", Value::U64(1)),
            ("kind", Value::Text("cli".into())),
        ]));
    }

    #[test]
    fn truncated_input_is_rejected() {
        let bytes = encode(&Value::Text("abcdef".to_string()));
        assert_eq!(decode(&bytes[..bytes.len() - 1]), Err(CborError::Truncated));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = encode(&Value::U64(1));
        bytes.push(0);
        assert_eq!(decode(&bytes), Err(CborError::TrailingBytes));
    }

    #[test]
    fn indefinite_length_is_rejected() {
        // 0x5F = bytes with additional info 31 (indefinite).
        assert_eq!(
            decode(&[0x5F, 0x40, 0xFF]),
            Err(CborError::UnsupportedAdditional(31))
        );
    }

    #[test]
    fn map_lookup_helpers() {
        let v = Value::map(vec![("a", Value::U64(7))]);
        assert_eq!(v.get("a").and_then(Value::as_u64), Some(7));
        assert_eq!(v.get("b"), None);
    }
}
