// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! JSON as Python's `json` module reads and writes it: objects keep their
//! document order, and output is `json.dumps(indent=2)` with ASCII escapes.

use std::fmt::{self, Write};

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// A JSON value; an object is its entries in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// An object entry.
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.as_obj()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn as_obj(&self) -> Option<&[(String, Json)]> {
        if let Json::Obj(o) = self { Some(o) } else { None }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Json::Str(s) = self { Some(s) } else { None }
    }

    /// Python's truth value.
    pub fn truthy(&self) -> bool {
        match self {
            Json::Null => false,
            Json::Bool(b) => *b,
            Json::Int(i) => *i != 0,
            Json::Float(f) => *f != 0.0,
            Json::Str(s) => !s.is_empty(),
            Json::Arr(a) => !a.is_empty(),
            Json::Obj(o) => !o.is_empty(),
        }
    }

    /// Python's `int(value or 0)`, with 0 for what `int()` would refuse.
    pub fn py_int(&self) -> i64 {
        match self {
            Json::Bool(b) => i64::from(*b),
            Json::Int(i) => *i,
            #[allow(clippy::cast_possible_truncation)]
            Json::Float(f) => *f as i64,
            Json::Str(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Python's `str(value)` for a scalar.
    pub fn py_str(&self) -> String {
        match self {
            Json::Null => "None".into(),
            Json::Bool(true) => "True".into(),
            Json::Bool(false) => "False".into(),
            Json::Int(i) => i.to_string(),
            Json::Float(f) => format!("{f:?}"),
            Json::Str(s) => s.clone(),
            other => other.pretty(),
        }
    }

    /// `json.dumps(value, indent=2)`.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Json::Arr(items) if !items.is_empty() => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Json::Obj(entries) if !entries.is_empty() => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    out.push_str(if i == 0 { "\n" } else { ",\n" });
                    indent(out, depth + 1);
                    quote(out, key);
                    out.push_str(": ");
                    value.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push('}');
            }
            Json::Arr(_) => out.push_str("[]"),
            Json::Obj(_) => out.push_str("{}"),
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(i) => out.push_str(&i.to_string()),
            Json::Float(f) => out.push_str(&format!("{f:?}")),
            Json::Str(s) => quote(out, s),
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    out.extend(std::iter::repeat_n(' ', depth * 2));
}

/// A string literal with `ensure_ascii`: everything outside space..`~` escaped.
fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Strings in a JSON array.
pub fn strings<I: IntoIterator<Item = S>, S: Into<String>>(items: I) -> Json {
    Json::Arr(items.into_iter().map(|s| Json::Str(s.into())).collect())
}

/// Parses JSON, keeping object order; a repeated key keeps its first place
/// and its last value, as `json.loads` does.
///
/// # Errors
/// Text that is not JSON.
pub fn parse(text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Json, D::Error> {
        d.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> Result<Json, E> {
        Ok(Json::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<Json, E> {
        Ok(Json::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<Json, E> {
        Ok(Json::Int(v))
    }
    fn visit_u64<E>(self, v: u64) -> Result<Json, E> {
        #[allow(clippy::cast_precision_loss)]
        Ok(i64::try_from(v).map_or(Json::Float(v as f64), Json::Int))
    }
    fn visit_f64<E>(self, v: f64) -> Result<Json, E> {
        Ok(Json::Float(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<Json, E> {
        Ok(Json::Str(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<Json, E> {
        Ok(Json::Str(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Json::Arr(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
        let mut entries: Vec<(String, Json)> = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Json>()? {
            match entries.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => entries.push((key, value)),
            }
        }
        Ok(Json::Obj(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_duplicates_follow_python() {
        let v = parse(r#"{"b": 1, "a": [true, null], "b": 2}"#).unwrap();
        assert_eq!(v, Json::Obj(vec![("b".into(), Json::Int(2)), ("a".into(), Json::Arr(vec![Json::Bool(true), Json::Null]))]));
    }

    #[test]
    fn pretty_matches_json_dumps_indent_2() {
        let v = Json::Obj(vec![
            ("a".into(), Json::Arr(vec![])),
            ("b".into(), Json::Obj(vec![])),
            ("c".into(), strings(["x", "é\u{1F600}\"\\\u{7f}"])),
            ("d".into(), Json::Obj(vec![("e".into(), Json::Int(3))])),
        ]);
        let want = "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    \"x\",\n    \"\\u00e9\\ud83d\\ude00\\\"\\\\\\u007f\"\n  ],\n  \"d\": {\n    \"e\": 3\n  }\n}";
        assert_eq!(v.pretty(), want);
    }
}
