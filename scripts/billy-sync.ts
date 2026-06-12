#!/usr/bin/env bun
/**
 * Billy → Rentemester incremental sync.
 *
 * Fetches new postings from the Billy API since the last sync and posts them
 * as journal entries in Rentemester. Designed to run repeatedly (cron, manual,
 * or via MCP) — each run only processes NEW postings.
 *
 * State is tracked in `{company}/sync/billy-sync-state.json`:
 *   { lastSyncDate, lastPostingIds[], syncCount }
 *
 * Usage:
 *   BILLY_API_KEY=... bun run scripts/billy-sync.ts \
 *     --company /path/to/company \
 *     [--actor user:mads] \
 *     [--dry-run]
 */

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { migrate } from "../src/core/db";
import { postJournalEntry } from "../src/core/ledger";
import { toOre } from "../src/core/money";
import {
  findUnlinkedSyncEntries,
  ingestSyncBilagFile,
  linkBilagDocument,
  matchEntriesToBillyTxns,
  type BillySyncTxn,
} from "../src/core/import/billy-sync-bilag";

const API_BASE = "https://api.billysbilling.com/v2";
const SYNC_PROGRAM = "billy-sync";

type PendingBilag = {
  txnId: string;
  entryId: number;
  entryNo: string;
  ownerReference: string;
};

type SyncState = {
  lastSyncDate: string;
  lastPostingIds: string[];
  syncCount: number;
  lastRunAt: string;
  // Transactions posted without a bilag — retried on every later run until
  // the attachment shows up in Billy.
  pendingBilag?: PendingBilag[];
};

type BillyTransaction = {
  id: string;
  originatorReference: string | null;
  isVoided: boolean;
};

type BillyAttachment = {
  id: string;
  ownerId: string;
  fileId: string | null;
};

type BillyPosting = {
  id: string;
  transactionId: string;
  entryDate: string;
  text: string;
  accountId: string;
  amount: number;
  side: string;
  isVoided: boolean;
  priority: number;
};

function parseArgs() {
  const args = process.argv.slice(2);
  let company = "";
  let actor = "";
  let dryRun = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--company" && args[i + 1]) company = args[++i]!;
    else if (args[i] === "--actor" && args[i + 1]) actor = args[++i]!;
    else if (args[i] === "--dry-run") dryRun = true;
  }
  if (!company) {
    console.error("Usage: billy-sync.ts --company <path> [--actor user:...] [--dry-run]");
    process.exit(2);
  }
  return { company, actor, dryRun };
}

/**
 * Fetches every row from a paginated Billy list endpoint.
 *
 * Billy's pagination is UNSTABLE without an explicit sortProperty: rows can
 * repeat on one page and silently vanish from all pages (observed live —
 * an attachment present in the unpaginated response was missing from every
 * pageSize=100 page). Always pass a sortProperty the endpoint supports
 * (postings/transactions: entryDate, attachments/invoices: createdTime),
 * dedup on id, and verify the collected count against meta.paging.total.
 */
async function billyGetAll(
  path: string,
  token: string,
  key: string,
  sortProperty?: string,
): Promise<unknown[]> {
  let page = 1;
  const pageSize = 1000;
  const all: unknown[] = [];
  const seen = new Set<string>();
  let expectedTotal: number | null = null;
  const sort = sortProperty ? `&sortProperty=${sortProperty}&sortDirection=ASC` : "";
  while (true) {
    const url = `${API_BASE}${path}${path.includes("?") ? "&" : "?"}pageSize=${pageSize}&page=${page}${sort}`;
    const res = await fetch(url, {
      headers: { "X-Access-Token": token, Accept: "application/json" },
    });
    if (!res.ok) throw new Error(`Billy API ${res.status}: ${await res.text()}`);
    const data = (await res.json()) as Record<string, unknown>;
    const paging = (data.meta as { paging?: { total?: number } } | undefined)?.paging;
    if (expectedTotal === null && typeof paging?.total === "number") expectedTotal = paging.total;
    const items = data[key];
    if (!Array.isArray(items) || items.length === 0) break;
    for (const item of items) {
      const id = (item as { id?: string }).id;
      if (id !== undefined) {
        if (seen.has(id)) continue; // unstable pagination can repeat rows
        seen.add(id);
      }
      all.push(item);
    }
    if (items.length < pageSize) break;
    page++;
  }
  if (expectedTotal !== null && all.length !== expectedTotal) {
    console.warn(
      `  WARNING: ${key} pagination returned ${all.length} of ${expectedTotal} rows — rerun or reduce churn; Billy pagination dropped rows`,
    );
  }
  return all;
}

