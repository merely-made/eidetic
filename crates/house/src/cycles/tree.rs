// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! A TOML document with its tables in document order, as `tomllib` gives it.
//! The `toml` crate sorts keys; its spans give the order back.

use toml::de::{DeTable, DeValue};

/// A TOML value; a table is its entries in document order.
#[derive(Clone, Debug, PartialEq)]
pub enum Toml {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Datetime(String),
    Arr(Vec<Toml>),
    Table(Vec<(String, Toml)>),
}

/// Parses a document.
///
/// # Errors
/// Text that is not TOML.
pub fn parse(text: &str) -> Result<Toml, String> {
    let doc = DeTable::parse(text).map_err(|e| e.to_string().trim_end().to_string())?;
    Ok(table(doc.get_ref()))
}

fn table(t: &DeTable<'_>) -> Toml {
    let mut entries: Vec<(usize, String, Toml)> =
        t.iter().map(|(k, v)| (k.span().start, k.get_ref().to_string(), value(v.get_ref()))).collect();
    entries.sort_by_key(|e| e.0);
    Toml::Table(entries.into_iter().map(|(_, k, v)| (k, v)).collect())
}

fn value(v: &DeValue<'_>) -> Toml {
    match v {
        DeValue::String(s) => Toml::Str(s.to_string()),
        DeValue::Integer(i) => Toml::Int(i64::from_str_radix(i.as_str(), i.radix()).unwrap_or(0)),
        DeValue::Float(f) => Toml::Float(f.as_str().replace('_', "").parse().unwrap_or(0.0)),
        DeValue::Boolean(b) => Toml::Bool(*b),
        DeValue::Datetime(d) => Toml::Datetime(d.to_string()),
        DeValue::Array(a) => Toml::Arr(a.iter().map(|e| value(e.get_ref())).collect()),
        DeValue::Table(t) => table(t),
    }
}

impl Toml {
    /// A table entry.
    pub fn get(&self, key: &str) -> Option<&Toml> {
        self.as_table()?.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn as_table(&self) -> Option<&[(String, Toml)]> {
        if let Toml::Table(t) = self { Some(t) } else { None }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Toml::Str(s) = self { Some(s) } else { None }
    }

    /// Python's truth value.
    pub fn truthy(&self) -> bool {
        match self {
            Toml::Str(s) => !s.is_empty(),
            Toml::Int(i) => *i != 0,
            Toml::Float(f) => *f != 0.0,
            Toml::Bool(b) => *b,
            Toml::Datetime(_) => true,
            Toml::Arr(a) => !a.is_empty(),
            Toml::Table(t) => !t.is_empty(),
        }
    }

    /// Python's `str(value)`.
    pub fn py_str(&self) -> String {
        match self {
            Toml::Str(s) | Toml::Datetime(s) => s.clone(),
            other => other.py_repr(),
        }
    }

    /// Python's `repr(value)`, for messages that print a whole entry.
    pub fn py_repr(&self) -> String {
        match self {
            Toml::Str(s) => repr_str(s),
            Toml::Int(i) => i.to_string(),
            Toml::Float(f) => format!("{f:?}"),
            Toml::Bool(true) => "True".into(),
            Toml::Bool(false) => "False".into(),
            Toml::Datetime(d) => d.clone(),
            Toml::Arr(a) => format!("[{}]", a.iter().map(Toml::py_repr).collect::<Vec<_>>().join(", ")),
            Toml::Table(t) => {
                let body: Vec<String> = t.iter().map(|(k, v)| format!("{}: {}", repr_str(k), v.py_repr())).collect();
                format!("{{{}}}", body.join(", "))
            }
        }
    }
}

fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::from(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_keep_document_order() {
        let doc = parse("[package]\nname = \"z\"\n[dependencies]\nzeta = \"1\"\nalpha = { git = \"x\" }\n[dev-dependencies.mid]\nversion = \"1\"\n").unwrap();
        let keys: Vec<&str> = doc.as_table().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["package", "dependencies", "dev-dependencies"]);
        let deps: Vec<&str> = doc.get("dependencies").unwrap().as_table().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(deps, ["zeta", "alpha"]);
        assert_eq!(doc.get("package").unwrap().get("name").unwrap().as_str(), Some("z"));
    }

    #[test]
    fn repr_is_pythons() {
        let t = Toml::Table(vec![("from".into(), Toml::Str("a".into())), ("n".into(), Toml::Bool(true))]);
        assert_eq!(t.py_repr(), "{'from': 'a', 'n': True}");
        assert_eq!(parse("x = 0x1f\n").unwrap().get("x"), Some(&Toml::Int(31)));
    }
}
