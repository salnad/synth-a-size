//! JSON with the semantics of the nlohmann/json 3.1.2 library that upstream
//! uses for presets and wavetables (`vital/third_party/json/json.h`).
//!
//! Matching upstream's saved files byte for byte needs more than a generic
//! JSON crate:
//! - objects are `std::map`s, so keys are always sorted (bytewise);
//! - integers and floats are distinct number types, and an `f32` stored in
//!   JSON becomes a double, so `0.1f` saves as `0.10000000149011612`;
//! - doubles are printed with nlohmann's own Grisu2 formatter (see `dtoa`),
//!   which is not always the shortest representation;
//! - when parsing, duplicate keys keep the first value, unsigned integers,
//!   negative integers and floats stay distinct, and numbers that overflow
//!   their integer type fall back to doubles;
//! - reads through `operator[]` on a missing key give `null`, and
//!   converting `null` (or any other wrong type) to a number throws.
//!
//! [`Value::get`], [`Value::at`], [`Value::count`] and the `to_*` accessors
//! mirror those reads, returning [`JsonError`] where upstream throws.

mod dtoa;

use std::collections::BTreeMap;
use std::fmt;

/// A JSON value with nlohmann's type distinctions.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// `number_integer` (signed). Values built from C++ `int`s and parsed
    /// negative integers.
    Int(i64),
    /// `number_unsigned`. Parsed non-negative integers.
    UInt(u64),
    /// `number_float` (always a double).
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

/// An error where nlohmann would throw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError(pub String);

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JsonError {}

pub type Result<T> = std::result::Result<T, JsonError>;

static NULL: Value = Value::Null;

impl Value {
    /// An empty object (`json::object()`).
    pub fn object() -> Value {
        Value::Object(BTreeMap::new())
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Int(_) | Value::UInt(_) | Value::Float(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Int(_) | Value::UInt(_) | Value::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// `json::count(key)`: 1 if this is an object containing `key`, else 0.
    pub fn count(&self, key: &str) -> usize {
        match self {
            Value::Object(map) => usize::from(map.contains_key(key)),
            _ => 0,
        }
    }

    /// `json::contains`-style convenience for `count(key) != 0`.
    pub fn contains(&self, key: &str) -> bool {
        self.count(key) != 0
    }

    /// Read access as upstream's non-const `data[key]`: a missing key (or a
    /// `null` value) reads as `null`; any other non-object type throws.
    pub fn get(&self, key: &str) -> Result<&Value> {
        match self {
            Value::Object(map) => Ok(map.get(key).unwrap_or(&NULL)),
            Value::Null => Ok(&NULL),
            other => Err(JsonError(format!(
                "[json.exception.type_error.305] cannot use operator[] with {}",
                other.type_name()
            ))),
        }
    }

    /// Read access as upstream's non-const `data[index]`: indices past the
    /// end read as `null`; non-array types other than `null` throw.
    pub fn at(&self, index: usize) -> Result<&Value> {
        match self {
            Value::Array(items) => Ok(items.get(index).unwrap_or(&NULL)),
            Value::Null => Ok(&NULL),
            other => Err(JsonError(format!(
                "[json.exception.type_error.305] cannot use operator[] with {}",
                other.type_name()
            ))),
        }
    }

    /// `json::size()`.
    pub fn size(&self) -> usize {
        match self {
            Value::Null => 0,
            Value::Array(items) => items.len(),
            Value::Object(map) => map.len(),
            _ => 1,
        }
    }

    /// The elements visited by a range-for over this value: array items,
    /// object values in key order, nothing for `null`, and the value itself
    /// for primitives.
    pub fn iter_values(&self) -> Box<dyn Iterator<Item = &Value> + '_> {
        match self {
            Value::Null => Box::new(std::iter::empty()),
            Value::Array(items) => Box::new(items.iter()),
            Value::Object(map) => Box::new(map.values()),
            other => Box::new(std::iter::once(other)),
        }
    }

    fn number_error(&self) -> JsonError {
        JsonError(format!(
            "[json.exception.type_error.302] type must be number, but is {}",
            self.type_name()
        ))
    }

    /// `get<double>()`.
    pub fn to_f64(&self) -> Result<f64> {
        match *self {
            Value::Int(v) => Ok(v as f64),
            Value::UInt(v) => Ok(v as f64),
            Value::Float(v) => Ok(v),
            _ => Err(self.number_error()),
        }
    }

    /// `get<float>()`: `static_cast<float>` of the stored number.
    pub fn to_f32(&self) -> Result<f32> {
        match *self {
            Value::Int(v) => Ok(v as f32),
            Value::UInt(v) => Ok(v as f32),
            Value::Float(v) => Ok(v as f32),
            _ => Err(self.number_error()),
        }
    }

    /// `get<int>()`: `static_cast<int>` of the stored number (floats
    /// truncate toward zero).
    pub fn to_i32(&self) -> Result<i32> {
        match *self {
            Value::Int(v) => Ok(v as i32),
            Value::UInt(v) => Ok(v as i32),
            Value::Float(v) => Ok(v as i32),
            _ => Err(self.number_error()),
        }
    }

    /// `get<int64_t>()`.
    pub fn to_i64(&self) -> Result<i64> {
        match *self {
            Value::Int(v) => Ok(v),
            Value::UInt(v) => Ok(v as i64),
            Value::Float(v) => Ok(v as i64),
            _ => Err(self.number_error()),
        }
    }

    /// `get<bool>()`: only booleans convert.
    pub fn to_bool(&self) -> Result<bool> {
        match *self {
            Value::Bool(v) => Ok(v),
            _ => Err(JsonError(format!(
                "[json.exception.type_error.302] type must be boolean, but is {}",
                self.type_name()
            ))),
        }
    }

    /// `get<std::string>()`: only strings convert.
    pub fn to_str(&self) -> Result<&str> {
        match self {
            Value::String(v) => Ok(v),
            _ => Err(JsonError(format!(
                "[json.exception.type_error.302] type must be string, but is {}",
                self.type_name()
            ))),
        }
    }

    /// Write access as `data[key] = value`. `null` becomes an object first.
    ///
    /// # Panics
    /// If this value is neither `null` nor an object (nlohmann throws).
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        if self.is_null() {
            *self = Value::object();
        }
        match self {
            Value::Object(map) => {
                map.insert(key.to_string(), value.into());
            }
            other => panic!("cannot use operator[] with {}", other.type_name()),
        }
    }

