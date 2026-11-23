import { afterEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { initialiseCompanyVolume } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { postDineroPostings } from "../../src/core/import/dinero-postings";
import { importBankCsv } from "../../src/core/bank";
import { ingestDocument } from "../../src/core/documents";
import { buildTrialBalance } from "../../src/core/financial-statements";
import { storeViesValidation } from "../../src/core/vies";
import { buildVatReport } from "../../src/core/vat";
import { postJournalEntry, reverseJournalEntry, verifyAuditChain } from "../../src/core/ledger";

const roleAccounts = [
  ["input_vat", "64060", "Købsmoms (indgående moms)"],
  ["output_vat", "64000", "Salgsmoms (udgående moms)"],
  ["reverse_charge_vat", "64040", "Moms af ydelser fra udlandet"],
] as const;
const companyName = "Syntetisk Rolleprøve ApS";
const actor = "user:role-reviewer";
const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-vat-role-"));
  roots.push(root);
  const company = join(root, "company");
  initialiseCompanyVolume(company, { name: companyName, cvr: "DK12345678", onboardingActor: actor });
  const db = openDb(join(company, "data/ledger.sqlite"));
  const source = join(root, "export");
  mkdirSync(join(source, "2025"), { recursive: true });
  writeFileSync(join(source, "Firmaoplysninger.csv"), `Firmanavn;CvrNr\n${companyName};12345678\n`);
  const chartPath = join(source, "2025/Kontoplan.csv");
  writeFileSync(
    chartPath,
    [
      "Nummer;Navn;Type;Momstype;Total",
      ...roleAccounts.map(([, no, name]) => `${no};${name};Passiv;;0,00`),
      "64020;Moms af varekøb i udlandet;Passiv;;0,00",
      "64888;Leverandørlån;Passiv;;0,00",
    ].join("\n"),
  );
  return { root, company, db, source, chartPath };
}

function legacyChart(db: Database) {
  for (const [, no, name] of roleAccounts) {
    db.query("INSERT INTO accounts (account_no,name,type,normal_balance) VALUES (?,?,'liability','credit')").run(
      no,
      name,
    );
  }
  db.run(
    "UPDATE account_role_mappings SET status='inactive' WHERE role IN ('input_vat','output_vat','reverse_charge_vat')",
  );
}

function cli(company: string, ...args: string[]) {
  const proc = Bun.spawnSync(
    [process.execPath, "src/cli.ts", ...args, "--company", company, "--actor", actor, "--json"],
    { cwd: join(import.meta.dir, "../..") },
  );
  const output = proc.stdout.toString();
  if (!output.trim()) throw new Error(proc.stderr.toString());
  const result = JSON.parse(output);
  expect(proc.exitCode).toBe(result.ok ? 0 : 1);
  return result;
}

function confirm(company: string, role: string, no: string, source?: string) {
  return cli(
    company,
    "accounts",
    "role-confirm",
    "--role",
    role,
    "--account",
    no,
    ...(source ? ["--dinero-export", source] : []),
  );
}

function ingestPurchases(db: Database, company: string, root: string) {
  const date = "2026-05-16";
  const bankFile = join(root, "bank.csv");
  writeFileSync(
    bankFile,
    [
      "transaction_date,booking_date,text,amount,currency,reference",
      `${date},${date},Domestic subscription,-625,DKK,DK-TEST`,
      `${date},${date},EU software service,-320,DKK,EU-TEST`,
    ].join("\n"),
  );
  expect(importBankCsv(db, company, bankFile).ok).toBe(true);
  const argsByRegion: Record<"domestic" | "european", string[]> = { domestic: [], european: [] };
  for (const [ref, amount, vat, supplier] of [
    ["DK", 625, 125, "DK11223344"],
    ["EU", 320, 0, "DE123456789"],
  ] as const) {
    const file = join(root, ref + ".txt");
    writeFileSync(file, `Synthetic invoice ${ref}. Subscription. Gross DKK ${amount}, VAT DKK ${vat}.\n`);
    const document = ingestDocument(db, company, file, {
      source: "email",
      issueDate: date,
      invoiceNo: `${ref}-001`,
      deliveryDescription: "Subscription",
      amountIncVat: amount,
      vatAmount: vat,
      currency: "DKK",
      sender: { name: `Synthetic vendor ${ref}`, address: "Supplier street 1", vatOrCvr: supplier },
      recipient: { name: companyName, address: "Testvej 1", vatOrCvr: "DK12345678" },
      paymentDetails: "Bank transfer",
    });
    expect(document.ok).toBe(true);
    const bank = db.query("SELECT id FROM bank_transactions WHERE reference = ?").get(ref + "-TEST") as {
      id: number;
    };
    const args = [
      "--document-id",
      String(document.documentId),
      "--bank-transaction-id",
      String(bank.id),
      "--expense-account",
      "3000",
      "--vat-treatment",
      ref === "DK" ? "standard" : "reverse_charge",
    ];
    argsByRegion[ref === "DK" ? "domestic" : "european"] = args;
  }
  return argsByRegion;
}

