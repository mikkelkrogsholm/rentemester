#!/usr/bin/env bun
// Billy API export script — fetches the chart of accounts, organisation data,
// journal entries and contacts from Billy's REST API into a local directory
// that the Billy `SourceParser` can consume.
//
// Usage:
//   BILLY_API_KEY=... bun run scripts/billy-export.ts --out /path/to/billy-export [--org-id ...]
//
// The Billy API key must be set via the BILLY_API_KEY environment variable.
// The organisation ID is auto-detected from the API key unless --org-id is given.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const API_BASE = "https://api.billysbilling.com/v2";

function parseArgs(): { outDir: string; orgId?: string; cutOverDate?: string } {
  const args = process.argv.slice(2);
  let outDir = "";
  let orgId: string | undefined;
  let cutOverDate: string | undefined;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--out" && args[i + 1]) {
      outDir = args[++i]!;
    } else if (args[i] === "--org-id" && args[i + 1]) {
      orgId = args[++i]!;
    } else if (args[i] === "--cut-over" && args[i + 1]) {
      cutOverDate = args[++i]!;
    }
  }
  if (!outDir) {
    console.error("Usage: billy-export.ts --out <directory> [--org-id <id>] [--cut-over YYYY-MM-DD]");
    process.exit(2);
  }
  return { outDir, orgId, cutOverDate };
}

async function billyGet(path: string, token: string): Promise<unknown> {
  const url = `${API_BASE}${path}`;
  const res = await fetch(url, {
    headers: { "X-Access-Token": token, Accept: "application/json" },
  });
  if (!res.ok) {
    throw new Error(`Billy API ${res.status} for ${path}: ${await res.text()}`);
  }
  return res.json();
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
    const url = `${path}${path.includes("?") ? "&" : "?"}pageSize=${pageSize}&page=${page}`;
    const data = (await billyGet(url, token)) as Record<string, unknown>;
    const items = data[key];
    if (!Array.isArray(items) || items.length === 0) break;
    all.push(...items);
    if (items.length < pageSize) break;
    page++;
  }
  return all;
}

