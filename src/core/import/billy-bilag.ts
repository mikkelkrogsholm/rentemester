/**
 * Billy bilag (receipt) ingest and journal-entry linking.
 *
 * Ingests downloaded attachment files from a Billy API export and links each
 * receipt to its voucher's journal entry via the `import_document_links` table.
 * Follows the same pattern as `dinero-bilag.ts` (#196).
 *
 * The mapping chain: attachment → ownerId (billId) → bill-transaction-map.json
 * → transactionId → historicalEntriesPosted.voucherRef → journal entry.
 *
 * Deterministic and idempotent: files are visited in sorted order, content is
 * hashed before ingest, and links are guarded against duplicates.
 */

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { ingestDocument } from "../documents";
import type { ImportResult } from "./types";

const SYSTEM = "billy";
export const BILLY_BILAG_SOURCE = "billy-import-bilag";

export type BillyLinkedBilag = {
  fileName: string;
  billId: string;
  transactionId: string;
  documentId: number;
  documentNo: string;
  sha256: string;
  journalEntryId: number;
  journalEntryNo: string;
};

export type BillyBilagIngestResult = {
  ok: boolean;
  linked: BillyLinkedBilag[];
  unmatched: Array<{ fileName: string; billId: string }>;
  duplicates: string[];
  auditTrail: string[];
  errors: string[];
};

type BillTransactionMapping = {
  billId: string;
  transactionId: string;
  voucherNo: string;
};

type AttachmentMeta = {
  id: string;
  ownerId: string;
  ownerReference: string;
  fileId: string;
};

function sha256Of(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

/**
 * Ingests Billy bilag files and links them to their journal entries.
 *
 * Expects the export directory to contain:
 * - `bilag/` — downloaded attachment files named `{billId}-{attachmentId}.bin`
 * - `attachments.json` — attachment metadata from the Billy API
 * - `bill-transaction-map.json` — billId → transactionId mapping
 *
 * `result` is the outcome of the ledger import: its `historicalEntriesPosted`
 * carries transactionId → journal entry mapping.
 */
export function ingestBillyBilag(
  db: Database,
  companyRoot: string,
  exportDir: string,
  result: ImportResult,
): BillyBilagIngestResult {
  const auditTrail: string[] = [];
  const errors: string[] = [];
  const linked: BillyLinkedBilag[] = [];
  const unmatched: Array<{ fileName: string; billId: string }> = [];
  const duplicates: string[] = [];

  // Load bill → transactionId mapping
  const mapPath = join(exportDir, "bill-transaction-map.json");
  if (!existsSync(mapPath)) {
    auditTrail.push("No bill-transaction-map.json — skipping bilag ingest");
    return { ok: true, linked, unmatched, duplicates, auditTrail, errors };
  }
  let billTxnMap: BillTransactionMapping[];
  try {
    billTxnMap = JSON.parse(readFileSync(mapPath, "utf8"));
  } catch {
    errors.push("bill-transaction-map.json: invalid JSON");
    return { ok: false, linked, unmatched, duplicates, auditTrail, errors };
  }

  const txnByBillId = new Map<string, string>();
  for (const m of billTxnMap) {
    txnByBillId.set(m.billId, m.transactionId);
  }

  // Build transactionId → journal entry mapping from import result
  const entryByTxnId = new Map<string, { entryId: number; entryNo: string }>();
  for (const posted of result.historicalEntriesPosted ?? []) {
    entryByTxnId.set(posted.voucherRef, {
      entryId: posted.entryId,
      entryNo: posted.entryNo,
    });
  }

  // List bilag files
  const bilagDir = join(exportDir, "bilag");
  if (!existsSync(bilagDir)) {
    auditTrail.push("No bilag/ directory — no receipts to ingest");
    return { ok: true, linked, unmatched, duplicates, auditTrail, errors };
  }

  const files = readdirSync(bilagDir).sort();
  if (files.length === 0) {
    auditTrail.push("bilag/ directory is empty — no receipts to ingest");
    return { ok: true, linked, unmatched, duplicates, auditTrail, errors };
  }

  const docBySha = db.query("SELECT id, document_no FROM documents WHERE sha256_hash = ?");
  const linkExists = db.query(
    "SELECT id FROM import_document_links WHERE document_id = ? AND journal_entry_id = ?",
  );
  const insertLink = db.query(
    `INSERT INTO import_document_links (source_system, voucher_ref, document_id, journal_entry_id)
     VALUES (?, ?, ?, ?)`,
  );

  for (const fileName of files) {
    // File name format: {billId}__{attachmentId}.{ext}
    // Double underscore separates billId from attachmentId.
    const sepIdx = fileName.indexOf("__");
    if (sepIdx < 1) continue;
    const billId = fileName.slice(0, sepIdx);

    const filePath = join(bilagDir, fileName);
    const bytes = readFileSync(filePath);
    if (bytes.length === 0) continue;

    const sha256 = sha256Of(bytes);

    // Check if already ingested (dedup on SHA-256, same as dinero-bilag.ts)
    const existing = docBySha.get(sha256) as { id: number; document_no: string } | null;
    let documentId: number;
    let documentNo: string;

    if (existing) {
      documentId = existing.id;
      documentNo = existing.document_no;
      duplicates.push(fileName);
    } else {
      const ingest = ingestDocument(db, companyRoot, filePath, {
        source: BILLY_BILAG_SOURCE,
        documentType: "cash_register_receipt",
      });
      if (!ingest.ok || ingest.documentId == null) {
        // ingestDocument may fail for MIME-type detection, etc. — skip, not fatal.
        continue;
      }
      documentId = ingest.documentId as unknown as number;
      documentNo = ingest.documentNo!;
    }

    // Map: billId → transactionId → journal entry
    const transactionId = txnByBillId.get(billId);
    if (!transactionId) {
      unmatched.push({ fileName, billId });
      continue;
    }

    const entry = entryByTxnId.get(transactionId);
    if (!entry) {
      unmatched.push({ fileName, billId });
      continue;
    }

    // Create link (idempotent)
    const alreadyLinked = linkExists.get(documentId, entry.entryId) as { id: number } | null;
    if (!alreadyLinked) {
      insertLink.run(SYSTEM, transactionId, documentId, entry.entryId);
    }

    linked.push({
      fileName,
      billId,
      transactionId,
      documentId,
      documentNo,
      sha256,
      journalEntryId: entry.entryId,
      journalEntryNo: entry.entryNo,
    });
  }

  auditTrail.push(
    `Billy bilag ingest: ${linked.length} receipt(s) linked, ` +
      `${unmatched.length} without a journal entry match, ` +
      `${duplicates.length} already ingested`,
  );

  return {
    ok: errors.length === 0,
    linked,
    unmatched,
    duplicates,
    auditTrail,
    errors,
  };
}
