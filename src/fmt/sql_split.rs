//! Split a SQL blob on top-level `;` boundaries while honoring `'…'` / `"…"`
//! string literals, `[…]` and `` `…` `` quoted identifiers, and `--` / `/* */`
//! comments. Returns each statement (trailing `;` preserved). The scanner works
//! on `char` boundaries so multi-byte UTF-8 (Chinese text, emoji, …) in
//! literals and comments is preserved verbatim.

pub fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        // Quoted string literal; doubled quotes are escaped inside it.
        if c == '\'' || c == '"' {
            buf.push(c);
            let quote = c;
            loop {
                match chars.next() {
                    Some(q) if q == quote && chars.peek() == Some(&quote) => {
                        buf.push(q);
                        buf.push(quote);
                        chars.next();
                    }
                    Some(q) if q == quote => {
                        buf.push(q);
                        break;
                    }
                    Some(ch) => buf.push(ch),
                    None => break,
                }
            }
            continue;
        }
        // [..] and `..` quoted identifiers.
        if c == '[' {
            buf.push(c);
            for ch in chars.by_ref() {
                buf.push(ch);
                if ch == ']' {
                    break;
                }
            }
            continue;
        }
        if c == '`' {
            buf.push(c);
            for ch in chars.by_ref() {
                buf.push(ch);
                if ch == '`' {
                    break;
                }
            }
            continue;
        }
        // -- line comment.
        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            for ch in chars.by_ref() {
                buf.push(ch);
                if ch == '\n' {
                    break;
                }
            }
            continue;
        }
        // /* block comment */.
        if c == '/' && chars.peek() == Some(&'*') {
            buf.push_str("/*");
            chars.next();
            loop {
                match chars.next() {
                    Some('*') if chars.peek() == Some(&'/') => {
                        chars.next();
                        buf.push_str("*/");
                        break;
                    }
                    Some(ch) => buf.push(ch),
                    None => break,
                }
            }
            continue;
        }
        if c == ';' {
            buf.push(';');
            let trimmed = buf.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
            buf.clear();
            continue;
        }
        buf.push(c);
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