async function main() {
  const apiKey = process.env.BILLY_API_KEY;
  if (!apiKey) {
    console.error("BILLY_API_KEY environment variable is required");
    process.exit(2);
  }

  const { outDir, orgId: argOrgId, cutOverDate: argCutOver } = parseArgs();
  mkdirSync(outDir, { recursive: true });

  console.log("Fetching organisation...");
  const orgData = (await billyGet("/organization", apiKey)) as Record<string, unknown>;
  const org = orgData.organization as Record<string, unknown>;
  const organizationId = argOrgId ?? (org?.id as string);
  if (!organizationId) {
    console.error("Could not determine organisation ID from API");
    process.exit(1);
  }
  writeFileSync(join(outDir, "organization.json"), JSON.stringify(org, null, 2), "utf8");
  console.log(`  Organisation: ${org?.name ?? "?"} (${organizationId})`);

  console.log("Fetching accounts...");
  const accounts = await billyGetAll(
    `/accounts?organizationId=${organizationId}`,
    apiKey,
    "accounts",
  );
  writeFileSync(join(outDir, "accounts.json"), JSON.stringify(accounts, null, 2), "utf8");
  console.log(`  ${accounts.length} accounts`);

  console.log("Fetching contacts...");
  const contacts = await billyGetAll(
    `/contacts?organizationId=${organizationId}`,
    apiKey,
    "contacts",
  );
  writeFileSync(join(outDir, "contacts.json"), JSON.stringify(contacts, null, 2), "utf8");
  console.log(`  ${contacts.length} contacts`);

  console.log("Fetching daybook transactions...");
  const transactions = await billyGetAll(
    `/daybookTransactions?organizationId=${organizationId}`,
    apiKey,
    "daybookTransactions",
  );
  writeFileSync(
    join(outDir, "daybook-transactions.json"),
    JSON.stringify(transactions, null, 2),
    "utf8",
  );
  console.log(`  ${transactions.length} daybook transactions`);

  console.log("Fetching invoices...");
  const invoices = await billyGetAll(
    `/invoices?organizationId=${organizationId}`,
    apiKey,
    "invoices",
  );
  writeFileSync(join(outDir, "invoices.json"), JSON.stringify(invoices, null, 2), "utf8");
  console.log(`  ${invoices.length} invoices`);

  console.log("Fetching bills...");
  const bills = await billyGetAll(
    `/bills?organizationId=${organizationId}`,
    apiKey,
    "bills",
  );
  writeFileSync(join(outDir, "bills.json"), JSON.stringify(bills, null, 2), "utf8");
  console.log(`  ${bills.length} bills`);

  console.log("Fetching account groups...");
  const groups = await billyGetAll(
    `/accountGroups?organizationId=${organizationId}`,
    apiKey,
    "accountGroups",
  );
  writeFileSync(join(outDir, "account-groups.json"), JSON.stringify(groups, null, 2), "utf8");
  console.log(`  ${groups.length} account groups`);

  console.log("Fetching tax rates...");
  const taxRates = await billyGetAll(
    `/taxRates?organizationId=${organizationId}`,
    apiKey,
    "taxRates",
  );
  writeFileSync(join(outDir, "tax-rates.json"), JSON.stringify(taxRates, null, 2), "utf8");
  console.log(`  ${taxRates.length} tax rates`);

  console.log("Fetching attachments...");
  const attachments = await billyGetAll(
    `/attachments?organizationId=${organizationId}`,
    apiKey,
    "attachments",
  ) as Array<{
    id: string;
    ownerId: string;
    ownerReference: string;
    fileId: string;
    documentDate: string | null;
    amount: number | null;
    supplier: string | null;
  }>;
  writeFileSync(join(outDir, "attachments.json"), JSON.stringify(attachments, null, 2), "utf8");
  console.log(`  ${attachments.length} attachments`);

  // Download attachment files with correct extensions
  const bilagDir = join(outDir, "bilag");
  mkdirSync(bilagDir, { recursive: true });
  let downloadedCount = 0;
  let downloadErrors = 0;
  const mimeToExt: Record<string, string> = {
    "application/pdf": ".pdf",
    "image/png": ".png",
    "image/jpeg": ".jpg",
    "image/gif": ".gif",
  };
  for (const att of attachments) {
    if (!att.fileId) continue;
    try {
      const fileRes = await fetch(`${API_BASE}/files/${att.fileId}`, {
        headers: { "X-Access-Token": apiKey },
      });
      if (fileRes.ok) {
        const contentType = (fileRes.headers.get("content-type") ?? "").split(";")[0]!.trim();
        const ext = mimeToExt[contentType] ?? ".pdf";
        const filePath = join(bilagDir, `${att.ownerId}__${att.id}${ext}`);
        const buffer = await fileRes.arrayBuffer();
        writeFileSync(filePath, Buffer.from(buffer));
        downloadedCount++;
      } else {
        downloadErrors++;
      }
    } catch {
      downloadErrors++;
    }
  }
  console.log(`  ${downloadedCount} files downloaded${downloadErrors > 0 ? `, ${downloadErrors} failed` : ""}`);

  // Build bill → transactionId mapping using entryDate + amount + text
  const billMap: Array<{ billId: string; transactionId: string; voucherNo: string }> = [];
  type PostingRecord = {
    transactionId: string;
    entryDate: string;
    text: string;
    amount: number;
    side: string;
    isVoided: boolean;
    accountId: string;
  };

  console.log("Fetching postings for balance computation...");
  const postings = await billyGetAll(
    `/postings?organizationId=${organizationId}`,
    apiKey,
    "postings",
  ) as PostingRecord[];
  writeFileSync(join(outDir, "postings.json"), JSON.stringify(postings, null, 2), "utf8");
  console.log(`  ${postings.length} postings`);

  // Build bill → transactionId mapping.
  // Group postings by transactionId, then match each bill by entryDate + grossAmount.
  const txnByKey = new Map<string, string>();
  const groupedByTxn = new Map<string, PostingRecord[]>();
  for (const p of postings) {
    if (p.isVoided) continue;
    if (!groupedByTxn.has(p.transactionId)) groupedByTxn.set(p.transactionId, []);
    groupedByTxn.get(p.transactionId)!.push(p);
  }
  for (const [txnId, group] of groupedByTxn) {
    const first = group[0]!;
    const totalDebit = group
      .filter((p) => p.side === "debit")
      .reduce((sum, p) => sum + p.amount, 0);
    const text = first.text || "";
    // Key: date|amount|first-8-chars-of-text for disambiguation
    const key = `${first.entryDate}|${Math.round(totalDebit * 100)}|${text.slice(0, 8).trim()}`;
    if (!txnByKey.has(key)) txnByKey.set(key, txnId);
  }
  for (const bill of bills as Array<{ id: string; entryDate: string; grossAmount: number; lineDescription: string; voucherNo: string }>) {
    const key = `${bill.entryDate}|${Math.round(bill.grossAmount * 100)}|${(bill.lineDescription || "").slice(0, 8).trim()}`;
    const txnId = txnByKey.get(key);
    if (txnId) {
      billMap.push({ billId: bill.id, transactionId: txnId, voucherNo: bill.voucherNo ?? "" });
    }
  }
  writeFileSync(join(outDir, "bill-transaction-map.json"), JSON.stringify(billMap, null, 2), "utf8");
  console.log(`  ${billMap.length} / ${(bills as unknown[]).length} bills mapped to transactionIds`);

  // Build account ID → { accountNo, natureId } map
  const accountById = new Map<string, { accountNo: number; natureId: string }>();
  for (const acct of accounts as Array<{ id: string; accountNo: number; natureId?: string }>) {
    accountById.set(acct.id, { accountNo: acct.accountNo, natureId: acct.natureId ?? "" });
  }

  // Balance sheet natures — only these go into the opening balance.
  // Income and expense accounts are P&L and net into retained earnings.
  const balanceSheetNatures = new Set(["asset", "liability", "equity"]);

  // Compute balances as of the cut-over date (start of current fiscal year)
  const cutOverDate = argCutOver ?? `${new Date().getFullYear()}-01-01`;
  const activePostings = postings.filter(
    (p) => !p.isVoided && p.entryDate < cutOverDate,
  );
  const balanceByAccount = new Map<number, number>();
  for (const p of activePostings) {
    const acct = accountById.get(p.accountId);
    if (!acct) continue;
    if (!balanceSheetNatures.has(acct.natureId)) continue;
    const signed = p.side === "debit" ? p.amount : -p.amount;
    balanceByAccount.set(acct.accountNo, (balanceByAccount.get(acct.accountNo) ?? 0) + signed);
  }
  // The balance sheet must balance: sum(debits) == sum(credits). P&L accounts
  // (income/expense) are excluded, so their net effect must be plugged into
  // retained earnings (account 7120 "Transferred Result"). The plug is simply
  // the imbalance: if debits > credits, retained earnings gets a credit (the
  // company is profitable), and vice versa.
  let debitSum = 0;
  let creditSum = 0;
  for (const amount of balanceByAccount.values()) {
    if (amount > 0) debitSum += amount;
    else creditSum += -amount;
  }
  const imbalance = debitSum - creditSum;
  if (Math.abs(imbalance) > 0.005) {
    // Positive imbalance → assets exceed liabilities+equity → net profit → credit 7120
    // Negative imbalance → liabilities+equity exceed assets → net loss → debit 7120
    const existing7120 = balanceByAccount.get(7120) ?? 0;
    balanceByAccount.set(7120, existing7120 - imbalance);
  }

  const balances = [...balanceByAccount.entries()]
    .filter(([, balance]) => Math.abs(balance) > 0.005)
    .sort(([a], [b]) => a - b)
    .map(([accountNo, balance]) => ({
      accountNo: String(accountNo),
      balance: Math.round(balance * 100) / 100,
    }));
  writeFileSync(
    join(outDir, "balances.json"),
    JSON.stringify({ cutOverDate, balances }, null, 2),
    "utf8",
  );
  console.log(`  ${balances.length} account balances computed (cut-over: ${cutOverDate})`);

  console.log(`\nBilly export complete → ${outDir}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
