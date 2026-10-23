//! Bounded shallow YAML grammar, matching the checked-in metadata reader.
use super::*;
pub fn scalar(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].into()
    } else {
        s.into()
    }
}
fn citation(rule: &mut Value, pending: &mut Option<Value>) -> Result<(), Error> {
    if let Some(c) = pending.take() {
        if !c["ref"].is_string() || !c["textHash"].is_string() {
            return Err(invalid(
                "every provisions entry needs both ref and text_hash",
            ));
        }
        rule["provisions"].as_array_mut().unwrap().push(c);
    }
    Ok(())
}
pub fn parse_rules(text: &str, bundle: &str) -> Result<(String, Vec<Value>), Error> {
    let mut version = None;
    let mut rules = vec![];
    let mut current = Value::Null;
    let mut pending = None;
    let (mut provisions, mut machine) = (false, false);
    let mut block: Option<String> = None;
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        if raw
            .chars()
            .take_while(|c| c.is_whitespace())
            .any(|c| c == '\t')
        {
            return Err(invalid("tab indentation"));
        }
        let n = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        if let Some(field) = block.clone() {
            if n > 4 {
                let v = current[&field].as_str().unwrap_or("");
                current[&field] = json!(format!(
                    "{}{}{}",
                    v,
                    if v.is_empty() { "" } else { " " },
                    line
                ));
                continue;
            }
            block = None;
        }
        if line.starts_with('#') {
            continue;
        }
        if machine && n >= 6 {
            continue;
        }
        machine = false;
        if n == 0 && line.starts_with("version:") {
            if version.is_some() {
                return Err(invalid("duplicate bundle version"));
            }
            version = Some(scalar(&line[8..]));
        }
        if let Some(id) = line.strip_prefix("- rule_id:") {
            if !current.is_null() {
                citation(&mut current, &mut pending)?;
                rules.push(current);
            }
            current = json!({"ruleId":scalar(id),"bundle":bundle,"sourceId":"","name":"","explanation":"","severity":"","category":"","provisions":[]});
            provisions = false;
            continue;
        }
        if current.is_null() {
            continue;
        }
        if n <= 4 && provisions {
            citation(&mut current, &mut pending)?;
            provisions = false;
        }
        if provisions {
            if n == 6 {
                citation(&mut current, &mut pending)?;
                let r = line
                    .strip_prefix("- ref:")
                    .ok_or_else(|| invalid("provisions entry must be - ref"))?;
                pending = Some(json!({"ref":scalar(r)}));
                continue;
            }
            if n >= 8 {
                let h = line
                    .strip_prefix("text_hash:")
                    .ok_or_else(|| invalid("expected text_hash"))?;
                let p = pending
                    .as_mut()
                    .ok_or_else(|| invalid("orphan text_hash"))?;
                if p.get("textHash").is_some() {
                    return Err(invalid("duplicate text_hash"));
                }
                p["textHash"] = json!(scalar(h));
                continue;
            }
            return Err(invalid("malformed provisions indentation"));
        }
        if n == 4 {
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| invalid("malformed rule key"))?;
            match key {
                "machine_rule" => machine = true,
                "provisions" => {
                    if !value.trim().is_empty() {
                        return Err(invalid("unsupported inline provisions"));
                    }
                    provisions = true;
                }
                "source_id" | "name" | "explanation" | "severity" | "category" => {
                    let field = if key == "source_id" { "sourceId" } else { key };
                    let value = value.trim();
                    if value.starts_with('|') {
                        return Err(invalid("literal block scalars unsupported"));
                    }
                    if [">", ">-", ">+"].contains(&value) {
                        if !["name", "explanation"].contains(&field) {
                            return Err(invalid("unsupported folded scalar"));
                        }
                        block = Some(field.into());
                        current[field] = json!("");
                    } else {
                        current[field] = json!(scalar(value));
                    }
                }
                _ => {
                    return Err(invalid(&format!("unsupported rule field: {key}")));
                }
            }
        } else if n >= 6 {
            return Err(invalid("unexpected indented line; use folded scalar"));
        }
    }
    if !current.is_null() {
        citation(&mut current, &mut pending)?;
        rules.push(current);
    }
    let v = version
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("missing bundle version"))?;
    for rule in &mut rules {
        rule["provisions"]
            .as_array_mut()
            .unwrap()
            .sort_by_cached_key(|citation| citation["ref"].as_str().unwrap_or("").to_owned());
    }
    Ok((v, rules))
}
pub type Scope = BTreeMap<String, Option<Vec<(String, String)>>>;
pub fn id_key(s: &str) -> (u64, String) {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    (s[..split].parse().unwrap_or(0), s[split..].to_lowercase())
}
fn para(s: &str) -> Option<String> {
    let s = s.trim().strip_prefix('§')?.trim().to_lowercase();
    let (n, suffix) = id_key(&s);
    if n == 0 || !suffix.chars().all(|c| c.is_alphabetic()) {
        return None;
    }
    Some(s)
}
pub fn parse_scope(text: &str) -> Result<(String, Scope), Error> {
    let (mut version, mut source, mut in_sources) = (None, None::<String>, false);
    let mut scope = Scope::new();
    let mut declared = BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if raw.contains('\t') {
            return Err(invalid("scope tab indentation"));
        }
        let n = raw.len() - raw.trim_start().len();
        match n {
            0 => {
                if let Some(v) = line.strip_prefix("version:") {
                    if version.is_some() {
                        return Err(invalid("duplicate scope version"));
                    }
                    version = Some(scalar(v));
                    in_sources = false;
                } else if line == "sources:" {
                    in_sources = true;
                } else {
                    return Err(invalid("unexpected scope top-level line"));
                }
                source = None;
            }
            2 if in_sources => {
                let s = line
                    .strip_suffix(':')
                    .filter(|s| {
                        s.starts_with("DK-")
                            && s.chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                    })
                    .ok_or_else(|| invalid("bad scope source key"))?;
                if scope.insert(s.into(), Some(vec![])).is_some() {
                    return Err(invalid("duplicate scope source"));
                }
                source = Some(s.into());
            }
            4 => {
                let s = source
                    .as_ref()
                    .ok_or_else(|| invalid("scope orphan line"))?;
                if !declared.insert(s.clone()) {
                    return Err(invalid("duplicate in_scope"));
                }
                if line == "in_scope: all" {
                    scope.insert(s.clone(), None);
                } else if line != "in_scope:" {
                    return Err(invalid("expected in_scope"));
                }
            }
            6 => {
                let s = source
                    .as_ref()
                    .ok_or_else(|| invalid("scope orphan range"))?;
                if !declared.contains(s) {
                    return Err(invalid("range without in_scope"));
                }
                let ranges = scope
                    .get_mut(s)
                    .and_then(Option::as_mut)
                    .ok_or_else(|| invalid("range after all"))?;
                let value = scalar(
                    line.strip_prefix("- ")
                        .ok_or_else(|| invalid("bad range"))?,
                );
                let parts: Vec<_> = value.split('-').collect();
                if parts.len() > 2 {
                    return Err(invalid("bad range"));
                }
                let low = para(parts[0]).ok_or_else(|| invalid("bad range token"))?;
                let high = para(parts.last().unwrap()).ok_or_else(|| invalid("bad range token"))?;
                if id_key(&low) > id_key(&high) {
                    return Err(invalid("inverted range"));
                }
                if ranges.contains(&(low.clone(), high.clone())) {
                    return Err(invalid("duplicate range"));
                }
                ranges.push((low, high));
            }
            _ => return Err(invalid("unexpected scope indentation")),
        }
    }
    for (s, r) in &scope {
        if !declared.contains(s) {
            return Err(invalid("source has no in_scope"));
        }
        if r.as_ref().is_some_and(Vec::is_empty) {
            return Err(invalid("in_scope has no ranges"));
        }
    }
    if scope.is_empty() {
        return Err(invalid("no scope sources"));
    }
    Ok((
        version
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("missing scope version"))?,
        scope,
    ))
}
pub fn in_scope(scope: &Scope, source: &str, id: &str) -> bool {
    match scope.get(source) {
        Some(None) => true,
        Some(Some(r)) => r
            .iter()
            .any(|(a, b)| id_key(a) <= id_key(id) && id_key(id) <= id_key(b)),
        None => false,
    }
}
pub fn reference(s: &str) -> Option<Vec<String>> {
    let parts: Vec<_> = s.trim().split(',').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut path = vec![para(parts[0])?];
    if parts.len() > 1 {
        let n = parts[1].strip_prefix("stk.")?.trim();
        if n.is_empty() || !n.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        path.push(n.into());
    }
    if parts.len() > 2 {
        let n = parts[2]
            .strip_prefix("nr.")
            .or_else(|| parts[2].strip_prefix("litra"))?
            .trim();
        if n.is_empty()
            || !(n.chars().all(|c| c.is_ascii_digit()) || n.chars().all(|c| c.is_alphabetic()))
        {
            return None;
        }
        path.push(n.to_lowercase());
    }
    Some(path)
}
