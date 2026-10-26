import type { Database } from "bun:sqlite";
import { insertAuditLog } from "./actor";
import { finalizeVatForm, type VatFormInput, type VatRubric } from "./vat-rubric";
import type { VatPeriodReport } from "./vat";

export const VAT_EVIDENCE_FIELDS = [
  "rubrikBVarerEuSalesList", "rubrikBVarerIkkeEuSalesList", "rubrikBYdelser",
  "olieOgFlaskegasafgift", "elafgift", "naturgasOgBygasafgift", "kulafgift",
  "co2Afgift", "vandafgift",
] as const;
type VatEvidenceField = (typeof VAT_EVIDENCE_FIELDS)[number];

function validText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.trim().length > 0 && value.trim().length <= max;
}

export function currentVatFilingEvidence(db: Database, from: string, to: string): Partial<Record<VatEvidenceField, number>> {
  const rows = db.query("SELECT field_name, amount_dkk FROM current_vat_filing_evidence WHERE period_start=? AND period_end=?").all(from, to) as Array<{field_name: VatEvidenceField; amount_dkk:number}>;
  return Object.fromEntries(rows.map(row => [row.field_name, row.amount_dkk]));
}

/** An actor-audited, append-only classification or refund evidence event. */
export function recordVatFilingEvidence(db: Database, input: {
  periodStart: string; periodEnd: string; fieldName: VatEvidenceField; amountDkk: number;
  evidenceRef: string; actor: string; principal: string; confirm: boolean;
}) {
  if (!input.confirm) return { ok: false as const, errors: ["CONFIRMATION_REQUIRED"] };
  if (!/^\d{4}-\d{2}-\d{2}$/.test(input.periodStart) || !/^\d{4}-\d{2}-\d{2}$/.test(input.periodEnd)
    || !VAT_EVIDENCE_FIELDS.includes(input.fieldName) || !Number.isFinite(input.amountDkk)
    || !validText(input.evidenceRef, 500) || !validText(input.actor, 160) || !validText(input.principal, 160)) {
    return { ok: false as const, errors: ["INVALID_VAT_FILING_EVIDENCE"] };
  }
  const current = db.query("SELECT id,amount_dkk,evidence_ref FROM current_vat_filing_evidence WHERE period_start=? AND period_end=? AND field_name=?").get(input.periodStart, input.periodEnd, input.fieldName) as {id:number;amount_dkk:number;evidence_ref:string}|null;
  if (current && current.amount_dkk === input.amountDkk && current.evidence_ref === input.evidenceRef.trim()) return { ok:true as const, id:current.id, idempotent:true };
  db.transaction(() => {
    if (current) db.query("INSERT INTO vat_filing_evidence_events(period_start,period_end,field_name,amount_dkk,evidence_ref,event_type,supersedes_event_id,actor,principal,created_at) VALUES(?,?,?,?,?,'superseded',?,?,?,?)").run(input.periodStart,input.periodEnd,input.fieldName,current.amount_dkk,current.evidence_ref,current.id,input.actor.trim(),input.principal.trim(),new Date().toISOString());
    db.query("INSERT INTO vat_filing_evidence_events(period_start,period_end,field_name,amount_dkk,evidence_ref,event_type,actor,principal,created_at) VALUES(?,?,?,?,?,'recorded',?,?,?)").run(input.periodStart,input.periodEnd,input.fieldName,input.amountDkk,input.evidenceRef.trim(),input.actor.trim(),input.principal.trim(),new Date().toISOString());
    insertAuditLog(db, {
      eventType: "vat_filing_evidence_recorded",
      entityType: "vat_filing_evidence",
      entityId: `${input.periodStart}:${input.periodEnd}:${input.fieldName}`,
      message: `Recorded evidence-backed VAT filing field ${input.fieldName}`,
      createdBy: input.actor.trim(),
      createdByProgram: input.principal.trim(),
    });
  })();
  const row = db.query("SELECT id FROM current_vat_filing_evidence WHERE period_start=? AND period_end=? AND field_name=?").get(input.periodStart,input.periodEnd,input.fieldName) as {id:number};
  return { ok:true as const,id:row.id,idempotent:false };
}

export function vatFilingFormForPeriod(db: Database, report: VatPeriodReport): VatRubric {
  const base = report.rubrikker;
  const evidence = currentVatFilingEvidence(db, report.periodStart, report.periodEnd);
  const raw: VatFormInput = { ...base, ...evidence };
  return finalizeVatForm(raw, report.netVatPayable);
}
