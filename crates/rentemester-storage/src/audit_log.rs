//! Bounded, read-only inspection of the company audit history.
use rentemester_engine::Error;
use rusqlite::{Transaction, params};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use time::{Date, Month, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

/// Filters are ANDed. `%` and `_` retain their SQL LIKE wildcard meaning.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Query {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub offset: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub event_type_like: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub actor_like: Option<String>,
}
// An omitted filter is allowed, but an explicitly null filter is not.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn invalid() -> Error {
    Error::new("invalid_audit_log_query", "invalid audit history filters")
}
fn unavailable(_: rusqlite::Error) -> Error {
    Error::new("audit_log_unavailable", "audit history could not be read")
}
fn nonempty(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}
struct Bound {
    instant: OffsetDateTime,
    exclusive: bool,
}
fn bound(value: &str, upper: bool) -> Result<Bound, Error> {
    if value.len() > 64 || value.chars().any(char::is_control) {
        return Err(invalid());
    }
    if value.len() == 10 {
        let b = value.as_bytes();
        if b[4] != b'-'
            || b[7] != b'-'
            || !b
                .iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        {
            return Err(invalid());
        }
        let year = value[..4].parse().map_err(|_| invalid())?;
        if year < 1 {
            return Err(invalid());
        }
        let month = Month::try_from(value[5..7].parse::<u8>().map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        let day = value[8..].parse().map_err(|_| invalid())?;
        let date = Date::from_calendar_date(year, month, day).map_err(|_| invalid())?;
        let date = if upper {
            date.next_day().ok_or_else(invalid)?
        } else {
            date
        };
        Ok(Bound {
            instant: date.midnight().assume_utc(),
            exclusive: upper,
        })
    } else {
        let instant = OffsetDateTime::parse(value, &Rfc3339)
            .map_err(|_| invalid())?
            .to_offset(UtcOffset::UTC);
        if !(0..=9999).contains(&instant.year()) {
            return Err(invalid());
        }
        Ok(Bound {
            instant,
            exclusive: false,
        })
    }
}
fn timestamp(value: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.nanosecond()
    )
}
impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=5000).contains(&self.limit.unwrap_or(500))
            || self.offset.unwrap_or(0) > 9_007_199_254_740_991
        {
            return Err(invalid());
        }
        for text in [&self.event_type_like, &self.actor_like]
            .into_iter()
            .filter_map(nonempty)
        {
            if text.chars().count() > 256 || text.chars().any(char::is_control) {
                return Err(invalid());
            }
        }
        if [
            &self.from_date,
            &self.to_date,
            &self.event_type_like,
            &self.actor_like,
        ]
        .iter()
        .filter_map(|v| v.as_ref())
        .map(|v| v.len())
        .sum::<usize>()
            > 4096
        {
            return Err(invalid());
        }
        let lower = nonempty(&self.from_date)
            .map(|s| bound(s, false))
            .transpose()?;
        let upper = nonempty(&self.to_date)
            .map(|s| bound(s, true))
            .transpose()?;
        if let (Some(l), Some(u)) = (lower, upper)
            && (l.instant > u.instant || (u.exclusive && l.instant == u.instant))
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Count and page share the caller's transaction. Separate requests observe separate snapshots.
pub fn read(tx: &Transaction<'_>, slug: &str, query: &Query) -> Result<Value, Error> {
    query.validate()?;
    let lower = nonempty(&query.from_date)
        .map(|s| bound(s, false))
        .transpose()?;
    let upper = nonempty(&query.to_date)
        .map(|s| bound(s, true))
        .transpose()?;
    let from = lower.map(|b| timestamp(b.instant));
    let until = upper.as_ref().map(|b| timestamp(b.instant));
    let exclusive = upper.is_some_and(|b| b.exclusive);
    let event = nonempty(&query.event_type_like).map(|v| format!("%{}%", v.to_lowercase()));
    let actor = nonempty(&query.actor_like).map(|v| format!("%{}%", v.to_lowercase()));
    let filters = "WHERE (?1 IS NULL OR julianday(created_at) >= julianday(?1)) AND (?2 IS NULL OR (CASE WHEN ?3 THEN julianday(created_at) < julianday(?2) ELSE julianday(created_at) <= julianday(?2) END)) AND (?4 IS NULL OR LOWER(event_type) LIKE ?4) AND (?5 IS NULL OR LOWER(actor) LIKE ?5)";
    let total: i64 = tx
        .query_row(
            &format!("SELECT COUNT(*) FROM audit_log {filters}"),
            params![from, until, exclusive, event, actor],
            |row| row.get(0),
        )
        .map_err(unavailable)?;
    let total = u64::try_from(total).map_err(|_| invalid())?;
    let limit = query.limit.unwrap_or(500);
    let offset = query.offset.unwrap_or(0);
    let sql_limit = i64::try_from(limit).map_err(|_| invalid())?;
    let sql_offset = i64::try_from(offset).map_err(|_| invalid())?;
    let mut statement = tx.prepare(&format!("SELECT id,event_type,entity_type,entity_id,message,actor,created_at FROM audit_log {filters} ORDER BY created_at DESC,id DESC LIMIT ?6 OFFSET ?7")).map_err(unavailable)?;
    let rows = statement.query_map(params![from,until,exclusive,event,actor,sql_limit,sql_offset], |row| Ok(json!({
        "id":row.get::<_,i64>(0)?, "eventType":row.get::<_,String>(1)?, "entityType":row.get::<_,String>(2)?, "entityId":row.get::<_,Option<String>>(3)?, "message":row.get::<_,String>(4)?, "actor":row.get::<_,String>(5)?, "createdAt":row.get::<_,String>(6)?
    }))).map_err(unavailable)?.collect::<Result<Vec<_>,_>>().map_err(unavailable)?;
    let count = rows.len() as u64;
    let has_more = offset + count < total;
    let mut result = json!({"ok":true,"slug":slug,"rows":rows,"total":total,"count":count,"limit":limit,"offset":offset,"hasMore":has_more});
    if has_more {
        result["nextOffset"] = json!(offset + count);
    }
    // Keep successful pages within the shared native transport budget. Never
    // truncate messages or rows: callers can explicitly request a smaller page.
    let bytes = serde_json::to_vec(&result).map_err(|_| {
        Error::new(
            "audit_log_unavailable",
            "audit history could not be encoded",
        )
    })?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::new(
            "audit_log_page_too_large",
            "audit history page exceeds 8 MiB; request fewer rows",
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
