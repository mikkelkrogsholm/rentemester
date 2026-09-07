import type { Database } from "bun:sqlite";

/**
 * Read-only, source-bound explanation of one journal entry.  This deliberately
 * does not attempt similarity matching: every relation below is an FK or an
 * append-only document-party link.
 */
export type JournalExplanation = {
  entry: { id: number; entryNo: string; transactionDate: string; text: string; status: string; registrationDatetime: string; documentId: number | null; bankTransactionId: number | null };
  sourceFacts: { document: Record<string, unknown> | null; bankTransaction: Record<string, unknown> | null; party: Record<string, unknown> | null; accounts: Array<Record<string, unknown>>; vatCodes: string[] };
  appliedRule: { ruleId: string; version: number; effectiveFrom: string; effectiveTo: string | null; outcome: unknown; rationale: string; provenance: string } | null;
  legalSource: { status: "not_registered"; text: string };
  professionalAssessment: { text: string; record: null };
  correction: { sentence: string | null; reversalOfEntryId: number | null; reversedByEntryId: number | null };
  evidence: { entryHash: string; previousHash: string | null; createdBy: string; createdByProgram: string; lines: Array<Record<string, unknown>> };
};

export function explainJournalEntry(db: Database, entryId: number): { ok: true; explanation: JournalExplanation } | { ok: false; errors: string[] } {
  if (!Number.isInteger(entryId) || entryId < 1) return { ok: false, errors: ["journal entry id must be a positive integer"] };
  const entry = db.query(`SELECT id,entry_no AS entryNo,transaction_date AS transactionDate,text,status,registration_datetime AS registrationDatetime,document_id AS documentId,source_bank_transaction_id AS bankTransactionId,entry_hash AS entryHash,previous_hash AS previousHash,created_by AS createdBy,created_by_program AS createdByProgram,reversal_of_entry_id AS reversalOfEntryId FROM journal_entries WHERE id=?`).get(entryId) as any;
  if (!entry) return { ok: false, errors: ["journal entry not found"] };
  const document = entry.documentId == null ? null : db.query(`SELECT id,document_no AS documentNo,source,original_filename AS originalFilename,sha256_hash AS sha256,invoice_no AS invoiceNo,invoice_date AS invoiceDate,amount_inc_vat AS amountIncVat,currency,status,document_type AS documentType,supplier_name AS supplierName,sender_name AS senderName,sender_vat_cvr AS senderVatCvr,vat_amount AS vatAmount FROM documents WHERE id=?`).get(entry.documentId) as any ?? null;
  const bankTransaction = entry.bankTransactionId == null ? null : db.query(`SELECT id,transaction_date AS transactionDate,booking_date AS bookingDate,text,amount,currency,reference,transaction_hash AS transactionHash,status,counterparty_name AS counterpartyName FROM bank_transactions WHERE id=?`).get(entry.bankTransactionId) as any ?? null;
  const party = entry.documentId == null ? null : db.query(`SELECT party_id AS partyId,party_role AS role,party_snapshot_json AS snapshot,evidence_kind AS evidenceKind FROM current_document_party_links WHERE document_id=? ORDER BY id DESC LIMIT 1`).get(entry.documentId) as any ?? null;
  const lines = db.query(`SELECT jl.id AS journalLineId,a.account_no AS accountNo,a.name AS accountName,a.type AS accountType,jl.debit_amount AS debit,jl.credit_amount AS credit,jl.vat_code AS vatCode,jl.text FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=? ORDER BY jl.id`).all(entryId) as any[];
  const app = entry.documentId == null ? null : db.query(`SELECT p.rule_id AS ruleId,p.version,p.effective_from AS effectiveFrom,p.effective_to AS effectiveTo,p.outcome_json AS outcomeJson,p.rationale,p.provenance FROM posting_rule_applications a JOIN posting_rule_versions p ON p.id=a.rule_version_id WHERE a.document_id=? AND a.decision='proposed' ORDER BY a.id DESC LIMIT 1`).get(entry.documentId) as any ?? null;
  const reversedBy = db.query(`SELECT id FROM journal_entries WHERE reversal_of_entry_id=? ORDER BY id DESC LIMIT 1`).get(entryId) as { id: number } | null;
  const correctionSentence = entry.reversalOfEntryId != null
    ? `Denne postering tilbagefører postering ${entry.reversalOfEntryId}; originalen er bevaret i revisionssporet.`
    : reversedBy ? `Denne postering er tilbageført af postering ${reversedBy.id}; begge poster er bevaret i revisionssporet.` : null;
  return { ok: true, explanation: {
    entry: { id: entry.id, entryNo: entry.entryNo, transactionDate: entry.transactionDate, text: entry.text, status: entry.status, registrationDatetime: entry.registrationDatetime, documentId: entry.documentId, bankTransactionId: entry.bankTransactionId },
    sourceFacts: { document, bankTransaction, party, accounts: lines.map(({ vatCode: _vatCode, ...line }) => line), vatCodes: [...new Set(lines.map((line) => line.vatCode).filter(Boolean))] },
    appliedRule: app ? { ruleId: app.ruleId, version: app.version, effectiveFrom: app.effectiveFrom, effectiveTo: app.effectiveTo, outcome: JSON.parse(app.outcomeJson), rationale: app.rationale, provenance: app.provenance } : null,
    legalSource: { status: "not_registered", text: "Ingen lov- eller myndighedskilde er eksplicit knyttet til denne postering." },
    professionalAssessment: { text: "ingen registreret vurdering", record: null },
    correction: { sentence: correctionSentence, reversalOfEntryId: entry.reversalOfEntryId, reversedByEntryId: reversedBy?.id ?? null },
    evidence: { entryHash: entry.entryHash, previousHash: entry.previousHash, createdBy: entry.createdBy, createdByProgram: entry.createdByProgram, lines },
  } };
}
