/** Executable R01/R08 wire baseline. Every invocation uses disposable synthetic copies. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { z } from "zod";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { initWorkspace } from "../../src/core/workspace";
import { registerAllTools } from "../../src/mcp/registry";
import { handleRequest } from "../../src/server/router";
import { exportTrialBalanceCsv } from "../../src/server/data/statement-exports";

const root = resolve(import.meta.dir, "../..");
const temp = mkdtempSync(join(tmpdir(), "rentemester-report-contracts-"));
const baseline = join(temp, "baseline");
const seed = join(temp, "seed");
const money = z.number().finite();
const exactMoney = z.string().regex(/^(?!-0\.00$)-?(0|[1-9]\d*)\.\d{2}$/);
const accountType = z.enum([
  "asset",
  "liability",
  "equity",
  "income",
  "expense",
  "vat",
]);
const row = z.strictObject({
  accountNo: z.string(),
  name: z.string(),
  type: accountType,
  debit: money,
  credit: money,
  balance: money,
});
const coreReport = z.strictObject({
  ok: z.boolean(),
  appliedRules: z.array(z.string()),
  periodStart: z.string(),
  periodEnd: z.string(),
  accounts: z.array(row.extend({ normalBalance: z.enum(["debit", "credit"]) })),
  totalDebit: money,
  totalCredit: money,
  balanced: z.boolean(),
  linesConsidered: z.number().int().nonnegative(),
  journalEntryCount: z.number().int().nonnegative(),
  errors: z.array(z.string()),
});
const httpReport = z.strictObject({
  ok: z.literal(true),
  trialBalance: z.strictObject({
    slug: z.string(),
    selectedYear: z.string(),
    archived: z.boolean(),
    archivedSource: z.string().nullable(),
    company: z.strictObject({
      name: z.string(),
      cvr: z.string().nullable(),
      country: z.string(),
      currency: z.string(),
      fiscalYearStartMonth: z.union([z.number(), z.string()]),
      fiscalYearLabelStrategy: z.string(),
    }),
    fiscalYears: z.array(
      z.strictObject({
        label: z.string(),
        start: z.string().nullable(),
        end: z.string().nullable(),
        source: z.enum(["live", "archive"]),
      }),
    ),
    periodStart: z.string(),
    periodEnd: z.string(),
    rows: z.array(row.extend({ type: z.union([accountType, z.literal("")]) })),
    totalDebit: money,
    totalCredit: money,
    balanced: z.boolean(),
  }),
});
const httpError = z.strictObject({
  ok: z.literal(false),
  errors: z.array(z.string()).min(1),
  code: z.enum([
    "bad_request",
    "not_found",
    "conflict",
    "unauthorized",
    "method_not_allowed",
    "internal",
  ]),
  subcode: z.string().optional(),
});
const rustReport = z.strictObject({
  ok: z.literal(true),
  format: z.literal("rentemester-trial-balance/1"),
  data: coreReport.extend({
    accounts: z.array(
      row.extend({
        normalBalance: z.enum(["debit", "credit"]),
        debit: exactMoney,
        credit: exactMoney,
        balance: exactMoney,
      }),
    ),
    totalDebit: exactMoney,
    totalCredit: exactMoney,
  }),
});
const receipts: { id: string; [key: string]: unknown }[] = [];
let sequence = 0;
function inventory(dir: string): Record<string, string> {
  return Object.fromEntries(
    readdirSync(dir, { recursive: true, withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => {
        const file = join(entry.parentPath, entry.name);
        return [
          file.slice(dir.length + 1),
          createHash("sha256").update(readFileSync(file)).digest("hex"),
        ];
      })
      .sort(([a], [b]) => a.localeCompare(b)),
  );
}
function copy() {
  const target = join(temp, `run-${sequence++}`);
  cpSync(baseline, target, { recursive: true });
  return target;
}
function cli(args: string[], dates = ["2026-01-01", "2026-12-31"]) {
  const workspace = copy();
  const env = Object.fromEntries(
    Object.entries(process.env).filter(
      ([key]) => !/^(RENTEMESTER_|OPENCLAW_)/.test(key),
    ),
  );
  const result = Bun.spawnSync(
    [
      process.execPath,
      "run",
      "src/cli.ts",
      "report",
      "trial-balance",
      "--company",
      join(workspace, "alpha"),
      "--from",
      dates[0],
      "--to",
      dates[1],
      ...args,
    ],
    { cwd: root, env, stdout: "pipe", stderr: "pipe" },
  );
  return {
    exit: result.exitCode,
    stdout: result.stdout.toString(),
    stderr: result.stderr.toString(),
  };
}
async function http(path: string, auth = false, method = "GET") {
  const workspace = copy();
  const response = await handleRequest(
    new Request(`http://localhost${path}`, { method }),
    {
      workspaceRoot: workspace,
      host: "127.0.0.1",
      port: 0,
      authRequired: auth,
      authToken: auth ? "synthetic-contract-token" : null,
    },
  );
  const bytes = Buffer.from(await response.arrayBuffer());
  return {
    status: response.status,
    headers: Object.fromEntries(response.headers),
    bytes,
  };
}
function normalize(report: z.infer<typeof coreReport>) {
  const amount = (v: number) => v.toFixed(2); // Fixture values are exactly representable cents; no general oracle conversion.
  return {
    ...report,
    totalDebit: amount(report.totalDebit),
    totalCredit: amount(report.totalCredit),
    accounts: report.accounts.map((a) => ({
      ...a,
      debit: amount(a.debit),
      credit: amount(a.credit),
      balance: amount(a.balance),
    })),
  };
}
try {
  initWorkspace(seed);
  for (const [slug, amount] of [
    ["alpha", 12.34],
    ["beta", 98.76],
    ["span", 5.67],
  ] as const) {
    const company = createCompany(seed, {
      name: `Synthetic ${slug}`,
      slug,
      ...(slug === "span"
        ? { fiscalYearStartMonth: 7, fiscalYearLabelStrategy: "span" }
        : {}),
    });
    const db = openDb(company.dbPath);
    try {
      db.run("UPDATE accounts SET name=? WHERE account_no='1100'", [
        'Synthetic; "debit"',
      ]);
      db.run(
        "INSERT INTO journal_entries(entry_no,transaction_date,text,rule_version,entry_hash) VALUES ('S1','2026-06-15','Synthetic contract fixture','synthetic','synthetic')",
      );
      const id = (
        db
          .query("SELECT id FROM journal_entries WHERE entry_no='S1'")
          .get() as { id: number }
      ).id;
      for (const [no, debit, credit] of [
        ["1100", amount, 0],
        ["2000", 0, amount],
      ] as const)
        db.run(
          "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) SELECT ?,id,?,? FROM accounts WHERE account_no=?",
          [id, debit, credit, no],
        );
      db.run(
        "INSERT INTO import_archive_years(source_system,fiscal_year,posting_count,balance_count) VALUES ('dinero',2024,0,2)",
      );
      const year = (
        db
          .query("SELECT id FROM import_archive_years WHERE fiscal_year=2024")
          .get() as { id: number }
      ).id;
      for (const [line, no, value] of [
        [0, "1100", 7.89],
        [1, "2000", -7.89],
      ] as const)
        db.run(
          "INSERT INTO import_archive_balances(archive_year_id,line_no,account_no,account_name,amount) VALUES (?,?,?,?,?)",
          [year, line, no, `Synthetic ${no}`, value],
        );
    } finally {
      db.close(true);
    }
  }
  // Legacy initializers can retain prepared statements until GC; never freeze their live directory as the oracle.
  Bun.gc(true);
  cpSync(seed, baseline, { recursive: true });
  const sourceBefore = inventory(baseline);
  const json = cli(["--format", "json"]);
  assert.equal(json.exit, 0);
  assert.equal(json.stderr, "");
  const parsed = coreReport.parse(JSON.parse(json.stdout));
  assert.equal(
    json.stdout,
    JSON.stringify(JSON.parse(json.stdout), null, 2) + "\n",
  );
  assert.deepEqual(
    {
      debit: parsed.totalDebit,
      credit: parsed.totalCredit,
      lines: parsed.linesConsidered,
      entries: parsed.journalEntryCount,
    },
    { debit: 12.34, credit: 12.34, lines: 2, entries: 1 },
  );
  assert.equal(parsed.ok, true);
  assert.equal(parsed.balanced, true);
  assert.deepEqual(parsed.errors, []);
  assert.deepEqual(parsed.appliedRules, ["financial-statements:trial-balance"]);
  receipts.push({ id: "cli.json.success", ...json });
  assert.deepEqual(cli(["--json"]), json);
  receipts.push({ id: "cli.json.alias", equalTo: "cli.json.success" });
  assert.deepEqual(cli([]), json);
  receipts.push({ id: "cli.pipe-default-json", equalTo: "cli.json.success" });
  const duplicate = cli(["--format", "json", "--from=2026-07-01"]);
  assert.equal(duplicate.exit, 0);
  assert.equal(duplicate.stderr, "");
  const duplicateBody = coreReport.parse(JSON.parse(duplicate.stdout));
  assert.equal(duplicateBody.periodStart, "2026-07-01");
  assert.equal(duplicateBody.totalDebit, 0);
  receipts.push({
    id: "cli.duplicate-attached-flag",
    ...duplicate,
    classification:
      "legacy last value wins; new Rust version refuses duplicates/attached syntax",
  });
  const human = cli(["--format", "human"]);
  assert.equal(human.exit, 0);
  assert.equal(human.stderr, "");
  assert.match(human.stdout, /Saldobalance/);
  receipts.push({ id: "cli.human.success", ...human });
  for (const [id, args, dates, expectedError] of [
    [
      "inverted",
      [],
      ["2026-12-31", "2026-01-01"],
      "periodStart must be before or equal to periodEnd",
    ],
    [
      "invalid-date",
      [],
      ["2026-02-30", "2026-12-31"],
      "periodStart must be YYYY-MM-DD",
    ],
  ] as [string, string[], string[], string][]) {
    const result = cli(["--format", "json", ...args], dates);
    assert.equal(result.exit, 1);
    assert.equal(result.stderr, "");
    const refusal = coreReport.parse(JSON.parse(result.stdout));
    assert.equal(refusal.ok, false);
    assert.deepEqual(refusal.errors, [expectedError]);
    assert.equal(refusal.totalDebit, 0);
    assert.deepEqual(refusal.accounts, []);
    receipts.push({ id: `cli.${id}`, ...result });
  }
  for (const args of [
    ["--unknown", "x"],
    ["--format", "invalid"],
  ]) {
    const result = cli(args);
    assert.equal(result.exit, 2);
    assert.equal(result.stdout, "");
    assert.equal(
      result.stderr,
      args[0] === "--unknown"
        ? "Unknown flag --unknown for report trial-balance.\n"
        : "--format must be either json or human\n",
    );
    receipts.push({ id: `cli.usage.${args[0]}`, ...result });
  }
  // Missing --to runs in its own process, without any possible production default company.
  const missing = Bun.spawnSync(
    [
      process.execPath,
      "run",
      join(root, "src/cli.ts"),
      "report",
      "trial-balance",
      "--company",
      join(copy(), "alpha"),
      "--from",
      "2026-01-01",
      "--format",
      "json",
    ],
    { cwd: temp, stdout: "pipe", stderr: "pipe" },
  );
  assert.equal(missing.exitCode, 2);
  assert.equal(missing.stdout.length, 0);
  assert.equal(
    missing.stderr.toString(),
    "Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>\n",
  );
  receipts.push({
    id: "cli.missing-to",
    exit: missing.exitCode,
    stdout: missing.stdout.toString(),
    stderr: missing.stderr.toString(),
  });
  const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
  const build = Bun.spawnSync(
    [cargo, "build", "--locked", "-p", "rentemester-cli"],
    { cwd: root, stdout: "pipe", stderr: "pipe" },
  );
  assert.equal(build.exitCode, 0, build.stderr.toString());
  const rust = Bun.spawnSync(
    [
      join(root, "target/debug/rentemester-rust"),
      "trial-balance",
      "--company",
      join(copy(), "alpha"),
      "--from",
      "2026-01-01",
      "--to",
      "2026-12-31",
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  assert.equal(rust.exitCode, 0);
  assert.equal(rust.stderr.length, 0);
  const rustParsed = rustReport.parse(JSON.parse(rust.stdout.toString()));
  assert.deepEqual(rustParsed.data, normalize(parsed));
  receipts.push({
    id: "rust.versioned-cli",
    exit: rust.exitCode,
    stdout: rust.stdout.toString(),
    stderr: "",
  });
  for (const [id, path, amount, archived] of [
    ["live", "alpha/trial-balance?year=2026", 12.34, false],
    ["default-year", "alpha/trial-balance", 12.34, false],
    ["beta", "beta/trial-balance?year=2026", 98.76, false],
    ["archive", "alpha/trial-balance?year=2024", 7.89, true],
    ["empty", "alpha/trial-balance?year=2023", 0, false],
  ] as const) {
    const result = await http(`/api/companies/${path}`);
    assert.equal(result.status, 200);
    assert.equal(
      result.headers["content-type"],
      "application/json; charset=utf-8",
    );
    const body = httpReport.parse(JSON.parse(result.bytes.toString()));
    assert.equal(body.trialBalance.totalDebit, amount);
    assert.equal(body.trialBalance.totalCredit, amount);
    assert.equal(body.trialBalance.archived, archived);
    assert.equal(body.trialBalance.archivedSource, archived ? "dinero" : null);
    assert.equal(body.trialBalance.company.cvr, null);
    if (id === "live")
      assert.deepEqual(
        body.trialBalance.rows,
        parsed.accounts.map(({ normalBalance: _, ...rest }) => rest),
      );
    receipts.push({
      id: `http.${id}`,
      status: result.status,
      headers: result.headers,
      body,
    });
  }
  for (const [id, path, auth, status, code] of [
    ["bad-year", "alpha/trial-balance?year=20xx", false, 400, "bad_request"],
    [
      "unknown-company",
      "ghost/trial-balance?year=2026",
      false,
      404,
      "not_found",
    ],
    [
      "unauthenticated",
      "alpha/trial-balance?year=2026",
      true,
      401,
      "unauthorized",
    ],
    [
      "invalid-export",
      "alpha/trial-balance/export?year=2026&format=xlsx",
      false,
      400,
      "bad_request",
    ],
  ] as const) {
    const result = await http(`/api/companies/${path}`, auth);
    assert.equal(result.status, status);
    const body = httpError.parse(JSON.parse(result.bytes.toString()));
    assert.equal(body.code, code);
    assert(!result.bytes.toString().includes(temp));
    receipts.push({
      id: `http.${id}`,
      status: result.status,
      headers: result.headers,
      body,
    });
  }
  const paddedYear = await http("/api/companies/alpha/trial-balance?year=0001");
  assert.equal(paddedYear.status, 200);
  const paddedBody = httpReport.parse(JSON.parse(paddedYear.bytes.toString()));
  assert.equal(paddedBody.trialBalance.periodStart, "1-01-01");
  assert.equal(paddedBody.trialBalance.totalDebit, 0);
  assert.equal(paddedBody.trialBalance.balanced, false);
  receipts.push({
    id: "legacy-defect.padded-year",
    body: paddedBody,
    correctedExpectation:
      "Keep canonical year padding or explicitly refuse; never report invalid core period as successful empty data",
  });
  const span = await http("/api/companies/span/trial-balance");
  assert.equal(span.status, 200);
  const spanBody = httpReport.parse(JSON.parse(span.bytes.toString()));
  assert.equal(spanBody.trialBalance.selectedYear, "2025-26");
  assert.equal(spanBody.trialBalance.periodStart, "2025-01-01");
  assert.equal(spanBody.trialBalance.totalDebit, 0);
  receipts.push({
    id: "legacy-defect.span-fiscal-year",
    body: spanBody,
    correctedExpectation:
      "Selected fiscal interval 2025-07-01..2026-06-30 includes synthetic posting 5.67; owner R42/R46 must refine correction before cutover",
  });
  const method = await http(
    "/api/companies/alpha/trial-balance?year=2026",
    false,
    "POST",
  );
  assert.equal(method.status, 405);
  const methodBody = httpError.parse(JSON.parse(method.bytes.toString()));
  assert.equal(methodBody.code, "method_not_allowed");
  receipts.push({
    id: "http.wrong-method",
    status: method.status,
    body: methodBody,
  });
  const csv = await http(
    "/api/companies/alpha/trial-balance/export?year=2026&format=csv",
  );
  assert.equal(csv.status, 200);
  assert.equal(csv.headers["content-type"], "text/csv; charset=utf-8");
  assert.equal(csv.headers["cache-control"], "private, no-store");
  assert.equal(csv.headers["x-content-type-options"], "nosniff");
  assert.equal(
    csv.headers["content-disposition"],
    "attachment; filename*=UTF-8''saldobalance-alpha-2026.csv",
  );
  const csvText = csv.bytes.toString();
  assert(csvText.startsWith("\uFEFFsep=;\r\n"));
  assert(!/(?<!\r)\n/.test(csvText));
  assert(
    csvText.includes('1100;"Synthetic; ""debit""";asset;12,34;0,00;12,34\r\n'),
  );
  assert.match(csvText, /Udtrukket;\d{4}-\d{2}-\d{2}\r\n/);
  receipts.push({
    id: "http.csv",
    status: csv.status,
    headers: csv.headers,
    body: csvText,
    nondeterministicField: "Udtrukket (UTC wall-clock date)",
  });
  const defaultCsv = await http(
    "/api/companies/alpha/trial-balance/export?year=2026",
  );
  assert.equal(defaultCsv.status, 200);
  assert.equal(defaultCsv.headers["content-type"], csv.headers["content-type"]);
  const caseCsv = await http(
    "/api/companies/alpha/trial-balance/export?year=2026&format=CSV",
  );
  assert.equal(caseCsv.status, 200);
  assert.equal(caseCsv.headers["content-type"], csv.headers["content-type"]);
  receipts.push({
    id: "http.csv-format-default-and-case",
    status: 200,
    contentType: csv.headers["content-type"],
  });
  const pdf = await http(
    "/api/companies/alpha/trial-balance/export?year=2026&format=pdf",
  );
  assert.equal(pdf.status, 200);
  assert.equal(pdf.headers["content-type"], "application/pdf");
  assert.equal(pdf.bytes.subarray(0, 5).toString(), "%PDF-");
  assert.equal(pdf.headers["cache-control"], "private, no-store");
  assert.equal(pdf.headers["x-content-type-options"], "nosniff");
  assert.match(
    pdf.headers["content-disposition"],
    /saldobalance-alpha-2026\.pdf/,
  );
  receipts.push({
    id: "http.pdf-format",
    status: pdf.status,
    headers: pdf.headers,
    bytes: pdf.bytes.length,
    sha256: createHash("sha256").update(pdf.bytes).digest("hex"),
    limit:
      "Format/header contract only; PDF amount rendering remains existing statement PDF suite coverage",
  });
  const exportWorkspace = copy();
  const first = exportTrialBalanceCsv(exportWorkspace, "alpha", 2026, {
    generatedAtIsoDate: "2026-01-01",
  });
  const expectedCsv =
    "\uFEFF" +
    [
      "sep=;",
      "Rapport;Saldobalance",
      "Virksomhed;Synthetic alpha",
      "Regnskabsår;2026",
      "Valuta;DKK",
      "Udtrukket;2026-01-01",
      "Kilde;Rentemester",
      "",
      "Konto;Navn;Type;Debet;Kredit;Saldo",
      '1100;"Synthetic; ""debit""";asset;12,34;0,00;12,34',
      "2000;Bank;asset;0,00;12,34;-12,34",
      ";I alt;;12,34;12,34;",
      "",
    ].join("\r\n");
  assert.equal(first.content, expectedCsv);
  const fixedHttpCsv = csvText.replace(
    /^Udtrukket;\d{4}-\d{2}-\d{2}\r$/m,
    "Udtrukket;2026-01-01\r",
  );
  assert.equal(fixedHttpCsv, expectedCsv);
  assert.throws(() =>
    assert.equal(
      expectedCsv.replace(";I alt;;12,34;12,34;\r\n", ""),
      expectedCsv,
    ),
  );
  assert.throws(() =>
    assert.equal(
      expectedCsv.replace("Konto;Navn;Type;Debet;Kredit;Saldo\r\n", ""),
      expectedCsv,
    ),
  );
  receipts.push({
    id: "csv.full-byte-oracle",
    body: expectedCsv,
    httpNormalization: "Only the named Udtrukket date line",
    negativeControls: ["omitted-total-row", "omitted-column-header"],
  });
  const second = exportTrialBalanceCsv(exportWorkspace, "alpha", 2026, {
    generatedAtIsoDate: "2026-01-02",
  });
  assert.notEqual(first.content, second.content);
  assert.equal(
    first.content.replace("Udtrukket;2026-01-01", "Udtrukket;2026-01-02"),
    second.content,
  );
  receipts.push({
    id: "csv.clock-dependence",
    changedFieldOnly: "Udtrukket",
    status: "documented nondeterminism; no blind whole-byte normalization",
  });
  const tools: string[] = [];
  registerAllTools({
    registerTool(name: string) {
      tools.push(name);
      return {};
    },
  } as never);
  assert(!tools.some((name) => /trial.?balance|saldobalance/.test(name)));
  assert(
    readFileSync(join(root, "docs/mcp-tool-surface.md"), "utf8").includes(
      "`report trial-balance`",
    ),
  );
  receipts.push({
    id: "mcp.absence",
    status: "CLI-only financial report, no corresponding registered MCP tool",
    registeredToolCount: tools.length,
  });
  // Comparator/schema negative controls must fail on a wrong number, omitted row, changed casing/nullability and string-vs-number money.
  assert.throws(() =>
    assert.deepEqual(
      { ...rustParsed.data, totalDebit: "0.00" },
      normalize(parsed),
    ),
  );
  assert.throws(() =>
    assert.deepEqual({ ...rustParsed.data, accounts: [] }, normalize(parsed)),
  );
  assert.throws(() =>
    coreReport.parse({
      ...parsed,
      TotalDebit: parsed.totalDebit,
      totalDebit: undefined,
    }),
  );
  assert.throws(() => coreReport.parse({ ...parsed, totalDebit: null }));
  assert.throws(() => coreReport.parse({ ...parsed, totalDebit: "12.34" }));
  receipts.push({
    id: "negative-controls",
    rejected: [
      "wrong-total",
      "omitted-account",
      "changed-casing",
      "null-money",
      "string-in-legacy-numeric-wire",
    ],
  });
  assert.deepEqual(inventory(baseline), sourceBefore);
  receipts.push({
    id: "oracle-immutable",
    filesChecked: Object.keys(sourceBefore).length,
    policy:
      "Every legacy/Rust execution uses a disposable synthetic copy; no claim legacy adapters are themselves read-only",
  });
  const sources = [
    "scripts/rust/verify-report-contracts.ts",
    "scripts/rust/verify.ts",
    "src/cli.ts",
    "src/cli/report.ts",
    "src/cli-format/report.ts",
    "src/cli-format/common.ts",
    "src/core/company.ts",
    "src/core/workspace.ts",
    "src/core/db.ts",
    "src/core/financial-statements.ts",
    "src/core/dates.ts",
    "src/core/money.ts",
    "src/server/router.ts",
    "src/server/router/statements.ts",
    "src/server/router/_shared.ts",
    "src/server/errors.ts",
    "src/server/data/shared.ts",
    "src/server/data/statements/trial-balance.ts",
    "src/server/data/statement-exports.ts",
    "src/server/data/statement-pdf.ts",
    "src/mcp/registry.ts",
    "docs/mcp-tool-surface.md",
    "Cargo.lock",
    "crates/rentemester-cli/src/main.rs",
    "crates/rentemester-engine/src/trial_balance.rs",
    "crates/rentemester-engine/src/money.rs",
    "crates/rentemester-storage/src/lib.rs",
    "crates/rentemester-storage/src/snapshot.rs",
    "crates/rentemester-storage/src/legacy-schema.json",
  ];
  console.log(
    JSON.stringify(
      {
        format: "rentemester-trial-balance-wire-baseline/1",
        ok: true,
        head: Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root })
          .stdout.toString()
          .trim(),
        sources: sources.map((path) => ({
          path,
          sha256: createHash("sha256")
            .update(readFileSync(join(root, path)))
            .digest("hex"),
        })),
        schemas: Object.fromEntries(
          Object.entries({
            legacyCli: coreReport,
            legacyHttp: httpReport,
            legacyHttpError: httpError,
            rustCli: rustReport,
          }).map(([key, schema]) => [key, z.toJSONSchema(schema)]),
        ),
        checks: receipts.length,
        receipts,
        limits: [
          "One report capability only; R01/R08 remain open",
          "Source hashes bind named anchors, not an exhaustive transitive dependency closure",
          "HTTP calls real router in-process; no socket/proxy/session/hosted-membership qualification",
          "PDF amounts, complete custom fiscal-label matrix, malformed storage and transport loss remain separate coverage",
          "Registry absence plus explicit CLI-only documentation; no semantically equivalent differently named MCP operation claimed",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(temp, { recursive: true, force: true });
}
