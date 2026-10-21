//! Reviewed native source acceptance. All ledger, provenance and external records
//! commit together; retained bytes are independently admitted by the application.
pub use crate::archive_import::{EntryFacts, SourceFacts};
use crate::{invalid, journal_post, opening_balance};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rentemester_engine::{
    Error,
    dinero_import::{self as policy, ParsedImport, SourceFile},
    fiscal_year::Date,
    money::Money,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod history;
mod receipts;
pub use history::{
    OriginalRef, VerifiedImport, originals, read_imports, validate_history, verify_original,
};
pub(crate) use history::{accepted_header_ids, accepted_journal_ids};
pub use receipts::{receipt_publications, supporting_originals};
#[cfg(test)]
mod tests;
pub const PACKAGE: &str = "rentemester-dinero-source/1";
pub(super) const CONTRACT: &str = "rentemester-native-dinero/1";
pub(super) const EVENT: &str = "native_dinero_import";
pub(super) fn refused(s: impl Into<String>) -> Error {
    Error::new("dinero_import_refused", s)
}
pub(super) fn hash(value: &impl Serialize) -> Result<String, Error> {
    Ok(sha(serde_json::to_vec(value).map_err(invalid)?))
}
pub(super) fn sha(bytes: impl AsRef<[u8]>) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Package {
    format: String,
    currency: String,
    files: Vec<Encoded>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Encoded {
    path: String,
    content_base64: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub plan_hash: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Binding {
    pub format: String,
    pub source: SourceFacts,
    pub parsed: Value,
    pub archive: Option<Value>,
    pub context: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_plan: Option<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct JournalProof {
    pub id: i64,
    pub hash: String,
    pub request: Value,
    pub voucher_ref: String,
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Attestation {
    pub binding: Binding,
    pub plan_hash: String,
    pub attempt_id: i64,
    pub source_id: i64,
    pub inventory_id: i64,
    pub opening: Option<Value>,
    pub journals: Vec<JournalProof>,
    pub receipt: Value,
    pub actor: String,
    pub accepted_at: String,
    pub archive_import_ids: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supporting_receipts: Option<Vec<receipts::Proof>>,
}
#[derive(Clone)]
pub struct ClosedSource {
    pub source: SourceFacts,
    pub parsed: rentemester_engine::archive_import::ParsedArchive,
    pub bytes: Vec<u8>,
}
#[derive(Clone)]
pub struct Decoded {
    pub files: Vec<SourceFile>,
    pub source: SourceFacts,
    pub parsed: ParsedImport,
    pub archive: Option<ClosedSource>,
}
pub struct Prepared {
    binding: Binding,
    decoded: Decoded,
    plan_hash: String,
    blockers: Vec<String>,
    review: Value,
}
impl Prepared {
    pub fn review(&self) -> Value {
        self.review.clone()
    }
}
/// Constructor private to this accepted-source adapter; manual posting cannot opt in.
pub(crate) struct AdmittedVoucher {
    _private: (),
}
pub fn decode_source(bytes: &[u8]) -> Result<Decoded, Error> {
    if bytes.is_empty() || bytes.len() > 25 * 1024 * 1024 {
        return Err(refused("source package exceeds25MiB"));
    }
    let package: Package =
        serde_json::from_slice(bytes).map_err(|_| refused("invalid Dinero source package"))?;
    if package.format != PACKAGE || package.files.is_empty() || package.files.len() > 4096 {
        return Err(refused("unsupported source package/inventory"));
    }
    let mut files = vec![];
    let mut entries = vec![];
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for file in package.files {
        let path = &file.path;
        if path.is_empty()
            || path.len() > 1024
            || path.starts_with('/')
            || path.contains('\\')
            || path.contains("..")
            || path.chars().any(char::is_control)
            || path.split('/').any(|p| p.is_empty() || p == ".")
            || !seen.insert(path.clone())
        {
            return Err(refused("unsafe or duplicate exact source path"));
        }
        let decoded = STANDARD
            .decode(file.content_base64.as_bytes())
            .map_err(|_| refused("invalid source base64"))?;
        total = total
            .checked_add(decoded.len())
            .ok_or_else(|| refused("source size overflow"))?;
        if decoded.len() > 16 * 1024 * 1024
            || total > 18 * 1024 * 1024
            || STANDARD.encode(&decoded) != file.content_base64
        {
            return Err(refused("noncanonical or oversized source entry"));
        }
        entries.push(EntryFacts {
            path: file.path.clone(),
            sha256: sha(&decoded),
            size_bytes: decoded.len() as u64,
        });
        files.push(SourceFile {
            path: file.path,
            bytes: decoded,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let digest = sha(bytes);
    let parsed = policy::parse(&files, &package.currency)?;
    let csv = files
        .iter()
        .filter(|f| rentemester_engine::archive_import::canonical_path(&f.path).is_ok())
        .collect::<Vec<_>>();
    let archive = if csv.iter().any(|f| {
        rentemester_engine::archive_import::canonical_path(&f.path)
            .is_ok_and(|(y, _)| y < parsed.latest_year)
    }) {
        let bytes=serde_json::to_vec(&json!({"format":rentemester_engine::archive_import::PACKAGE,"currency":package.currency,"files":csv.iter().map(|f|json!({"path":f.path,"contentBase64":STANDARD.encode(&f.bytes)})).collect::<Vec<_>>()})).map_err(invalid)?;
        let (source, parsed) = crate::archive_import::decode_source(&bytes)?;
        Some(ClosedSource {
            source,
            parsed,
            bytes,
        })
    } else {
        None
    };
    Ok(Decoded {
        files,
        source: SourceFacts {
            path: format!("documents/originals/{digest}.json"),
            sha256: digest,
            size_bytes: bytes.len() as u64,
            entries,
        },
        parsed,
        archive,
    })
}
fn context(tx: &Transaction<'_>) -> Result<Value, Error> {
    let uuid: String = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let head: Option<(i64, String)> = tx
        .query_row(
            "SELECT id,entry_hash FROM journal_entries ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let mut accounts = vec![];
    let mut q=tx.prepare("SELECT account_no,name,type,normal_balance,default_vat_code,active,allow_direct_posting FROM accounts ORDER BY account_no").map_err(invalid)?;
    for r in q
        .query_map([], |r| {
            Ok(json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?
            ]))
        })
        .map_err(invalid)?
    {
        accounts.push(r.map_err(invalid)?);
    }
    let company:Value=tx.query_row("SELECT name,cvr,currency,fiscal_year_start_month,fiscal_year_label_strategy,vat_period_type FROM companies WHERE id=1",[],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?]))).map_err(invalid)?;
    let roles = crate::company_setup::accounts(tx, "")?;
    let periods = crate::periods::list(tx, "")?;
    Ok(
        json!({"ledgerUuid":uuid,"head":head,"company":company,"accounts":accounts,"roles":roles["accountRoles"],"companyProfile":roles["company"],"periods":periods["periods"]}),
    )
}
fn counts(parsed: &ParsedImport) -> Value {
    json!({"accounts":parsed.chart.len(),"openingLines":parsed.opening_lines.len(),"historicalVouchers":parsed.vouchers.len(),"importedInvoices":parsed.schedule.as_ref().map_or(0,|s|s.invoices.len()),"importedEvents":parsed.schedule.as_ref().map_or(0,|s|s.invoices.iter().map(|i|i.events.len()).sum::<usize>()),"archiveYears":0})
}
pub fn prepare(tx: &Transaction<'_>, decoded: &Decoded, latest: Date) -> Result<Prepared, Error> {
    let source = &decoded.source;
    let parsed = &decoded.parsed;
    crate::validate(tx)?;
    validate_history(tx)?;
    let mut current = context(tx)?;
    current["receiptDate"] = json!(latest.to_string());
    let mut blockers = vec![];
    if let Some(archive) = &decoded.archive {
        let plan = crate::archive_import::prepare(tx, &archive.source, &archive.parsed)?;
        current["archivePlan"] = plan.review();
    }
    if parsed.declared_currency != "DKK" || current["company"][2] != "DKK" {
        blockers.push("Dinero live ledger requires explicit DKK source and company".into());
    }
    if parsed.control_date.is_some_and(|d| d > latest) {
        blockers.push("source control date exceeds trusted posting horizon".into());
    }
    let existing:Option<i64>=tx.query_row("SELECT id FROM dinero_import_attempts WHERE source_raw_sha256=?1 AND outcome='accepted' ORDER BY id LIMIT 1",[&source.sha256],|r|r.get(0)).optional().map_err(invalid)?;
    if existing.is_some() {
        blockers.push(
            "source already accepted; inspect canonical status and original keyed receipt".into(),
        );
    }
    if !parsed.opening_lines.is_empty() && current["head"] != Value::Null {
        blockers.push("live journal is not empty; source opening/history overlap requires explicit reconciliation".into());
    }
    for account in &parsed.chart {
        if let Some(old) = current["accounts"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r[0] == account.account_no))
            && (old[2] != account.kind
                || old[3] != account.normal_balance
                || old[4] != json!(account.default_vat_code)
                || old[5] != 1
                || old[6] != 1)
        {
            blockers.push(format!(
                "source account {} conflicts with current chart; resolve explicitly",
                account.account_no
            ));
        }
    }
    let mut projected = BTreeMap::<String, Money>::new();
    for line in parsed
        .opening_lines
        .iter()
        .chain(parsed.vouchers.iter().flat_map(|v| v.lines.iter()))
    {
        let v = projected.entry(line.account_no.clone()).or_default();
        v.add(&line.debit_amount);
        *v = v.subtract(&line.credit_amount);
    }
    let mut controls = vec![];
    let mut seen = BTreeSet::new();
    for control in &parsed.controls {
        seen.insert(control.account_no.clone());
        let expected = projected
            .get(&control.account_no)
            .cloned()
            .unwrap_or_default();
        let absolute = if control.kind == "payable" {
            Money::default().subtract(&expected)
        } else {
            expected
        };
        let schedule = parsed
            .schedule
            .as_ref()
            .filter(|_| control.kind == "receivable")
            .map(|s| {
                rentemester_engine::imported_receivables::balance(
                    s,
                    parsed.control_date.unwrap_or(latest),
                    &control.account_no,
                )
            });
        let matches =
            absolute == control.amount && schedule.as_ref().is_none_or(|s| s == &control.amount);
        if !matches {
            blockers.push(format!(
                "source control {} does not reconcile to projected ledger/schedule in øre",
                control.account_no
            ));
        }
        controls.push(json!({"accountNo":control.account_no,"kind":control.kind,"sourceAmount":control.amount.decimal(),"projectedAmount":absolute.decimal(),"scheduleAmount":schedule.map(|s|s.decimal()),"reconciles":matches}));
    }
    if let Some(schedule) = &parsed.schedule {
        for invoice in &schedule.invoices {
            if !seen.contains(&invoice.control_account_no) {
                blockers.push(format!(
                    "schedule control {} has no authoritative source control",
                    invoice.control_account_no
                ));
            }
        }
    }
    let receipt_plan = policy::receipts::plan(&decoded.files, parsed)?;
    let receipt_plan = if receipt_plan.receipts.is_empty() {
        None
    } else {
        Some(serde_json::to_value(receipt_plan).map_err(invalid)?)
    };
    let binding = Binding {
        format: CONTRACT.into(),
        source: source.clone(),
        parsed: serde_json::to_value(parsed).map_err(invalid)?,
        archive: decoded
            .archive
            .as_ref()
            .map(|a| json!({"source":a.source,"parsed":a.parsed})),
        context: current,
        receipt_plan,
    };
    let plan_hash = hash(&binding)?;
    let mut summary_counts = counts(parsed);
    summary_counts["archiveYears"] =
        json!(decoded.archive.as_ref().map_or(0, |a| a.parsed.years.len()));
    let review = json!({"status":if blockers.is_empty(){"ready"}else{"blocked"},"planHash":plan_hash,"source":source,"latestYear":parsed.latest_year,"cutOverDate":parsed.cut_over_date,"controlDate":parsed.control_date,"counts":summary_counts,"controls":controls,"blockers":blockers,"warnings":parsed.notices,"existingAttemptId":existing});
    Ok(Prepared {
        binding,
        decoded: decoded.clone(),
        plan_hash,
        blockers,
        review,
    })
}
fn install_chart(tx: &Transaction<'_>, parsed: &ParsedImport, subject: &str) -> Result<(), Error> {
    for a in &parsed.chart {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1)",
                [&a.account_no],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if !exists {
            crate::company_setup::create_account(
                tx,
                &crate::company_setup::Account {
                    account_no: a.account_no.clone(),
                    name: a.name.clone(),
                    r#type: a.kind.clone(),
                    normal_balance: a.normal_balance.clone(),
                    default_vat_code: a.default_vat_code.clone(),
                },
                subject,
            )?;
        }
    }
    for p in &parsed.role_proposals {
        tx.execute("INSERT INTO account_role_proposals(role,account_no,source) SELECT ?1,?2,?3 WHERE NOT EXISTS(SELECT 1 FROM account_role_proposals WHERE role=?1 AND account_no=?2 AND source=?3)",params![p.role,p.account_no,p.source]).map_err(invalid)?;
    }
    let master = &parsed.company_master;
    if let Some(name) = master["name"].as_str().filter(|s| !s.trim().is_empty()) {
        tx.execute("UPDATE companies SET name=?1 WHERE id=1 AND (trim(name)='' OR name='Rentemester company')",[name.trim()]).map_err(invalid)?;
    }
    if let Some(cvr) = master["cvr"].as_str() {
        let cvr = cvr.trim().trim_start_matches("DK");
        if cvr.len() == 8 && cvr.bytes().all(|b| b.is_ascii_digit()) {
            tx.execute(
                "UPDATE companies SET cvr=?1 WHERE id=1 AND (cvr IS NULL OR trim(cvr)='')",
                [cvr],
            )
            .map_err(invalid)?;
        }
    }
    Ok(())
}
fn voucher_request(
    parsed: &ParsedImport,
    v: &policy::Voucher,
) -> Result<rentemester_engine::manual_journal::ManualJournal, Error> {
    use rentemester_engine::{
        currency::Currency,
        identifiers::AccountNo,
        manual_journal::{JournalLine, ManualJournal},
    };
    let text = if v.entry_type.trim().is_empty() {
        format!("Import: bilag {} — {}", v.voucher_ref, v.text)
    } else {
        format!(
            "Import: {} (bilag {}) — {}",
            v.entry_type, v.voucher_ref, v.text
        )
    };
    Ok(ManualJournal {
        transaction_date: v.transaction_date,
        text,
        currency: Currency::parse("DKK")?,
        document_id: None,
        source_bank_transaction_id: None,
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        created_by_program: Some("rentemester-import-postings".into()),
        lines: v
            .lines
            .iter()
            .map(|l| {
                Ok(JournalLine {
                    account_no: AccountNo::parse(&l.account_no)?,
                    debit_amount: l.debit_amount.clone(),
                    credit_amount: l.credit_amount.clone(),
                    vat_code: l.vat_code.clone().or_else(|| {
                        parsed
                            .chart
                            .iter()
                            .find(|a| a.account_no == l.account_no)
                            .and_then(|a| a.default_vat_code.clone())
                    }),
                    text: l.text.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}
fn source_records(
    tx: &Transaction<'_>,
    p: &Prepared,
    receipt: &Value,
    actor: &str,
    at: &str,
) -> Result<(i64, i64, i64), Error> {
    let source = &p.binding.source;
    let listing = hash(&source.entries)?;
    let sid:i64=tx.query_row("INSERT INTO dinero_import_sources(raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count,received_at)VALUES(?1,?2,?3,?4,?5) RETURNING id",params![source.sha256,source.size_bytes as i64,listing,source.entries.len() as i64,at],|r|r.get(0)).map_err(invalid)?;
    let total = source.entries.iter().try_fold(0i64, |n, e| {
        n.checked_add(i64::try_from(e.size_bytes).map_err(invalid)?)
            .ok_or_else(|| refused("inventory overflow"))
    })?;
    let inventory:i64=tx.query_row("INSERT INTO dinero_import_inventories(source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes,created_at)VALUES(?1,?2,?3,?4,?4,?5,?6) RETURNING id",params![sid,source.sha256,listing,source.entries.len()as i64,total,at],|r|r.get(0)).map_err(invalid)?;
    for e in &source.entries {
        tx.execute("INSERT INTO dinero_import_inventory_entries(inventory_id,entry_path,entry_size_bytes,entry_sha256)VALUES(?1,?2,?3,?4)",params![inventory,e.path,e.size_bytes as i64,e.sha256]).map_err(invalid)?;
    }
    let date = p
        .decoded
        .parsed
        .cut_over_date
        .or(p.decoded.parsed.control_date)
        .map(|d| d.to_string())
        .unwrap_or_else(|| at[..10].into());
    let attempt:i64=tx.query_row("INSERT INTO dinero_import_attempts(inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256,recorded_at)VALUES(?1,?2,?3,?4,?5,?6,'accepted',?7,?8)RETURNING id",params![inventory,sid,source.sha256,CONTRACT,actor,date,hash(receipt)?,at],|r|r.get(0)).map_err(invalid)?;
    Ok((attempt, sid, inventory))
}
fn ledger_control(tx: &Transaction<'_>, account: &str, date: Date) -> Result<Money, Error> {
    let mut total = Money::default();
    let mut q=tx.prepare("SELECT l.debit_amount,l.credit_amount FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_entry_id JOIN accounts a ON a.id=l.account_id WHERE a.account_no=?1 AND j.transaction_date<=?2 ORDER BY j.id,l.id").map_err(invalid)?;
    let mut rs = q
        .query(params![account, date.to_string()])
        .map_err(invalid)?;
    while let Some(r) = rs.next().map_err(invalid)? {
        total.add(&crate::legacy_money(r.get_ref(0).map_err(invalid)?)?);
        total = total.subtract(&crate::legacy_money(r.get_ref(1).map_err(invalid)?)?);
    }
    Ok(total)
}
pub fn apply(
    tx: &Transaction<'_>,
    p: &Prepared,
    input: &Apply,
    latest: Date,
    subject: &str,
    now_ms: i64,
) -> Result<Value, Error> {
    apply_with_archives(tx, p, input, latest, subject, now_ms, &[])
}
pub fn apply_with_archives(
    tx: &Transaction<'_>,
    p: &Prepared,
    input: &Apply,
    latest: Date,
    subject: &str,
    now_ms: i64,
    verified: &[crate::archive_import::VerifiedArchive],
) -> Result<Value, Error> {
    journal_post::require_current(tx)?;
    crate::idempotency::validate_key(&input.idempotency_key)?;
    if !input.confirm
        || subject.is_empty()
        || subject.chars().any(char::is_control)
        || input.plan_hash != p.plan_hash
    {
        return Err(refused(
            "explicit current reviewed import confirmation required",
        ));
    }
    let fresh = prepare(tx, &p.decoded, latest)?;
    if fresh.plan_hash != p.plan_hash || !fresh.blockers.is_empty() {
        return Err(refused(
            "import plan stale or blocked; review current source and ledger",
        ));
    }
    let at = rentemester_engine::company_knowledge::timestamp(now_ms)?;
    let actor = format!("user:{subject}");
    let parsed = &p.decoded.parsed;
    let token = AdmittedVoucher { _private: () };
    let mut archived = verified.to_vec();
    let mut archive_import_ids = vec![];
    if let Some(source) = &p.decoded.archive {
        let prepared = crate::archive_import::prepare(tx, &source.source, &source.parsed)?;
        let result = crate::archive_import::apply(
            tx,
            &prepared,
            &crate::archive_import::Apply {
                plan_hash: prepared.review()["planHash"]
                    .as_str()
                    .ok_or_else(|| refused("archive plan unavailable"))?
                    .into(),
                idempotency_key: input.idempotency_key.clone(),
                confirm: true,
            },
            subject,
            now_ms,
        )?;
        let id = result["archiveImport"]["importId"]
            .as_i64()
            .ok_or_else(|| refused("archive receipt unavailable"))?;
        archive_import_ids.push(id);
        let reference = crate::archive_import::originals(tx)?
            .into_iter()
            .find(|r| r.import_id == id)
            .ok_or_else(|| refused("archive source reference unavailable"))?;
        archived.push(crate::archive_import::verify_original(
            tx,
            &reference,
            &source.bytes,
        )?);
    }
    install_chart(tx, parsed, subject)?;
    let opening = if let Some(date) = parsed.cut_over_date {
        let input_open = opening_balance::Input {
            cut_over_date: date,
            lines: parsed
                .opening_lines
                .iter()
                .map(|l| {
                    Ok(rentemester_engine::opening_balance::Line {
                        account_no: rentemester_engine::identifiers::AccountNo::parse(
                            &l.account_no,
                        )?,
                        debit_amount: l.debit_amount.clone(),
                        credit_amount: l.credit_amount.clone(),
                        text: l.text.clone(),
                    })
                })
                .collect::<Result<_, Error>>()?,
            note: Some("Import fra dinero".into()),
            source_archive_import_ids: archive_import_ids.clone(),
        };
        let plan = opening_balance::prepare_import(tx, &input_open, &archived, latest, &token)?;
        let input = opening_balance::Apply {
            input: input_open,
            plan_hash: plan.review()["planHash"]
                .as_str()
                .ok_or_else(|| refused("opening plan unavailable"))?
                .into(),
            idempotency_key: input.idempotency_key.clone(),
            confirm: true,
        };
        let result = opening_balance::apply_import(
            tx,
            &input,
            &archived,
            latest,
            journal_post::Actor {
                subject: &actor,
                program: opening_balance::PROGRAM,
            },
            now_ms,
            &token,
        )?;
        Some(result["openingBalance"].clone())
    } else {
        None
    };
    let mut journals = vec![];
    for voucher in &parsed.vouchers {
        let request = voucher_request(parsed, voucher)?;
        let result = journal_post::post_historical_import(tx, &request, latest, &actor, &token)?;
        if result["ok"] != true {
            return Err(refused(format!(
                "historical voucher {} refused: {}",
                voucher.voucher_ref, result
            )));
        }
        journals.push(JournalProof {
            id: result["entryId"]
                .as_i64()
                .ok_or_else(|| refused("entry ID unavailable"))?,
            hash: result["entryHash"]
                .as_str()
                .ok_or_else(|| refused("entry hash unavailable"))?
                .into(),
            request: serde_json::to_value(request).map_err(invalid)?,
            voucher_ref: voucher.voucher_ref.clone(),
            kind: "historical".into(),
        });
    }
    for c in &parsed.controls {
        let actual = ledger_control(tx, &c.account_no, parsed.control_date.unwrap_or(latest))?;
        let actual = if c.kind == "payable" {
            Money::default().subtract(&actual)
        } else {
            actual
        };
        if actual != c.amount {
            return Err(refused(
                "source control does not reconcile to actual imported ledger in øre",
            ));
        }
    }
    let next_attempt: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(id),0)+1 FROM dinero_import_attempts",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let mut receipt = json!({"ok":true,"attemptId":next_attempt,"sourceSha256":p.binding.source.sha256,"sourceSizeBytes":p.binding.source.size_bytes,"parserContract":CONTRACT,"cutOverDate":parsed.cut_over_date,"controlDate":parsed.control_date,"acceptedAt":at,"counts":p.review["counts"],"journalCount":journals.len(),"firstHistoricalEntryId":journals.first().map(|j|j.id),"lastHistoricalEntryId":journals.last().map(|j|j.id),"openingBalance":opening,"archiveImportIds":archive_import_ids,"sourceAssurance":"retained-native"});
    let supporting_receipts = if p.binding.receipt_plan.is_some() {
        let proofs = receipts::register(tx, p, &journals, &at, &actor)?;
        receipt["supportingDocuments"] = receipts::summary(&proofs);
        Some(proofs)
    } else {
        None
    };
    let (attempt_id, source_id, inventory_id) = source_records(tx, p, &receipt, &actor, &at)?;
    let supporting_receipts = supporting_receipts
        .map(|proofs| receipts::link_provenance(tx, p, attempt_id, inventory_id, proofs, &at))
        .transpose()?;
    if attempt_id != next_attempt {
        return Err(refused("attempt identity changed"));
    }
    if let Some(schedule) = &parsed.schedule {
        crate::imported_receivables::persist(tx, attempt_id, schedule)?;
    }
    for control in &parsed.controls {
        let batch:i64=tx.query_row("INSERT INTO migration_open_item_batches(dinero_import_attempt_id,control_account_no,kind,source_control_amount,opening_journal_entry_id)VALUES(?1,?2,?3,?4,?5)RETURNING id",params![attempt_id,control.account_no,control.kind,journal_post::money(&control.amount)?,opening.as_ref().and_then(|o|o["journalEntryId"].as_i64())],|r|r.get(0)).map_err(invalid)?;
        if control.amount != Money::default() {
            tx.execute("INSERT INTO migration_open_items(batch_id,external_ref,original_amount,open_amount_at_import,source_kind,resolution_status)VALUES(?1,?2,?3,?3,'control_balance','unallocated')",params![batch,format!("UNALLOCATED:{}",control.account_no),journal_post::money(&control.amount)?]).map_err(invalid)?;
        }
    }
    let event = Attestation {
        binding: p.binding.clone(),
        plan_hash: p.plan_hash.clone(),
        attempt_id,
        source_id,
        inventory_id,
        opening,
        journals,
        receipt: receipt.clone(),
        actor: actor.clone(),
        accepted_at: at.clone(),
        archive_import_ids,
        supporting_receipts,
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES(?1,'dinero_import',?2,?3,?4,?5)",params![EVENT,attempt_id.to_string(),serde_json::to_string(&event).map_err(invalid)?,actor,at]).map_err(invalid)?;
    validate_history(tx)?;
    Ok(receipt)
}
