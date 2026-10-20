//! Immutable extraction history on an admitted caller transaction. Namespace physical proof belongs to the host.
use rentemester_engine::{
    Error,
    invoice_extraction::{
        self as engine, Context, ProviderFailure, ProviderIdentity, ProviderOutput, SelectedBuyer,
        SuppliedMetadata,
    },
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn bad<T>(_: T) -> Error {
    engine::tampered()
}
fn canonical(v: &Value) -> String {
    crate::intercompany_dispositions::canonical(v)
}
fn hash(v: &Value) -> String {
    crate::intercompany::sha(&canonical(v))
}
fn bytes_hash(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn decode(raw: &str) -> Result<Value, Error> {
    serde_json::from_str(raw).map_err(bad)
}
fn time(at: &str) -> Result<(), Error> {
    crate::cookie_session::timestamp(at)
        .map(|_| ())
        .map_err(bad)
}
fn author(actor: &str, at: &str) -> Result<(), Error> {
    time(at)?;
    if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
        Err(engine::tampered())
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct ExtractionSource {
    pub document_id: i64,
    pub sha256: String,
    pub pdf_bytes: Vec<u8>,
}
type DocumentRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<f64>,
);
fn document(db: &Connection, id: i64) -> Result<(String, Context), Error> {
    let (sha,mime,path,invoice,currency,gross):DocumentRow=db.query_row("SELECT sha256_hash,mime_type,stored_path,invoice_no,currency,amount_inc_vat FROM documents WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(bad)?;
    if id <= 0
        || sha.len() != 64
        || !sha
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || mime.as_deref() != Some("application/pdf")
        || path.is_none_or(|p| p.is_empty())
        || gross.is_some_and(|v| !v.is_finite())
    {
        return Err(engine::tampered());
    }
    let buyer = db
        .query_row(
            "SELECT name,country,cvr FROM companies WHERE id=1",
            [],
            |r| {
                Ok(SelectedBuyer {
                    name: r.get(0)?,
                    country: r.get(1)?,
                    legal_id: r.get(2)?,
                })
            },
        )
        .map_err(bad)?;
    Ok((
        sha,
        Context {
            supplied_metadata: SuppliedMetadata {
                invoice_number: invoice,
                currency,
                gross_amount: gross,
            },
            selected_buyer: buyer,
            confidence_threshold: 0.8,
        },
    ))
}
fn context_value(c: &Context) -> Result<Value, Error> {
    serde_json::to_value(c).map_err(bad)
}
fn source_rows(db: &Connection) -> Result<Vec<ExtractionSource>, Error> {
    let mut q=db.prepare("SELECT id,document_id,company_id,sha256_hash,pdf_bytes,byte_length,created_at FROM invoice_extraction_documents ORDER BY id").map_err(bad)?;
    let mut rows = q.query([]).map_err(bad)?;
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(bad)? {
        let id: i64 = r.get(0).map_err(bad)?;
        let document_id: i64 = r.get(1).map_err(bad)?;
        let company: i64 = r.get(2).map_err(bad)?;
        let sha256: String = r.get(3).map_err(bad)?;
        let pdf_bytes: Vec<u8> = r.get(4).map_err(bad)?;
        let len: i64 = r.get(5).map_err(bad)?;
        let at: String = r.get(6).map_err(bad)?;
        time(&at)?;
        if id <= 0
            || company != 1
            || pdf_bytes.is_empty()
            || pdf_bytes.len() > engine::MAX_PDF
            || len != pdf_bytes.len() as i64
            || bytes_hash(&pdf_bytes) != sha256
            || document(db, document_id)?.0 != sha256
        {
            return Err(engine::tampered());
        }
        out.push(ExtractionSource {
            document_id,
            sha256,
            pdf_bytes,
        });
    }
    Ok(out)
}
#[derive(Clone, Debug)]
pub struct Prepared {
    document_id: i64,
    source_hash: String,
    identity: ProviderIdentity,
    context: Context,
    context_hash: String,
    attempt_id: i64,
    cached: Option<Value>,
}
impl Prepared {
    pub fn document_id(&self) -> i64 {
        self.document_id
    }
    pub fn source_hash(&self) -> &str {
        &self.source_hash
    }
    pub fn identity(&self) -> &ProviderIdentity {
        &self.identity
    }
    pub fn context_hash(&self) -> &str {
        &self.context_hash
    }
    pub fn attempt_id(&self) -> i64 {
        self.attempt_id
    }
    pub fn cached(&self) -> Option<&Value> {
        self.cached.as_ref()
    }
}
struct Loaded {
    surface: Value,
    identity: ProviderIdentity,
    context: Option<Context>,
    terminal: Option<Value>,
    start: Option<Value>,
}
fn audit(db: &Connection, event: &str, id: i64) -> Result<Vec<(String, Option<String>)>, Error> {
    let mut q=db.prepare("SELECT message,actor FROM audit_log WHERE event_type=?1 AND entity_type='invoice_extraction_attempt' AND entity_id=?2 ORDER BY id").map_err(bad)?;
    q.query_map(params![event, id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(bad)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)
}
fn load(db: &Connection, id: i64) -> Result<Loaded, Error> {
    let (doc,sha,provider,version,state,at,actor):(i64,String,String,String,String,String,Option<String>)=db.query_row("SELECT d.document_id,d.sha256_hash,a.extractor_id,a.extractor_version,a.status,a.created_at,a.initiated_by FROM invoice_extraction_attempts a JOIN invoice_extraction_documents d ON d.id=a.extraction_document_id WHERE a.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(bad)?;
    time(&at)?;
    if id <= 0 || state != "started" {
        return Err(engine::tampered());
    }
    let audits = audit(db, "invoice_extraction_attempt", id)?;
    if audits.len() != 1 || (actor.is_some() && audits[0].1 != actor) {
        return Err(engine::tampered());
    }
    let start = if audits[0].0 == "Invoice extraction attempt started" {
        None
    } else {
        let v = decode(&audits[0].0)?;
        if v["version"] != engine::NATIVE_CODEC
            || canonical(&v) != audits[0].0
            || v["documentId"] != doc
            || v["sourceSha256"] != sha
            || v["actor"] != json!(actor)
        {
            return Err(engine::tampered());
        }
        Some(v)
    };
    let provenance = actor.clone().or_else(|| audits[0].1.clone());
    let result:Option<(i64,String,String,Option<String>)>=db.query_row("SELECT id,result_json,created_at,initiated_by FROM invoice_extraction_results WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(bad)?;
    let mut fields = vec![];
    let mut header = None;
    if let Some((rid, raw, rat, ra)) = result {
        time(&rat)?;
        if rid <= 0
            || rat < at
            || (ra.is_some() && ra != provenance)
            || (start.is_some() && ra != actor)
        {
            return Err(engine::tampered());
        }
        let value = decode(&raw)?;
        let mut q=db.prepare("SELECT id,field_key,value_json,confidence,page_number,source_text,box_json FROM invoice_extraction_fields WHERE result_id=?1 ORDER BY id").map_err(bad)?;
        let mut rr = q.query([rid]).map_err(bad)?;
        while let Some(f) = rr.next().map_err(bad)? {
            let fid: i64 = f.get(0).map_err(bad)?;
            if fid <= 0 {
                return Err(engine::tampered());
            }
            let mut field = json!({"key":f.get::<_,String>(1).map_err(bad)?,"value":decode(&f.get::<_,String>(2).map_err(bad)?)?,"confidence":f.get::<_,f64>(3).map_err(bad)?,"page":f.get::<_,i64>(4).map_err(bad)?,"sourceText":f.get::<_,String>(5).map_err(bad)?});
            if let Some(raw) = f.get::<_, Option<String>>(6).map_err(bad)? {
                field["box"] = decode(&raw)?;
            }
            fields.push(field);
        }
        header = Some((value, raw));
    }
    let checked = engine::fields(&fields).map_err(bad)?;
    let mut resolutions = vec![];
    let mut terminal = None;
    let mut failure = None;
    let mut q=db.prepare("SELECT id,field_key,resolution_json,resolved_by,created_at FROM invoice_extraction_resolutions WHERE attempt_id=?1 ORDER BY id").map_err(bad)?;
    let mut rr = q.query([id]).map_err(bad)?;
    while let Some(r) = rr.next().map_err(bad)? {
        let rid: i64 = r.get(0).map_err(bad)?;
        let key: String = r.get(1).map_err(bad)?;
        let raw: String = r.get(2).map_err(bad)?;
        let by: Option<String> = r.get(3).map_err(bad)?;
        let rat: String = r.get(4).map_err(bad)?;
        time(&rat)?;
        if rid <= 0 || rat < at || by != provenance {
            return Err(engine::tampered());
        }
        let value = decode(&raw)?;
        match key.as_str() {
            "provider_failure" => {
                if failure.replace(value.clone()).is_some()
                    || terminal.is_some()
                    || value.as_object().is_none_or(|m| m.len() != 1)
                    || !["EXTRACTION_PROVIDER_UNAVAILABLE", "EXTRACTION_UNSUPPORTED"]
                        .contains(&value["code"].as_str().unwrap_or(""))
                {
                    return Err(engine::tampered());
                }
            }
            "extraction_status" => {
                if terminal.replace(value.clone()).is_some()
                    || value.as_object().is_none_or(|m| m.len() != 2)
                    || !["completed", "needs_resolution"]
                        .contains(&value["status"].as_str().unwrap_or(""))
                    || value["errors"]
                        .as_array()
                        .is_none_or(|a| a.iter().any(|v| v.as_str().is_none()))
                {
                    return Err(engine::tampered());
                }
            }
            _ => return Err(engine::tampered()),
        }
        resolutions.push(json!({"fieldKey":key,"value":value}));
    }
    let mut identity = ProviderIdentity {
        id: provider.clone(),
        version: version.clone(),
    };
    let mut context = None;
    if let Some((h, raw)) = &header {
        identity = serde_json::from_value(h["extractor"].clone()).map_err(bad)?;
        identity.validate()?;
        if engine::retry_number(&version, &identity.version).is_none()
            || identity.id
                != db
                    .query_row(
                        "SELECT extractor_id FROM invoice_extraction_attempts WHERE id=?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                    .map_err(bad)?
        {
            return Err(engine::tampered());
        }
        if start.is_some() {
            let c: Context = serde_json::from_value(h["context"].clone()).map_err(bad)?;
            if h.as_object().is_none_or(|m| m.len() != 6)
                || h["version"] != engine::NATIVE_CODEC
                || h["sourceSha256"] != sha
                || h["contextHash"] != hash(&context_value(&c)?)
                || h["fieldsHash"] != hash(&json!(checked))
                || canonical(h) != *raw
                || c.confidence_threshold != 0.8
                || start.as_ref().is_none_or(|s| {
                    s["context"] != h["context"]
                        || s["contextHash"] != h["contextHash"]
                        || s["extractor"] != h["extractor"]
                })
            {
                return Err(engine::tampered());
            }
            if terminal
                .as_ref()
                .is_none_or(|t| t["errors"] != json!(engine::conflicts(&checked, &c)))
            {
                return Err(engine::tampered());
            }
            context = Some(c);
        } else if h.as_object().is_none_or(|m| m.len() != 1) {
            return Err(engine::tampered());
        }
    } else if !checked.is_empty() {
        return Err(engine::tampered());
    }
    if let Some(s) = &start {
        let c: Context = serde_json::from_value(s["context"].clone()).map_err(bad)?;
        identity = serde_json::from_value(s["extractor"].clone()).map_err(bad)?;
        identity.validate()?;
        if identity.id != provider
            || s.as_object().is_none_or(|m| m.len() != 7)
            || s["contextHash"] != hash(&context_value(&c)?)
            || c.confidence_threshold != 0.8
            || engine::retry_number(&version, &identity.version).is_none()
        {
            return Err(engine::tampered());
        }
        context = Some(c);
    }
    let status = terminal
        .as_ref()
        .and_then(|v| v["status"].as_str())
        .unwrap_or("started");
    if let Some(t) = &terminal {
        let errors = t["errors"].as_array().ok_or_else(engine::tampered)?;
        if (status == "completed") != errors.is_empty()
            || (status == "completed" && header.is_none())
            || (failure.is_some()
                && (header.is_some() || errors != &vec![failure.as_ref().unwrap()["code"].clone()]))
        {
            return Err(engine::tampered());
        }
    } else if header.is_some() || failure.is_some() {
        return Err(engine::tampered());
    }
    let exception:Option<(i64,String)>=db.query_row("SELECT id,status FROM exceptions WHERE type='INVOICE_EXTRACTION' AND related_document_id=?1 AND resolution_key=?2 ORDER BY id DESC LIMIT 1",params![doc,format!("invoice-extraction:{id}")],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(bad)?;
    if status == "needs_resolution" && exception.is_none() {
        return Err(engine::tampered());
    }
    if start.is_some() && terminal.is_some() {
        let proofs = audit(db, "invoice_extraction_result", id)?;
        let proof = json!({"version":engine::NATIVE_CODEC,"terminal":terminal,"resultHash":header.as_ref().map(|(_,raw)|crate::intercompany::sha(raw))});
        if proofs.len() != 1 || proofs[0] != (canonical(&proof), actor.clone()) {
            return Err(engine::tampered());
        }
    }
    let surface = json!({"attemptId":id,"documentId":doc,"status":status,"originalHash":sha,"fields":checked,"resolutions":resolutions,"conflicts":terminal.as_ref().map(|v|v["errors"].clone()).unwrap_or(json!([])),"exceptionState":exception.map(|(id,status)|json!({"id":id,"status":status}))});
    Ok(Loaded {
        surface,
        identity,
        context,
        terminal,
        start,
    })
}
fn latest(db: &Connection, id: i64) -> Result<Option<i64>, Error> {
    db.query_row("SELECT a.id FROM invoice_extraction_attempts a JOIN invoice_extraction_documents d ON d.id=a.extraction_document_id WHERE d.document_id=?1 ORDER BY a.id DESC LIMIT 1",[id],|r|r.get(0)).optional().map_err(bad)
}
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    source_rows(db)?;
    for query in [
        "SELECT EXISTS(SELECT 1 FROM invoice_extraction_attempts a LEFT JOIN invoice_extraction_documents d ON d.id=a.extraction_document_id WHERE d.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM invoice_extraction_results r LEFT JOIN invoice_extraction_attempts a ON a.id=r.attempt_id WHERE a.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM invoice_extraction_fields f LEFT JOIN invoice_extraction_results r ON r.id=f.result_id WHERE r.id IS NULL)",
        "SELECT EXISTS(SELECT 1 FROM invoice_extraction_resolutions r LEFT JOIN invoice_extraction_attempts a ON a.id=r.attempt_id WHERE a.id IS NULL)",
    ] {
        if db
            .query_row(query, [], |r| r.get::<_, bool>(0))
            .map_err(bad)?
        {
            return Err(engine::tampered());
        }
    }
    let mut q = db
        .prepare("SELECT id FROM invoice_extraction_attempts ORDER BY id")
        .map_err(bad)?;
    for id in q.query_map([], |r| r.get::<_, i64>(0)).map_err(bad)? {
        load(db, id.map_err(bad)?)?;
    }
    Ok(())
}
pub fn sources(db: &Connection) -> Result<Vec<ExtractionSource>, Error> {
    validate_history(db)?;
    source_rows(db)
}
pub fn inspect(db: &Connection, document_id: i64) -> Result<Option<Value>, Error> {
    validate_history(db)?;
    latest(db, document_id)?
        .map(|id| load(db, id).map(|e| e.surface))
        .transpose()
}
pub fn require_completed_or_absent(db: &Connection, document_id: i64) -> Result<(), Error> {
    if inspect(db, document_id)?.is_some_and(|v| v["status"] != "completed") {
        return Err(Error::new(
            "EXTRACTION_NEEDS_RESOLUTION",
            "Invoice extraction evidence requires resolution before this purchase workflow",
        ));
    }
    Ok(())
}
pub fn prepare(
    db: &Connection,
    document_id: i64,
    pdf_bytes: &[u8],
    identity: &ProviderIdentity,
    actor: &str,
    at: &str,
) -> Result<Prepared, Error> {
    identity.validate()?;
    author(actor, at)?;
    validate_history(db)?;
    let (source_hash, context) = document(db, document_id)?;
    if pdf_bytes.is_empty()
        || pdf_bytes.len() > engine::MAX_PDF
        || bytes_hash(pdf_bytes) != source_hash
    {
        return Err(Error::new(
            "EXTRACTION_SNAPSHOT_HASH_MISMATCH",
            "Exact registered PDF bytes required",
        ));
    }
    let context_hash = hash(&context_value(&context)?);
    if let Some(id) = latest(db, document_id)? {
        let loaded = load(db, id)?;
        if loaded.surface["status"] == "completed"
            && loaded.identity == *identity
            && loaded.context.as_ref() == Some(&context)
        {
            return Ok(Prepared {
                document_id,
                source_hash,
                identity: identity.clone(),
                context,
                context_hash,
                attempt_id: id,
                cached: Some(loaded.surface),
            });
        }
    }
    if db.is_autocommit() {
        return Err(Error::new(
            "transaction_required",
            "Extraction start requires the admitted write transaction",
        ));
    }
    let old:Option<(i64,i64)>=db.query_row("SELECT id,document_id FROM invoice_extraction_documents WHERE sha256_hash=?1 OR document_id=?2",params![source_hash,document_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(bad)?;
    let source_id = match old {
        Some((id, doc)) if doc == document_id => id,
        Some(_) => return Err(engine::tampered()),
        None => {
            db.execute("INSERT INTO invoice_extraction_documents(document_id,company_id,sha256_hash,pdf_bytes,byte_length,created_at) VALUES(?1,1,?2,?3,?4,?5)",params![document_id,source_hash,pdf_bytes,pdf_bytes.len() as i64,at]).map_err(bad)?;
            db.last_insert_rowid()
        }
    };
    let mut q=db.prepare("SELECT extractor_version FROM invoice_extraction_attempts WHERE extraction_document_id=?1 AND extractor_id=?2 ORDER BY id").map_err(bad)?;
    let versions = q
        .query_map(params![source_id, identity.id], |r| r.get::<_, String>(0))
        .map_err(bad)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(bad)?;
    let retries = versions
        .iter()
        .filter_map(|v| engine::retry_number(v, &identity.version))
        .max()
        .map(|n| n.checked_add(1).ok_or_else(engine::tampered))
        .transpose()?;
    let version = retries
        .map(|n| format!("{}#retry-{n}", identity.version))
        .unwrap_or_else(|| identity.version.clone());
    db.execute("INSERT INTO invoice_extraction_attempts(extraction_document_id,extractor_id,extractor_version,status,created_at,initiated_by) VALUES(?1,?2,?3,'started',?4,?5)",params![source_id,identity.id,version,at,actor]).map_err(bad)?;
    let attempt_id = db.last_insert_rowid();
    let start = json!({"version":engine::NATIVE_CODEC,"documentId":document_id,"sourceSha256":source_hash,"context":context,"contextHash":context_hash,"extractor":identity,"actor":actor});
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('invoice_extraction_attempt','invoice_extraction_attempt',?1,?2,?3)",params![attempt_id,canonical(&start),actor]).map_err(bad)?;
    Ok(Prepared {
        document_id,
        source_hash,
        identity: identity.clone(),
        context,
        context_hash,
        attempt_id,
        cached: None,
    })
}
pub fn recheck(db: &Connection, p: &Prepared) -> Result<(), Error> {
    validate_history(db)?;
    let (source, context) = document(db, p.document_id)?;
    if source != p.source_hash
        || context != p.context
        || hash(&context_value(&context)?) != p.context_hash
        || latest(db, p.document_id)? != Some(p.attempt_id)
    {
        return Err(Error::new(
            "EXTRACTION_CONTEXT_CHANGED",
            "Current document, validation context or extraction attempt changed",
        ));
    }
    let current = load(db, p.attempt_id)?;
    if current.identity != p.identity
        || current.context.as_ref() != Some(&p.context)
        || match &p.cached {
            Some(c) => current.surface != *c,
            None => current.terminal.is_some(),
        }
    {
        return Err(Error::new(
            "EXTRACTION_CONTEXT_CHANGED",
            "Current extraction history changed",
        ));
    }
    Ok(())
}
fn resolution(
    db: &Connection,
    p: &Prepared,
    key: &str,
    value: &Value,
    actor: &str,
    at: &str,
) -> Result<(), Error> {
    db.execute("INSERT INTO invoice_extraction_resolutions(attempt_id,field_key,resolution_json,resolved_by,created_at) VALUES(?1,?2,?3,?4,?5)",params![p.attempt_id,key,canonical(value),actor,at]).map(|_|()).map_err(bad)
}
fn exception(db: &Connection, p: &Prepared, errors: &[String], at: &str) -> Result<(), Error> {
    db.execute("INSERT INTO exceptions(type,severity,related_document_id,message,required_action,source_evidence,resolution_key,created_at) VALUES('INVOICE_EXTRACTION','high',?1,?2,'Review cited invoice evidence and append a resolution before posting',?3,?4,?5)",params![p.document_id,errors.join("; "),canonical(&json!({"attemptId":p.attempt_id,"errors":errors})),format!("invoice-extraction:{}",p.attempt_id),at]).map(|_|()).map_err(bad)
}
pub fn finish(
    db: &Connection,
    p: &Prepared,
    outcome: &Result<ProviderOutput, ProviderFailure>,
    actor: &str,
    at: &str,
) -> Result<Value, Error> {
    author(actor, at)?;
    recheck(db, p)?;
    if let Some(c) = &p.cached {
        return Ok(c.clone());
    }
    if db.is_autocommit() {
        return Err(Error::new(
            "transaction_required",
            "Extraction finish requires the admitted write transaction",
        ));
    }
    let start = load(db, p.attempt_id)?.start.ok_or_else(engine::tampered)?;
    if start["actor"] != actor {
        return Err(engine::tampered());
    }
    let mut result_hash = None;
    let errors = match outcome {
        Err(failure) => {
            resolution(
                db,
                p,
                "provider_failure",
                &json!({"code":failure.as_str()}),
                actor,
                at,
            )?;
            vec![failure.as_str().into()]
        }
        Ok(output) => match engine::fields(&output.fields) {
            Err(errors) => errors,
            Ok(fields) => {
                let header = json!({"version":engine::NATIVE_CODEC,"extractor":p.identity,"sourceSha256":p.source_hash,"context":p.context,"contextHash":p.context_hash,"fieldsHash":hash(&json!(fields))});
                let raw = canonical(&header);
                result_hash = Some(crate::intercompany::sha(&raw));
                db.execute("INSERT INTO invoice_extraction_results(attempt_id,result_json,created_at,initiated_by) VALUES(?1,?2,?3,?4)",params![p.attempt_id,raw,at,actor]).map_err(bad)?;
                let rid = db.last_insert_rowid();
                for f in &fields {
                    db.execute("INSERT INTO invoice_extraction_fields(result_id,field_key,value_json,confidence,page_number,source_text,box_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![rid,f.key,canonical(&f.value),f.confidence,f.page,f.source_text,f.r#box.as_ref().map(|b|serde_json::to_value(b).map(|v|canonical(&v))).transpose().map_err(bad)?]).map_err(bad)?;
                }
                engine::conflicts(&fields, &p.context)
            }
        },
    };
    let status = if errors.is_empty() {
        "completed"
    } else {
        "needs_resolution"
    };
    if !errors.is_empty() {
        exception(db, p, &errors, at)?;
    }
    let terminal = json!({"status":status,"errors":errors});
    resolution(db, p, "extraction_status", &terminal, actor, at)?;
    let proof =
        json!({"version":engine::NATIVE_CODEC,"terminal":terminal,"resultHash":result_hash});
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('invoice_extraction_result','invoice_extraction_attempt',?1,?2,?3)",params![p.attempt_id,canonical(&proof),actor]).map_err(bad)?;
    Ok(load(db, p.attempt_id)?.surface)
}
#[cfg(test)]
mod tests;
