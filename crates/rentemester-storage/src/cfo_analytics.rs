//! Admitted company-local CFO source projection. Callers hold current control authorization.
use rentemester_engine::{
    Error,
    cfo_analytics::{self as policy, CompanyRows, Dimension, PartyLinkProvenance, Query, Row},
    fiscal_year::Date,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
mod dimensions;
#[cfg(test)]
mod tests;
fn sql(_: impl std::fmt::Display) -> Error {
    policy::refused("CFO source is unavailable or invalid")
}
/// Complete current schema, full immutable journal chain and canonical link history precede projection.
/// Physical originals remain the host's admitted namespace verification responsibility.
pub fn collect(
    company: &Transaction<'_>,
    control: &Transaction<'_>,
    slug: &str,
    query: &Query,
    visible_companies: &BTreeSet<String>,
) -> Result<CompanyRows, Error> {
    query.validate()?;
    if !visible_companies.contains(slug) {
        return Err(policy::refused("company is not accessible"));
    }
    crate::journal_preview::require_current(company)?;
    crate::migration::integrity(company)?;
    if !crate::journal_hash::verify(company)?.ok {
        return Err(policy::refused("journal integrity differs"));
    }
    crate::document_party_links::validate_backup(company)?;
    let currency: String = company
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(sql)?;
    if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(policy::refused("unverified functional currency"));
    }
    let mut q=company.prepare("SELECT j.id,j.entry_no,j.entry_hash,j.transaction_date,j.document_id,d.sha256_hash,d.supplier_name,j.text,l.id,l.debit_amount,l.credit_amount,l.text,a.account_no,a.name,l.currency FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id JOIN accounts a ON a.id=l.account_id LEFT JOIN documents d ON d.id=j.document_id WHERE j.status IN('posted','reversed') AND j.transaction_date BETWEEN ? AND ? ORDER BY j.id,l.id").map_err(sql)?;
    let mut rs = q.query(params![query.from, query.to]).map_err(sql)?;
    let mut rows = vec![];
    while let Some(r) = rs.next().map_err(sql)? {
        let entry: i64 = r.get(0).map_err(sql)?;
        let entry_hash: String = r.get(2).map_err(sql)?;
        let line: i64 = r.get(8).map_err(sql)?;
        let date: String = r.get(3).map_err(sql)?;
        Date::parse(&date)?;
        if r.get::<_, String>(14).map_err(sql)? != currency {
            return Err(policy::refused("line and functional currency differ"));
        }
        let amount = crate::legacy_money(r.get_ref(9).map_err(sql)?)?
            .subtract(&crate::legacy_money(r.get_ref(10).map_err(sql)?)?);
        let document: Option<i64> = r.get(4).map_err(sql)?;
        let mut row = Row {
            company_slug: slug.into(),
            source_type: "ledger".into(),
            source_hash: entry_hash.clone(),
            source_id: format!("journal:{entry}:line:{line}"),
            journal_entry_id: Some(entry),
            journal_entry_no: Some(r.get(1).map_err(sql)?),
            document_id: document,
            document_hash: r.get(5).map_err(sql)?,
            party_id: None,
            party_name: None,
            document_party_name: r.get(6).map_err(sql)?,
            party_link_provenance: None,
            account_no: r.get(12).map_err(sql)?,
            account_name: r.get(13).map_err(sql)?,
            transaction_date: date,
            currency: currency.clone(),
            amount,
            text: r
                .get::<_, Option<String>>(11)
                .map_err(sql)?
                .or(r.get(7).map_err(sql)?),
            dimensions: dimensions::read(company, line, entry, &entry_hash, &currency)?,
        };
        if let Some(id) = document {
            party(company, control, id, visible_companies, &mut row)?;
        }
        rows.push(row);
    }
    rows.extend(archive(company)?);
    let posted:i64=company.query_row("SELECT COUNT(*) FROM journal_entries WHERE status='posted' AND reversal_of_entry_id IS NULL AND transaction_date BETWEEN ? AND ? AND document_id IS NULL",params![query.from,query.to],|r|r.get(0)).map_err(sql)?;
    let exceptions: i64 = company
        .query_row(
            "SELECT COUNT(*) FROM exceptions WHERE status='open'",
            [],
            |r| r.get(0),
        )
        .map_err(sql)?;
    Ok(CompanyRows {
        company_slug: slug.into(),
        rows,
        evidence: json!({"companySlug":slug,"status":"ready","postedWithoutDocument":posted,"openExceptions":exceptions}),
    })
}
fn party(
    company: &Transaction<'_>,
    control: &Transaction<'_>,
    document: i64,
    visible: &BTreeSet<String>,
    row: &mut Row,
) -> Result<(), Error> {
    let erased:bool=company.query_row("SELECT EXISTS(SELECT 1 FROM gdpr_erasures WHERE source='documents' AND source_row_id=? AND redacted_fields!='[]')",[document],|r|r.get(0)).map_err(sql)?;
    if erased {
        row.document_party_name = None;
        row.text = None;
        return Ok(());
    }
    let link:Option<(i64,String,String,String,String,String,String)>=company.query_row("SELECT id,party_id,party_role,evidence_kind,party_snapshot_json,plan_hash,created_at FROM current_document_party_links WHERE document_id=? ORDER BY CASE party_role WHEN 'supplier' THEN 0 WHEN 'issuer' THEN 1 WHEN 'vendor' THEN 2 WHEN 'customer' THEN 3 WHEN 'recipient' THEN 4 ELSE 99 END,id DESC LIMIT 1",[document],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(sql)?;
    if let Some((id, party, role, kind, raw, plan, at)) = link {
        let snapshot: Value = serde_json::from_str(&raw).map_err(sql)?;
        let name = snapshot["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| sql("snapshot name"))?;
        if snapshot["partyId"] != party {
            return Err(sql("snapshot identity"));
        }
        if !crate::parties::visible(control, &party, visible)? {
            row.document_party_name = None;
            row.text = None;
            return Ok(());
        }
        // Company-local erasure aliases also fence historical immutable snapshot display.
        let tax = snapshot["identifiers"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|i| i["identifier"].as_str());
        if let Err(error) =
            crate::gdpr::require_contact_identity(company, "vendors", None, name, tax)
        {
            if error.code != "gdpr_refused" {
                return Err(error);
            }
            row.document_party_name = None;
            row.text = None;
            return Ok(());
        }
        row.party_id = Some(party);
        row.party_name = Some(name.into());
        row.party_link_provenance = Some(PartyLinkProvenance {
            link_id: id,
            role,
            evidence_kind: kind,
            plan_hash: plan,
            linked_at: at,
        });
    }
    Ok(())
}
fn archive(tx: &Transaction<'_>) -> Result<Vec<Row>, Error> {
    let present:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM import_archive_years) OR EXISTS(SELECT 1 FROM import_archive_postings)",[],|r|r.get(0)).map_err(sql)?;
    if present {
        return Err(Error::new(
            "cfo_archive_unavailable",
            "Retained archive posting source assurance is unavailable: native closed-history ingestion and physical source attestation are not implemented",
        ));
    }
    Ok(vec![])
}
