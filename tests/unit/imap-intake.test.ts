// Tests: src/core/imap-intake.ts (deterministic IMAP bilagsmail transport — #181)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { listExceptions } from "../../src/core/exceptions";
import { cleanupDir } from "../helpers/cleanup";
import {
  pollImapMailbox,
  type ImapClient,
  type ImapMessage,
} from "../../src/core/imap-intake";

/**
 * Builds a minimal multipart/mixed RFC-822 message with a single base64
 * attachment — the raw bytes an IMAP server would return for a fetched
 * message. Kept inline so fixtures stay deterministic and explicit.
 */
function buildRawMessage(opts: {
  messageId: string;
  from?: string;
  subject?: string;
  date?: string;
  attachmentBytes?: Buffer;
  noAttachment?: boolean;
}): string {
  const headers = [
    `From: ${opts.from ?? "Leverandør ApS <faktura@leverandor.dk>"}`,
    `Subject: ${opts.subject ?? "Faktura"}`,
    `Date: ${opts.date ?? "Mon, 18 May 2026 09:00:00 +0000"}`,
    `Message-ID: ${opts.messageId}`,
    "MIME-Version: 1.0",
  ];
  if (opts.noAttachment) {
    return [...headers, "Content-Type: text/plain; charset=utf-8", "", "Ingen vedhæftning.", ""].join("\r\n");
  }
  const bytes = opts.attachmentBytes ?? Buffer.from("%PDF-1.4\n%minimal pdf body\n");
  const b64 = bytes.toString("base64").replace(/(.{76})/g, "$1\r\n");
  return [
    ...headers,
    'Content-Type: multipart/mixed; boundary="rmb"',
    "",
    "--rmb",
    "Content-Type: text/plain; charset=utf-8",
    "",
    "Se vedhæftede bilag.",
    "--rmb",
    'Content-Type: application/pdf; name="faktura.pdf"',
    "Content-Transfer-Encoding: base64",
    'Content-Disposition: attachment; filename="faktura.pdf"',
    "",
    b64,
    "--rmb--",
    "",
  ].join("\r\n");
}

const baseMetadata = {
  issueDate: "2026-05-18",
  invoiceNo: "INV-IMAP-1",
  deliveryDescription: "Bogføring og momsafstemning",
  amountIncVat: 1250,
  currency: "DKK",
  sender: { name: "Leverandør ApS", address: "Sælgervej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
  recipient: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
  vatAmount: 250,
};

/**
 * An in-memory fake IMAP client. The poller never touches a real server in
 * tests: every connect/fetch is served from this deterministic store. The
 * `connects` / `closes` counters let tests assert lifecycle discipline.
 */
function fakeImapClient(messages: ImapMessage[]): ImapClient & { connects: number; closes: number } {
  const store = [...messages];
  return {
    connects: 0,
    closes: 0,
    async connect() {
      this.connects += 1;
    },
    async fetchSince() {
      // Deterministic: messages are always returned in the same order.
      return store.map((m) => ({ uid: m.uid, raw: m.raw }));
    },
    async close() {
      this.closes += 1;
    },
  };
}

describe("pollImapMailbox", () => {
  test("fetches messages via the injected client and ingests attachments", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([
      { uid: 1, raw: buildRawMessage({ messageId: "<imap-1@example.com>" }) },
    ]);
    const result = await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });

    expect(result.ok).toBe(true);
    expect(result.messagesFetched).toBe(1);
    expect(result.attachmentsIngested).toBe(1);
    expect(result.documents).toHaveLength(1);
    expect(client.connects).toBe(1);
    expect(client.closes).toBe(1);

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(1);

    db.close();
    cleanupDir(companyRoot);
  });

  test("a second poll creates no duplicate documents (rerun-stable across polls)", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-dup-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([
      { uid: 7, raw: buildRawMessage({ messageId: "<imap-dup@example.com>" }) },
    ]);

    const first = await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });
    expect(first.attachmentsIngested).toBe(1);
    expect(first.attachmentsSkipped).toBe(0);

    const second = await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });
    expect(second.attachmentsIngested).toBe(0);
    expect(second.attachmentsSkipped).toBe(1);
    expect(second.documents).toHaveLength(0);

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(1);

    db.close();
    cleanupDir(companyRoot);
  });

  test("routes a message with no attachment into the exception queue", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-noatt-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([
      { uid: 3, raw: buildRawMessage({ messageId: "<imap-noatt@example.com>", noAttachment: true }) },
    ]);
    const result = await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });

    expect(result.ok).toBe(true);
    expect(result.attachmentsIngested).toBe(0);
    expect(result.exceptionsCreated).toBe(1);

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.count).toBe(1);
    expect(exceptions.rows[0]!.type).toBe("MAIL_INTAKE_NO_ATTACHMENT");

    db.close();
    cleanupDir(companyRoot);
  });

  test("re-polling a no-attachment message does not duplicate the exception", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-exdup-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([
      { uid: 4, raw: buildRawMessage({ messageId: "<imap-exdup@example.com>", noAttachment: true }) },
    ]);
    await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });
    await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });

    const exceptions = listExceptions(db, { status: "open" });
    expect(exceptions.count).toBe(1);

    db.close();
    cleanupDir(companyRoot);
  });

  test("processes multiple messages deterministically", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-multi-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([
      { uid: 11, raw: buildRawMessage({ messageId: "<imap-m1@example.com>", attachmentBytes: Buffer.from("%PDF-1.4\none\n") }) },
      { uid: 12, raw: buildRawMessage({ messageId: "<imap-m2@example.com>", attachmentBytes: Buffer.from("%PDF-1.4\ntwo\n") }) },
    ]);
    const result = await pollImapMailbox(db, companyRoot, client, {
      metadataPerMessage: {
        "<imap-m1@example.com>": { ...baseMetadata, invoiceNo: "INV-IMAP-M1" },
        "<imap-m2@example.com>": { ...baseMetadata, invoiceNo: "INV-IMAP-M2" },
      },
    });
    expect(result.messagesFetched).toBe(2);
    expect(result.attachmentsIngested).toBe(2);

    const docCount = db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number };
    expect(docCount.n).toBe(2);

    db.close();
    cleanupDir(companyRoot);
  });

  test("closes the client even when an empty mailbox is polled", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-imap-empty-"));
    const db = openDb(ensureCompanyDirs(companyRoot).db);
    migrate(db);

    const client = fakeImapClient([]);
    const result = await pollImapMailbox(db, companyRoot, client, { metadata: baseMetadata });

    expect(result.ok).toBe(true);
    expect(result.messagesFetched).toBe(0);
    expect(result.attachmentsIngested).toBe(0);
    expect(client.connects).toBe(1);
    expect(client.closes).toBe(1);

    db.close();
    cleanupDir(companyRoot);
  });
});
