//! Source-backed advisory preview. No transaction, number/hash reservation or posting authority.
use crate::{Snapshot, company_baseline, migration};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    identifiers::{AccountNo, BankTransactionId, DocumentId, JournalEntryId, PeriodId},
    journal_rules::{ManualJournalRules, RuleBundle},
    manual_journal::{
        self, AccountFact, AccountKind, BankFact, DocumentFact, JournalFacts, ManualJournal,
        PeriodFact, PeriodKind, PeriodState, Validation,
    },
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
pub const REQUEST_FORMAT: &str = "rentemester-manual-journal-request/1";
pub const MAX_REQUEST_BYTES: u64 = 1_048_576;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub format: String,
    pub expected_rule_version: String,
    pub latest_allowed_date: Date,
    pub request: ManualJournal,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    pub advisory: bool,
    pub clock_policy: &'static str,
    pub latest_allowed_date: Date,
    pub rule_artifact_sha256: String,
    pub validation: Validation,
}
#[derive(Deserialize)]
struct Catalogue {
    bundles: Vec<RuleBundle>,
}
fn compiled_rules() -> Result<ManualJournalRules, Error> {
    let catalogue: Catalogue = serde_json::from_str(include_str!("manual-rule-bundles.json"))
        .map_err(|_| Error::new("invalid_rules", "compiled manual metadata is invalid"))?;
    ManualJournalRules::from_bundles(catalogue.bundles)
}
pub fn manual_journal_rule_version() -> Result<String, Error> {
    Ok(compiled_rules()?.version().to_owned())
}
fn malformed(_: impl std::fmt::Display) -> Error {
    Error::new(
        "invalid_ledger",
        "journal preview source facts are malformed or unsupported",
    )
}
/// All source access uses a private captured image; caller-supplied horizon is advisory only.
pub fn preview_manual_journal(company: &Path, input: PreviewInput) -> Result<PreviewReport, Error> {
    if input.format != REQUEST_FORMAT {
        return Err(Error::new(
            "invalid_input",
            "unsupported journal preview request format",
        ));
    }
    let rules = compiled_rules()?;
    rules.require_version(&input.expected_rule_version)?;
    // Unvalidated capture lets EVERY admission/refusal pass through checked owned cleanup.
    let Snapshot { conn, _dir } = Snapshot::capture_unvalidated(company)?;
    let result = (|| {
        crate::validate(&conn)?;
        let expected: Vec<crate::Migration> =
            serde_json::from_str(include_str!("legacy-schema.json")).map_err(malformed)?;
        let history = migration::strict_history(&conn, "schema_migrations", &expected)?;
        if history.len() != expected.len()
            || migration::schema(&conn)? != company_baseline::load_images()?.current
        {
            return Err(Error::new(
                "unsupported_schema",
                "journal preview requires complete canonical current company schema",
            ));
        }
        let facts = collect_facts(&conn, &input.request, input.latest_allowed_date)?;
        let validation =
            manual_journal::validate(&input.request, &facts, &rules, &input.expected_rule_version)?;
        Ok(PreviewReport {
            advisory: true,
            clock_policy: "explicit-preview-horizon",
            latest_allowed_date: input.latest_allowed_date,
            rule_artifact_sha256: format!(
                "{:x}",
                Sha256::digest(include_bytes!("manual-rule-bundles.json"))
            ),
            validation,
        })
    })();
    let close = conn.close().map_err(|_| {
        Error::new(
            "storage_failure",
            "private journal preview connection could not close",
        )
    });
    let cleanup = _dir.close().map_err(|_| {
        Error::new(
            "storage_failure",
            "private journal preview could not be removed",
        )
    });
    close?;
    cleanup?;
    result
}
fn account_kind(value: &str) -> Result<AccountKind, Error> {
    Ok(match value {
        "asset" => AccountKind::Asset,
        "liability" => AccountKind::Liability,
        "equity" => AccountKind::Equity,
        "income" => AccountKind::Income,
        "expense" => AccountKind::Expense,
        "vat" => AccountKind::Vat,
        _ => return Err(malformed("account kind")),
    })
}
fn period_kind(value: &str) -> Result<PeriodKind, Error> {
    Ok(match value {
        "vat_period" => PeriodKind::VatPeriod,
        "vat_quarter" => PeriodKind::VatQuarter,
        "fiscal_year" => PeriodKind::FiscalYear,
        "custom" => PeriodKind::Custom,
        _ => return Err(malformed("period kind")),
    })
}
fn collect_facts(
    db: &Connection,
    request: &ManualJournal,
    latest_allowed_date: Date,
) -> Result<JournalFacts, Error> {
    let mut accounts = Vec::new();
    let mut query = db
        .prepare("SELECT account_no,type,normal_balance,active FROM accounts ORDER BY account_no")
        .map_err(malformed)?;
    let mut rows = query.query([]).map_err(malformed)?;
    while let Some(r) = rows.next().map_err(malformed)? {
        let number: String = r.get(0).map_err(malformed)?;
        let kind: String = r.get(1).map_err(malformed)?;
        let normal: String = r.get(2).map_err(malformed)?;
        let active: i64 = r.get(3).map_err(malformed)?;
        if !["debit", "credit"].contains(&normal.as_str()) {
            return Err(malformed("account normal balance"));
        }
        let legacy_control = active == 1
            && kind == "liability"
            && normal == "credit"
            && ["64000", "64020", "64040"].contains(&number.as_str());
        accounts.push(AccountFact {
            account_no: AccountNo::parse(&number).map_err(malformed)?,
            kind: account_kind(&kind)?,
            active: active != 0,
            vat_control: kind == "vat" || legacy_control,
            vat_settlement: false,
        });
    }
    let mut roles=db.prepare("SELECT m.role,m.account_no,a.type,a.normal_balance,a.active FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.status='confirmed' AND m.role IN ('output_vat','input_vat','reverse_charge_vat','vat_settlement')").map_err(malformed)?;
    let mut rows = roles.query([]).map_err(malformed)?;
    while let Some(r) = rows.next().map_err(malformed)? {
        let role: String = r.get(0).map_err(malformed)?;
        let number: String = r.get(1).map_err(malformed)?;
        let kind: String = r.get(2).map_err(malformed)?;
        let normal: String = r.get(3).map_err(malformed)?;
        let active: i64 = r.get(4).map_err(malformed)?;
        let (expected_kind, expected_normal) = match role.as_str() {
            "input_vat" => ("vat", "debit"),
            "output_vat" | "reverse_charge_vat" => ("vat", "credit"),
            "vat_settlement" => ("liability", "credit"),
            _ => return Err(malformed("role")),
        };
        if active != 1 || kind != expected_kind || normal != expected_normal {
            continue;
        }
        let a = accounts
            .iter_mut()
            .find(|a| a.account_no.as_str() == number)
            .ok_or_else(|| malformed("role account"))?;
        if role == "vat_settlement" {
            a.vat_settlement = true;
        } else {
            a.vat_control = true;
        }
    }
    let mut documents = Vec::new();
    if let Some(id) = request.document_id {
        let present: Option<i64> = db
            .query_row("SELECT id FROM documents WHERE id=?1", [id.value()], |r| {
                r.get(0)
            })
            .optional()
            .map_err(malformed)?;
        if let Some(found) = present {
            let entry:Option<String>=db.query_row("SELECT existing.entry_no FROM documents credit JOIN journal_entries existing ON existing.document_id=credit.id WHERE credit.id=?1 AND credit.document_type='credit_note' ORDER BY existing.id ASC LIMIT 1",[id.value()],|r|r.get(0)).optional().map_err(malformed)?;
            documents.push(DocumentFact {
                id: DocumentId::from_sqlite(found),
                existing_credit_note_entry: entry,
            });
        }
    }
    let mut banks = Vec::new();
    if let Some(id) = request.source_bank_transaction_id {
        let present: Option<i64> = db
            .query_row(
                "SELECT id FROM bank_transactions WHERE id=?1",
                [id.value()],
                |r| r.get(0),
            )
            .optional()
            .map_err(malformed)?;
        if let Some(found) = present {
            let link:Option<i64>=db.query_row("SELECT journal_entry_id FROM bank_journal_reconciliation_links WHERE bank_transaction_id=?1",[id.value()],|r|r.get(0)).optional().map_err(malformed)?;
            banks.push(BankFact {
                id: BankTransactionId::from_sqlite(found),
                linked_journal: link.map(JournalEntryId::from_sqlite),
            });
        }
    }
    let mut periods = Vec::new();
    let date = request.transaction_date.to_string();
    let mut query=db.prepare("SELECT id,period_start,period_end,kind,status,reference FROM accounting_periods WHERE period_start<=?1 AND period_end>=?1 AND status IN ('closed','reported') ORDER BY period_end,id").map_err(malformed)?;
    let mut rows = query.query([&date]).map_err(malformed)?;
    while let Some(r) = rows.next().map_err(malformed)? {
        let id: i64 = r.get(0).map_err(malformed)?;
        let start: String = r.get(1).map_err(malformed)?;
        let end: String = r.get(2).map_err(malformed)?;
        let kind: String = r.get(3).map_err(malformed)?;
        let status: String = r.get(4).map_err(malformed)?;
        let reference: Option<String> = r.get(5).map_err(malformed)?;
        let mut state = match status.as_str() {
            "closed" => PeriodState::Closed,
            "reported" => PeriodState::Reported,
            _ => return Err(malformed("period state")),
        };
        let mut audit=db.prepare("SELECT event_type FROM audit_log WHERE entity_type='accounting_period' AND entity_id=?1 ORDER BY id").map_err(malformed)?;
        let mut events = audit.query([id.to_string()]).map_err(malformed)?;
        while let Some(e) = events.next().map_err(malformed)? {
            let event: String = e.get(0).map_err(malformed)?;
            if matches!(state, PeriodState::Reported) {
                continue;
            }
            state = match event.as_str() {
                "period_report" => PeriodState::Reported,
                "period_close" => PeriodState::Closed,
                "period_reopen" => PeriodState::Open,
                _ => state,
            };
        }
        let start = Date::parse(&start).map_err(malformed)?;
        let end = Date::parse(&end).map_err(malformed)?;
        if start > end {
            return Err(malformed("period range"));
        }
        periods.push(PeriodFact {
            id: PeriodId::from_sqlite(id),
            start,
            end,
            kind: period_kind(&kind)?,
            effective_state: state,
            reference,
        });
    }
    Ok(JournalFacts {
        transaction_date: request.transaction_date,
        latest_allowed_date,
        accounts,
        documents,
        banks,
        periods,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_current_image_matches_reconstructed_native_schema() {
        let images = company_baseline::load_images().unwrap();
        let db = Connection::open_in_memory().unwrap();
        for kind in ["table", "index", "view", "trigger"] {
            for object in images.current.iter().filter(|o| o.kind == kind) {
                db.execute_batch(&object.sql).unwrap();
            }
        }
        assert_eq!(migration::schema(&db).unwrap(), images.current);
        assert!(company_baseline::recognizes(&db, 36).unwrap());
    }
}
