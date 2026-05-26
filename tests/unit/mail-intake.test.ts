// Tests: src/core/mail-intake.ts (deterministic bilagsmail intake — #122)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { parseEml, ingestMailDrop } from "../../src/core/mail-intake";
import { listExceptions } from "../../src/core/exceptions";

/**
 * Builds a minimal multipart/mixed MIME message with a single base64
 * attachment. Kept inline so fixtures stay deterministic and explicit.
 */
function buildEml(opts: {
  messageId: string;
  from?: string;
  subject?: string;
  date?: string;
  attachmentName?: string;
  attachmentContentType?: string;
  attachmentBytes?: Buffer;
  noAttachment?: boolean;
}): string {
  const boundary = "rmboundary";
  const headers = [
    opts.from !== undefined ? `From: ${opts.from}` : null,
    `Subject: ${opts.subject ?? "Bilag"}`,
    opts.date !== undefined ? `Date: ${opts.date}` : null,
    `Message-ID: ${opts.messageId}`,
    "MIME-Version: 1.0",
  ].filter(Boolean);

  if (opts.noAttachment) {
    return [
      ...headers,
      "Content-Type: text/plain; charset=utf-8",
      "",
      "Hej, her er ingen vedhæftning.",
      "",
    ].join("\r\n");
  }

  const bytes = opts.attachmentBytes ?? Buffer.from("%PDF-1.4\n%minimal pdf body\n");
  const b64 = bytes.toString("base64").replace(/(.{76})/g, "$1\r\n");
  return [
    ...headers,
    `Content-Type: multipart/mixed; boundary="${boundary}"`,
    "",
    `--${boundary}`,
    "Content-Type: text/plain; charset=utf-8",
    "",
    "Se vedhæftede bilag.",
    `--${boundary}`,
    `Content-Type: ${opts.attachmentContentType ?? "application/pdf"}; name="${opts.attachmentName ?? "faktura.pdf"}"`,
    "Content-Transfer-Encoding: base64",
    `Content-Disposition: attachment; filename="${opts.attachmentName ?? "faktura.pdf"}"`,
    "",
    b64,
    `--${boundary}--`,
    "",
  ].join("\r\n");
}

const baseMetadata = {
  issueDate: "2026-05-16",
  invoiceNo: "INV-MAIL-1",
  deliveryDescription: "Bogføring og momsafstemning",
  amountIncVat: 1250,
  currency: "DKK",
  sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
  recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
  vatAmount: 250,
};

