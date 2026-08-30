import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { isValidIsoDate } from "./dates";
import { toOre } from "./money";

export type ImportedReceivableSchedule = {
  contract: "rentemester-imported-receivables-v1";
  sourceDocumentHash: string;
  invoices: Array<{
    id: string; customerId?: string; customerName?: string; invoiceDate: string; dueDate?: string;
    grossAmount: number; controlAccountNo: string; recognitionRef: string; documentHash: string;
    payments?: Array<{ id: string; paymentDate: string; amount: number; paymentRef: string; documentHash: string }>;
  }>;
};

const sha = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const text = (value: unknown) => typeof value === "string" ? value.trim() : "";
const hash = (value: unknown) => /^[a-f0-9]{64}$/i.test(text(value));
const amount = (value: unknown) => typeof value === "number" && Number.isFinite(value) && toOre(value) > 0n;

/** Validate the explicitly supported schedule contract; never derive invoices from voucher text. */
export function validateImportedReceivableSchedule(input: unknown): { ok: true; schedule: ImportedReceivableSchedule; hash: string } | { ok: false; errors: string[] } {
  const schedule = input as ImportedReceivableSchedule;
  const errors: string[] = [];
  if (!schedule || schedule.contract !== "rentemester-imported-receivables-v1") errors.push("imported receivable schedule must use contract rentemester-imported-receivables-v1");
  if (!hash(schedule?.sourceDocumentHash)) errors.push("imported receivable schedule needs a source document SHA-256");
  if (!Array.isArray(schedule?.invoices) || schedule.invoices.length === 0) errors.push("imported receivable schedule needs at least one invoice");
  const ids = new Set<string>();
  for (const invoice of schedule?.invoices ?? []) {
    const id = text(invoice?.id); if (!id || ids.has(id)) errors.push(`imported receivable invoice has missing or duplicate id '${id}'`); ids.add(id);
    if (!isValidIsoDate(invoice?.invoiceDate ?? "")) errors.push(`imported receivable ${id || "?"} has invalid invoice date`);
    if (invoice?.dueDate != null && !isValidIsoDate(invoice.dueDate)) errors.push(`imported receivable ${id || "?"} has invalid due date`);
    if (!amount(invoice?.grossAmount)) errors.push(`imported receivable ${id || "?"} needs a positive gross amount`);
    if (!text(invoice?.controlAccountNo) || !text(invoice?.recognitionRef) || !hash(invoice?.documentHash)) errors.push(`imported receivable ${id || "?"} lacks authoritative source evidence`);
    let paid = 0n; const paymentIds = new Set<string>();
    for (const payment of invoice?.payments ?? []) { const paymentId=text(payment?.id); if (!paymentId || paymentIds.has(paymentId)) errors.push(`imported receivable ${id || "?"} has missing or duplicate payment id '${paymentId}'`); paymentIds.add(paymentId); if (!isValidIsoDate(payment?.paymentDate ?? "") || !amount(payment?.amount) || !text(payment?.paymentRef) || !hash(payment?.documentHash)) errors.push(`imported receivable ${id || "?"} payment ${paymentId || "?"} lacks authoritative evidence`); if (amount(payment?.amount)) paid += toOre(payment.amount); }
    if (amount(invoice?.grossAmount) && paid > toOre(invoice.grossAmount)) errors.push(`imported receivable ${id || "?"} payments exceed the invoice amount`);
  }
  return errors.length ? { ok: false, errors } : { ok: true, schedule, hash: sha(schedule) };
}

/** Persist one immutable imported schedule. Replays are identical or fail closed. */
export function recordImportedReceivableSchedule(db: Database, attemptId: number, input: unknown): { ok: boolean; errors: string[]; scheduleHash?: string } {
  const checked = validateImportedReceivableSchedule(input); if (!checked.ok) return checked;
  const prior = db.query("SELECT schedule_hash FROM imported_receivable_headers WHERE dinero_import_attempt_id=? LIMIT 1").get(attemptId) as { schedule_hash:string } | null;
  if (prior) return prior.schedule_hash === checked.hash ? { ok:true, errors:[], scheduleHash: checked.hash } : { ok:false, errors:["imported receivable schedule conflicts with accepted source"] };
  try { db.transaction(() => {
    const add = db.query("INSERT INTO imported_receivable_headers(dinero_import_attempt_id,external_invoice_id,source_document_hash,customer_external_id,customer_name,invoice_date,due_date,gross_amount,control_account_no,source_recognition_ref,schedule_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?) RETURNING id");
    const addPayment = db.query("INSERT INTO imported_receivable_payments(receivable_id,external_payment_id,payment_date,amount,source_payment_ref,source_document_hash,schedule_hash) VALUES(?,?,?,?,?,?,?)");
    for (const invoice of checked.schedule.invoices) {
      if (!db.query("SELECT id FROM accounts WHERE account_no=?").get(invoice.controlAccountNo)) throw new Error(`imported receivable ${invoice.id} has unknown control account`);
      const row = add.get(attemptId,invoice.id,invoice.documentHash,text(invoice.customerId)||null,text(invoice.customerName)||null,invoice.invoiceDate,invoice.dueDate ?? null,invoice.grossAmount,invoice.controlAccountNo,invoice.recognitionRef,checked.hash) as {id:number};
      for (const payment of invoice.payments ?? []) addPayment.run(row.id,payment.id,payment.paymentDate,payment.amount,payment.paymentRef,payment.documentHash,checked.hash);
    }
  }).immediate(); return {ok:true,errors:[],scheduleHash:checked.hash}; } catch (error) { return {ok:false,errors:[error instanceof Error ? error.message : String(error)]}; }
}

/** Exact imported source balance at a date, including paid and fully-settled invoices. */
export function importedReceivableBalanceOre(db: Database, cutoff: string, controlAccountNo: string): { total: bigint; evidence: Array<Record<string, unknown>> } {
  const records = db.query(`SELECT h.id,h.external_invoice_id,h.customer_external_id,h.customer_name,h.invoice_date,h.due_date,h.gross_amount,h.source_document_hash,h.schedule_hash,COALESCE(SUM(CASE WHEN p.payment_date<=? THEN p.amount ELSE 0 END),0) paid_amount FROM imported_receivable_headers h LEFT JOIN imported_receivable_payments p ON p.receivable_id=h.id WHERE h.invoice_date<=? AND h.control_account_no=? GROUP BY h.id ORDER BY h.id`).all(cutoff,cutoff,controlAccountNo) as Array<Record<string,unknown>>;
  return { total: records.reduce((sum,row)=>sum+toOre(Number(row.gross_amount))-toOre(Number(row.paid_amount)),0n), evidence: records.map(row=>({source:"imported-receivable",externalInvoiceId:row.external_invoice_id,customerExternalId:row.customer_external_id,customerName:row.customer_name,invoiceDate:row.invoice_date,dueDate:row.due_date,grossDkk:row.gross_amount,paidDkk:row.paid_amount,sourceDocumentHash:row.source_document_hash,scheduleHash:row.schedule_hash})) };
}
