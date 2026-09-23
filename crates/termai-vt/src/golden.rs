//! TERMAI-GRID 1 golden snapshot format (kernel/01 section 3.7).
//!
//! Determinism rules: no timestamps, memory addresses, build hashes or random ids are
//! ever written. The hash line is not part of its own input; the hash covers the
//! header, meta, rows, attr and link lines through the end marker.

use std::collections::BTreeMap;
use std::fmt;

use termai_core::grid::{Cell, CellPos, Color, CursorState, GridSnapshot, LinkSpan};

use crate::grid::{color_name, flag_names, parse_color, parse_flags};

/// Error produced while parsing a golden document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoldenError {
    /// The first line was not TERMAI-GRID 1.
    BadHeader(String),
    /// The meta line was missing.
    MissingMeta,
    /// The meta line could not be parsed.
    BadMeta(String),
    /// A row line could not be parsed.
    BadRow(String),
    /// An attr line could not be parsed.
    BadAttr(String),
    /// A link line could not be parsed.
    BadLink(String),
    /// The end marker was missing.
    MissingEnd,
    /// The hash line was missing.
    MissingHash,
    /// The hash line was malformed.
    BadHash(String),
    /// The recomputed hash did not match the stored one.
    HashMismatch {
        /// Hash found in the document.
        stored: String,
        /// Hash recomputed over the body.
        computed: String,
    },
}

impl fmt::Display for GoldenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoldenError::BadHeader(value) => write!(f, "bad golden header: {value}"),
            GoldenError::MissingMeta => write!(f, "missing meta line"),
            GoldenError::BadMeta(value) => write!(f, "bad meta line: {value}"),
            GoldenError::BadRow(value) => write!(f, "bad row line: {value}"),
            GoldenError::BadAttr(value) => write!(f, "bad attr line: {value}"),
            GoldenError::BadLink(value) => write!(f, "bad link line: {value}"),
            GoldenError::MissingEnd => write!(f, "missing end marker"),
            GoldenError::MissingHash => write!(f, "missing hash line"),
            GoldenError::BadHash(value) => write!(f, "bad hash line: {value}"),
            GoldenError::HashMismatch { stored, computed } => {
                write!(f, "hash mismatch: stored {stored}, computed {computed}")
            }
        }
    }
}

impl std::error::Error for GoldenError {}

/// A parsed golden document.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GoldenDoc {
    /// The reconstructed snapshot.
    pub snapshot: GridSnapshot,
    /// The blake3 hash string (blake3:...).
    pub hash: String,
}

/// Hash canonical golden text: "blake3:<hex>".
#[must_use]
pub fn golden_hash(canonical: &str) -> String {
    let hash = blake3::hash(canonical.as_bytes());
    format!("blake3:{}", hash.to_hex())
}

/// Render a snapshot as TERMAI-GRID 1 text.
#[must_use]
pub fn write_golden(s: &GridSnapshot) -> String {
    let mut body = String::new();
    body.push_str("TERMAI-GRID 1\n");
    body.push_str(&format!(
        "meta {{\"cols\":{},\"rows\":{},\"cursor\":[{},{}],\"cursor_visible\":{},\"alt\":{},\"wrap\":{},\"origin\":{},\"modes\":\"0x{:016x}\",\"scroll\":[{},{}],\"scrollback\":{},\"title\":{},\"backend\":{}}}\n",
        s.cols,
        s.rows,
        s.cursor.pos.row,
        s.cursor.pos.col,
        s.cursor.visible,
        s.alt,
        s.wrap_pending,
        s.origin_mode,
        s.modes,
        s.scroll.0,
        s.scroll.1,
        s.scrollback_len,
        json_string(&s.title),
        json_string(&s.backend),
    ));
    for row in 0..s.rows {
        let cells = row_cells(s, row);
        body.push_str(&format!("row {row:04} {}\n", escape_row(&cells)));
        write_attr_lines(s, row, &mut body);
        for link in s.links.iter().filter(|link| link.row == row) {
            body.push_str(&format!(
                "link {row:04} {:04} {:04} id={} target={}\n",
                link.start_col,
                link.end_col,
                escape_token(&link.id),
                escape_token(&link.target),
            ));
        }
    }
    body.push_str("end\n");
    let hash = golden_hash(&body);
    format!("{body}hash {hash}\n")
}

