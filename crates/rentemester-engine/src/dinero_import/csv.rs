use super::*;
pub(super) fn records(bytes: &[u8]) -> Result<Vec<Vec<String>>, Error> {
    let raw = std::str::from_utf8(bytes).map_err(|_| refused("CSV must be UTF-8"))?;
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut chars = raw.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut cell = String::new();
    let mut row = vec![];
    let mut rows = vec![];
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        if closed && ![';', '\r', '\n'].contains(&c) {
            if c == ' ' || c == '\t' {
                continue;
            }
            return Err(refused("invalid CSV quote suffix"));
        }
        match c {
            '"' => {
                if !cell.trim().is_empty() || closed {
                    return Err(refused("invalid CSV quote"));
                }
                cell.clear();
                quoted = true;
            }
            ';' => {
                row.push(std::mem::take(&mut cell).trim().into());
                closed = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut cell).trim().into());
                if row.iter().any(|s: &String| !s.is_empty()) {
                    rows.push(std::mem::take(&mut row));
                }
                row.clear();
                closed = false;
            }
            _ => cell.push(c),
        }
    }
    if quoted {
        return Err(refused("unclosed CSV quote"));
    }
    row.push(cell.trim().into());
    if row.iter().any(|s| !s.is_empty()) {
        rows.push(row);
    }
    Ok(rows)
}
pub(super) fn read(bytes: &[u8], header: &[&str]) -> Result<Vec<Vec<String>>, Error> {
    let mut rows = records(bytes)?;
    if rows.is_empty()
        || rows[0].len() != header.len()
        || rows[0].iter().zip(header).any(|(a, b)| {
            !(a.eq_ignore_ascii_case(b) || *b == "Momstype" && a.eq_ignore_ascii_case("Moms"))
        })
    {
        return Err(refused(
            "CSV header does not match the versioned archive contract",
        ));
    }
    rows.remove(0);
    if rows.iter().any(|r| {
        r.len() != header.len()
            || r[0].is_empty()
            || r[0].len() > 256
            || r[0].chars().any(char::is_control)
    }) {
        return Err(refused("invalid archive CSV row width/account"));
    }
    Ok(rows)
}
