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

const API_BASE = "https://api.billysbilling.com/v2";
const SYNC_PROGRAM = "billy-sync";

type SyncState = {
  lastSyncDate: string;
  lastPostingIds: string[];
  syncCount: number;
  lastRunAt: string;
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

async function billyGetAll(
  path: string,
  token: string,
  key: string,
): Promise<unknown[]> {
  let page = 1;
  const pageSize = 100;
  const all: unknown[] = [];
  while (true) {
    const url = `${API_BASE}${path}${path.includes("?") ? "&" : "?"}pageSize=${pageSize}&page=${page}`;
    const res = await fetch(url, {
      headers: { "X-Access-Token": token, Accept: "application/json" },
    });
    if (!res.ok) throw new Error(`Billy API ${res.status}: ${await res.text()}`);
    const data = (await res.json()) as Record<string, unknown>;
    const items = data[key];
    if (!Array.isArray(items) || items.length === 0) break;
    all.push(...items);
    if (items.length < pageSize) break;
    page++;
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
  const orgData = (await (await fetch(`${API_BASE}/organization`, {
    headers: { "X-Access-Token": apiKey, Accept: "application/json" },
  })).json()) as { organization: { id: string; name: string } };
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

  if (newPostings.length === 0) {
    console.log("Nothing to sync.");
    state.lastRunAt = new Date().toISOString();
    state.syncCount += 1;
    saveSyncState(company, state);
    return;
  }

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

  if (dryRun) {
    console.log("\n[DRY RUN] Would post:");
    for (const txn of transactions) {
      const lines = txn.postings
        .map((p) => `  ${billyIdToNo.get(p.accountId) ?? "?"} ${p.side} ${p.amount}`)
        .join("\n");
      console.log(`  ${txn.date} ${txn.text}\n${lines}`);
    }
    return;
  }

  // Open Rentemester DB and post
  const db = new Database(dbPath);
  migrate(db);
  const accountMap = loadAccountMap(db);

  let posted = 0;
  let skipped = 0;
  let errors = 0;
  const syncedPostingIds: string[] = [...state.lastPostingIds];
  let latestDate = state.lastSyncDate;

  for (const txn of transactions) {
    // Build journal entry lines
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
      lines.push({
        accountNo,
        ...(p.side === "debit" ? { debitAmount: p.amount } : { creditAmount: p.amount }),
        text: p.text || txn.text,
      });
    }

    if (!valid || lines.length < 2) {
      skipped++;
      for (const p of txn.postings) syncedPostingIds.push(p.id);
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
      for (const p of txn.postings) syncedPostingIds.push(p.id);
      continue;
    }

    const result = postJournalEntry(db, {
      transactionDate: txn.date,
      text: `Billy sync: ${txn.text}`,
      createdBy: actor || undefined,
      createdByProgram: SYNC_PROGRAM,
      importedHistorical: true,
      lines,
    });

    if (result.ok) {
      posted++;
      for (const p of txn.postings) syncedPostingIds.push(p.id);
      if (txn.date > latestDate) latestDate = txn.date;
    } else {
      errors++;
      console.error(`  Failed txn ${txn.txnId}: ${result.errors.join(", ")}`);
    }
  }

  db.close();

  // Update sync state
  state.lastSyncDate = latestDate;
  state.lastPostingIds = syncedPostingIds;
  state.syncCount += 1;
  state.lastRunAt = new Date().toISOString();
  saveSyncState(company, state);

  console.log(`\nSync complete: ${posted} posted, ${skipped} skipped, ${errors} errors`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
