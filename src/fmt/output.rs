//! Rendering of SQLite result sets in table, CSV and JSON form.
//!
//! CSV / JSON stream rows directly off a prepared statement so large result
//! sets never materialize in memory; table mode still collects the whole
//! result set first because column widths must be known before printing.

use anyhow::Result;
use rusqlite::types::{Value, ValueRef};
use rusqlite::Statement;
use std::fmt;
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Csv,
    Json,
}

/// One materialized result set. Column types are preserved per cell so the
/// renderer can format JSON numerically and table mode can show NULL distinctly.
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl ResultSet {
    pub fn collect(stmt: &mut Statement<'_>) -> Result<ResultSet> {
        let columns: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        let n = columns.len();
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let mut row = Vec::with_capacity(n);
            for i in 0..n {
                let v: Value = r.get_ref(i)?.into();
                row.push(v);
            }
            out.push(row);
        }
        Ok(ResultSet { columns, rows: out })
    }
}

fn display_width(s: &str) -> usize {
    // unicode-width reports the terminal cells a string occupies: CJK wide
    // glyphs count as 2, combining marks / control chars as 0, so table
    // columns stay aligned even for Chinese-heavy IM data.
    s.width()
}

// ---- table ----

/// Number of characters `i.to_string()` would produce, without allocating.
fn integer_digits(i: i64) -> usize {
    let mut n = i.unsigned_abs();
    let mut len = usize::from(i < 0);
    loop {
        len += 1;
        n /= 10;
        if n == 0 {
            return len;
        }
    }
}

fn cell_width(v: &Value) -> usize {
    match v {
        Value::Null => 4,
        Value::Integer(i) => integer_digits(*i),
        Value::Real(f) => format!("{f}").len(),
        Value::Text(s) => s.width(),
        Value::Blob(b) => format!("<blob:{} bytes>", b.len()).len(),
    }
}

fn write_cell(w: &mut impl Write, v: &Value, width: usize) -> io::Result<()> {
    let used = match v {
        Value::Null => {
            write!(w, "NULL")?;
            4
        }
        Value::Integer(i) => {
            write!(w, "{i}")?;
            integer_digits(*i)
        }
        Value::Real(f) => {
            let s = format!("{f}");
            write!(w, "{s}")?;
            s.len()
        }
        Value::Text(s) => {
            write!(w, "{s}")?;
            s.width()
        }
        Value::Blob(b) => {
            let s = format!("<blob:{} bytes>", b.len());
            write!(w, "{s}")?;
            s.len()
        }
    };
    for _ in used..width {
        write!(w, " ")?;
    }
    Ok(())
}

fn pad(w: &mut impl Write, n: usize) -> io::Result<()> {
    for _ in 0..n {
        write!(w, " ")?;
    }
    Ok(())
}

pub fn render_table(rs: &ResultSet, w: &mut impl Write) -> io::Result<()> {
    let n = rs.columns.len();
    if n == 0 {
        writeln!(w, "(no columns)")?;
        return Ok(());
    }

    let mut widths: Vec<usize> = rs.columns.iter().map(|s| display_width(s)).collect();
    for row in &rs.rows {
        for (i, v) in row.iter().enumerate() {
            let cw = cell_width(v);
            if cw > widths[i] {
                widths[i] = cw;
            }
        }
    }

    // header
    let mut first = true;
    for (i, name) in rs.columns.iter().enumerate() {
        if !first {
            write!(w, "  ")?;
        }
        first = false;
        write!(w, "{name}")?;
        pad(w, widths[i] - display_width(name))?;
    }
    writeln!(w)?;

    // separator
    let mut first = true;
    for (i, _) in rs.columns.iter().enumerate() {
        if !first {
            write!(w, "  ")?;
        }
        first = false;
        write!(w, "{}", "-".repeat(widths[i]))?;
    }
    writeln!(w)?;

    // rows
    for row in &rs.rows {
        let mut first = true;
        for (i, v) in row.iter().enumerate() {
            if !first {
                write!(w, "  ")?;
            }
            first = false;
            write_cell(w, v, widths[i])?;
        }
        writeln!(w)?;
    }

    writeln!(
        w,
        "({} row{})",
        rs.rows.len(),
        if rs.rows.len() == 1 { "" } else { "s" }
    )?;
    Ok(())
}

// ---- csv ----

pub fn render_csv_stream(stmt: &mut Statement<'_>, w: &mut impl Write) -> Result<()> {
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let n = cols.len();
    let mut rowbuf = String::new();
    write_csv_row(&mut rowbuf, cols.iter().map(|s| s.as_str()))?;
    w.write_all(rowbuf.as_bytes())?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        rowbuf.clear();
        for i in 0..n {
            if i > 0 {
                rowbuf.push(',');
            }
            write_csv_cell(&mut rowbuf, r.get_ref(i)?)?;
        }
        rowbuf.push('\n');
        w.write_all(rowbuf.as_bytes())?;
    }
    Ok(())
}

fn write_csv_row<'a>(
    w: &mut impl fmt::Write,
    cells: impl IntoIterator<Item = &'a str>,
) -> fmt::Result {
    let mut first = true;
    for c in cells {
        if !first {
            write!(w, ",")?;
        }
        first = false;
        write_csv_escaped(w, c)?;
    }
    writeln!(w)?;
    Ok(())
}