    /// `push_back`. `null` becomes an array first.
    ///
    /// # Panics
    /// If this value is neither `null` nor an array (nlohmann throws).
    pub fn push(&mut self, value: impl Into<Value>) {
        if self.is_null() {
            *self = Value::Array(Vec::new());
        }
        match self {
            Value::Array(items) => items.push(value.into()),
            other => panic!("cannot use push_back() with {}", other.type_name()),
        }
    }

    /// `dump()`: compact serialization.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    /// `dump(indent)`: pretty serialization with `indent` spaces per level.
    pub fn dump_pretty(&self, indent: usize) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(indent), 0);
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>, current_indent: usize) {
        match self {
            Value::Object(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                match indent {
                    Some(step) => {
                        out.push_str("{\n");
                        let new_indent = current_indent + step;
                        let last = map.len() - 1;
                        for (i, (key, value)) in map.iter().enumerate() {
                            push_spaces(out, new_indent);
                            out.push('"');
                            write_escaped(out, key);
                            out.push_str("\": ");
                            value.write(out, indent, new_indent);
                            if i != last {
                                out.push_str(",\n");
                            }
                        }
                        out.push('\n');
                        push_spaces(out, current_indent);
                        out.push('}');
                    }
                    None => {
                        out.push('{');
                        for (i, (key, value)) in map.iter().enumerate() {
                            if i != 0 {
                                out.push(',');
                            }
                            out.push('"');
                            write_escaped(out, key);
                            out.push_str("\":");
                            value.write(out, None, current_indent);
                        }
                        out.push('}');
                    }
                }
            }
            Value::Array(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                match indent {
                    Some(step) => {
                        out.push_str("[\n");
                        let new_indent = current_indent + step;
                        let last = items.len() - 1;
                        for (i, value) in items.iter().enumerate() {
                            push_spaces(out, new_indent);
                            value.write(out, indent, new_indent);
                            if i != last {
                                out.push_str(",\n");
                            }
                        }
                        out.push('\n');
                        push_spaces(out, current_indent);
                        out.push(']');
                    }
                    None => {
                        out.push('[');
                        for (i, value) in items.iter().enumerate() {
                            if i != 0 {
                                out.push(',');
                            }
                            value.write(out, None, current_indent);
                        }
                        out.push(']');
                    }
                }
            }
            Value::String(s) => {
                out.push('"');
                write_escaped(out, s);
                out.push('"');
            }
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Int(v) => out.push_str(&v.to_string()),
            Value::UInt(v) => out.push_str(&v.to_string()),
            Value::Float(v) => dtoa::write_f64(out, *v),
            Value::Null => out.push_str("null"),
        }
    }
}

