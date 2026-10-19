use super::*;
#[derive(Clone, Debug)]
pub struct OriginalRef {
    pub import_id: i64,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct VerifiedArchive {
    import_id: i64,
    source_hash: String,
    binding_hash: String,
    ledger_uuid: String,
}
pub(super) struct Import {
    pub id: i64,
    pub event: Attestation,
}
pub(super) struct HistoricalYear {
    pub id: i64,
    pub year: Year,
    pub currency: String,
    pub import_id: i64,
    pub source_hash: String,
    pub imported_at: String,
    pub posting_ids: Vec<i64>,
}
pub(super) struct Checked {
    pub imports: Vec<Import>,
    pub years: BTreeMap<u16, HistoricalYear>,
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn source_shape(source: &SourceFacts, archive: &ParsedArchive) -> Result<(), Error> {
    if !hex(&source.sha256)
        || source.path != format!("documents/originals/{}.json", source.sha256)
        || source.size_bytes == 0
        || source.size_bytes > 25 * 1024 * 1024
        || source.entries.len() > 64
        || archive.parser_contract != policy::FORMAT
        || archive.years.is_empty()
        || archive.years.len() > 31
        || archive.declared_currency.len() != 3
        || !archive
            .declared_currency
            .bytes()
            .all(|b| b.is_ascii_uppercase())
    {
        return Err(sql("source shape"));
    }
    let mut postings = BTreeSet::new();
    let mut balances = BTreeSet::new();
    let mut previous = None;
    let mut total = 0u64;
    for e in &source.entries {
        let (y, b) = policy::canonical_path(&e.path)?;
        if !hex(&e.sha256)
            || e.size_bytes > 16 * 1024 * 1024
            || previous.is_some_and(|s: &str| s >= e.path.as_str())
        {
            return Err(sql("inventory"));
        }
        previous = Some(&e.path);
        total = total.checked_add(e.size_bytes).ok_or_else(|| sql("size"))?;
        if b {
            balances.insert(y);
        } else {
            postings.insert(y);
        }
    }
    if total > 18 * 1024 * 1024
        || postings.len() != archive.years.len() + 1
        || postings.last() != Some(&archive.excluded_latest_year)
        || !balances.is_subset(&postings)
    {
        return Err(sql("year inventory"));
    }
    let mut years = BTreeSet::new();
    let mut count = 0;
    let mut previous_year = 0;
    for year in &archive.years {
        if year.fiscal_year <= previous_year
            || year.fiscal_year >= archive.excluded_latest_year
            || !postings.contains(&year.fiscal_year)
            || year.has_balances != balances.contains(&year.fiscal_year)
            || !year.has_balances && !year.balances.is_empty()
        {
            return Err(sql("year shape"));
        }
        previous_year = year.fiscal_year;
        years.insert(year.fiscal_year);
        for (i, p) in year.postings.iter().enumerate() {
            if p.line_no as usize != i
                || p.account_no.is_empty()
                || rentemester_engine::fiscal_year::Date::parse(&p.transaction_date).is_err()
            {
                return Err(sql("posting shape"));
            }
        }
        for (i, b) in year.balances.iter().enumerate() {
            if b.line_no as usize != i || b.account_no.is_empty() {
                return Err(sql("balance shape"));
            }
        }
        count += year.postings.len() + year.balances.len();
        numeric::check_year(year)?;
    }
    years.insert(archive.excluded_latest_year);
    if years != postings || count > 100000 {
        return Err(sql("incomplete source inventory"));
    }
    Ok(())
}
fn stored_year(db: &Connection, id: i64, year: &Year) -> Result<(String, Vec<i64>), Error> {
    let(system,fiscal,postings,balances,at):(String,u16,i64,i64,String)=db.query_row("SELECT source_system,fiscal_year,posting_count,balance_count,imported_at FROM import_archive_years WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(sql)?;
    if id <= 0
        || system != "dinero"
        || fiscal != year.fiscal_year
        || postings != year.postings.len() as i64
        || balances != year.balances.len() as i64
    {
        return Err(sql("archive header differs"));
    }
    let mut q=db.prepare("SELECT id,line_no,account_no,account_name,transaction_date,voucher,voucher_type,text,vat_type,amount,running_balance FROM import_archive_postings WHERE archive_year_id=? ORDER BY line_no,id").map_err(sql)?;
    let mut rows = q.query([id]).map_err(sql)?;
    let mut ids = vec![];
    for p in &year.postings {
        let r = rows
            .next()
            .map_err(sql)?
            .ok_or_else(|| sql("missing posting"))?;
        if r.get::<_, u32>(1).map_err(sql)? != p.line_no
            || r.get::<_, String>(2).map_err(sql)? != p.account_no
            || r.get::<_, String>(3).map_err(sql)? != p.account_name
            || r.get::<_, String>(4).map_err(sql)? != p.transaction_date
            || r.get::<_, String>(5).map_err(sql)? != p.voucher
            || r.get::<_, String>(6).map_err(sql)? != p.voucher_type
            || r.get::<_, String>(7).map_err(sql)? != p.text
            || r.get::<_, String>(8).map_err(sql)? != p.vat_type
        {
            return Err(sql("posting differs"));
        }
        numeric::verify(r.get_ref(9).map_err(sql)?, &p.amount)?;
        match (&p.running_balance, r.get_ref(10).map_err(sql)?) {
            (Some(v), cell) => numeric::verify(cell, v)?,
            (None, rusqlite::types::ValueRef::Null) => {}
            _ => return Err(sql("running balance differs")),
        }
        ids.push(r.get(0).map_err(sql)?);
    }
    if rows.next().map_err(sql)?.is_some() {
        return Err(sql("extra posting"));
    }
    let mut q=db.prepare("SELECT line_no,account_no,account_name,amount FROM import_archive_balances WHERE archive_year_id=? ORDER BY line_no,id").map_err(sql)?;
    let mut rows = q.query([id]).map_err(sql)?;
    for b in &year.balances {
        let r = rows
            .next()
            .map_err(sql)?
            .ok_or_else(|| sql("missing balance"))?;
        if r.get::<_, u32>(0).map_err(sql)? != b.line_no
            || r.get::<_, String>(1).map_err(sql)? != b.account_no
            || r.get::<_, String>(2).map_err(sql)? != b.account_name
        {
            return Err(sql("balance differs"));
        }
        numeric::verify(r.get_ref(3).map_err(sql)?, &b.amount)?;
    }
    if rows.next().map_err(sql)?.is_some() {
        return Err(sql("extra balance"));
    }
    Ok((at, ids))
}
pub(super) fn checked(db: &Connection) -> Result<Checked, Error> {
    crate::journal_preview::require_current(db)?;
    let uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    let mut q=db.prepare("SELECT id,entity_type,entity_id,message,actor,created_at FROM audit_log WHERE event_type=? ORDER BY id").map_err(sql)?;
    let mut rows = q.query([EVENT]).map_err(sql)?;
    let mut result = Checked {
        imports: vec![],
        years: BTreeMap::new(),
    };
    let mut prefix: Vec<(i64, String)> = vec![];
    let mut sources = BTreeSet::new();
    let mut ids = BTreeSet::new();
    while let Some(row) = rows.next().map_err(sql)? {
        let id: i64 = row.get(0).map_err(sql)?;
        let body: String = row.get(3).map_err(sql)?;
        let event: Attestation = serde_json::from_str(&body).map_err(sql)?;
        if id <= 0
            || event.version != 1
            || event.binding.format != policy::FORMAT
            || serde_json::to_string(&event).map_err(sql)? != body
            || event.binding.context.ledger_uuid != uuid
            || event.binding.context.currency != event.binding.archive.declared_currency
            || event.binding.context.archive_state_hash
                != sha(serde_json::to_vec(&prefix).map_err(sql)?)
            || event.plan_hash != sha(serde_json::to_vec(&event.binding).map_err(sql)?)
            || event.subject.is_empty()
            || event.subject.chars().any(char::is_control)
            || row.get::<_, String>(1).map_err(sql)? != "closed_history_archive"
            || row.get::<_, String>(2).map_err(sql)? != event.binding.source.sha256
            || row.get::<_, String>(4).map_err(sql)? != format!("user:{}", event.subject)
            || row.get::<_, String>(5).map_err(sql)? != event.at
            || !sources.insert(event.binding.source.sha256.clone())
            || event.installed.len() != event.binding.archive.years.len()
        {
            return Err(sql("archive attestation differs"));
        }
        rentemester_engine::company_knowledge::iso(&event.at)?;
        source_shape(&event.binding.source, &event.binding.archive)?;
        for (installed, year) in event.installed.iter().zip(&event.binding.archive.years) {
            if installed.fiscal_year != year.fiscal_year {
                return Err(sql("installed year differs"));
            }
            let (at, posting_ids) = stored_year(db, installed.id, year)?;
            if at != installed.imported_at {
                return Err(sql("archive imported timestamp differs"));
            }
            rentemester_engine::company_knowledge::iso(&at)?;
            if let Some(old) = result.years.get(&year.fiscal_year) {
                if old.id != installed.id
                    || old.year != *year
                    || old.currency != event.binding.archive.declared_currency
                {
                    return Err(sql("archive history collision"));
                }
            } else {
                if !ids.insert(installed.id) {
                    return Err(sql("archive identity collision"));
                }
                result.years.insert(
                    year.fiscal_year,
                    HistoricalYear {
                        id: installed.id,
                        year: year.clone(),
                        currency: event.binding.archive.declared_currency.clone(),
                        import_id: id,
                        source_hash: event.binding.source.sha256.clone(),
                        imported_at: at,
                        posting_ids,
                    },
                );
            }
        }
        prefix.push((id, body));
        result.imports.push(Import { id, event });
    }
    let headers: i64 = db
        .query_row("SELECT COUNT(*) FROM import_archive_years", [], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    let orphans:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM import_archive_postings p LEFT JOIN import_archive_years y ON y.id=p.archive_year_id WHERE y.id IS NULL) OR EXISTS(SELECT 1 FROM import_archive_balances b LEFT JOIN import_archive_years y ON y.id=b.archive_year_id WHERE y.id IS NULL)",[],|r|r.get(0)).map_err(sql)?;
    if headers != ids.len() as i64 || orphans {
        return Err(policy::refused(
            "legacy or unattested archive inventory is unsupported",
        ));
    }
    Ok(result)
}
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    checked(db).map(|_| ())
}
pub fn originals(db: &Connection) -> Result<Vec<OriginalRef>, Error> {
    Ok(checked(db)?
        .imports
        .into_iter()
        .map(|i| OriginalRef {
            import_id: i.id,
            path: i.event.binding.source.path,
            sha256: i.event.binding.source.sha256,
            size_bytes: i.event.binding.source.size_bytes,
        })
        .collect())
}
pub fn verify_original(
    db: &Connection,
    reference: &OriginalRef,
    bytes: &[u8],
) -> Result<VerifiedArchive, Error> {
    let checked = checked(db)?;
    let import = checked
        .imports
        .iter()
        .find(|i| i.id == reference.import_id)
        .ok_or_else(|| sql("import not found"))?;
    let (source, parsed) = decode_source(bytes)?;
    if source.path != reference.path
        || source.sha256 != reference.sha256
        || source.size_bytes != reference.size_bytes
        || source != import.event.binding.source
        || parsed != import.event.binding.archive
    {
        return Err(policy::refused(
            "actual archive original differs from complete stored source history",
        ));
    }
    Ok(VerifiedArchive {
        import_id: import.id,
        source_hash: source.sha256,
        binding_hash: import.event.plan_hash.clone(),
        ledger_uuid: import.event.binding.context.ledger_uuid.clone(),
    })
}
fn coverage(checked: &Checked, verified: &[VerifiedArchive]) -> Result<(), Error> {
    let mut ids = BTreeSet::new();
    for proof in verified {
        let import = checked
            .imports
            .iter()
            .find(|i| i.id == proof.import_id)
            .ok_or_else(|| sql("unknown source proof"))?;
        if !ids.insert(proof.import_id)
            || proof.source_hash != import.event.binding.source.sha256
            || proof.binding_hash != import.event.plan_hash
            || proof.ledger_uuid != import.event.binding.context.ledger_uuid
        {
            return Err(sql("source proof differs"));
        }
    }
    if ids.len() != checked.imports.len() {
        return Err(Error::new(
            "archive_original_unverified",
            "complete physical archive original verification is required",
        ));
    }
    Ok(())
}
pub fn read(
    tx: &Transaction<'_>,
    verified: &[VerifiedArchive],
    fiscal_year: Option<u16>,
) -> Result<Value, Error> {
    let checked = checked(tx)?;
    coverage(&checked, verified)?;
    if fiscal_year.is_some_and(|y| !checked.years.contains_key(&y)) {
        return Err(policy::refused("archived fiscal year not found"));
    }
    let mut years = vec![];
    for (y, h) in checked.years {
        if fiscal_year.is_some_and(|requested| requested != y) {
            continue;
        }
        let mut row = json!({"fiscalYear":y,"sourceSystem":"dinero","postingCount":h.year.postings.len(),"balanceCount":h.year.balances.len(),"importedAt":h.imported_at,"importId":h.import_id,"sourceHash":h.source_hash,"parserContract":policy::FORMAT,"declaredCurrency":h.currency,"hasBalances":h.year.has_balances});
        if fiscal_year.is_some() {
            row["postings"]=json!(h.year.postings.iter().map(|p|json!({"lineNo":p.line_no,"accountNo":p.account_no,"accountName":p.account_name,"transactionDate":p.transaction_date,"voucher":p.voucher,"voucherType":p.voucher_type,"text":p.text,"vatType":p.vat_type,"amount":p.amount.money(),"sourceAmount":p.amount,"runningBalance":p.running_balance.as_ref().map(ArchiveDecimal::money),"sourceRunningBalance":p.running_balance})).collect::<Vec<_>>());
            row["balances"]=json!(h.year.balances.iter().map(|b|json!({"lineNo":b.line_no,"accountNo":b.account_no,"accountName":b.account_name,"amount":b.amount.money(),"sourceAmount":b.amount})).collect::<Vec<_>>());
        }
        years.push(row);
    }
    Ok(json!({"ok":true,"years":years,"errors":[]}))
}
pub fn cfo_rows(
    tx: &Transaction<'_>,
    verified: &[VerifiedArchive],
    slug: &str,
    from: &str,
    to: &str,
) -> Result<Vec<Row>, Error> {
    let checked = checked(tx)?;
    coverage(&checked, verified)?;
    let mut rows = vec![];
    for h in checked.years.into_values() {
        for (p, id) in h.year.postings.iter().zip(h.posting_ids) {
            if p.transaction_date.as_str() < from || p.transaction_date.as_str() > to {
                continue;
            }
            rows.push(Row {
                company_slug: slug.into(),
                source_type: "archive".into(),
                source_hash: h.source_hash.clone(),
                source_id: format!("archive:{}:posting:{id}", h.id),
                journal_entry_id: None,
                journal_entry_no: (!p.voucher.is_empty()).then(|| p.voucher.clone()),
                document_id: None,
                document_hash: None,
                party_id: None,
                party_name: None,
                document_party_name: None,
                party_link_provenance: None,
                account_no: p.account_no.clone(),
                account_name: Some(p.account_name.clone()),
                transaction_date: p.transaction_date.clone(),
                currency: h.currency.clone(),
                amount: p.amount.money(),
                text: Some(p.text.clone()),
                dimensions: vec![],
            });
        }
    }
    Ok(rows)
}
