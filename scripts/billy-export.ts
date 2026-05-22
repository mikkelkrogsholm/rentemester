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

function parseArgs(): { outDir: string; orgId?: string } {
  const args = process.argv.slice(2);
  let outDir = "";
  let orgId: string | undefined;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--out" && args[i + 1]) {
      outDir = args[++i]!;
    } else if (args[i] === "--org-id" && args[i + 1]) {
      orgId = args[++i]!;
    }
  }
  if (!outDir) {
    console.error("Usage: billy-export.ts --out <directory> [--org-id <id>]");
    process.exit(2);
  }
  return { outDir, orgId };
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

  const { outDir, orgId: argOrgId } = parseArgs();
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

  console.log(`\nBilly export complete → ${outDir}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