/// Parse TERMAI-GRID 1 text.
pub fn parse_golden(text: &str) -> Result<GoldenDoc, GoldenError> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if lines.first().copied() != Some("TERMAI-GRID 1") {
        return Err(GoldenError::BadHeader(
            lines.first().copied().unwrap_or("").to_string(),
        ));
    }
    let mut snapshot = GridSnapshot::default();
    let mut hash: Option<String> = None;
    let mut hash_index: Option<usize> = None;
    let mut end_seen = false;
    let mut row_map: BTreeMap<u16, Vec<Cell>> = BTreeMap::new();
    let mut attrs: Vec<(u16, u16, u16, Color, Color, u16)> = Vec::new();
    let mut links: Vec<LinkSpan> = Vec::new();

    let mut meta_seen = false;
    for (index, line) in lines.iter().enumerate().skip(1) {
        if let Some(rest) = line.strip_prefix("meta ") {
            parse_meta(rest, &mut snapshot)?;
            meta_seen = true;
        } else if let Some(rest) = line.strip_prefix("row ") {
            let (row, cells) = parse_row(rest, snapshot.cols)?;
            row_map.insert(row, cells);
        } else if let Some(rest) = line.strip_prefix("attr ") {
            attrs.push(parse_attr(rest)?);
        } else if let Some(rest) = line.strip_prefix("link ") {
            links.push(parse_link(rest)?);
        } else if line == &"end" {
            end_seen = true;
        } else if let Some(rest) = line.strip_prefix("hash ") {
            hash = Some(rest.to_string());
            hash_index = Some(index);
            break;
        } else if line.is_empty() {
            continue;
        } else {
            return Err(GoldenError::BadMeta(line.to_string()));
        }
    }
    if !meta_seen {
        return Err(GoldenError::MissingMeta);
    }
    if !end_seen {
        return Err(GoldenError::MissingEnd);
    }
    let hash = hash.ok_or(GoldenError::MissingHash)?;
    if !hash.starts_with("blake3:") {
        return Err(GoldenError::BadHash(hash));
    }
    let hash_index = hash_index.unwrap_or(0);
    let mut body = lines.get(..hash_index).unwrap_or(&[]).join("\n");
    body.push('\n');
    let computed = golden_hash(&body);
    if computed != hash {
        return Err(GoldenError::HashMismatch {
            stored: hash,
            computed,
        });
    }

    let cols = snapshot.cols;
    let rows = snapshot.rows;
    if cols == 0 || rows == 0 {
        return Err(GoldenError::BadMeta(
            "cols/rows must be non-zero".to_string(),
        ));
    }
    let mut cells = vec![Cell::BLANK; usize::from(cols) * usize::from(rows)];
    for row in 0..rows {
        let mut row_cells = row_map.remove(&row).unwrap_or_default();
        row_cells.resize(usize::from(cols), Cell::BLANK);
        row_cells.truncate(usize::from(cols));
        for (r, start, end, fg, bg, flags) in &attrs {
            if *r != row {
                continue;
            }
            let mut col = *start;
            while col < *end {
                if let Some(cell) = row_cells.get_mut(usize::from(col)) {
                    cell.fg = *fg;
                    cell.bg = *bg;
                    cell.attrs = *flags;
                }
                col = col.saturating_add(1);
            }
        }
        let base = usize::from(row) * usize::from(cols);
        for (col, cell) in row_cells.into_iter().enumerate() {
            if let Some(slot) = cells.get_mut(base + col) {
                *slot = cell;
            }
        }
    }
    snapshot.cells = cells;
    // Rebuild the per-cell OSC 8 link index (Cell.link = span index + 1).
    for (index, link) in links.iter().enumerate() {
        let value = (index as u32) + 1;
        let mut col = link.start_col;
        while col < link.end_col {
            let at = usize::from(link.row) * usize::from(cols) + usize::from(col);
            if let Some(cell) = snapshot.cells.get_mut(at) {
                cell.link = value;
            }
            col = col.saturating_add(1);
        }
    }
    snapshot.links = links;
    Ok(GoldenDoc { snapshot, hash })
}