function loadSyncState(company: string): SyncState {
  const statePath = join(company, "sync", "billy-sync-state.json");
  if (existsSync(statePath)) {
    return JSON.parse(readFileSync(statePath, "utf8"));
  }
  return { lastSyncDate: "", lastPostingIds: [], syncCount: 0, lastRunAt: "" };
}

function saveSyncState(company: string, state: SyncState): void {
  const syncDir = join(company, "sync");
  mkdirSync(syncDir, { recursive: true });
  writeFileSync(
    join(syncDir, "billy-sync-state.json"),
    JSON.stringify(state, null, 2),
    "utf8",
  );
}

// Rate-limit: pause between file downloads to avoid 429 responses
// (same cadence as billy-export.ts).
let downloadCount = 0;
async function throttleDownloads(): Promise<void> {
  downloadCount++;
  if (downloadCount % 10 === 0) await new Promise((r) => setTimeout(r, 200));
}

/**
 * Downloads one Billy attachment to `bilagDir` and returns the file path,
 * or null when the file is unavailable. Billy's /v2/files returns JSON with
 * a downloadUrl — not the file itself.
 */
async function downloadAttachment(
  att: BillyAttachment,
  apiKey: string,
  bilagDir: string,
): Promise<string | null> {
  if (!att.fileId) { console.error(`  [bilag] ${att.id}: no fileId`); return null; }
  await throttleDownloads();
  try {
    const metaRes = await fetch(`${API_BASE}/files/${att.fileId}`, {
      headers: { "X-Access-Token": apiKey, Accept: "application/json" },
    });
    if (!metaRes.ok) { console.error(`  [bilag] ${att.id}: /files ${metaRes.status}`); return null; }
    const meta = (await metaRes.json()) as {
      file?: { downloadUrl?: string; fileType?: string };
    };
    const downloadUrl = meta.file?.downloadUrl;
    if (!downloadUrl) return null;
    const fileType = (meta.file?.fileType ?? "pdf").toLowerCase();
    const ext = fileType === "jpg" || fileType === "jpeg" ? ".jpg"
      : fileType === "png" ? ".png" : ".pdf";
    const filePath = join(bilagDir, `${att.ownerId}__${att.id}${ext}`);
    const fileRes = await fetch(downloadUrl);
    if (!fileRes.ok) return null;
    const bytes = Buffer.from(await fileRes.arrayBuffer());
    if (bytes.length === 0) return null;
    writeFileSync(filePath, bytes);
    return filePath;
  } catch {
    return null;
  }
}

/**
 * Downloads a Billy-generated sales invoice PDF (the evidence for an income
 * entry — invoices carry no attachment). The downloadUrl is a pre-signed URL
 * from the invoice object.
 */
async function downloadInvoicePdf(
  invoiceId: string,
  downloadUrl: string,
  bilagDir: string,
): Promise<string | null> {
  await throttleDownloads();
  try {
    const res = await fetch(downloadUrl);
    if (!res.ok) return null;
    const bytes = Buffer.from(await res.arrayBuffer());
    if (bytes.length === 0) return null;
    const filePath = join(bilagDir, `${invoiceId}__invoice.pdf`);
    writeFileSync(filePath, bytes);
    return filePath;
  } catch {
    return null;
  }
}

/** Extracts the owner id from an originatorReference like `bill:<id>`. */
function ownerIdOf(originatorReference: string | null): string | null {
  if (!originatorReference) return null;
  const idx = originatorReference.indexOf(":");
  return idx > 0 ? originatorReference.slice(idx + 1) : null;
}