fn push_spaces(out: &mut String, count: usize) {
    out.extend(std::iter::repeat_n(' ', count));
}

fn write_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0C}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) <= 0x1F => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.dump())
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(i64::from(v))
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(f64::from(v))
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(v: BTreeMap<String, Value>) -> Self {
        Value::Object(v)
    }
}

/// Parses a complete JSON text (`json::parse`). Trailing non-whitespace is
/// an error.
pub fn parse(text: &str) -> Result<Value> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("expected end of input"));
    }
    Ok(value)
}

/// Parses JSON from bytes, rejecting invalid UTF-8 as nlohmann's lexer does.
pub fn parse_bytes(bytes: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(bytes).map_err(|e| {
        JsonError(format!(
            "[json.exception.parse_error.101] invalid UTF-8: {e}"
        ))
    })?;
    parse(text)
}

const MAX_DEPTH: usize = 512;

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> JsonError {
        JsonError(format!(
            "[json.exception.parse_error.101] parse error at {}: {message}",
            self.pos + 1
        ))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.pos += 1;
        }
    }

    fn expect_literal(&mut self, literal: &str, value: Value) -> Result<Value> {
        if self.bytes[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(self.error(&format!("invalid literal, expected '{literal}'")))
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<Value> {
        if depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                let mut map = BTreeMap::new();
                self.skip_whitespace();
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                loop {
                    self.skip_whitespace();
                    if self.peek() != Some(b'"') {
                        return Err(self.error("expected string literal"));
                    }
                    let key = self.parse_string()?;
                    self.skip_whitespace();
                    if self.peek() != Some(b':') {
                        return Err(self.error("expected ':'"));
                    }
                    self.pos += 1;
                    self.skip_whitespace();
                    let value = self.parse_value(depth + 1)?;
                    map.entry(key).or_insert(value);
                    self.skip_whitespace();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Value::Object(map));
                        }
                        _ => return Err(self.error("expected ',' or '}'")),
                    }
                }
            }
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_whitespace();
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    self.skip_whitespace();
                    items.push(self.parse_value(depth + 1)?);
                    self.skip_whitespace();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Value::Array(items));
                        }
                        _ => return Err(self.error("expected ',' or ']'")),
                    }
                }
            }
            Some(b'"') => Ok(Value::String(self.parse_string()?)),
            Some(b't') => self.expect_literal("true", Value::Bool(true)),
            Some(b'f') => self.expect_literal("false", Value::Bool(false)),
            Some(b'n') => self.expect_literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err(self.error("unexpected token")),
        }
    }

    fn parse_hex4(&mut self) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(c @ b'0'..=b'9') => u32::from(c - b'0'),
                Some(c @ b'a'..=b'f') => u32::from(c - b'a' + 10),
                Some(c @ b'A'..=b'F') => u32::from(c - b'A' + 10),
                _ => return Err(self.error("'\\u' must be followed by 4 hex digits")),
            };
            value = value * 16 + digit;
            self.pos += 1;
        }
        Ok(value)
    }

    fn parse_string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(self.error("missing closing quote"));
            };
            self.pos += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err(self.error("invalid escape"));
                    };
                    self.pos += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let first = self.parse_hex4()?;
                            let codepoint = if (0xD800..=0xDBFF).contains(&first) {
                                if self.bytes[self.pos..].starts_with(b"\\u") {
                                    self.pos += 2;
                                    let second = self.parse_hex4()?;
                                    if !(0xDC00..=0xDFFF).contains(&second) {
                                        return Err(self.error(
                                            "surrogate U+DC00..U+DFFF must follow U+D800..U+DBFF",
                                        ));
                                    }
                                    ((first - 0xD800) << 10) + (second - 0xDC00) + 0x10000
                                } else {
                                    return Err(self.error(
                                        "surrogate U+DC00..U+DFFF must follow U+D800..U+DBFF",
                                    ));
                                }
                            } else if (0xDC00..=0xDFFF).contains(&first) {
                                return Err(self
                                    .error("surrogate U+DC00..U+DFFF must follow U+D800..U+DBFF"));
                            } else {
                                first
                            };
                            let ch = char::from_u32(codepoint)
                                .ok_or_else(|| self.error("invalid code point"))?;
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                0x00..=0x1F => return Err(self.error("control character must be escaped")),
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| self.error("invalid UTF-8"))
    }

    fn parse_number(&mut self) -> Result<Value> {
        let start = self.pos;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                while let Some(b'0'..=b'9') = self.peek() {
                    self.pos += 1;
                }
            }
            _ => return Err(self.error("invalid number")),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number; expected digit after '.'"));
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }
        if let Some(b'e' | b'E') = self.peek() {
            is_float = true;
            self.pos += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error("invalid number; expected digit after exponent"));
            }
            while let Some(b'0'..=b'9') = self.peek() {
                self.pos += 1;
            }
        }

        let text = std::str::from_utf8(&self.bytes[start..self.pos]).expect("ascii number");
        if !is_float {
            if negative {
                if let Ok(v) = text.parse::<i64>() {
                    return Ok(Value::Int(v));
                }
            } else if let Ok(v) = text.parse::<u64>() {
                return Ok(Value::UInt(v));
            }
        }
        let value: f64 = text.parse().map_err(|_| self.error("invalid number"))?;
        if !value.is_finite() {
            return Err(JsonError(format!(
                "[json.exception.out_of_range.406] number overflow parsing '{text}'"
            )));
        }
        Ok(Value::Float(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sorted_and_numbers_typed() {
        let v = parse(r#"{"b": 1, "a": -2, "c": 0.5, "d": [true, null, "x"], "e": 1e2}"#).unwrap();
        assert_eq!(
            v.dump(),
            r#"{"a":-2,"b":1,"c":0.5,"d":[true,null,"x"],"e":100.0}"#
        );
        assert_eq!(v.get("b").unwrap(), &Value::UInt(1));
        assert_eq!(v.get("a").unwrap(), &Value::Int(-2));
    }

    #[test]
    fn duplicate_keys_keep_first() {
        let v = parse(r#"{"a": 1, "a": 2}"#).unwrap();
        assert_eq!(v.dump(), r#"{"a":1}"#);
    }

    #[test]
    fn integer_overflow_falls_back_to_float() {
        let v = parse("[18446744073709551616, -9223372036854775809, -0]").unwrap();
        assert_eq!(
            v.dump(),
            "[1.8446744073709552e+19,-9.223372036854776e+18,0]"
        );
    }

    #[test]
    fn missing_keys_read_as_null() {
        let v = parse(r#"{"a": 1}"#).unwrap();
        assert!(v.get("missing").unwrap().is_null());
        assert!(v.get("missing").unwrap().to_f32().is_err());
        assert!(v.get("a").unwrap().get("x").is_err());
        assert_eq!(v.count("a"), 1);
        assert_eq!(v.count("b"), 0);
    }

    #[test]
    fn conversions_follow_static_cast() {
        assert_eq!(Value::Float(2.9).to_i32().unwrap(), 2);
        assert_eq!(Value::Float(-2.9).to_i32().unwrap(), -2);
        assert_eq!(Value::UInt(3).to_f32().unwrap(), 3.0);
        assert!(Value::Int(1).to_bool().is_err());
        assert!(Value::Bool(true).to_f32().is_err());
    }

    #[test]
    fn floats_from_f32_save_as_doubles() {
        let mut v = Value::Null;
        v.set("x", 0.1f32);
        v.set("n", 3);
        v.set("s", "q\"\n\u{1}é");
        assert_eq!(
            v.dump(),
            "{\"n\":3,\"s\":\"q\\\"\\n\\u0001é\",\"x\":0.10000000149011612}"
        );
    }

    #[test]
    fn pretty_dump() {
        let v = parse(r#"{"a":[1,2],"b":{}}"#).unwrap();
        assert_eq!(
            v.dump_pretty(2),
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {}\n}"
        );
    }

    #[test]
    fn rejects_invalid_text() {
        for text in [
            "",
            "01",
            "1.",
            "-",
            "[1,]",
            "{\"a\" 1}",
            "\"\u{1}\"",
            "1e999",
            "[1] x",
            "\"\\ud800\"",
        ] {
            assert!(parse(text).is_err(), "{text:?} should fail");
        }
    }

    #[test]
    fn unicode_escapes_decode() {
        let v = parse(r#""\u00e9\ud83d\ude00\/""#).unwrap();
        assert_eq!(v.to_str().unwrap(), "é😀/");
    }
}