fn row_cells(s: &GridSnapshot, row: u16) -> Vec<Cell> {
    let mut out = Vec::with_capacity(usize::from(s.cols));
    for col in 0..s.cols {
        out.push(s.cell(row, col).copied().unwrap_or(Cell::BLANK));
    }
    out
}

fn write_attr_lines(s: &GridSnapshot, row: u16, out: &mut String) {
    let mut col = 0u16;
    while col < s.cols {
        let start = col;
        let first = s.cell(row, start).copied().unwrap_or(Cell::BLANK);
        let nondefault =
            first.fg != Color::Default || first.bg != Color::Default || first.attrs != 0;
        if !nondefault {
            col = col.saturating_add(1);
            continue;
        }
        let mut end = col.saturating_add(1);
        while end < s.cols {
            let cell = s.cell(row, end).copied().unwrap_or(Cell::BLANK);
            if cell.fg == first.fg && cell.bg == first.bg && cell.attrs == first.attrs {
                end = end.saturating_add(1);
            } else {
                break;
            }
        }
        out.push_str(&format!(
            "attr {row:04} {start:04} {end:04} fg={} bg={} flags={}\n",
            color_name(first.fg),
            color_name(first.bg),
            flag_names(first.attrs),
        ));
        col = end;
    }
}

fn escape_row(cells: &[Cell]) -> String {
    let mut out = String::new();
    for cell in cells {
        if cell.is_wide_continuation() {
            out.push_str("\\0");
        } else {
            out.push_str(&escape_char(cell.ch));
        }
    }
    out
}

fn escape_char(ch: char) -> String {
    match ch {
        '\\' => "\\\\".to_string(),
        _ => {
            let code = ch as u32;
            if code < 0x20 || code == 0x7F || (0x80..=0x9F).contains(&code) {
                format!("\\x{code:02x}")
            } else {
                ch.to_string()
            }
        }
    }
}

fn escape_token(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ' ' => out.push_str("\\x20"),
            _ => {
                let code = ch as u32;
                if code < 0x20 || code == 0x7F {
                    out.push_str(&format!("\\x{code:02x}"));
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out
}

fn unescape(text: &str) -> Result<Vec<char>, String> {
    let bytes = text.as_bytes();
    let mut out: Vec<char> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let rest = std::str::from_utf8(&bytes[i..]).map_err(|err| err.to_string())?;
            let ch = rest.chars().next().ok_or_else(|| "empty".to_string())?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let next = bytes
            .get(i + 1)
            .copied()
            .ok_or_else(|| "trailing backslash".to_string())?;
        match next {
            b'\\' => {
                out.push('\\');
                i += 2;
            }
            b'0' => {
                out.push('\0');
                i += 2;
            }
            b'x' => {
                let hi = bytes
                    .get(i + 2)
                    .copied()
                    .ok_or_else(|| "bad \\x".to_string())?;
                let lo = bytes
                    .get(i + 3)
                    .copied()
                    .ok_or_else(|| "bad \\x".to_string())?;
                let value = (hex_value(hi).ok_or_else(|| "bad \\x".to_string())? << 4)
                    | hex_value(lo).ok_or_else(|| "bad \\x".to_string())?;
                out.push(char::from(value));
                i += 4;
            }
            other => {
                return Err(format!("unknown escape \\{}", char::from(other)));
            }
        }
    }
    Ok(out)
}

fn unescape_token(text: &str) -> Result<String, String> {
    let chars = unescape(text)?;
    Ok(chars.into_iter().collect())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_row(rest: &str, cols: u16) -> Result<(u16, Vec<Cell>), GoldenError> {
    let (index, text) = match rest.split_once(' ') {
        Some((index, text)) => (index, text),
        None => (rest, ""),
    };
    let row = index
        .parse::<u16>()
        .map_err(|_| GoldenError::BadRow(rest.to_string()))?;
    let chars = unescape(text).map_err(GoldenError::BadRow)?;
    let mut cells: Vec<Cell> = Vec::with_capacity(chars.len());
    for ch in chars {
        if ch == '\0' {
            cells.push(Cell::WIDE_CONTINUATION);
        } else {
            cells.push(Cell { ch, ..Cell::BLANK });
        }
    }
    cells.resize(usize::from(cols), Cell::BLANK);
    Ok((row, cells))
}

#[allow(clippy::type_complexity)]
fn parse_attr(rest: &str) -> Result<(u16, u16, u16, Color, Color, u16), GoldenError> {
    let parts: Vec<&str> = rest.split(' ').collect();
    if parts.len() != 6 {
        return Err(GoldenError::BadAttr(rest.to_string()));
    }
    let row = parts[0]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadAttr(rest.to_string()))?;
    let start = parts[1]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadAttr(rest.to_string()))?;
    let end = parts[2]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadAttr(rest.to_string()))?;
    let fg = parts[3]
        .strip_prefix("fg=")
        .and_then(parse_color)
        .ok_or_else(|| GoldenError::BadAttr(rest.to_string()))?;
    let bg = parts[4]
        .strip_prefix("bg=")
        .and_then(parse_color)
        .ok_or_else(|| GoldenError::BadAttr(rest.to_string()))?;
    let flags = parts[5]
        .strip_prefix("flags=")
        .and_then(parse_flags)
        .ok_or_else(|| GoldenError::BadAttr(rest.to_string()))?;
    Ok((row, start, end, fg, bg, flags))
}

