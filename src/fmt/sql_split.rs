//! Split a SQL blob on top-level `;` boundaries while honoring `'…'` / `"…"`
//! string literals, `[…]` and `` `…` `` quoted identifiers, and `--` / `/* */`
//! comments. Returns each statement (trailing `;` preserved). The scanner works
//! on `char` boundaries so multi-byte UTF-8 (Chinese text, emoji, …) in
//! literals and comments is preserved verbatim.

pub fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_bracket = false;
    let mut in_backtick = false;

    while i < sql.len() {
        let c = sql[i..]
            .chars()
            .next()
            .expect("i always points at a char boundary");
        let next_byte = bytes.get(i + 1).copied();

        if in_single {
            buf.push(c);
            if c == '\'' {
                if next_byte == Some(b'\'') {
                    buf.push('\'');
                    i += 2;
                    continue;
                }
                in_single = false;
            }
            i += c.len_utf8();
            continue;
        }
        if in_double {
            buf.push(c);
            if c == '"' {
                if next_byte == Some(b'"') {
                    buf.push('"');
                    i += 2;
                    continue;
                }
                in_double = false;
            }
            i += c.len_utf8();
            continue;
        }
        if in_bracket {
            buf.push(c);
            if c == ']' {
                in_bracket = false;
            }
            i += c.len_utf8();
            continue;
        }
        if in_backtick {
            buf.push(c);
            if c == '`' {
                in_backtick = false;
            }
            i += c.len_utf8();
            continue;
        }

        if c == '-' && next_byte == Some(b'-') {
            while i < bytes.len() && bytes[i] != b'\n' {
                let ch = sql[i..].chars().next().expect("char boundary");
                buf.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }
        if c == '/' && next_byte == Some(b'*') {
            buf.push_str("/*");
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                let ch = sql[i..].chars().next().expect("char boundary");
                buf.push(ch);
                i += ch.len_utf8();
            }
            if i + 1 < bytes.len() {
                buf.push_str("*/");
                i += 2;
            }
            continue;
        }

        match c {
            '\'' => {
                in_single = true;
                buf.push(c);
            }
            '"' => {
                in_double = true;
                buf.push(c);
            }
            '[' => {
                in_bracket = true;
                buf.push(c);
            }
            '`' => {
                in_backtick = true;
                buf.push(c);
            }
            ';' => {
                buf.push(';');
                let trimmed = buf.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                buf.clear();
            }
            _ => buf.push(c),
        }
        i += c.len_utf8();
    }

    let trimmed = buf.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_two_simple_statements() {
        let v = split_statements("SELECT 1; SELECT 2;");
        assert_eq!(v, vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn ignores_semicolons_in_strings() {
        let v = split_statements("INSERT INTO t VALUES ('a;b'); SELECT 1;");
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("'a;b'"));
    }

    #[test]
    fn handles_doubled_quotes() {
        let v = split_statements("SELECT 'it''s'; SELECT 2;");
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("'it''s'"));
    }

    #[test]
    fn skips_comments() {
        let v = split_statements("-- ;\nSELECT 1; /* ; */ SELECT 2;");
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn no_trailing_semicolon_still_emits() {
        let v = split_statements("SELECT 1");
        assert_eq!(v, vec!["SELECT 1"]);
    }

    #[test]
    fn preserves_utf8_in_string_literals() {
        let v = split_statements("SELECT '你好;世界'; SELECT 2;");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], "SELECT '你好;世界';");
        assert_eq!(v[1], "SELECT 2;");
    }

    #[test]
    fn preserves_utf8_in_comments() {
        let v = split_statements("-- 注释;里带分号\nSELECT '中文' /* 块注释; */; SELECT 2;");
        assert_eq!(v.len(), 2);
        assert!(v[0].contains("注释;里带分号"));
        assert!(v[0].contains("SELECT '中文'"));
        assert_eq!(v[1], "SELECT 2;");
    }

    #[test]
    fn preserves_utf8_in_quoted_identifiers() {
        let v = split_statements("SELECT `名字`, \"备注\" FROM t;");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0], "SELECT `名字`, \"备注\" FROM t;");
    }
}
