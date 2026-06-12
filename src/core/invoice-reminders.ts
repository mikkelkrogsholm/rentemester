import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { getInvoiceStatus } from "./invoice-payments";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate, diffDays } from "./dates";
import { roundDkk } from "./money";

const RULE_ID = "DK-INVOICE-REMINDER-FEE-001";
const BOOKKEEPING_RULE_ID = "DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001";
const MAX_REMINDER_FEE_DKK = 100;
const MAX_REMINDERS_PER_CLAIM = 3;
const MIN_DAYS_BETWEEN_REMINDERS = 10;

export type RegisterInvoiceReminderInput = {
  invoiceDocumentId: number;
  reminderDate: string;
  feeAmount?: number;
  note?: string;
  // Actor attribution for the registration audit_log row — the MCP/CLI caller
  // threads these so the row is stamped with the booking agent, not the OS
  // user that resolveActor would otherwise infer. The post step already
  // forwards them; the register step must too.
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterInvoiceReminderResult = {
  ok: boolean;
  reminderId?: number;
  reminderSequence?: number;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  reminderDate?: string;
  feeAmount?: number;
  totalReminderFees?: number;
  appliedRules: string[];
  errors: string[];
};

export type PostInvoiceReminderToLedgerInput = {
  invoiceDocumentId: number;
  reminderId?: number;
  transactionDate?: string;
  receivableAccountNo?: string;
  reminderIncomeAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PostInvoiceReminderToLedgerResult = JournalPostResult & {
  reminderId?: number;
  invoiceDocumentId?: number;
  invoiceNumber?: string;
  reminderDate?: string;
  feeAmount?: number;
  claimOpenBalance?: number;
};

export function registerInvoiceReminder(db: Database, input: RegisterInvoiceReminderInput): RegisterInvoiceReminderResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) errors.push("invoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.reminderDate)) errors.push("reminderDate must be YYYY-MM-DD");
  if (input.feeAmount !== undefined && (!Number.isFinite(input.feeAmount) || input.feeAmount <= 0)) errors.push("feeAmount must be a positive number when present");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const invoice = db.query(`SELECT id, invoice_no, currency, document_type FROM documents WHERE id = ?`).get(input.invoiceDocumentId) as { id: number; invoice_no: string; currency: string | null; document_type: string } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  if ((invoice.currency ?? "DKK") !== "DKK") return { ok: false, appliedRules: [RULE_ID], errors: ["only DKK issued invoices are supported in the current reminder flow"] };

  const feeAmount = roundDkk(input.feeAmount ?? MAX_REMINDER_FEE_DKK);
  if (feeAmount > MAX_REMINDER_FEE_DKK) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`reminder fee ${feeAmount} exceeds statutory maximum ${MAX_REMINDER_FEE_DKK}`] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId, input.reminderDate);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID], errors: status.errors };
  if (!status.isOverdue || !(Number(status.openBalance ?? 0) > 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoice must be overdue with positive open balance on reminderDate"] };
  }

  const reminders = db.query(
    `SELECT id, reminder_date, fee_amount
     FROM invoice_reminders
     WHERE invoice_document_id = ?
     ORDER BY reminder_date ASC, id ASC`
  ).all(input.invoiceDocumentId) as Array<{ id: number; reminder_date: string; fee_amount: number }>;

  if (reminders.length >= MAX_REMINDERS_PER_CLAIM) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`cannot register more than ${MAX_REMINDERS_PER_CLAIM} reminder fees for the same invoice claim`] };
  }

  const latestReminder = reminders.at(-1);
  if (latestReminder) {
    const daysSinceLatest = diffDays(latestReminder.reminder_date, input.reminderDate);
    if (daysSinceLatest < MIN_DAYS_BETWEEN_REMINDERS) {
      return { ok: false, appliedRules: [RULE_ID], errors: [`reminderDate must be at least ${MIN_DAYS_BETWEEN_REMINDERS} days after the previous reminder on ${latestReminder.reminder_date}`] };
    }
  }

  const inserted = db.query(
    `INSERT INTO invoice_reminders (invoice_document_id, reminder_date, fee_amount, currency, note)
     VALUES (?, ?, ?, 'DKK', ?)
     RETURNING id`
  ).get(input.invoiceDocumentId, input.reminderDate, feeAmount, input.note ?? null) as { id: number };

  insertAuditLog(db, {
    eventType: "invoice_reminder_register",
    entityType: "invoice_reminder",
    entityId: inserted.id,
    message: `Registered reminder fee ${feeAmount} on invoice ${invoice.invoice_no}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  const totalReminderFees = roundDkk(reminders.reduce((sum, reminder) => sum + Number(reminder.fee_amount), 0) + feeAmount);
  return {
    ok: true,
    reminderId: inserted.id,
    reminderSequence: reminders.length + 1,
    invoiceDocumentId: input.invoiceDocumentId,
    invoiceNumber: invoice.invoice_no,
    reminderDate: input.reminderDate,
    feeAmount,
    totalReminderFees,
    appliedRules: [RULE_ID],
    errors: [],
  };
}

export function postInvoiceReminderToLedger(db: Database, input: PostInvoiceReminderToLedgerInput): PostInvoiceReminderToLedgerResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }

  const reminder = db.query(
    `SELECT r.id, r.invoice_document_id, r.reminder_date, r.fee_amount, d.invoice_no
     FROM invoice_reminders r
     JOIN documents d ON d.id = r.invoice_document_id
     WHERE r.invoice_document_id = ?
       AND (? IS NULL OR r.id = ?)
     ORDER BY r.reminder_date ASC, r.id ASC
     LIMIT 1`
  ).get(input.invoiceDocumentId, input.reminderId ?? null, input.reminderId ?? null) as {
    id: number;
    invoice_document_id: number;
    reminder_date: string;
    fee_amount: number;
    invoice_no: string;
  } | null;

  if (!reminder) {
    return { ok: false, appliedRules: [BOOKKEEPING_RULE_ID], errors: [input.reminderId ? `reminder ${input.reminderId} does not exist for invoice ${input.invoiceDocumentId}` : `invoice ${input.invoiceDocumentId} has no registered reminder fee`] };
  }

  const existing = db.query(
    `SELECT p.id, p.journal_entry_id, j.entry_no
     FROM invoice_reminder_postings p
     JOIN journal_entries j ON j.id = p.journal_entry_id
     WHERE p.reminder_id = ?`
  ).get(reminder.id) as { id: number; journal_entry_id: number; entry_no: string } | null;

  if (existing) {
    return {
      ok: false,
      reminderId: reminder.id,
      invoiceDocumentId: reminder.invoice_document_id,
      invoiceNumber: reminder.invoice_no,
      reminderDate: reminder.reminder_date,
      feeAmount: roundDkk(Number(reminder.fee_amount)),
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [`reminder ${reminder.id} is already posted in journal entry ${existing.entry_no}`],
    };
  }

  const amount = roundDkk(Number(reminder.fee_amount));
  try {
    return db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: input.transactionDate ?? reminder.reminder_date,
        text: `Reminder fee ${reminder.invoice_no}`,
        documentId: reminder.invoice_document_id,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: input.receivableAccountNo ?? "1100", debitAmount: amount, text: `Reminder receivable ${reminder.invoice_no}` },
          { accountNo: input.reminderIncomeAccountNo ?? "1010", creditAmount: amount, text: `Reminder income ${reminder.invoice_no}` },
        ],
      });
      if (!journal.ok) {
        return { ...journal, reminderId: reminder.id, invoiceDocumentId: reminder.invoice_document_id, invoiceNumber: reminder.invoice_no, reminderDate: reminder.reminder_date, feeAmount: amount, appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])] };
      }

      db.run(
        `INSERT INTO invoice_reminder_postings (reminder_id, journal_entry_id) VALUES (?, ?)`,
        reminder.id,
        journal.entryId,
      );

      insertAuditLog(db, {
        eventType: "invoice_reminder_post",
        entityType: "invoice_reminder",
        entityId: reminder.id,
        message: `Posted reminder fee ${amount} for invoice ${reminder.invoice_no} in journal entry ${journal.entryNo}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const statusAfter = getInvoiceStatus(db, reminder.invoice_document_id, input.transactionDate ?? reminder.reminder_date);
      return {
        ...journal,
        reminderId: reminder.id,
        invoiceDocumentId: reminder.invoice_document_id,
        invoiceNumber: reminder.invoice_no,
        reminderDate: reminder.reminder_date,
        feeAmount: amount,
        claimOpenBalance: statusAfter.ok ? statusAfter.claimOpenBalance : undefined,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), BOOKKEEPING_RULE_ID])],
      };
    })();
  } catch (error) {
    return {
      ok: false,
      reminderId: reminder.id,
      invoiceDocumentId: reminder.invoice_document_id,
      invoiceNumber: reminder.invoice_no,
      reminderDate: reminder.reminder_date,
      feeAmount: amount,
      appliedRules: [BOOKKEEPING_RULE_ID],
      errors: [String(error)],
    };
  }
}