function treeDigests(root: string) {
  return Object.fromEntries(
    readdirSync(root, { recursive: true, withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => {
        const path = join(entry.parentPath, entry.name);
        return [path.slice(root.length), createHash("sha256").update(readFileSync(path)).digest("hex")];
      }),
  );
}

function history(db: Database) {
  return {
    journal: db.query("SELECT * FROM journal_entries WHERE transaction_date < '2026-01-01' ORDER BY id").all(),
    lines: db
      .query(
        "SELECT l.* FROM journal_lines l JOIN journal_entries j ON j.id = l.journal_entry_id WHERE j.transaction_date < '2026-01-01' ORDER BY l.id",
      )
      .all(),
    controls: db.query("SELECT * FROM accounts WHERE account_no IN ('64000','64040','64060') ORDER BY id").all(),
    balances: buildTrialBalance(db, "2025-01-01", "2025-12-31"),
    vat: buildVatReport(db, "2025-01-01", "2025-12-31"),
  };
}

test("legacy Dinero vouchers survive audited, retry-safe review and bank/document purchase previews", () => {
  const { root, company, db, source } = fixture();
  try {
    legacyChart(db);
    const vouchers = [
      {
        transactionDate: "2025-01-15",
        text: "Synthetic foreign service",
        voucherRef: "H1",
        entryType: "Køb",
        lines: [
          { accountNo: "3010", debitAmount: 160 },
          { accountNo: "64060", debitAmount: 40 },
          { accountNo: "2000", creditAmount: 160 },
          { accountNo: "64040", creditAmount: 40 },
        ],
      },
      {
        transactionDate: "2025-02-01",
        text: "Synthetic domestic sale",
        voucherRef: "H2",
        entryType: "Salg",
        lines: [
          { accountNo: "1000", creditAmount: 200, vatCode: "DK_SALE_25" },
          { accountNo: "64000", creditAmount: 50 },
          { accountNo: "2000", debitAmount: 250 },
        ],
      },
    ];
    expect(
      postDineroPostings(db, vouchers, new Set(vouchers.flatMap((v) => v.lines.map((l) => l.accountNo)))).errors,
    ).toEqual([]);
    const purchases = ingestPurchases(db, company, root);
    const before = history(db);
    expect(before.vat).toMatchObject({
      ok: true,
      inputVat: 40,
      outputVat: 90,
      rubrikker: { rubrikAYdelser: 160 },
    });
    for (const [role, no] of roleAccounts)
      expect(confirm(company, role, no).errors).toContain(`account ${no} is not compatible with role '${role}'`);
    expect(cli(company, "expense", "dry-run", ...purchases.domestic).errors).toContain(
      "account role 'input_vat' has no active confirmed mapping",
    );
    expect(cli(company, "expense", "dry-run", ...purchases.european).errors.join(" ")).toContain(
      "VIES lookup not yet performed",
    );
    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      rawResponse: '{"fixture":"synthetic","valid":true}',
    });
    expect(cli(company, "expense", "dry-run", ...purchases.european).errors).toContain(
      "account role 'input_vat' has no active confirmed mapping",
    );
    for (const [role, no] of roleAccounts) {
      const result = confirm(company, role, no, source);
      expect(result).toMatchObject({
        ok: true,
        resolution: {
          accountNo: no,
          role,
          confirmedBy: actor,
          dineroReview: { chartPath: "2025/Kontoplan.csv", lastExistingEntryId: 2, companyCvr: "DK12345678" },
        },
      });
      expect(confirm(company, role, no, source).resolution).toEqual(result.resolution);
      expect(history(db)).toEqual(before);
    }
    const reviews = db
      .query(`SELECT m.role, m.confirmed_by, r.chart_path, length(r.chart_sha256) AS hash_length, r.last_existing_entry_id
      FROM dinero_vat_role_reviews r JOIN account_role_mappings m ON m.id = r.mapping_id ORDER BY m.id`)
      .all();
    expect(reviews).toEqual(
      roleAccounts.map(([role]) => ({
        role,
        confirmed_by: actor,
        chart_path: "2025/Kontoplan.csv",
        hash_length: 64,
        last_existing_entry_id: 2,
      })),
    );
    expect(
      db
        .query(
          "SELECT count(*) AS n FROM audit_log WHERE event_type='account_role_confirmed' AND actor LIKE 'user:role-reviewer via %'",
        )
        .get(),
    ).toEqual({ n: 3 });
    const diskBefore = treeDigests(company);
    const domestic = cli(company, "expense", "dry-run", ...purchases.domestic);
    expect(domestic).toMatchObject({
      ok: true,
      dryRun: true,
      lines: [
        { accountNo: "3000", debitAmount: 500, vatCode: "DK_PURCHASE_25" },
        { accountNo: "64060", debitAmount: 125 },
        { accountNo: "2000", creditAmount: 625 },
      ],
    });
    const european = cli(company, "expense", "dry-run", ...purchases.european);
    expect(european).toMatchObject({
      ok: true,
      dryRun: true,
      lines: [
        { accountNo: "3000", debitAmount: 320, vatCode: "EU_SERVICE_REVERSE_CHARGE" },
        { accountNo: "64060", debitAmount: 80 },
        { accountNo: "2000", creditAmount: 320 },
        { accountNo: "64040", creditAmount: 80 },
      ],
    });
    expect(cli(company, "expense", "dry-run", ...purchases.european).lines).toEqual(european.lines);
    expect(treeDigests(company)).toEqual(diskBefore);
    expect(cli(company, "expense", "book", ...purchases.domestic).ok).toBe(true);
    expect(cli(company, "expense", "book", ...purchases.european).ok).toBe(true);
    expect(buildVatReport(db, "2026-05-01", "2026-05-31")).toMatchObject({
      ok: true,
      inputVat: 205,
      outputVat: 80,
      rubrikker: { rubrikAYdelser: 320 },
    });
    expect(history(db)).toEqual(before);
    expect(verifyAuditChain(db).errors).toEqual([]);
  } finally {
    db.close();
  }
}, 30_000);

