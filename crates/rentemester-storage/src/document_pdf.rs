//! Append-only PDF evidence on caller-authorized transactions. Physical source checks belong to the namespace owner.
use rentemester_engine::{
    Error,
    document_pdf::{self as pdf, OperationalError, ParseResult, ParserIdentity},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
fn bad<T>(_: T) -> Error {
    pdf::tampered()
}
fn canonical<T: Serialize>(v: &T) -> Result<String, Error> {
    let value = serde_json::to_value(v).map_err(bad)?;
    let raw = crate::intercompany_dispositions::canonical(&value);
    if raw.len() > pdf::MAX_JSON {
        return Err(pdf::tampered());
    }
    Ok(raw)
}
fn hash<T: Serialize>(v: &T) -> Result<String, Error> {
    Ok(crate::intercompany::sha(&canonical(v)?))
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    #[serde(flatten)]
    pub result: ParseResult,
    pub cached: bool,
    pub parse_id: i64,
    pub result_hash: String,
}
impl Run {
    pub fn summary(&self, document_id: i64) -> Value {
        json!({"documentId":document_id,"status":self.result.status,"errorCode":self.result.error_code,"cached":self.cached,"pageCount":self.result.pages.len(),"itemCount":self.result.item_count(),"textLength":self.result.text_length(),"resultHash":self.result_hash})
    }
}
#[derive(Clone, Debug)]
pub struct Prepared {
    document_id: i64,
    source_hash: String,
    identity: ParserIdentity,
    cached: Option<Run>,
}
impl Prepared {
    pub fn id(&self) -> i64 {
        self.document_id
    }
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }
    pub fn identity(&self) -> &ParserIdentity {
        &self.identity
    }
    pub fn cached(&self) -> Option<&Run> {
        self.cached.as_ref()
    }
}
fn source(db: &Connection, id: i64) -> Result<String, Error> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> = db
        .query_row(
            "SELECT sha256_hash,mime_type,stored_path FROM documents WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(bad)?;
    match row {
        Some((Some(hash), Some(mime), Some(path)))
            if id > 0 && pdf::hex(&hash) && mime == "application/pdf" && !path.is_empty() =>
        {
            Ok(hash)
        }
        _ => Err(Error::new(
            "source_invalid",
            "Registered PDF original required",
        )),
    }
}
#[derive(Clone)]
struct Stored {
    id: i64,
    document: i64,
    source: String,
    identity: ParserIdentity,
    status: String,
    error: Option<String>,
    pages: i64,
    items: i64,
    text: i64,
    raw: String,
    hash: String,
    at: String,
}
fn stored(db: &Connection, id: i64) -> Result<Stored, Error> {
    db.query_row("SELECT id,document_id,source_sha256_hash,parser_id,parser_version,contract_version,status,error_code,page_count,item_count,text_length,result_json,result_sha256_hash,created_at FROM document_pdf_parse_results WHERE id=?1",[id],|r|Ok(Stored{id:r.get(0)?,document:r.get(1)?,source:r.get(2)?,identity:ParserIdentity{parser_id:r.get(3)?,parser_version:r.get(4)?,contract_version:r.get(5)?},status:r.get(6)?,error:r.get(7)?,pages:r.get(8)?,items:r.get(9)?,text:r.get(10)?,raw:r.get(11)?,hash:r.get(12)?,at:r.get(13)?})).map_err(bad)
}
fn time(at: &str) -> Result<(), Error> {
    let canonical = if at.len() == 19 && at.as_bytes()[10] == b' ' {
        format!("{}T{}.000Z", &at[..10], &at[11..])
    } else {
        at.into()
    };
    crate::cookie_session::timestamp(&canonical)
        .map(|_| ())
        .map_err(bad)
}
fn hydrate(db: &Connection, s: &Stored, cached: bool) -> Result<Run, Error> {
    let result = pdf::decode(s.raw.as_bytes())?;
    result.validate(&s.source, &s.identity)?;
    time(&s.at)?;
    if s.id <= 0
        || source(db, s.document).map_err(bad)? != s.source
        || canonical(&result)? != s.raw
        || hash(&result)? != s.hash
        || serde_json::to_value(result.status).map_err(bad)? != s.status
        || serde_json::to_value(result.error_code).map_err(bad)?
            != s.error.clone().map_or(Value::Null, Value::String)
        || s.pages != result.pages.len() as i64
        || s.items != result.item_count() as i64
        || s.text != result.text_length() as i64
    {
        return Err(pdf::tampered());
    }
    let mut q=db.prepare("SELECT page_number,width,height,rotation,text,layout_json,item_count,page_sha256_hash FROM document_pdf_parse_pages WHERE result_id=?1 ORDER BY page_number").map_err(bad)?;
    let mut rows = q.query([s.id]).map_err(bad)?;
    let mut index = 0;
    while let Some(r) = rows.next().map_err(bad)? {
        let page = result.pages.get(index).ok_or_else(pdf::tampered)?;
        let (number, width, height, rotation, text, layout, count, page_hash): (
            i64,
            f64,
            f64,
            i64,
            String,
            String,
            i64,
            String,
        ) = (
            r.get(0).map_err(bad)?,
            r.get(1).map_err(bad)?,
            r.get(2).map_err(bad)?,
            r.get(3).map_err(bad)?,
            r.get(4).map_err(bad)?,
            r.get(5).map_err(bad)?,
            r.get(6).map_err(bad)?,
            r.get(7).map_err(bad)?,
        );
        if number != page.page_number as i64
            || width != page.width
            || height != page.height
            || rotation != i64::from(page.rotation)
            || text != page.text
            || layout != canonical(&page.layout)?
            || count != page.layout.len() as i64
            || page_hash != hash(page)?
        {
            return Err(pdf::tampered());
        }
        index += 1;
    }
    if index != result.pages.len() {
        return Err(pdf::tampered());
    }
    Ok(Run {
        result,
        cached,
        parse_id: s.id,
        result_hash: s.hash.clone(),
    })
}
fn ids(db: &Connection, query: &str) -> Result<Vec<i64>, Error> {
    let mut q = db.prepare(query).map_err(bad)?;
    q.query_map([], |r| r.get(0))
        .map_err(bad)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)
}
/// Full retained history, independent of current parser selection. Never reparses or repairs evidence.
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    for id in ids(db, "SELECT id FROM document_pdf_parse_results ORDER BY id")? {
        let s = stored(db, id)?;
        hydrate(db, &s, true)?;
        let parsed:i64=db.query_row("SELECT count(*) FROM document_pdf_parse_attempts WHERE outcome='parsed' AND result_id=?1",[id],|r|r.get(0)).map_err(bad)?;
        let audited:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM audit_log WHERE event_type='document_pdf_parse_result' AND entity_type='document' AND entity_id=?1 AND message=?2)",params![s.document.to_string(),format!("PDF parse result {}",s.hash)],|r|r.get(0)).map_err(bad)?;
        if parsed != 1 || !audited {
            return Err(pdf::tampered());
        }
    }
    let mut q=db.prepare("SELECT id,document_id,source_sha256_hash,parser_id,parser_version,contract_version,outcome,error_code,result_id,actor,program,created_at FROM document_pdf_parse_attempts ORDER BY id").map_err(bad)?;
    let mut rows = q.query([]).map_err(bad)?;
    while let Some(r) = rows.next().map_err(bad)? {
        let id: i64 = r.get(0).map_err(bad)?;
        let doc: i64 = r.get(1).map_err(bad)?;
        let src: Option<String> = r.get(2).map_err(bad)?;
        let identity = ParserIdentity {
            parser_id: r.get(3).map_err(bad)?,
            parser_version: r.get(4).map_err(bad)?,
            contract_version: r.get(5).map_err(bad)?,
        };
        identity.validate()?;
        let outcome: String = r.get(6).map_err(bad)?;
        let error: Option<String> = r.get(7).map_err(bad)?;
        let result: Option<i64> = r.get(8).map_err(bad)?;
        let actor: String = r.get(9).map_err(bad)?;
        let program: String = r.get(10).map_err(bad)?;
        let at: String = r.get(11).map_err(bad)?;
        time(&at)?;
        if id <= 0
            || doc <= 0
            || actor.is_empty()
            || program.is_empty()
            || src.as_ref().is_some_and(|h| !pdf::hex(h))
        {
            return Err(pdf::tampered());
        }
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM documents WHERE id=?1)",
                [doc],
                |r| r.get(0),
            )
            .map_err(bad)?;
        if !exists {
            return Err(pdf::tampered());
        }
        match outcome.as_str() {
            "parsed" | "cached" => {
                let s = stored(db, result.ok_or_else(pdf::tampered)?)?;
                if error.is_some()
                    || s.document != doc
                    || Some(s.source.as_str()) != src.as_deref()
                    || s.identity != identity
                {
                    return Err(pdf::tampered());
                }
                if outcome == "parsed" {
                    let audit_actor = if actor == "system" && program == "system" {
                        "system".into()
                    } else {
                        format!("{actor} via {program}")
                    };
                    let valid:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM audit_log WHERE event_type='document_pdf_parse_result' AND entity_type='document' AND entity_id=?1 AND message=?2 AND actor=?3)",params![doc.to_string(),format!("PDF parse result {}",s.hash),audit_actor],|r|r.get(0)).map_err(bad)?;
                    if !valid {
                        return Err(pdf::tampered());
                    }
                }
            }
            "failed" => {
                if result.is_some() {
                    return Err(pdf::tampered());
                }
                let error = error.ok_or_else(pdf::tampered)?;
                if identity.contract_version == pdf::LEGACY_CONTRACT {
                    let _: pdf::ErrorCode = serde_json::from_value(json!(error)).map_err(bad)?;
                } else {
                    let _: OperationalError = serde_json::from_value(json!(error)).map_err(bad)?;
                }
            }
            _ => return Err(pdf::tampered()),
        }
    }
    let orphan:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM document_pdf_parse_pages p LEFT JOIN document_pdf_parse_results r ON r.id=p.result_id WHERE r.id IS NULL)",[],|r|r.get(0)).map_err(bad)?;
    if orphan {
        return Err(pdf::tampered());
    }
    Ok(())
}
pub fn prepare(
    db: &Connection,
    id: i64,
    source_hash: &str,
    identity: &ParserIdentity,
) -> Result<Prepared, Error> {
    identity.validate()?;
    validate_history(db)?;
    if source(db, id)? != source_hash {
        return Err(pdf::tampered());
    }
    let found:Option<i64>=db.query_row("SELECT id FROM document_pdf_parse_results WHERE document_id=?1 AND source_sha256_hash=?2 AND parser_id=?3 AND parser_version=?4 AND contract_version=?5",params![id,source_hash,identity.parser_id,identity.parser_version,identity.contract_version],|r|r.get(0)).optional().map_err(bad)?;
    let cached = found
        .map(|id| hydrate(db, &stored(db, id)?, true))
        .transpose()?;
    Ok(Prepared {
        document_id: id,
        source_hash: source_hash.into(),
        identity: identity.clone(),
        cached,
    })
}
#[allow(clippy::too_many_arguments)]
fn attempt(
    tx: &Transaction<'_>,
    p: &Prepared,
    outcome: &str,
    error: Option<OperationalError>,
    result: Option<i64>,
    actor: &str,
    program: &str,
    at: &str,
) -> Result<(), Error> {
    crate::idempotency::writer(tx)?;
    time(at)?;
    if actor.is_empty() || program.is_empty() {
        return Err(pdf::tampered());
    }
    let code = error
        .map(|e| serde_json::to_value(e).map_err(bad))
        .transpose()?
        .and_then(|v| v.as_str().map(str::to_owned));
    tx.execute("INSERT INTO document_pdf_parse_attempts(document_id,source_sha256_hash,parser_id,parser_version,contract_version,outcome,error_code,result_id,actor,program,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![p.document_id,p.source_hash,p.identity.parser_id,p.identity.parser_version,p.identity.contract_version,outcome,code,result,actor,program,at]).map_err(bad)?;
    Ok(())
}
pub fn record_cached(
    tx: &Transaction<'_>,
    p: &Prepared,
    actor: &str,
    program: &str,
    at: &str,
) -> Result<Run, Error> {
    let fresh = prepare(tx, p.id(), p.source_hash(), p.identity())?;
    let run = fresh.cached.ok_or_else(pdf::tampered)?;
    attempt(
        tx,
        p,
        "cached",
        None,
        Some(run.parse_id),
        actor,
        program,
        at,
    )?;
    Ok(run)
}
pub fn record_result(
    tx: &Transaction<'_>,
    p: &Prepared,
    result: &ParseResult,
    actor: &str,
    program: &str,
    at: &str,
) -> Result<Run, Error> {
    let fresh = prepare(tx, p.id(), p.source_hash(), p.identity())?;
    if fresh.cached.is_some() {
        return record_cached(tx, p, actor, program, at);
    }
    result.validate(p.source_hash(), p.identity())?;
    let raw = canonical(result)?;
    let result_hash = hash(result)?;
    crate::idempotency::writer(tx)?;
    time(at)?;
    if actor.is_empty() || program.is_empty() {
        return Err(pdf::tampered());
    }
    let status = serde_json::to_value(result.status).map_err(bad)?;
    let error = serde_json::to_value(result.error_code).map_err(bad)?;
    let id:i64=tx.query_row("INSERT INTO document_pdf_parse_results(document_id,source_sha256_hash,parser_id,parser_version,contract_version,status,error_code,page_count,item_count,text_length,result_json,result_sha256_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) RETURNING id",params![p.id(),p.source_hash(),p.identity.parser_id,p.identity.parser_version,p.identity.contract_version,status.as_str(),error.as_str(),result.pages.len() as i64,result.item_count() as i64,result.text_length() as i64,raw,result_hash,at],|r|r.get(0)).map_err(bad)?;
    for page in &result.pages {
        tx.execute("INSERT INTO document_pdf_parse_pages(result_id,page_number,width,height,rotation,text,layout_json,item_count,page_sha256_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,page.page_number as i64,page.width,page.height,page.rotation,page.text,canonical(&page.layout)?,page.layout.len() as i64,hash(page)?]).map_err(bad)?;
    }
    attempt(tx, p, "parsed", None, Some(id), actor, program, at)?;
    let audit_actor = if actor == "system" && program == "system" {
        "system".into()
    } else {
        format!("{actor} via {program}")
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES('document_pdf_parse_result','document',?1,?2,?3,?4)",params![p.id().to_string(),format!("PDF parse result {result_hash}"),audit_actor,at]).map_err(bad)?;
    Ok(Run {
        result: result.clone(),
        cached: false,
        parse_id: id,
        result_hash,
    })
}
#[allow(clippy::too_many_arguments)]
pub fn record_failure(
    tx: &Transaction<'_>,
    document_id: i64,
    source_hash: Option<&str>,
    identity: &ParserIdentity,
    error: OperationalError,
    actor: &str,
    program: &str,
    at: &str,
) -> Result<(), Error> {
    identity.validate()?;
    crate::idempotency::writer(tx)?;
    time(at)?;
    if actor.is_empty() || program.is_empty() || source_hash.is_some_and(|s| !pdf::hex(s)) {
        return Err(pdf::tampered());
    }
    if identity.contract_version == pdf::LEGACY_CONTRACT
        && matches!(
            error,
            OperationalError::WorkerUnavailable | OperationalError::MemoryLimit
        )
    {
        return Err(pdf::tampered());
    }
    let code = serde_json::to_value(error).map_err(bad)?;
    tx.execute("INSERT INTO document_pdf_parse_attempts(document_id,source_sha256_hash,parser_id,parser_version,contract_version,outcome,error_code,result_id,actor,program,created_at) VALUES(?1,?2,?3,?4,?5,'failed',?6,NULL,?7,?8,?9)",params![document_id,source_hash,identity.parser_id,identity.parser_version,identity.contract_version,code.as_str(),actor,program,at]).map_err(bad)?;
    Ok(())
}
fn latest(db: &Connection, id: i64) -> Result<Option<(Stored, Run)>, Error> {
    validate_history(db)?;
    let found:Option<i64>=db.query_row("SELECT id FROM document_pdf_parse_results WHERE document_id=?1 ORDER BY id DESC LIMIT 1",[id],|r|r.get(0)).optional().map_err(bad)?;
    found
        .map(|id| {
            let s = stored(db, id)?;
            let run = hydrate(db, &s, true)?;
            Ok((s, run))
        })
        .transpose()
}
fn status_value(s: &Stored, run: &Run) -> Value {
    json!({"documentId":s.document,"sourceSha256":s.source,"parserId":s.identity.parser_id,"parserVersion":s.identity.parser_version,"contractVersion":s.identity.contract_version,"status":run.result.status,"errorCode":run.result.error_code,"pageCount":run.result.pages.len(),"itemCount":run.result.item_count(),"textLength":run.result.text_length(),"resultHash":run.result_hash})
}
pub fn status(db: &Connection, id: i64) -> Result<Option<Value>, Error> {
    Ok(latest(db, id)?.map(|(s, r)| status_value(&s, &r)))
}
pub fn text(db: &Connection, id: i64, offset: usize, limit: usize) -> Result<Value, Error> {
    if limit == 0 || limit > 10 {
        return Err(Error::new("invalid_input", "PDF page limit must be 1..10"));
    }
    let Some((s, run)) = latest(db, id)? else {
        return Ok(
            json!({"parse":null,"pages":[],"offset":offset,"limit":limit,"nextOffset":null}),
        );
    };
    let pages=run.result.pages.iter().skip(offset).take(limit).map(|p|Ok(json!({"pageNumber":p.page_number,"width":p.width,"height":p.height,"rotation":p.rotation,"text":p.text,"itemCount":p.layout.len(),"layoutHash":hash(&p.layout)?}))).collect::<Result<Vec<_>,Error>>()?;
    let end = offset.saturating_add(pages.len());
    Ok(
        json!({"parse":status_value(&s,&run),"pages":pages,"offset":offset,"limit":limit,"nextOffset":(end<run.result.pages.len()).then_some(end)}),
    )
}
pub fn pending(
    db: &Connection,
    identity: &ParserIdentity,
    cursor: i64,
    limit: usize,
) -> Result<Value, Error> {
    identity.validate()?;
    validate_history(db)?;
    if cursor < 0 || limit == 0 || limit > 100 {
        return Err(Error::new("invalid_input", "PDF cursor/limit invalid"));
    }
    let mut q=db.prepare("SELECT d.id FROM documents d WHERE d.id>?1 AND d.mime_type='application/pdf' AND d.stored_path IS NOT NULL AND d.sha256_hash IS NOT NULL AND NOT EXISTS(SELECT 1 FROM document_pdf_parse_results p WHERE p.document_id=d.id AND p.source_sha256_hash=d.sha256_hash AND p.parser_id=?2 AND p.parser_version=?3 AND p.contract_version=?4) ORDER BY d.id LIMIT ?5").map_err(bad)?;
    let ids = q
        .query_map(
            params![
                cursor,
                identity.parser_id,
                identity.parser_version,
                identity.contract_version,
                limit as i64
            ],
            |r| r.get::<_, i64>(0),
        )
        .map_err(bad)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)?;
    Ok(
        json!({"documentIds":ids,"cursor":cursor,"nextCursor":(ids.len()==limit).then(||ids.last().copied()).flatten()}),
    )
}
#[cfg(test)]
mod tests;