fn parse_link(rest: &str) -> Result<LinkSpan, GoldenError> {
    let parts: Vec<&str> = rest.split(' ').collect();
    if parts.len() != 5 {
        return Err(GoldenError::BadLink(rest.to_string()));
    }
    let row = parts[0]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadLink(rest.to_string()))?;
    let start_col = parts[1]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadLink(rest.to_string()))?;
    let end_col = parts[2]
        .parse::<u16>()
        .map_err(|_| GoldenError::BadLink(rest.to_string()))?;
    let id = parts[3]
        .strip_prefix("id=")
        .map(unescape_token)
        .transpose()
        .map_err(GoldenError::BadLink)?
        .ok_or_else(|| GoldenError::BadLink(rest.to_string()))?;
    let target = parts[4]
        .strip_prefix("target=")
        .map(unescape_token)
        .transpose()
        .map_err(GoldenError::BadLink)?
        .ok_or_else(|| GoldenError::BadLink(rest.to_string()))?;
    Ok(LinkSpan {
        row,
        start_col,
        end_col,
        id,
        target,
    })
}

fn parse_meta(rest: &str, snapshot: &mut GridSnapshot) -> Result<(), GoldenError> {
    let json = parse_json(rest).map_err(GoldenError::BadMeta)?;
    let cols = json_u64(&json, "cols").ok_or_else(|| GoldenError::BadMeta("cols".to_string()))?;
    let rows = json_u64(&json, "rows").ok_or_else(|| GoldenError::BadMeta("rows".to_string()))?;
    snapshot.cols = u16::try_from(cols).map_err(|_| GoldenError::BadMeta("cols".to_string()))?;
    snapshot.rows = u16::try_from(rows).map_err(|_| GoldenError::BadMeta("rows".to_string()))?;
    let cursor =
        json_arr(&json, "cursor").ok_or_else(|| GoldenError::BadMeta("cursor".to_string()))?;
    let cursor_row = cursor.first().and_then(Json::as_u64).unwrap_or(0);
    let cursor_col = cursor.get(1).and_then(Json::as_u64).unwrap_or(0);
    snapshot.cursor = CursorState {
        pos: CellPos {
            row: u16::try_from(cursor_row).unwrap_or(0),
            col: u16::try_from(cursor_col).unwrap_or(0),
        },
        visible: json_bool(&json, "cursor_visible").unwrap_or(true),
        style: 0,
    };
    snapshot.alt = json_bool(&json, "alt").unwrap_or(false);
    snapshot.wrap_pending = json_bool(&json, "wrap").unwrap_or(false);
    snapshot.origin_mode = json_bool(&json, "origin").unwrap_or(false);
    snapshot.modes = json_str(&json, "modes")
        .as_deref()
        .and_then(parse_mode_string)
        .unwrap_or(0);
    if let Some(scroll) = json_arr(&json, "scroll") {
        let top = scroll.first().and_then(Json::as_u64).unwrap_or(0);
        let bottom = scroll.get(1).and_then(Json::as_u64).unwrap_or(0);
        snapshot.scroll = (top as u32, bottom as u32);
    }
    snapshot.scrollback_len = json_u64(&json, "scrollback").unwrap_or(0) as u32;
    snapshot.title = json_str(&json, "title").unwrap_or_default();
    snapshot.backend = json_str(&json, "backend").unwrap_or_default();
    Ok(())
}