test("first purchase after review works without an earlier VAT-bearing voucher and preserves old liability movements and reversals", () => {
  const { root, company, db, source } = fixture();
  try {
    legacyChart(db);
    const old = postJournalEntry(db, {
      transactionDate: "2025-02-07",
      text: "Legacy liability transfer",
      lines: [
        { accountNo: "64060", debitAmount: 7 },
        { accountNo: "2000", creditAmount: 7 },
      ],
    });
    expect(old.errors).toEqual([]);
    const before = history(db);
    expect(before.vat).toMatchObject({ ok: true, inputVat: 0, outputVat: 0 });
    const purchases = ingestPurchases(db, company, root);
    for (const [role, no] of roleAccounts) expect(confirm(company, role, no, source).ok).toBe(true);
    expect(history(db)).toEqual(before);
    const reversed = reverseJournalEntry(db, {
      entryId: old.entryId!,
      transactionDate: "2026-04-19",
      reason: "Synthetic historical correction",
    });
    expect(reversed.errors).toEqual([]);
    expect(buildVatReport(db, "2026-04-01", "2026-04-30")).toMatchObject({ ok: true, inputVat: 0, outputVat: 0 });
    expect(confirm(company, "input_vat", "64060", source).resolution.version).toBe(2);
    expect(cli(company, "expense", "book", ...purchases.domestic).ok).toBe(true);
    expect(buildVatReport(db, "2026-05-01", "2026-05-31")).toMatchObject({
      ok: true,
      inputVat: 125,
      outputVat: 0,
    });
    expect(confirm(company, "input_vat", "4000").ok).toBe(true);
    expect(buildVatReport(db, "2026-05-01", "2026-05-31")).toMatchObject({
      ok: true,
      inputVat: 125,
      outputVat: 0,
    });
    expect(buildVatReport(db, "2025-01-01", "2025-12-31")).toEqual(before.vat);
  } finally {
    db.close();
  }
}, 20_000);

