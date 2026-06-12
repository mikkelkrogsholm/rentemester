// Tests: src/core/email.ts (#180 email delivery)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";
import { registerInvoiceReminder } from "../../src/core/invoice-reminders";
import { cleanupDir } from "../helpers/cleanup";
import {
  buildInvoiceEmailMessage,
  sendInvoiceEmail,
  type EmailTransport,
  type SmtpConfig,
} from "../../src/core/email";

const SMTP_CONFIG: SmtpConfig = {
  host: "smtp.example.test",
  port: 587,
  fromAddress: "faktura@rentemester.test",
  fromName: "Rentemester ApS",
};

/**
 * Deterministic in-memory fake transport — records every send, never touches
 * the network. Tests inject this so the SMTP boundary is fully observable.
 */
function fakeTransport() {
  const sent: Array<{ to: string; subject: string; messageId: string; rawMessage: string }> = [];
  const transport: EmailTransport = {
    send(message) {
      sent.push({
        to: message.to,
        subject: message.subject,
        messageId: message.messageId,
        rawMessage: message.rawMessage,
      });
      return { ok: true, messageId: message.messageId };
    },
  };
  return { transport, sent };
}

function seedIssuedInvoice(root: string, db: ReturnType<typeof openDb>) {
  const issued = issueInvoice(db, root, {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    dueDate: "2026-06-15",
    invoiceNumber: "2026-0001",
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
    lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
  });
  expect(issued.ok).toBe(true);
  return issued.documentId!;
}

describe("email message build", () => {
  test("builds a deterministic MIME message with the PDF attached", () => {
    const pdf = Buffer.from("%PDF-1.4 fake invoice bytes");
    const first = buildInvoiceEmailMessage({
      smtp: SMTP_CONFIG,
      to: "kunde@example.test",
      kind: "invoice",
      invoiceNumber: "2026-0001",
      pdfBytes: pdf,
      pdfFilename: "2026-0001.pdf",
    });
    const second = buildInvoiceEmailMessage({
      smtp: SMTP_CONFIG,
      to: "kunde@example.test",
      kind: "invoice",
      invoiceNumber: "2026-0001",
      pdfBytes: pdf,
      pdfFilename: "2026-0001.pdf",
    });

    // Same inputs => byte-identical message and message-id (no timestamps/random).
    expect(first.rawMessage).toBe(second.rawMessage);
    expect(first.messageId).toBe(second.messageId);
    expect(first.to).toBe("kunde@example.test");
    expect(first.subject).toContain("2026-0001");
    expect(first.rawMessage).toContain("From: Rentemester ApS <faktura@rentemester.test>");
    expect(first.rawMessage).toContain("To: kunde@example.test");
    expect(first.rawMessage).toContain("Content-Type: application/pdf");
    expect(first.rawMessage).toContain('filename="2026-0001.pdf"');
    // The PDF bytes are present base64-encoded.
    expect(first.rawMessage).toContain(pdf.toString("base64"));
  });

  test("reminder messages and invoice messages get distinct subjects/ids", () => {
    const pdf = Buffer.from("%PDF-1.4 fake");
    const invoice = buildInvoiceEmailMessage({
      smtp: SMTP_CONFIG,
      to: "kunde@example.test",
      kind: "invoice",
      invoiceNumber: "2026-0001",
      pdfBytes: pdf,
      pdfFilename: "2026-0001.pdf",
    });
    const reminder = buildInvoiceEmailMessage({
      smtp: SMTP_CONFIG,
      to: "kunde@example.test",
      kind: "reminder",
      invoiceNumber: "2026-0001",
      pdfBytes: pdf,
      pdfFilename: "2026-0001.pdf",
    });
    expect(reminder.messageId).not.toBe(invoice.messageId);
    expect(reminder.subject).not.toBe(invoice.subject);
  });
});

describe("sendInvoiceEmail", () => {
  test("sends an issued invoice via the injected transport and records the send log", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-send-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    const { transport, sent } = fakeTransport();

    const result = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: "kunde@example.test",
      smtp: SMTP_CONFIG,
      transport,
    });

    expect(result.ok).toBe(true);
    expect(result.duplicate).toBe(false);
    expect(result.recipient).toBe("kunde@example.test");
    expect(result.messageId).toBeDefined();
    expect(sent).toHaveLength(1);
    expect(sent[0]!.to).toBe("kunde@example.test");

    const rows = db.query("SELECT recipient, kind, message_id FROM email_send_log").all() as Array<{
      recipient: string;
      kind: string;
      message_id: string;
    }>;
    expect(rows).toHaveLength(1);
    expect(rows[0]!.recipient).toBe("kunde@example.test");
    expect(rows[0]!.kind).toBe("invoice");
    expect(rows[0]!.message_id).toBe(result.messageId!);

    db.close();
    cleanupDir(root);
  });

  test("is idempotent — a second identical send reuses the log and does not re-transmit", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-idempotent-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    const { transport, sent } = fakeTransport();

    const first = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: "kunde@example.test",
      smtp: SMTP_CONFIG,
      transport,
    });
    const second = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: "kunde@example.test",
      smtp: SMTP_CONFIG,
      transport,
    });

    expect(first.ok).toBe(true);
    expect(first.duplicate).toBe(false);
    expect(second.ok).toBe(true);
    expect(second.duplicate).toBe(true);
    expect(second.messageId).toBe(first.messageId);
    // The transport was only invoked once — no silent re-send.
    expect(sent).toHaveLength(1);
    expect(db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 1 });

    db.close();
    cleanupDir(root);
  });

  test("fails clearly when the recipient email is missing", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-no-recipient-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    const { transport, sent } = fakeTransport();

    const result = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: undefined,
      smtp: SMTP_CONFIG,
      transport,
    });

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("recipient email");
    expect(sent).toHaveLength(0);
    expect(db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("fails clearly when the SMTP config is missing or incomplete", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-no-config-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    const { transport, sent } = fakeTransport();

    const result = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: "kunde@example.test",
      smtp: { host: "", port: 587, fromAddress: "", fromName: "" },
      transport,
    });

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("SMTP");
    expect(sent).toHaveLength(0);
    expect(db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("records a transport failure without writing a success log row", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-transport-fail-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    const failing: EmailTransport = {
      send() {
        return { ok: false, error: "simulated SMTP connection refused" };
      },
    };

    const result = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "invoice",
      to: "kunde@example.test",
      smtp: SMTP_CONFIG,
      transport: failing,
    });

    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toContain("simulated SMTP connection refused");
    expect(db.query("SELECT COUNT(*) AS n FROM email_send_log").get()).toEqual({ n: 0 });

    db.close();
    cleanupDir(root);
  });

  test("sends a reminder for an overdue invoice and records kind=reminder", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-reminder-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);
    const documentId = seedIssuedInvoice(root, db);
    expect(
      registerInvoiceReminder(db, { invoiceDocumentId: documentId, reminderDate: "2026-06-26" }).ok,
    ).toBe(true);
    const { transport, sent } = fakeTransport();

    const result = sendInvoiceEmail(db, root, {
      invoiceDocumentId: documentId,
      kind: "reminder",
      to: "kunde@example.test",
      smtp: SMTP_CONFIG,
      transport,
    });

    expect(result.ok).toBe(true);
    expect(sent).toHaveLength(1);
    const row = db.query("SELECT kind FROM email_send_log").get() as { kind: string };
    expect(row.kind).toBe("reminder");

    db.close();
    cleanupDir(root);
  });
});
