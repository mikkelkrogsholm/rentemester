use super::*;
use regex::Regex;
use serde_json::{Map, json};
use std::sync::OnceLock;
pub(super) const PATCH_FIELDS: &[&str] = &[
    "title",
    "description",
    "nextAction",
    "scope",
    "assignee",
    "waitingOn",
    "workDate",
    "deadline",
    "period",
    "references",
    "evidenceRequired",
    "relevance",
    "verificationRequired",
    "reminders",
];
fn pattern(pattern: &'static str, value: &str) -> bool {
    Regex::new(pattern)
        .expect("fixed task pattern")
        .is_match(value)
}
pub fn text(value: &Value, field: &str, max: usize, required: bool) -> Result<String, Error> {
    let s = value
        .as_str()
        .ok_or_else(|| invalid(format!("Invalid {field}")))?;
    if s.encode_utf16().count() > max
        || s.chars()
            .any(|c| matches!(c, '\x00'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\x7f'))
    {
        return Err(invalid(format!("Invalid {field}")));
    }
    let s = crate::company_knowledge::trim(s);
    if required && s.is_empty() {
        return Err(invalid(format!("{field} is required")));
    }
    Ok(s.into())
}
pub(super) fn content(
    value: &Value,
    field: &str,
    max: usize,
    required: bool,
) -> Result<String, Error> {
    let s = text(value, field, max, required)?;
    static PRIVATE: OnceLock<Vec<Regex>> = OnceLock::new();
    let private = PRIVATE.get_or_init(|| [
        r"(?-u:\b)(?:0[1-9]|[12][0-9]|3[01])(?:0[1-9]|1[0-2])[0-9]{2}[- ]?[0-9]{4}(?-u:\b)",
        r"(?-u:\b)[A-Z]{2}[0-9]{2}(?:[ ]?[A-Z0-9]){11,30}(?-u:\b)",
        r"(?i)(?-u:\b)(?:reg(?:istrerings)?\.?\s*(?:nr\.?)?|konto(?:nummer)?|bankkonto)\s*[:#]?\s*[0-9][0-9 -]{6,}[0-9](?-u:\b)",
    ].into_iter().map(|p| Regex::new(p).expect("fixed privacy pattern")).collect());
    if private.iter().any(|p| p.is_match(&s)) {
        return Err(refusal(
            "private_data",
            "CPR og fulde betalingsoplysninger skal gemmes i den beskyttede kilde. Brug en sikker produktreference i opgaven.",
        ));
    }
    Ok(s)
}
pub fn id(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .filter(|s| s.is_ascii() && pattern(r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$", s))
        .map(String::from)
        .ok_or_else(|| invalid("Invalid task reference id"))
}
fn slug(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .filter(|s| pattern(r"^[a-z0-9][a-z0-9-]{0,119}$", s))
        .map(String::from)
        .ok_or_else(|| invalid("Invalid company scope"))
}
pub fn date(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .filter(|s| crate::calendar::valid_iso_date(s))
        .map(String::from)
        .ok_or_else(|| invalid("Invalid task calendar date"))
}
/// Canonical UTC milliseconds plus Unix milliseconds for source-check freshness.
pub fn instant(value: &str) -> Result<(String, i128), Error> {
    if !value.is_ascii()
        || !pattern(
            r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$",
            value,
        )
    {
        return Err(invalid("Invalid timestamp"));
    }
    date(&json!(&value[..10]))?;
    let number = |a, b| {
        value[a..b]
            .parse::<u16>()
            .map_err(|_| invalid("Invalid timestamp"))
    };
    let d = time::Date::from_calendar_date(
        i32::from(number(0, 4)?),
        time::Month::try_from(number(5, 7)? as u8).map_err(|_| invalid("Invalid timestamp"))?,
        number(8, 10)? as u8,
    )
    .map_err(|_| invalid("Invalid timestamp"))?;
    let fraction = if value.len() > 20 {
        &value[20..value.len() - 1]
    } else {
        ""
    };
    let ms = format!("{fraction:0<3}")
        .parse::<u16>()
        .map_err(|_| invalid("Invalid timestamp"))?;
    let hour = number(11, 13)?;
    let (d, hour) = if hour == 24 && number(14, 16)? == 0 && number(17, 19)? == 0 && ms == 0 {
        (
            d.next_day().ok_or_else(|| invalid("Timestamp overflow"))?,
            0,
        )
    } else {
        (d, hour)
    };
    let t =
        time::Time::from_hms_milli(hour as u8, number(14, 16)? as u8, number(17, 19)? as u8, ms)
            .map_err(|_| invalid("Invalid timestamp"))?;
    if !(0..=9999).contains(&d.year()) {
        return Err(invalid("Timestamp outside supported range"));
    }
    Ok((
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            d.year(),
            d.month() as u8,
            d.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.millisecond()
        ),
        time::PrimitiveDateTime::new(d, t)
            .assume_utc()
            .unix_timestamp_nanos()
            / 1_000_000,
    ))
}
pub(super) fn object<'a>(v: &'a Value, fields: &[&str]) -> Result<&'a Map<String, Value>, Error> {
    let m = v
        .as_object()
        .ok_or_else(|| invalid("Task input must be an object"))?;
    if let Some(k) = m.keys().find(|k| !fields.contains(&k.as_str())) {
        return Err(invalid(format!("Unsupported task field: {k}")));
    }
    Ok(m)
}
pub(super) fn choice(v: &Value, choices: &[&str]) -> Result<String, Error> {
    v.as_str()
        .filter(|s| choices.contains(s))
        .map(String::from)
        .ok_or_else(|| invalid("Invalid task choice"))
}
pub fn scope(v: &Value) -> Result<Scope, Error> {
    match v["kind"].as_str() {
        Some("company") => {
            object(v, &["kind", "companySlug"])?;
            Ok(Scope::Company {
                company_slug: slug(&v["companySlug"])?,
            })
        }
        Some("workspace") => {
            object(v, &["kind", "companySlugs"])?;
            let a = v["companySlugs"]
                .as_array()
                .filter(|a| a.len() <= 100)
                .ok_or_else(|| invalid("Invalid workspace scope"))?;
            let mut slugs = a.iter().map(slug).collect::<Result<Vec<_>, _>>()?;
            slugs.sort();
            slugs.dedup();
            Ok(Scope::Workspace {
                company_slugs: slugs,
            })
        }
        _ => Err(invalid("Explicit task scope is required")),
    }
}
fn opaque(v: &Value) -> Result<String, Error> {
    v.as_str()
        .filter(|s| {
            pattern(r"^[A-Za-z0-9][A-Za-z0-9._:/-]{0,239}$", s)
                && !s.contains("..")
                && !s.contains("://")
        })
        .map(String::from)
        .ok_or_else(|| invalid("References must be opaque product references, not paths or URLs"))
}
pub fn references(v: &Value, scope: &Scope) -> Result<Vec<Reference>, Error> {
    let a = v
        .as_array()
        .filter(|a| a.len() <= 100)
        .ok_or_else(|| invalid("Invalid references"))?;
    a.iter()
        .map(|r| {
            object(r, &["kind", "ref", "companySlug"])?;
            let company_slug = if r["companySlug"].is_null() {
                None
            } else {
                Some(slug(&r["companySlug"])?)
            };
            if company_slug
                .as_deref()
                .is_some_and(|s| !scope.companies().contains(&s))
            {
                return Err(invalid("Reference company is outside task scope"));
            }
            Ok(Reference {
                kind: choice(
                    &r["kind"],
                    &[
                        "document",
                        "bank_transaction",
                        "period",
                        "approval",
                        "knowledge",
                        "external_receipt",
                        "party",
                    ],
                )?,
                reference: opaque(&r["ref"])?,
                company_slug,
            })
        })
        .collect()
}
pub(super) fn fields(v: &Value, scope: &Scope) -> Result<Map<String, Value>, Error> {
    let values = object(v, PATCH_FIELDS)?;
    let mut out = Map::new();
    for (k, v) in values {
        let normalized = match k.as_str() {
            "title" => json!(content(v, k, 240, true)?),
            "description" | "nextAction" | "waitingOn" => json!(content(
                v,
                k,
                if k == "description" { 8000 } else { 2000 },
                false
            )?),
            "scope" => json!(super::scope(v)?),
            "workDate" => {
                if v.is_null() {
                    Value::Null
                } else {
                    json!(date(v)?)
                }
            }
            "deadline" if !v.is_null() => {
                object(v, &["date", "kind", "certainty", "basis", "ruleId"])?;
                let mut d = json!({"date":date(&v["date"] )?,"kind":choice(&v["kind"],&["statutory","agreement","internal"] )?,"certainty":choice(&v["certainty"],&["confirmed","unconfirmed"] )?,"basis":content(&v["basis"],"deadline basis",2000,true)?});
                if v.get("ruleId").is_some() {
                    d["ruleId"] = json!(id(&v["ruleId"])?);
                }
                d
            }
            "period" if !v.is_null() => {
                object(v, &["from", "to", "label"])?;
                let from = date(&v["from"])?;
                let to = date(&v["to"])?;
                if from > to {
                    return Err(invalid("Period ends before its start"));
                }
                json!({"from":from,"to":to,"label":content(&v["label"],"period label",240,true)?})
            }
            "references" => json!(references(v, scope)?),
            "evidenceRequired" | "verificationRequired" => {
                json!(v.as_bool().ok_or_else(|| invalid(format!("Invalid {k}")))?)
            }
            "relevance" => json!(choice(v, &["relevant", "unknown", "not_relevant"])?),
            "assignee" if !v.is_null() => {
                let kind = choice(&v["kind"], &["member", "external"])?;
                object(
                    v,
                    if kind == "member" {
                        &["kind", "name", "userId"]
                    } else {
                        &["kind", "name"]
                    },
                )?;
                let mut a =
                    json!({"kind":kind,"name":content(&v["name"],"assignee name",240,true)?});
                if kind == "member" {
                    a["userId"] = json!(id(&v["userId"])?);
                }
                a
            }
            "reminders" => {
                let a = v
                    .as_array()
                    .filter(|a| a.len() <= 20)
                    .ok_or_else(|| invalid("Invalid reminders"))?;
                let mut seen = std::collections::BTreeSet::new();
                let mut result = Vec::new();
                for r in a {
                    object(
                        r,
                        &[
                            "reminderId",
                            "recipientId",
                            "enabled",
                            "kind",
                            "frequency",
                            "timeZone",
                            "daysBefore",
                            "followUpDate",
                        ],
                    )?;
                    let reminder_id = id(&r["reminderId"])?;
                    if !seen.insert(reminder_id.clone()) {
                        return Err(invalid("Duplicate reminder id"));
                    }
                    let kind = choice(&r["kind"], &["deadline", "follow_up"])?;
                    let tz = text(&r["timeZone"], "timeZone", 80, true)?;
                    if tz.parse::<chrono_tz::Tz>().is_err() {
                        return Err(invalid("Invalid reminder time zone"));
                    }
                    let mut reminder = json!({"reminderId":reminder_id,"recipientId":id(&r["recipientId"] )?,"enabled":r["enabled"].as_bool().ok_or_else(|| invalid("Invalid reminder"))?,"kind":kind,"frequency":choice(&r["frequency"],&["once","daily"] )?,"timeZone":tz});
                    if kind == "deadline" {
                        reminder["daysBefore"] = json!(
                            r["daysBefore"]
                                .as_u64()
                                .filter(|n| *n <= 366)
                                .ok_or_else(|| invalid("Invalid reminder daysBefore"))?
                        );
                    } else {
                        reminder["followUpDate"] = json!(date(&r["followUpDate"])?);
                    }
                    result.push(reminder);
                }
                json!(result)
            }
            "deadline" | "period" | "assignee" => Value::Null,
            _ => return Err(invalid("Unsupported task field")),
        };
        out.insert(k.clone(), normalized);
    }
    Ok(out)
}
pub fn draft(v: &Value) -> Result<Value, Error> {
    let mut allowed = PATCH_FIELDS.to_vec();
    allowed.extend([
        "taskId",
        "type",
        "origin",
        "status",
        "columnId",
        "workspaceColumnId",
        "source",
        "seriesId",
        "occurrenceKey",
    ]);
    let values = object(v, &allowed)?;
    let scope = scope(&v["scope"])?;
    let patch: Map<_, _> = values
        .iter()
        .filter(|(k, _)| PATCH_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut out = fields(&Value::Object(patch), &scope)?;
    out.insert(
        "title".into(),
        json!(content(&v["title"], "title", 240, true)?),
    );
    out.insert("scope".into(), json!(scope));
    for (k, choices) in [
        ("type", &["ad_hoc", "routine", "obligation"][..]),
        ("origin", &["manual", "system", "proposal"][..]),
        ("status", STATUSES),
    ] {
        if let Some(value) = values.get(k) {
            out.insert(k.into(), json!(choice(value, choices)?));
        }
    }
    if out.get("status") == Some(&json!("done")) {
        return Err(invalid("Use documented task completion"));
    }
    for k in [
        "taskId",
        "columnId",
        "workspaceColumnId",
        "seriesId",
        "occurrenceKey",
    ] {
        if let Some(value) = values.get(k) {
            if value.is_null() && ["workspaceColumnId", "seriesId", "occurrenceKey"].contains(&k) {
                continue;
            }
            out.insert(k.into(), json!(id(value)?));
        }
    }
    if out.contains_key("seriesId") != out.contains_key("occurrenceKey") {
        return Err(invalid(
            "Series and occurrence references must be provided together",
        ));
    }
    if let Some(s) = values.get("source").filter(|s| !s.is_null()) {
        object(
            s,
            &[
                "kind",
                "identity",
                "companySlug",
                "ref",
                "state",
                "contentHash",
                "href",
            ],
        )?;
        let company = slug(&s["companySlug"])?;
        if !scope.companies().contains(&company.as_str()) {
            return Err(invalid("Source company outside task scope"));
        }
        let hash = s["contentHash"]
            .as_str()
            .filter(|s| pattern(r"^[a-f0-9]{64}$", s))
            .ok_or_else(|| invalid("Invalid source content hash"))?;
        let href = s["href"]
            .as_str()
            .filter(|s| {
                s.encode_utf16().count() <= 500
                    && s.starts_with(&format!("/companies/{company}/"))
                    && !s.contains("..")
                    && !s.contains('#')
            })
            .ok_or_else(|| invalid("Source requires a safe company product route"))?;
        let (path, query) = href.split_once('?').unwrap_or((href, ""));
        if !pattern(r"^/[A-Za-z0-9/._:-]+$", path)
            || (!query.is_empty()
                && query
                    .split('&')
                    .any(|p| !pattern(r"^runId=[0-9]{1,12}$", p)))
        {
            return Err(invalid("Source requires a safe company product route"));
        }
        out.insert(
            "source".into(),
            json!(Source {
                kind: id(&s["kind"])?,
                identity: id(&s["identity"])?,
                company_slug: company,
                reference: opaque(&s["ref"])?,
                state: choice(&s["state"], &["open", "resolved", "unknown"])?,
                content_hash: hash.into(),
                href: href.into()
            }),
        );
    }
    Ok(Value::Object(out))
}
/// The JS contract orders property names by UTF-16 code units, including non-BMP text.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(m) => {
            let mut keys = m.keys().collect::<Vec<_>>();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.iter()
                    .map(|k| format!("{}:{}", json!(k), canonical_json(&m[*k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical_json).collect::<Vec<_>>().join(",")
        ),
        Value::Number(n) => ryu_js::Buffer::new()
            .format(n.as_f64().expect("finite JSON number"))
            .to_string(),
        _ => value.to_string(),
    }
}
pub fn payload_hash(value: &Value) -> Result<String, Error> {
    let canonical = canonical_json(value);
    if canonical.encode_utf16().count() > 1_048_576 {
        return Err(invalid("Task payload must be bounded JSON"));
    }
    Ok(crate::sha256_bytes(canonical.as_bytes()))
}