/**
 * Downloads and ingests all attachments for an owner. Returns the ingested
 * document ids (first one is used as the entry's document_id).
 */
async function ingestOwnerBilag(
  db: Database,
  company: string,
  apiKey: string,
  bilagDir: string,
  attachments: BillyAttachment[],
): Promise<number[]> {
  const documentIds: number[] = [];
  for (const att of attachments) {
    const filePath = await downloadAttachment(att, apiKey, bilagDir);
    if (!filePath) continue;
    const ingest = ingestSyncBilagFile(db, company, filePath);
    if (ingest.ok && ingest.documentId != null) documentIds.push(ingest.documentId);
  }
  return documentIds;
}

function loadAccountMap(db: Database): Map<string, string> {
  const rows = db
    .query("SELECT account_no, name FROM accounts ORDER BY account_no")
    .all() as Array<{ account_no: string; name: string }>;
  const map = new Map<string, string>();
  for (const row of rows) map.set(row.account_no, row.name);
  return map;
}

async function main() {
  const apiKey = process.env.BILLY_API_KEY;
  if (!apiKey) {
    console.error("BILLY_API_KEY environment variable is required");
    process.exit(2);
  }

  const { company, actor, dryRun } = parseArgs();
  const dbPath = join(company, "data", "ledger.sqlite");
  if (!existsSync(dbPath)) {
    console.error(`Company not initialized: ${dbPath} does not exist`);
    process.exit(1);
  }

  const state = loadSyncState(company);

  // If no sync state exists, initialize from existing journal entries.
  // This handles the case where `import run --system billy` was already used.
  if (!state.lastSyncDate) {
    const db = new Database(dbPath);
    migrate(db);
    const latest = db.query(
      `SELECT MAX(transaction_date) as latest FROM journal_entries
       WHERE created_by_program IN ('rentemester-import-cli', 'rentemester-import-postings', 'billy-sync')`,
    ).get() as { latest: string | null } | null;
    db.close();
    if (latest?.latest) {
      state.lastSyncDate = latest.latest;
      console.log(`Initialized sync from existing import (latest entry: ${state.lastSyncDate})`);
    }
  }

  console.log(
    state.lastSyncDate
      ? `Last sync: ${state.lastRunAt || "initial"} (${state.syncCount} runs, last date: ${state.lastSyncDate})`
      : "First sync — will fetch all postings",
  );

  // Get Billy org ID
  const orgRes = await fetch(`${API_BASE}/organization`, {
    headers: { "X-Access-Token": apiKey, Accept: "application/json" },
  });
  if (!orgRes.ok) {
    console.error(`Billy API error ${orgRes.status}: ${await orgRes.text()}`);
    process.exit(1);
  }
  const orgData = (await orgRes.json()) as { organization: { id: string; name: string } };
  const organizationId = orgData.organization.id;
  console.log(`Organisation: ${orgData.organization.name}`);

  // Fetch Billy accounts for ID → accountNo mapping
  const billyAccounts = await billyGetAll(
    `/accounts?organizationId=${organizationId}`,
    apiKey,
    "accounts",
  ) as Array<{ id: string; accountNo: number }>;
  const billyIdToNo = new Map<string, string>();
  for (const a of billyAccounts) billyIdToNo.set(a.id, String(a.accountNo));

  // Fetch postings
  console.log("Fetching postings from Billy...");
  const allPostings = await billyGetAll(
    `/postings?organizationId=${organizationId}`,
    apiKey,
    "postings",
    "entryDate",
  ) as BillyPosting[];
  console.log(`  ${allPostings.length} total postings`);

  // Filter: non-voided, after last sync date, not already synced
  const knownIds = new Set(state.lastPostingIds);
  const newPostings = allPostings.filter((p) => {
    if (p.isVoided) return false;
    if (state.lastSyncDate && p.entryDate < state.lastSyncDate) return false;
    if (knownIds.has(p.id)) return false;
    return true;
  });
  console.log(`  ${newPostings.length} new postings to sync`);

  // Group by transactionId
  const txnMap = new Map<string, BillyPosting[]>();
  for (const p of newPostings) {
    if (!txnMap.has(p.transactionId)) txnMap.set(p.transactionId, []);
    txnMap.get(p.transactionId)!.push(p);
  }

  // Sort transactions by date
  const transactions = [...txnMap.entries()]
    .map(([txnId, postings]) => ({
      txnId,
      postings: postings.sort((a, b) => (a.priority ?? 0) - (b.priority ?? 0)),
      date: postings[0]!.entryDate,
      text: postings.find((p) => p.text && p.text.trim().length > 0)?.text ?? `Billy txn ${txnId.slice(0, 8)}`,
    }))
    .sort((a, b) => a.date.localeCompare(b.date));

  console.log(`  ${transactions.length} transactions to post`);

  // Transaction → originatorReference (`bill:<id>` etc.) gives a direct link
  // to the attachment owner — no date/amount heuristics needed.
  console.log("Fetching transactions and attachments from Billy...");
  const billyTxns = (await billyGetAll(
    `/transactions?organizationId=${organizationId}`,
    apiKey,
    "transactions",
    "entryDate",
  )) as BillyTransaction[];
  const ownerRefByTxnId = new Map<string, string>();
  for (const t of billyTxns) {
    if (!t.isVoided && t.originatorReference) ownerRefByTxnId.set(t.id, t.originatorReference);
  }
  const billyAttachments = (await billyGetAll(
    `/attachments?organizationId=${organizationId}`,
    apiKey,
    "attachments",
    "createdTime",
  )) as BillyAttachment[];
  const attachmentsByOwner = new Map<string, BillyAttachment[]>();
  for (const a of billyAttachments) {
    if (!attachmentsByOwner.has(a.ownerId)) attachmentsByOwner.set(a.ownerId, []);
    attachmentsByOwner.get(a.ownerId)!.push(a);
  }

  // Sales invoices carry no attachment — their evidence is the
  // Billy-generated invoice PDF (pre-signed downloadUrl on the invoice).
  const billyInvoices = (await billyGetAll(
    `/invoices?organizationId=${organizationId}`,
    apiKey,
    "invoices",
    "createdTime",
  )) as Array<{ id: string; downloadUrl: string | null }>;
  const invoiceUrlById = new Map<string, string>();
  for (const inv of billyInvoices) {
    if (inv.downloadUrl) invoiceUrlById.set(inv.id, inv.downloadUrl);
  }
  console.log(
    `  ${billyTxns.length} transactions, ${billyAttachments.length} attachments, ${billyInvoices.length} invoices`,
  );

  const evidenceCountForTxn = (txnId: string): number => {
    const ownerRef = ownerRefByTxnId.get(txnId) ?? null;
    const ownerId = ownerIdOf(ownerRef);
    if (!ownerId) return 0;
    const attCount = (attachmentsByOwner.get(ownerId) ?? []).length;
    if (attCount > 0) return attCount;
    return ownerRef!.startsWith("invoice:") && invoiceUrlById.has(ownerId) ? 1 : 0;
  };

  if (dryRun) {
    console.log("\n[DRY RUN] Would post:");
    for (const txn of transactions) {
      const lines = txn.postings
        .map((p) => `  ${billyIdToNo.get(p.accountId) ?? "?"} ${p.side} ${p.amount}`)
        .join("\n");
      console.log(`  ${txn.date} ${txn.text} (bilag klar: ${evidenceCountForTxn(txn.txnId)})\n${lines}`);
    }
    const pendingCount = state.pendingBilag?.length ?? 0;
    if (pendingCount > 0) console.log(`\n[DRY RUN] ${pendingCount} pending bilag would be retried`);
    return;
  }

  const bilagDir = join(company, "sync", "billy-bilag");
  mkdirSync(bilagDir, { recursive: true });

  // Open Rentemester DB and post
  const db = new Database(dbPath);
  let posted = 0;
  let skipped = 0;
  let errors = 0;
  let bilagAttached = 0;
  let backfilled = 0;
  const pending: PendingBilag[] = [];
  const stillPending: PendingBilag[] = [];

  // Evidence for a transaction: its owner's attachments, or — for a sales
  // invoice without attachments — the Billy-generated invoice PDF.
  const ingestEvidence = async (ownerReference: string | null | undefined): Promise<number[]> => {
    const ownerId = ownerIdOf(ownerReference ?? null);
    if (!ownerId) return [];
    const atts = attachmentsByOwner.get(ownerId) ?? [];
    const documentIds =
      atts.length > 0 ? await ingestOwnerBilag(db, company, apiKey, bilagDir, atts) : [];
    if (documentIds.length === 0 && ownerReference?.startsWith("invoice:")) {
      const url = invoiceUrlById.get(ownerId);
      if (url) {
        const filePath = await downloadInvoicePdf(ownerId, url, bilagDir);
        if (filePath) {
          const ingest = ingestSyncBilagFile(db, company, filePath);
          if (ingest.ok && ingest.documentId != null) documentIds.push(ingest.documentId);
        }
      }
    }
    return documentIds;
  };
  // Only track posting IDs for the boundary date — older dates are already
  // excluded by the date filter, so storing their IDs wastes space.
  const postedPostingIds = new Set<string>();
  let latestDate = state.lastSyncDate;

  try {
    migrate(db);
    const accountMap = loadAccountMap(db);

    for (const txn of transactions) {
      const lines: Array<{
        accountNo: string;
        debitAmount?: number;
        creditAmount?: number;
        text: string;
      }> = [];

      let valid = true;
      for (const p of txn.postings) {
        const accountNo = billyIdToNo.get(p.accountId);
        if (!accountNo || !accountMap.has(accountNo)) {
          valid = false;
          break;
        }
        if (p.amount === 0) continue;
        const absAmount = Math.abs(p.amount);
        if (p.side === "debit") {
          lines.push({ accountNo, debitAmount: absAmount, text: p.text || txn.text });
        } else if (p.side === "credit") {
          lines.push({ accountNo, creditAmount: absAmount, text: p.text || txn.text });
        }
      }

      if (!valid || lines.length < 2) {
        skipped++;
        // Do NOT mark skipped postings as synced — they should be retried
        // after the missing accounts are added to the chart.
        continue;
      }

      // Balance check
      let debitOre = 0n;
      let creditOre = 0n;
      for (const line of lines) {
        if (line.debitAmount) debitOre += toOre(line.debitAmount);
        if (line.creditAmount) creditOre += toOre(line.creditAmount);
      }
      if (debitOre !== creditOre) {
        skipped++;
        continue;
      }

      // Fetch the transaction's bilag BEFORE posting: when it exists, the
      // entry carries real document evidence instead of relying on the
      // imported-historical waiver.
      const documentIds = await ingestEvidence(ownerRefByTxnId.get(txn.txnId));

      const result = postJournalEntry(db, {
        transactionDate: txn.date,
        text: `Billy sync: ${txn.text}`,
        createdBy: actor || undefined,
        createdByProgram: SYNC_PROGRAM,
        importedHistorical: true,
        documentId: documentIds[0],
        lines,
      });

      if (result.ok) {
        posted++;
        for (const p of txn.postings) postedPostingIds.add(p.id);
        if (txn.date > latestDate) latestDate = txn.date;

        const entryId = result.entryId as unknown as number;
        for (const docId of documentIds.slice(1)) {
          linkBilagDocument(db, txn.txnId, docId, entryId);
        }
        if (documentIds.length > 0) {
          bilagAttached++;
        } else {
          const ownerReference = ownerRefByTxnId.get(txn.txnId);
          if (ownerReference) {
            pending.push({ txnId: txn.txnId, entryId, entryNo: result.entryNo!, ownerReference });
          }
        }
      } else {
        errors++;
        console.error(`  Failed txn ${txn.txnId}: ${result.errors.join(", ")}`);
      }
    }

    // Backfill 1: transactions previously posted without a bilag — link the
    // attachment as soon as it shows up in Billy.
    for (const p of state.pendingBilag ?? []) {
      const documentIds = await ingestEvidence(p.ownerReference);
      if (documentIds.length === 0) {
        stillPending.push(p);
        continue;
      }
      for (const docId of documentIds) linkBilagDocument(db, p.txnId, docId, p.entryId);
      backfilled++;
    }

    // Backfill 2: billy-sync entries posted before bilag support carry no
    // stored transactionId — reconstruct it from date + amount + text and
    // link their bilag. Ambiguous matches are reported, never guessed.
    const pendingEntryIds = new Set([...stillPending, ...pending].map((p) => p.entryId));
    const unlinked = findUnlinkedSyncEntries(db).filter((e) => !pendingEntryIds.has(e.entryId));
    if (unlinked.length > 0) {
      const allTxnMap = new Map<string, BillyPosting[]>();
      for (const p of allPostings) {
        if (p.isVoided) continue;
        if (!allTxnMap.has(p.transactionId)) allTxnMap.set(p.transactionId, []);
        allTxnMap.get(p.transactionId)!.push(p);
      }
      const candidates: BillySyncTxn[] = [];
      for (const [txnId, group] of allTxnMap) {
        let totalDebitOre = 0n;
        for (const p of group) {
          if (p.side === "debit" && p.amount !== 0) totalDebitOre += toOre(Math.abs(p.amount));
        }
        candidates.push({
          transactionId: txnId,
          entryDate: group[0]!.entryDate,
          text:
            group.find((p) => p.text && p.text.trim().length > 0)?.text ??
            `Billy txn ${txnId.slice(0, 8)}`,
          totalDebitOre,
        });
      }

      const matchResult = matchEntriesToBillyTxns(unlinked, candidates);
      for (const m of matchResult.matches) {
        const ownerReference = ownerRefByTxnId.get(m.transactionId);
        const documentIds = await ingestEvidence(ownerReference);
        if (documentIds.length === 0) {
          if (ownerReference) {
            stillPending.push({
              txnId: m.transactionId,
              entryId: m.entryId,
              entryNo: m.entryNo,
              ownerReference,
            });
          }
          continue;
        }
        for (const docId of documentIds) linkBilagDocument(db, m.transactionId, docId, m.entryId);
        backfilled++;
      }
      if (matchResult.ambiguous.length > 0) {
        console.log(
          `  Backfill: ${matchResult.ambiguous.length} entry/entries ambiguous, skipped: ${matchResult.ambiguous.join(", ")}`,
        );
      }
      if (matchResult.unmatched.length > 0) {
        console.log(
          `  Backfill: ${matchResult.unmatched.length} entry/entries without a Billy match: ${matchResult.unmatched.join(", ")}`,
        );
      }
    }
  } finally {
    db.close();

    // Persist progress even when a posting or backfill throws: transactions
    // already posted in this run must never be re-posted by the next run.
    // Update sync state — only keep posting IDs for the latest date boundary.
    // Postings before lastSyncDate are excluded by the date filter on the next
    // run. When the boundary date did not advance, MERGE with the existing
    // IDs: replacing them on a run that posted nothing would re-sync the
    // boundary date's postings next time.
    if (latestDate > state.lastSyncDate) {
      state.lastPostingIds = [...postedPostingIds];
    } else {
      state.lastPostingIds = [...new Set([...state.lastPostingIds, ...postedPostingIds])];
    }
    state.lastSyncDate = latestDate;
    // A crash mid-backfill can drop queue items from pendingBilag — that is
    // self-healing: any entry still without a document/link is rediscovered
    // by the reconstruction pass (backfill 2) on the next run.
    state.pendingBilag = [...stillPending, ...pending];
    state.syncCount += 1;
    state.lastRunAt = new Date().toISOString();
    saveSyncState(company, state);
  }

  console.log(
    `\nSync complete: ${posted} posted (${bilagAttached} with bilag), ${skipped} skipped, ${errors} errors, ` +
      `${backfilled} bilag backfilled, ${state.pendingBilag.length} pending bilag`,
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
