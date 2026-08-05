//! Shared "run one SQL statement against a connection and emit its results"
//! entry point used by both `query` and `shell`, so the two never drift.
//!
//! Status / acknowledgement lines go to `status_w` (`query` routes them to
//! stderr to keep stdout clean for piping; `shell` prints them to stdout).
//! `first_result` is toggled to insert a blank line between consecutive result
//! sets — `query` threads it across statements, `shell` passes a fresh `true`
//! per statement to disable separation.

use super::output::{render_csv_stream, render_json_stream, render_table, Format, ResultSet};
use anyhow::Result;
use rusqlite::Connection;
use std::io::Write;

pub fn execute_stmt(
    conn: &Connection,
    sql: &str,
    fmt: Format,
    data_w: &mut impl Write,
    status_w: &mut impl Write,
    first_result: &mut bool,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    if stmt.column_count() == 0 {
        // No result set (DDL / INSERT / UPDATE / DELETE). Drop stmt first so
        // the borrow on conn is released, then read changes() afterward.
        let n = stmt.execute([])?;
        drop(stmt);
        writeln!(
            status_w,
            "[+] ok ({} row{} changed)",
            n,
            if n == 1 { "" } else { "s" }
        )?;
        return Ok(());
    }

    // Separate multiple result sets. For JSON we emit one document per set,
    // separated by blank lines (caller can split on `^\[$`); for table/csv
    // a blank line is enough to keep them visually distinct.
    if !*first_result {
        writeln!(data_w)?;
    }
    *first_result = false;

    match fmt {
        Format::Table => {
            let rs = ResultSet::collect(&mut stmt)?;
            drop(stmt);
            render_table(&rs, data_w)?;
        }
        Format::Csv => render_csv_stream(&mut stmt, data_w)?,
        Format::Json => render_json_stream(&mut stmt, data_w)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::output::Format;
    use rusqlite::Connection;

    #[test]
    fn write_statement_reports_changed_rows_to_status_writer() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (a INTEGER);").unwrap();
        let mut data = Vec::new();
        let mut status = Vec::new();
        let mut first = true;
        execute_stmt(
            &conn,
            "INSERT INTO t VALUES (1), (2)",
            Format::Table,
            &mut data,
            &mut status,
            &mut first,
        )
        .unwrap();
        assert!(data.is_empty());
        assert_eq!(
            String::from_utf8(status).unwrap(),
            "[+] ok (2 rows changed)\n"
        );
    }

    #[test]
    fn result_set_is_streamed_for_json() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE t (a INTEGER);
             INSERT INTO t VALUES (1), (2);",
        )
        .unwrap();
        let mut data = Vec::new();
        let mut status = Vec::new();
        let mut first = true;
        execute_stmt(
            &conn,
            "SELECT a FROM t",
            Format::Json,
            &mut data,
            &mut status,
            &mut first,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&data).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }
}