describe("parseEml", () => {
  test("extracts message-id, sender, subject, date and PDF attachment", () => {
    const raw = buildEml({
      messageId: "<msg-001@example.com>",
      from: "Leverandør ApS <faktura@leverandor.dk>",
      subject: "Faktura 1001",
      date: "Mon, 16 May 2026 09:00:00 +0000",
    });
    const parsed = parseEml(Buffer.from(raw, "utf8"));
    expect(parsed.messageId).toBe("<msg-001@example.com>");
    expect(parsed.from).toBe("Leverandør ApS <faktura@leverandor.dk>");
    expect(parsed.subject).toBe("Faktura 1001");
    expect(parsed.date).toBe("Mon, 16 May 2026 09:00:00 +0000");
    expect(parsed.attachments).toHaveLength(1);
    expect(parsed.attachments[0]!.filename).toBe("faktura.pdf");
    expect(parsed.attachments[0]!.content.subarray(0, 5).toString()).toBe("%PDF-");
  });

  test("attachment hash is stable across reparses (deterministic)", () => {
    const raw = buildEml({ messageId: "<msg-002@example.com>" });
    const a = parseEml(Buffer.from(raw, "utf8"));
    const b = parseEml(Buffer.from(raw, "utf8"));
    expect(a.attachments[0]!.sha256).toBe(b.attachments[0]!.sha256);
    expect(a.attachments[0]!.sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  test("reports zero attachments for a plain-text message", () => {
    const raw = buildEml({ messageId: "<msg-003@example.com>", noAttachment: true });
    const parsed = parseEml(Buffer.from(raw, "utf8"));
    expect(parsed.attachments).toHaveLength(0);
  });
});

describe("ingestMailDrop", () => {
  test("ingests an EML attachment into the document pipeline", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-"));
    const emlPath = join(dropRoot, "message.eml");
    writeFileSync(emlPath, buildEml({ messageId: "<ingest-1@example.com>" }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    expect(result.ok).toBe(true);
    expect(result.messagesProcessed).toBe(1);
    expect(result.attachmentsIngested).toBe(1);
    expect(result.documents).toHaveLength(1);
    expect(result.documents[0]!.documentNo).toContain("DOC-");

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("rerunning the same maildrop creates no duplicate documents (dedup stable)", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-dup-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-dup-"));
    const emlPath = join(dropRoot, "message.eml");
    writeFileSync(emlPath, buildEml({ messageId: "<dup-1@example.com>" }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const first = ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    expect(first.ok).toBe(true);
    expect(first.attachmentsIngested).toBe(1);
    expect(first.attachmentsSkipped).toBe(0);

    const second = ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    expect(second.ok).toBe(true);
    expect(second.attachmentsIngested).toBe(0);
    expect(second.attachmentsSkipped).toBe(1);
    expect(second.documents).toHaveLength(0);

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("routes a message with no usable attachment into the exception queue", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-noatt-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-noatt-"));
    const emlPath = join(dropRoot, "no-attachment.eml");
    writeFileSync(emlPath, buildEml({ messageId: "<noatt-1@example.com>", noAttachment: true }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    expect(result.ok).toBe(true);
    expect(result.attachmentsIngested).toBe(0);
    expect(result.exceptionsCreated).toBe(1);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.count).toBe(1);
    expect(exceptions.rows[0]!.type).toBe("MAIL_INTAKE_NO_ATTACHMENT");

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("routes a message with no Message-ID into the exception queue as ambiguous", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-ambig-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-ambig-"));
    const emlPath = join(dropRoot, "ambiguous.eml");
    // Build a message, then strip its Message-ID header.
    const raw = buildEml({ messageId: "<to-strip@example.com>" })
      .replace(/^Message-ID: .*\r\n/m, "");
    writeFileSync(emlPath, raw);

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    expect(result.ok).toBe(true);
    expect(result.attachmentsIngested).toBe(0);
    expect(result.exceptionsCreated).toBe(1);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.rows[0]!.type).toBe("MAIL_INTAKE_AMBIGUOUS_METADATA");

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("re-routing an ambiguous/no-attachment message does not duplicate the exception", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-exdup-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-exdup-"));
    const emlPath = join(dropRoot, "no-attachment.eml");
    writeFileSync(emlPath, buildEml({ messageId: "<exdup-1@example.com>", noAttachment: true }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.count).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("processes a maildrop directory deterministically and dedups across messages", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-dir-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-dir-"));
    writeFileSync(join(dropRoot, "b-second.eml"), buildEml({
      messageId: "<dir-2@example.com>",
      attachmentBytes: Buffer.from("%PDF-1.4\nsecond invoice\n"),
    }));
    writeFileSync(join(dropRoot, "a-first.eml"), buildEml({
      messageId: "<dir-1@example.com>",
      attachmentBytes: Buffer.from("%PDF-1.4\nfirst invoice\n"),
    }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const first = ingestMailDrop(db, companyRoot, dropRoot, {
      metadataPerMessage: {
        "<dir-1@example.com>": { ...baseMetadata, invoiceNo: "INV-DIR-1" },
        "<dir-2@example.com>": { ...baseMetadata, invoiceNo: "INV-DIR-2" },
      },
    });
    expect(first.ok).toBe(true);
    expect(first.messagesProcessed).toBe(2);
    expect(first.attachmentsIngested).toBe(2);

    const rerun = ingestMailDrop(db, companyRoot, dropRoot, {
      metadataPerMessage: {
        "<dir-1@example.com>": { ...baseMetadata, invoiceNo: "INV-DIR-1" },
        "<dir-2@example.com>": { ...baseMetadata, invoiceNo: "INV-DIR-2" },
      },
    });
    expect(rerun.attachmentsIngested).toBe(0);
    expect(rerun.attachmentsSkipped).toBe(2);

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(2);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("ingests the committed examples/bilagsmail-faktura.eml fixture", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-fixture-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const metadata = JSON.parse(readFileSync("examples/bilagsmail.metadata.json", "utf8"));
    const result = ingestMailDrop(db, companyRoot, "examples/bilagsmail-faktura.eml", { metadata });
    expect(result.ok).toBe(true);
    expect(result.attachmentsIngested).toBe(1);
    expect(result.documents[0]!.messageId).toBe("<inv-1001@leverandor.dk>");

    // Reruns of the committed fixture are idempotent.
    const rerun = ingestMailDrop(db, companyRoot, "examples/bilagsmail-faktura.eml", { metadata });
    expect(rerun.attachmentsIngested).toBe(0);
    expect(rerun.attachmentsSkipped).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("records dedup rows in mail_intake_messages keyed on message-id + attachment hash", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-mailintake-rows-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-rows-"));
    const emlPath = join(dropRoot, "message.eml");
    writeFileSync(emlPath, buildEml({ messageId: "<rows-1@example.com>" }));

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    ingestMailDrop(db, companyRoot, emlPath, { metadata: baseMetadata });
    const row = db.query(
      "SELECT message_id, attachment_sha256, document_id FROM mail_intake_messages LIMIT 1",
    ).get() as { message_id: string; attachment_sha256: string; document_id: number | null };
    expect(row.message_id).toBe("<rows-1@example.com>");
    expect(row.attachment_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(row.document_id).toBeGreaterThan(0);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });

  test("parseEml extracts Reply-To and X-Forwarded-For for forwarded mails (#352)", () => {
    const raw = [
      "From: forwarder@example.com",
      "Reply-To: original-sender@example.com",
      "X-Forwarded-For: original-sender@example.com",
      "Subject: Fwd: Faktura",
      "Message-ID: <fwd-1@example.com>",
      "Date: Mon, 01 Jan 2026 10:00:00 +0100",
      "Content-Type: text/plain; charset=utf-8",
      "",
      "Forwarded message …",
      "",
    ].join("\r\n");
    const parsed = parseEml(raw);
    expect(parsed.replyTo).toBe("original-sender@example.com");
    expect(parsed.forwardedFrom).toBe("original-sender@example.com");
    expect(parsed.from).toBe("forwarder@example.com");
    expect(parsed.rawByteSize).toBeGreaterThan(0);
  });

  test("ingestMailDrop refuses oversize EMLs with a MAIL_INTAKE_TOO_LARGE exception (#352)", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-toobig-"));
    const dropRoot = mkdtempSync(join(tmpdir(), "rentemester-maildrop-toobig-eml-"));
    const emlPath = join(dropRoot, "huge.eml");
    // Build a single-attachment EML der er > maxEmlSizeBytes (her sat lavt
    // til 4 KB så testen er hurtig).
    const bigBytes = Buffer.from("X".repeat(8 * 1024));
    writeFileSync(
      emlPath,
      buildEml({
        messageId: "<too-big@example.com>",
        attachmentBytes: bigBytes,
      }),
    );

    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const result = ingestMailDrop(db, companyRoot, emlPath, {
      metadata: baseMetadata,
      maxEmlSizeBytes: 4 * 1024,
    });
    expect(result.attachmentsIngested).toBe(0);
    expect(result.exceptionsCreated).toBe(1);

    const ex = listExceptions(db, { status: "open" });
    expect(ex.ok).toBe(true);
    expect(ex.rows[0]!.type).toBe("MAIL_INTAKE_TOO_LARGE");
    expect((ex.rows[0]!.sourceEvidence as any).rawByteSize).toBeGreaterThan(
      4 * 1024,
    );

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(dropRoot, { recursive: true, force: true });
  });
});