fn parse_mode_string(text: &str) -> Option<u64> {
    let hex = text.strip_prefix("0x").unwrap_or(text);
    u64::from_str_radix(hex, 16).ok()
}

fn json_string(text: &str) -> String {
    let mut out = String::from('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => {
                let code = ch as u32;
                if code < 0x20 {
                    out.push_str(&format!("\\u{code:04x}"));
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Minimal JSON reader for the meta line only.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(value) if *value >= 0.0 => Some(*value as u64),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(value) => Some(value),
            _ => None,
        }
    }
}

fn json_u64(root: &Json, key: &str) -> Option<u64> {
    root.get(key).and_then(Json::as_u64)
}

fn json_bool(root: &Json, key: &str) -> Option<bool> {
    match root.get(key) {
        Some(Json::Bool(value)) => Some(*value),
        _ => None,
    }
}

fn json_str(root: &Json, key: &str) -> Option<String> {
    root.get(key).and_then(Json::as_str).map(str::to_string)
}

fn json_arr(root: &Json, key: &str) -> Option<Vec<Json>> {
    match root.get(key) {
        Some(Json::Arr(items)) => Some(items.clone()),
        _ => None,
    }
}

fn parse_json(text: &str) -> Result<Json, String> {
    let mut parser = JsonParser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_ws();
    let value = parser.value()?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err("trailing data after JSON value".to_string());
    }
    Ok(value)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl JsonParser<'_> {
    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.bytes.get(self.pos).copied() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string(),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(_) => self.number(),
            None => Err("unexpected end of JSON".to_string()),
        }
    }

    fn literal(&mut self, text: &str, value: Json) -> Result<Json, String> {
        if self.bytes.get(self.pos..self.pos + text.len()) == Some(text.as_bytes()) {
            self.pos += text.len();
            Ok(value)
        } else {
            Err(format!("expected {text}"))
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut entries = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Json::Obj(entries));
        }
        loop {
            self.skip_ws();
            let key = match self.string()? {
                Json::Str(key) => key,
                _ => return Err("object key must be a string".to_string()),
            };
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err("expected ':'".to_string());
            }
            self.pos += 1;
            let value = self.value()?;
            entries.push((key, value));
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("expected ',' or '}'".to_string()),
            }
        }
        Ok(Json::Obj(entries))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err("expected ',' or ']'".to_string()),
            }
        }
        Ok(Json::Arr(items))
    }

    fn string(&mut self) -> Result<Json, String> {
        if self.bytes.get(self.pos) != Some(&b'"') {
            return Err("expected string".to_string());
        }
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let byte = *self.bytes.get(self.pos).ok_or("unterminated string")?;
            self.pos += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escape = *self.bytes.get(self.pos).ok_or("bad escape")?;
                    self.pos += 1;
                    match escape {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let code = self.unicode_escape()?;
                            let ch = char::from_u32(u32::from(code))
                                .ok_or_else(|| "bad unicode escape".to_string())?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err("unknown escape".to_string()),
                    }
                }
                _ => out.push(byte),
            }
        }
        Ok(Json::Str(String::from_utf8_lossy(&out).into_owned()))
    }

    fn unicode_escape(&mut self) -> Result<u16, String> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let byte = *self.bytes.get(self.pos).ok_or("bad unicode escape")?;
            self.pos += 1;
            let digit = hex_value(byte).ok_or("bad unicode escape")?;
            value = (value << 4) | u16::from(digit);
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while matches!(
            self.bytes.get(self.pos),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.pos).ok_or("bad number")?)
            .map_err(|err| err.to_string())?;
        text.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("bad number: {text}"))
    }
}