test("reviewed source rejects wrong entity, ambiguous or contradictory controls, ordinary liabilities, inactive accounts and goods as services", () => {
  const { company, db, source, chartPath } = fixture();
  try {
    legacyChart(db);
    db.run(
      "INSERT INTO accounts (account_no,name,type,normal_balance) VALUES ('64020','Moms af varekøb i udlandet','liability','credit'),('64888','Leverandørlån','liability','credit')",
    );
    const correct = readFileSync(chartPath, "utf8");
    const companyPath = join(source, "Firmaoplysninger.csv");
    const originalCompany = readFileSync(companyPath, "utf8");
    writeFileSync(companyPath, originalCompany.replace("12345678", "87654321"));
    expect(confirm(company, "input_vat", "64060", source).ok).toBe(false);
    writeFileSync(companyPath, originalCompany);
    for (const invalid of [
      correct + "\n64060;Købsmoms;Passiv;;0,00",
      correct.replace("Købsmoms (indgående moms);Passiv", "Intercompany loan;Passiv"),
      correct.replace("Købsmoms (indgående moms);Passiv", "Købsmoms (indgående moms);Aktiv"),
      correct.replace("Købsmoms (indgående moms);Passiv;;", "Købsmoms (indgående moms);Passiv;I25;"),
    ]) {
      writeFileSync(chartPath, invalid);
      expect(confirm(company, "input_vat", "64060", source).ok).toBe(false);
    }
    writeFileSync(chartPath, correct);
    for (const [role, no] of [
      ["input_vat", "64888"],
      ["reverse_charge_vat", "64020"],
      ["output_vat", "64060"],
    ]) {
      expect(confirm(company, role!, no!, source).ok).toBe(false);
    }
    db.run("UPDATE accounts SET active=0 WHERE account_no='64060'");
    expect(confirm(company, "input_vat", "64060", source).ok).toBe(false);
    db.run("UPDATE accounts SET active=1, name='Other loan' WHERE account_no='64060'");
    expect(confirm(company, "input_vat", "64060", source).ok).toBe(false);
    expect(db.query("SELECT count(*) AS n FROM dinero_vat_role_reviews").get()).toEqual({ n: 0 });
    expect(
      db
        .query(
          "SELECT count(*) AS n FROM account_role_mappings WHERE status='confirmed' AND role IN ('input_vat','output_vat','reverse_charge_vat')",
        )
        .get(),
    ).toEqual({ n: 0 });
  } finally {
    db.close();
  }
}, 20_000);

test("fresh Dinero import proposes typed controls and the same purchase previews work with ordinary confirmation", () => {
  const { root, company, db } = fixture();
  try {
    const directory = join(import.meta.dir, "../../examples/import-dinero");
    const imported = cli(company, "import", "run", "--system", "dinero", "--file", directory);
    expect(imported.errors).toEqual([]);
    const roles = cli(company, "accounts", "roles-status");
    for (const [role, no] of roleAccounts) {
      expect(roles.proposals).toContainEqual(expect.objectContaining({ role, accountNo: no, compatible: true }));
      const reviewed = confirm(company, role, no);
      expect(reviewed).toMatchObject({ ok: true, resolution: { role, accountNo: no, confirmedBy: actor } });
      expect(confirm(company, role, no).resolution.version).toEqual(reviewed.resolution.version);
    }
    expect(confirm(company, "bank", "5510").ok).toBe(true);
    const purchases = ingestPurchases(db, company, root);
    storeViesValidation(db, {
      vatOrCvr: "DE123456789",
      valid: true,
      rawResponse: '{"fixture":"synthetic","valid":true}',
    });
    const domestic = cli(company, "expense", "dry-run", ...purchases.domestic);
    const european = cli(company, "expense", "dry-run", ...purchases.european);
    expect(domestic.lines.map((line: { accountNo: string }) => line.accountNo)).toEqual(["3000", "64060", "5510"]);
    expect(european.lines.map((line: { accountNo: string }) => line.accountNo)).toEqual([
      "3000",
      "64060",
      "5510",
      "64040",
    ]);
    expect(db.query("SELECT count(*) AS n FROM dinero_vat_role_reviews").get()).toEqual({ n: 0 });
  } finally {
    db.close();
  }
}, 20_000);

test("confirming output and reverse-charge roles separately leaves native historical sales in the original rubric", () => {
  const { company, db, source } = fixture();
  try {
    legacyChart(db);
    db.run(
      "UPDATE account_role_mappings SET status='confirmed' WHERE role IN ('output_vat','reverse_charge_vat') AND confirmation_source='native_seed'",
    );
    const lines = [
      { accountNo: "1000", creditAmount: 200, vatCode: "DK_SALE_25" },
      { accountNo: "1200", creditAmount: 50 },
      { accountNo: "2000", debitAmount: 250 },
    ];
    expect(
      postDineroPostings(
        db,
        [{ transactionDate: "2025-02-07", text: "Synthetic prior sale", voucherRef: "H1", lines }],
        new Set(lines.map((line) => line.accountNo)),
      ).errors,
    ).toEqual([]);
    const before = history(db);
    expect(before.vat).toMatchObject({ ok: true, rubrikker: { salgsmoms: 50, momsAfYdelseskobUdland: 0 } });
    for (const [role, no] of roleAccounts.slice(1)) {
      expect(confirm(company, role, no, source).ok).toBe(true);
      expect(history(db)).toEqual(before);
    }
  } finally {
    db.close();
  }
}, 10_000);
