// Shared fixture builders and helpers for the cockpit-API tests.
// This file is intentionally NOT named *.test.ts so bun does not try to
// execute it as a test suite. Tests under tests/unit/server-api/ import
// from "./_shared".
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { handleRequest } from "../../../src/server/router";
import { resolveServerConfig, type ServerConfig } from "../../../src/server/config";
import { createCompany } from "../../../src/core/company";
import {
  initWorkspace,
  companyRootForSlug,
  loadWorkspaceManifest,
  saveWorkspaceManifest,
} from "../../../src/core/workspace";
import { companyPaths } from "../../../src/core/paths";
import { openDb, migrate } from "../../../src/core/db";
import { postJournalEntry } from "../../../src/core/ledger";
import { ingestDocument } from "../../../src/core/documents";
import { issueInvoice } from "../../../src/core/issued-invoices";
import { createCustomer, createVendor } from "../../../src/core/master-data";
import { recordException } from "../../../src/core/exceptions";

// Re-export commonly used pieces so test files only need to import "./_shared".
export {
  mkdtempSync,
  rmSync,
  writeFileSync,
  join,
  tmpdir,
  handleRequest,
  resolveServerConfig,
  createCompany,
  initWorkspace,
  companyRootForSlug,
  loadWorkspaceManifest,
  saveWorkspaceManifest,
  companyPaths,
  openDb,
  migrate,
  postJournalEntry,
  ingestDocument,
  issueInvoice,
  createCustomer,
  createVendor,
  recordException,
};
export type { ServerConfig };

export function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

/** A workspace with the named companies created in it. */
export function makeWorkspace(label: string, companyNames: string[] = []) {
  const root = tmpRoot(label);
  initWorkspace(root);
  for (const name of companyNames) createCompany(root, { name });
  return root;
}

export function config(overrides: Partial<ServerConfig> & { workspaceRoot: string }): ServerConfig {
  return {
    host: "127.0.0.1",
    port: 0,
    authRequired: false,
    authToken: null,
    ...overrides,
  };
}

export async function get(cfg: ServerConfig, path: string, init?: RequestInit) {
  const res = await handleRequest(new Request(`http://localhost${path}`, init), cfg);
  const body = await res.json();
  return { status: res.status, body };
}

