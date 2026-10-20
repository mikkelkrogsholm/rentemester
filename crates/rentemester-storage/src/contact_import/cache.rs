use super::*;
use rusqlite::types::{Value as SqlValue, ValueRef};
use time::{Date, Duration, Month};
const FIELDS: &[(&str, &str)] = &[
    ("cvr", "cvr"),
    ("name", "name"),
    ("address", "address"),
    ("postal_code", "postalCode"),
    ("city", "city"),
    ("municipality_code", "municipalityCode"),
    ("company_form_code", "companyFormCode"),
    ("company_form_short", "companyFormShort"),
    ("company_form_long", "companyFormLong"),
    ("status", "status"),
    ("industry_code", "industryCode"),
    ("industry_text", "industryText"),
    ("email", "email"),
    ("phone", "phone"),
    ("website", "website"),
    ("start_date", "startDate"),
    ("fiscal_year_start", "fiscalYearStart"),
    ("fiscal_year_end", "fiscalYearEnd"),
    ("audit_waived", "auditWaived"),
    ("share_capital", "shareCapital"),
    ("share_capital_currency", "shareCapitalCurrency"),
    ("employees", "employees"),
    ("advertising_protected", "advertisingProtected"),
];
#[derive(Clone)]
pub(crate) struct Cached {
    pub company: CvrCompany,
    pub expires_at: String,
    pub fetched_at: String,
}
impl Cached {
    pub fn fresh(&self, now: &str) -> bool {
        rentemester_engine::company_knowledge::iso(&self.expires_at)
            .is_ok_and(|expiry| expiry.as_str() >= now)
    }
}
pub(crate) fn read(tx: &Transaction<'_>, cvr: &str) -> Result<Option<Cached>, Error> {
    let mut q = tx
        .prepare("SELECT * FROM cvr_lookups WHERE cvr=?1")
        .map_err(sql)?;
    let mut rows = q.query([cvr]).map_err(sql)?;
    let Some(row) = rows.next().map_err(sql)? else {
        return Ok(None);
    };
    let mut object = serde_json::Map::new();
    for (column, key) in FIELDS {
        let value = match row.get_ref(*column).map_err(sql)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(v) => json!(v),
            ValueRef::Real(v) => json!(v),
            ValueRef::Text(v) => json!(std::str::from_utf8(v).map_err(sql)?),
            _ => return Err(sql("cache cell")),
        };
        object.insert((*key).into(), value);
    }
    if object["name"].is_null() {
        object.insert("name".into(), json!("Ukendt"));
    }
    for key in ["auditWaived", "advertisingProtected"] {
        if !object[key].is_null() || key == "advertisingProtected" {
            object.insert(key.into(), json!(object[key] == 1));
        }
    }
    let raw: Option<String> = row.get("management_json").map_err(sql)?;
    let management = raw
        .and_then(|s| {
            serde_json::from_str::<Vec<rentemester_engine::contact_import::Management>>(&s).ok()
        })
        .unwrap_or_default();
    object.insert("management".into(), json!(management));
    let month = object["fiscalYearStart"]
        .as_str()
        .filter(|s| s.is_ascii() && s.len() == 7 && s.starts_with("--") && s.as_bytes()[4] == b'-')
        .and_then(|s| s[2..4].parse::<u8>().ok())
        .filter(|m| (1..=12).contains(m));
    object.insert("fiscalYearStartMonth".into(), json!(month));
    Ok(Some(Cached {
        company: serde_json::from_value(Value::Object(object)).map_err(sql)?,
        expires_at: row.get("expires_at").map_err(sql)?,
        fetched_at: row.get("fetched_at").map_err(sql)?,
    }))
}
pub(crate) fn store(
    tx: &Transaction<'_>,
    company: &CvrCompany,
    entity: &Value,
    now: &str,
) -> Result<(), Error> {
    let object = serde_json::to_value(company).map_err(sql)?;
    let mut values = vec![];
    for (_, key) in FIELDS {
        values.push(match &object[*key] {
            Value::Null => SqlValue::Null,
            Value::String(v) => SqlValue::Text(v.clone()),
            Value::Bool(v) => SqlValue::Integer(i64::from(*v)),
            Value::Number(v) => {
                if let Some(n) = v.as_i64() {
                    SqlValue::Integer(n)
                } else {
                    SqlValue::Real(v.as_f64().ok_or_else(|| sql("cache number"))?)
                }
            }
            _ => return Err(sql("cache shape")),
        });
    }
    // `now` is already canonical UTC milliseconds; calendar arithmetic needs no optional time features.
    let date = Date::from_calendar_date(
        now[..4].parse().map_err(sql)?,
        Month::try_from(now[5..7].parse::<u8>().map_err(sql)?).map_err(sql)?,
        now[8..10].parse().map_err(sql)?,
    )
    .map_err(sql)?;
    let expiry = date
        .checked_add(Duration::days(30))
        .ok_or_else(|| sql("cache expiry"))?;
    let expires = format!(
        "{:04}-{:02}-{:02}{}",
        expiry.year(),
        u8::from(expiry.month()),
        expiry.day(),
        &now[10..]
    );
    values.extend([
        SqlValue::Text(serde_json::to_string(&company.management).map_err(sql)?),
        SqlValue::Text(serde_json::to_string(entity).map_err(sql)?),
        SqlValue::Text(now.into()),
        SqlValue::Text(expires),
    ]);
    let columns = FIELDS
        .iter()
        .map(|(c, _)| *c)
        .chain([
            "management_json",
            "raw_response",
            "fetched_at",
            "expires_at",
        ])
        .collect::<Vec<_>>();
    let updates = columns
        .iter()
        .skip(1)
        .map(|c| format!("{c}=excluded.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    let placeholders = (1..=columns.len())
        .map(|n| format!("?{n}"))
        .collect::<Vec<_>>()
        .join(",");
    tx.execute(&format!("INSERT INTO cvr_lookups({})VALUES({placeholders})ON CONFLICT(cvr)DO UPDATE SET {updates}",columns.join(",")),rusqlite::params_from_iter(values)).map_err(sql)?;
    Ok(())
}