fn write_csv_cell(w: &mut impl fmt::Write, v: ValueRef<'_>) -> fmt::Result {
    match v {
        ValueRef::Null => {}
        ValueRef::Integer(i) => write!(w, "{i}")?,
        ValueRef::Real(f) => write!(w, "{f}")?,
        ValueRef::Text(s) => write_csv_escaped(w, &String::from_utf8_lossy(s))?,
        ValueRef::Blob(b) => write_csv_escaped(w, &hex_encode(b))?,
    }
    Ok(())
}

fn write_csv_escaped(w: &mut impl fmt::Write, c: &str) -> fmt::Result {
    if c.contains([',', '"', '\n', '\r']) {
        write!(w, "\"")?;
        for ch in c.chars() {
            if ch == '"' {
                write!(w, "\"\"")?;
            } else {
                write!(w, "{ch}")?;
            }
        }
        write!(w, "\"")?;
    } else {
        write!(w, "{c}")?;
    }
    Ok(())
}

// ---- json ----

pub fn render_json_stream(stmt: &mut Statement<'_>, w: &mut impl Write) -> Result<()> {
    let cols: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let mut rows = stmt.query([])?;
    writeln!(w, "[")?;
    let mut first = true;
    let mut rowbuf = String::new();
    while let Some(r) = rows.next()? {
        if !first {
            writeln!(w, ",")?;
        }
        first = false;
        rowbuf.clear();
        rowbuf.push_str("  {");
        for (ci, name) in cols.iter().enumerate() {
            if ci > 0 {
                rowbuf.push_str(", ");
            }
            rowbuf.push('"');
            write_json_str(&mut rowbuf, name)?;
            rowbuf.push_str("\": ");
            write_json_value_ref(&mut rowbuf, r.get_ref(ci)?)?;
        }
        rowbuf.push('}');
        w.write_all(rowbuf.as_bytes())?;
    }
    writeln!(w)?;
    writeln!(w, "]")?;
    Ok(())
}

fn write_json_value_ref(w: &mut impl fmt::Write, v: ValueRef<'_>) -> fmt::Result {
    match v {
        ValueRef::Null => write!(w, "null"),
        ValueRef::Integer(i) => write!(w, "{i}"),
        ValueRef::Real(f) => {
            if f.is_finite() {
                write!(w, "{f}")
            } else {
                // SQLite stores neither NaN nor +/-Inf, but be defensive
                write!(w, "null")
            }
        }
        ValueRef::Text(s) => {
            write!(w, "\"")?;
            write_json_str(w, &String::from_utf8_lossy(s))?;
            write!(w, "\"")
        }
        ValueRef::Blob(b) => {
            // Encode blobs as hex strings, prefixed so a consumer can tell
            // them apart from regular text columns.
            write!(w, "\"hex:{}\"", hex_encode(b))
        }
    }
}

fn write_json_str(w: &mut impl fmt::Write, s: &str) -> fmt::Result {
    for ch in s.chars() {
        match ch {
            '"' => write!(w, "\\\"")?,
            '\\' => write!(w, "\\\\")?,
            '\n' => write!(w, "\\n")?,
            '\r' => write!(w, "\\r")?,
            '\t' => write!(w, "\\t")?,
            '\x08' => write!(w, "\\b")?,
            '\x0c' => write!(w, "\\f")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => write!(w, "{c}")?,
        }
    }
    Ok(())
}

fn hex_encode(b: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &byte in b {
        s.push(HEX[(byte >> 4) as usize] as char);
        s.push(HEX[(byte & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn display_width_counts_cjk_as_two_cells() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("你好"), 4);
        assert_eq!(display_width("a你b"), 4);
    }

    #[test]
    fn render_table_pads_to_widest_cell() {
        let rs = ResultSet {
            columns: vec!["name".to_string(), "n".to_string()],
            rows: vec![
                vec![Value::Text("你好".to_string()), Value::Integer(1)],
                vec![Value::Text("a".to_string()), Value::Integer(22)],
            ],
        };
        let mut out = Vec::new();
        render_table(&rs, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.lines().any(|l| l.starts_with("name  n ")), "{text}");
        assert!(text.lines().any(|l| l.starts_with("a     22")), "{text}");
        assert!(text.contains("(2 rows)"), "{text}");
    }

    #[test]
    fn render_csv_stream_quotes_and_escapes() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"CREATE TABLE t (a, b);
               INSERT INTO t VALUES (1, 'x,y');
               INSERT INTO t VALUES (NULL, 'q"z');"#,
        )
        .unwrap();
        let mut stmt = conn.prepare("SELECT a, b FROM t ORDER BY a").unwrap();
        let mut out = Vec::new();
        render_csv_stream(&mut stmt, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "a,b");
        // SQLite sorts NULLs before integers in ASC order.
        assert_eq!(lines[1], ",\"q\"\"z\"");
        assert_eq!(lines[2], "1,\"x,y\"");
    }

    #[test]
    fn render_json_stream_escapes_and_preserves_numbers() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (a, b);
             INSERT INTO t VALUES (1, 'x');
             INSERT INTO t VALUES (2, 'a\n\"\\\u{1}b');",
        )
        .unwrap();
        let mut stmt = conn.prepare("SELECT a, b FROM t ORDER BY a").unwrap();
        let mut out = Vec::new();
        render_json_stream(&mut stmt, &mut out).unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed[0]["a"], 1);
        assert_eq!(parsed[1]["b"], "a\n\"\\\u{1}b");
        assert!(out.windows(2).any(|w| w == b",\n"), "rows comma-joined");
    }
}