/** Posts one balanced entry into a workspace company's ledger. */
export function postEntry(ws: string, slug: string, transactionDate: string) {
  const dbPath = companyPaths(companyRootForSlug(ws, slug)).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    // Two asset accounts keep the entry document-free (no income/expense line).
    const res = postJournalEntry(db, {
      transactionDate,
      text: "Test posting",
      lines: [
        { accountNo: "1100", debitAmount: 100 },
        { accountNo: "2000", creditAmount: 100 },
      ],
    });
    if (!res.ok) throw new Error(res.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Posts a profit-and-loss pair into a workspace company's ledger: a sale that
 * credits income account `1000` (VAT code `DK_SALE_25`) and a purchase that
 * debits expense account `3000` (VAT code `DK_PURCHASE_25`). Both are 25%-VAT
 * standard-rated, so the overview's VAT block is exercised. The cash side runs
 * over bank account `2000`. Income/expense lines require a document, so a
 * minimal one is ingested first.
 */
export function postPnlEntry(
  ws: string,
  slug: string,
  transactionDate: string,
  income: number,
  expense: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const dbPath = companyPaths(companyRoot).db;
  const db = openDb(dbPath);
  try {
    migrate(db);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-ov-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    // Date-unique content so the helper can be called for several years in
    // one test without colliding on the document content-hash dedupe.
    writeFileSync(sourceFile, `Bilag ${transactionDate}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: transactionDate,
      invoiceNo: `OV-${transactionDate}`,
      deliveryDescription: "Overblik testbilag",
      amountIncVat: 1,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Acme ApS", address: "Vej 2", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bankoverførsel",
    });
    if (!doc.ok) throw new Error("doc ingest failed: " + (doc.errors ?? []).join("; "));

    if (income > 0) {
      const sale = postJournalEntry(db, {
        transactionDate,
        text: "Overblik salg",
        documentId: doc.documentId,
        lines: [
          { accountNo: "2000", debitAmount: income * 1.25 },
          { accountNo: "1000", creditAmount: income, vatCode: "DK_SALE_25" },
          { accountNo: "1200", creditAmount: income * 0.25 },
        ],
      });
      if (!sale.ok) throw new Error("sale post failed: " + sale.errors.join("; "));
    }
    if (expense > 0) {
      const purchase = postJournalEntry(db, {
        transactionDate,
        text: "Overblik køb",
        documentId: doc.documentId,
        lines: [
          { accountNo: "3000", debitAmount: expense, vatCode: "DK_PURCHASE_25" },
          { accountNo: "4000", debitAmount: expense * 0.25 },
          { accountNo: "2000", creditAmount: expense * 1.25 },
        ],
      });
      if (!purchase.ok) throw new Error("purchase post failed: " + purchase.errors.join("; "));
    }
  } finally {
    db.close();
  }
}

/**
 * Posts a bad-debt (debitortab) write-off journal entry — the same shape
 * `core/invoice-bad-debt.ts` books: a `DK_BAD_DEBT_25` loss-basis debit, a
 * debit on the output-VAT account `1200` to claim the VAT relief, and a credit
 * writing off the receivable. `net` is the loss base; the VAT relief is 25% of
 * it. Used to reproduce the cockpit VAT bugs (#271, #272).
 */
export function postBadDebtWriteoff(
  ws: string,
  slug: string,
  transactionDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const inbox = mkdtempSync(join(tmpdir(), "rentemester-baddebt-inbox-"));
    const sourceFile = join(inbox, "doc.txt");
    writeFileSync(sourceFile, `Debitortab ${transactionDate}\n1 DKK\n`);
    const doc = ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: transactionDate,
      invoiceNo: `BD-${transactionDate}`,
      deliveryDescription: "Debitortab testbilag",
      amountIncVat: 1,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Acme ApS", address: "Vej 2", vatOrCvr: "DK12345678" },
      vatAmount: 0,
      paymentDetails: "Bankoverførsel",
    });
    if (!doc.ok) throw new Error("doc ingest failed: " + (doc.errors ?? []).join("; "));
    const vat = net * 0.25;
    const res = postJournalEntry(db, {
      transactionDate,
      text: "Tab på debitor — bad debt write-off",
      documentId: doc.documentId,
      lines: [
        { accountNo: "3080", debitAmount: net, vatCode: "DK_BAD_DEBT_25" },
        { accountNo: "1200", debitAmount: vat },
        { accountNo: "1100", creditAmount: net + vat },
      ],
    });
    if (!res.ok) throw new Error("bad-debt post failed: " + res.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Inserts an imported bank transaction directly into a company's ledger.
 * When `balanceAfter` is given the import's running balance is recorded — the
 * statement figure the actual-balance helper reads.
 */
export function seedBankTransaction(
  ws: string,
  slug: string,
  transactionDate: string,
  text: string,
  amount: number,
  balanceAfter?: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    db.query(
      `INSERT INTO bank_transactions
         (transaction_date, text, amount, currency, transaction_hash, status,
          balance_after)
       VALUES (?, ?, ?, 'DKK', ?, 'imported', ?)`,
    ).run(
      transactionDate,
      text,
      amount,
      `hash-${transactionDate}-${text}`,
      balanceAfter ?? null,
    );
  } finally {
    db.close();
  }
}

/** Records an open exception of the given type directly in a company's ledger. */
export function seedException(
  ws: string,
  slug: string,
  type: string,
  message: string,
  requiredAction?: string,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    recordException(db, {
      type,
      severity: "medium",
      message,
      ...(requiredAction ? { requiredAction } : {}),
    });
  } finally {
    db.close();
  }
}

/**
 * Seeds a read-only archived fiscal year (#197) directly into a company's
 * `import_archive_*` tables — the same shape `archiveDineroYears` writes. The
 * `balances` are `[accountNo, accountName, amount]` SaldoBalance lines; the
 * `postings` are `[accountNo, amount]` archived Posteringer rows.
 */
export function seedArchiveYear(
  ws: string,
  slug: string,
  fiscalYear: number,
  balances: Array<[string, string, number]>,
  postings: Array<[string, number]> = [],
) {
  const db = openDb(companyPaths(companyRootForSlug(ws, slug)).db);
  try {
    migrate(db);
    const info = db
      .query(
        `INSERT INTO import_archive_years
           (source_system, fiscal_year, posting_count, balance_count)
         VALUES ('dinero', ?, ?, ?)`,
      )
      .run(fiscalYear, postings.length, balances.length);
    const yearId = Number(info.lastInsertRowid);
    balances.forEach(([accountNo, name, amount], i) => {
      db.query(
        `INSERT INTO import_archive_balances
           (archive_year_id, line_no, account_no, account_name, amount)
         VALUES (?, ?, ?, ?, ?)`,
      ).run(yearId, i, accountNo, name, amount);
    });
    postings.forEach(([accountNo, amount], i) => {
      db.query(
        `INSERT INTO import_archive_postings
           (archive_year_id, line_no, account_no, amount)
         VALUES (?, ?, ?, ?)`,
      ).run(yearId, i, accountNo, amount);
    });
  } finally {
    db.close();
  }
}

/** Issues one sales invoice into a workspace company's ledger. */
export function issueTestInvoice(
  ws: string,
  slug: string,
  issueDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const vat = net * 0.25;
    const result = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate,
      seller: {
        name: "Acme ApS",
        address: "Testvej 1, 2100 København Ø",
        vatOrCvr: "DK12345678",
      },
      buyer: {
        name: "Kunde A/S",
        address: "Købervej 9, 8000 Aarhus C",
        vatOrCvr: "DK87654321",
      },
      lines: [
        {
          description: "Ydelse",
          quantity: 1,
          unitPriceExVat: net,
          lineTotalExVat: net,
        },
      ],
      totals: { netAmount: net, vatRate: 0.25, vatAmount: vat, grossAmount: net + vat },
      currency: "DKK",
    });
    if (!result.ok) throw new Error("issue failed: " + result.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Like `issueTestInvoice`, but the buyer name is parametrized so a single
 * workspace can carry invoices for several customers (#439). The seller and
 * VAT shape stay constant — only the buyer + amount + issue date vary.
 */
export function issueTestInvoiceForBuyer(
  ws: string,
  slug: string,
  buyerName: string,
  issueDate: string,
  net: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    const vat = net * 0.25;
    const result = issueInvoice(db, companyRoot, {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate,
      seller: {
        name: "Acme ApS",
        address: "Testvej 1, 2100 København Ø",
        vatOrCvr: "DK12345678",
      },
      buyer: {
        name: buyerName,
        address: "Købervej 9, 8000 Aarhus C",
      },
      lines: [
        {
          description: "Ydelse",
          quantity: 1,
          unitPriceExVat: net,
          lineTotalExVat: net,
        },
      ],
      totals: { netAmount: net, vatRate: 0.25, vatAmount: vat, grossAmount: net + vat },
      currency: "DKK",
    });
    if (!result.ok) throw new Error("issue failed: " + result.errors.join("; "));
  } finally {
    db.close();
  }
}

/**
 * Posts a balanced liability accrual into a company's ledger: a credit to a
 * liability account (created on the fly if absent) against a debit to bank
 * account `2000`. Neither side is an income/expense account, so no document
 * is required. This is the shape that surfaces in the obligations endpoint.
 */
export function postLiability(
  ws: string,
  slug: string,
  transactionDate: string,
  accountNo: string,
  accountName: string,
  amount: number,
) {
  const companyRoot = companyRootForSlug(ws, slug);
  const db = openDb(companyPaths(companyRoot).db);
  try {
    migrate(db);
    db.query(
      `INSERT OR IGNORE INTO accounts (account_no, name, type, normal_balance)
       VALUES (?, ?, 'liability', 'credit')`,
    ).run(accountNo, accountName);
    const entry = postJournalEntry(db, {
      transactionDate,
      text: `Hensættelse ${accountName}`,
      lines: [
        { accountNo: "2000", debitAmount: amount },
        { accountNo, creditAmount: amount },
      ],
    });
    if (!entry.ok) throw new Error("liability post failed: " + entry.errors.join("; "));
  } finally {
    db.close();
  }
}
