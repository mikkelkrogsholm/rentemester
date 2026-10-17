/** Synthetic full-schema integration checks. Never opens a real workspace. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { buildTrialBalance } from "../../src/core/financial-statements";
import { formatAmount } from "../../src/core/money";

const root = resolve(import.meta.dir, "../..");
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo", "bin", "cargo");
const build = Bun.spawnSync(
  [cargo, "build", "--locked", "-p", "rentemester-cli"],
  { cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust");
const temp = mkdtempSync(join(tmpdir(), "rentemester-rust-parity-"));
const checks: string[] = [];
let sequence = 0;
function inventory(dir: string, prefix = ""): Record<string, string> {
  const result: Record<string, string> = {};
  for (const ent of readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
    a.name.localeCompare(b.name),
  )) {
    const key = prefix + ent.name;
    if (ent.isDirectory())
      Object.assign(result, inventory(join(dir, ent.name), `${key}/`));
    else if (ent.isSymbolicLink()) result[key] = "symbolic-link";
    else
      result[key] = createHash("sha256")
        .update(readFileSync(join(dir, ent.name)))
        .digest("hex");
  }
  return result;
}
function invoke(
  company: string,
  from = "2026-01-01",
  to = "2026-12-31",
  extra: string[] = [],
): { status: number; value: any } {
  const p = Bun.spawnSync(
    [
      binary,
      "trial-balance",
      "--company",
      company,
      "--from",
      from,
      "--to",
      to,
      ...extra,
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  assert.equal(p.stderr.length, 0, p.stderr.toString());
  const text = p.stdout.toString();
  assert.equal(text.trim().split("\n").length, 1);
  return { status: p.exitCode, value: JSON.parse(text) };
}
const first = "𐀀",
  second = "\uE000"; // JS UTF-16 sorts these opposite to SQLite UTF-8.
function fixture(name: string, amount = 1.005) {
  const company = join(temp, `${sequence++}-${name}`);
  mkdirSync(join(company, "data"), { recursive: true });
  const db = new Database(join(company, "data/ledger.sqlite"));
  migrate(db);
  db.exec(
    "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0",
  );
  db.run(
    "INSERT INTO accounts(account_no,name,type,normal_balance) VALUES (?,?,'asset','debit')",
    [first, "Synthetic debit"],
  );
  db.run(
    "INSERT INTO accounts(account_no,name,type,normal_balance) VALUES (?,?,'liability','credit')",
    [second, "Synthetic credit"],
  );
  const account = (no: string) =>
    (
      db.query("SELECT id FROM accounts WHERE account_no=?").get(no) as {
        id: number;
      }
    ).id;
  const a = account(first),
    b = account(second);
  const entry = (
    no: string,
    date: string,
    lines: [number, number, number][],
    reverse?: number,
  ) => {
    const id = Number(
      (
        db
          .query("SELECT COALESCE(MAX(id),0)+1 AS id FROM journal_entries")
          .get() as { id: number }
      ).id,
    );
    db.run(
      "INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash,status,reversal_of_entry_id) VALUES (?,?,?,?,'synthetic-v1',?,?,?)",
      [
        id,
        no,
        date,
        "Synthetic parity fixture",
        no,
        reverse ? "reversed" : "posted",
        reverse ?? null,
      ],
    );
    for (const [account, debit, credit] of lines)
      db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (?,?,?,?)",
        [id, account, debit, credit],
      );
    return id;
  };
  const sum = Number(formatAmount(amount)) * 2;
  const original = entry("S1", "2026-01-01", [
    [a, amount, 0],
    [a, amount, 0],
    [b, 0, sum],
  ]);
  entry("S2", "2026-02-01", [
    [a, 1.0049999999999997, 0],
    [b, 0, 1.0049999999999997],
  ]);
  entry(
    "S3",
    "2026-02-28",
    [
      [a, 0, amount],
      [a, 0, amount],
      [b, sum, 0],
    ],
    original,
  );
  entry("S4", "2025-12-31", [
    [a, 4.13, 0],
    [b, 0, 4.13],
  ]);
  entry("S5", "2027-01-01", [
    [a, 8.23, 0],
    [b, 0, 8.23],
  ]);
  return { company, db, a, b, entry };
}
function normalize(report: any): any {
  return {
    ...report,
    accounts: report.accounts.map((a: any) => ({
      ...a,
      debit: formatAmount(a.debit),
      credit: formatAmount(a.credit),
      balance: formatAmount(a.balance),
    })),
    totalDebit: formatAmount(report.totalDebit),
    totalCredit: formatAmount(report.totalCredit),
  };
}
function same(left: any, right: any) {
  assert.deepEqual(left, right);
}
try {
  const alpha = fixture("alpha"),
    beta = fixture("beta", 9.005);
  for (const [from, to] of [
    ["2026-01-01", "2026-01-31"],
    ["2026-01-01", "2026-02-28"],
    ["2026-02-01", "2026-02-01"],
    ["2026-03-01", "2026-03-01"],
    ["0000-01-01", "9999-12-31"],
  ]) {
    const legacy = normalize(buildTrialBalance(alpha.db, from, to));
    const before = inventory(alpha.company);
    const rust = invoke(alpha.company, from, to);
    assert.equal(rust.status, 0, JSON.stringify(rust));
    assert.equal(rust.value.format, "rentemester-trial-balance/1");
    same(rust.value.data, legacy);
    same(inventory(alpha.company), before);
    checks.push(
      `full-schema WAL differential, source unchanged: ${from}/${to}`,
    );
  }
  const jan = invoke(alpha.company, "2026-01-01", "2026-01-31").value.data;
  assert.equal(jan.totalDebit, "2.02");
  assert.equal(jan.totalCredit, "2.02");
  assert.equal(jan.journalEntryCount, 1);
  assert.equal(jan.linesConsidered, 3);
  assert.equal(jan.accounts[0].accountNo, first);
  assert.equal(jan.accounts[0].balance, "2.02");
  checks.push(
    "independent line rounding, inclusive period, UTF-16 order and counts",
  );
  const both = invoke(alpha.company, "2026-01-01", "2026-02-28").value.data;
  assert.equal(both.totalDebit, "5.04");
  assert.equal(both.accounts[0].balance, "1.00");
  assert.equal(both.linesConsidered, 8);
  checks.push("independent reversal and cross-period totals");
  const other = invoke(beta.company, "2026-01-01", "2026-01-31").value.data;
  assert.equal(other.totalDebit, "18.02");
  assert.notDeepEqual(other, jan);
  checks.push("explicit company isolation on two synthetic ledgers");
  assert.throws(() => same({ ...jan, totalDebit: "999.00" }, jan));
  assert.throws(() => same({ ...jan, linesConsidered: 0 }, jan));
  checks.push(
    "comparison negative controls reject wrong amount and missing rows",
  );
  const repeated = invoke(alpha.company, "2026-01-01", "2026-01-31");
  same(repeated.value.data, jan);
  checks.push("repeat deterministic read");
  for (const [from, to, extra] of [
    ["2026-02-30", "2026-12-31", []],
    ["2026-12-31", "2026-01-01", []],
    [" 2026-01-01", "2026-12-31", []],
    ["2026-01-01", "2026-12-31", ["--company", beta.company]],
    ["2026-01-01", "2026-12-31", ["--unknown", "x"]],
  ] as [string, string, string[]][]) {
    const before = inventory(alpha.company);
    const r = invoke(alpha.company, from, to, extra);
    assert.equal(r.status, 2);
    assert.equal(r.value.ok, false);
    same(inventory(alpha.company), before);
    checks.push(`usage refuses without writes: ${from} ${extra.join(" ")}`);
  }
  const missing = join(temp, "must-not-create");
  assert.equal(invoke(missing).status, 1);
  assert.equal(existsSync(missing), false);
  checks.push("missing company refused without implicit creation");
  const linked = join(temp, "linked-company");
  symlinkSync(alpha.company, linked);
  assert.equal(invoke(linked).status, 1);
  checks.push("symlink company denied");
  assert.equal(invoke(linked + "/").status, 1);
  assert.equal(invoke(linked + "/./").status, 1);
  checks.push("trailing slash/dot cannot bypass symlink refusal");
  const escape = join(temp, "escape");
  mkdirSync(escape);
  symlinkSync(join(alpha.company, "data"), join(escape, "data"));
  assert.equal(invoke(escape).status, 1);
  checks.push("data-directory escape denied");
  for (const kind of [
    "newer",
    "pending",
    "checksum",
    "gap",
    "foreign-key",
    "invalid-type",
    "unsafe-integer",
    "unsafe-entry-id",
    "infinity",
    "malformed-amount",
    "symlink-db",
    "symlink-wal",
    "corrupt-db",
  ]) {
    const f = fixture(kind);
    f.db.exec("PRAGMA wal_checkpoint(TRUNCATE)");
    if (["newer", "pending", "checksum", "gap"].includes(kind)) {
      const triggers = f.db
        .query(
          "SELECT name FROM sqlite_schema WHERE type='trigger' AND tbl_name='schema_migrations'",
        )
        .all() as { name: string }[];
      for (const t of triggers)
        f.db.exec(`DROP TRIGGER "${t.name.replaceAll('"', '""')}"`);
      if (kind === "newer")
        f.db.run(
          "INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES (37,'future','unknown','future')",
        );
      if (kind === "pending")
        f.db.run("DELETE FROM schema_migrations WHERE id=36");
      if (kind === "checksum")
        f.db.run("UPDATE schema_migrations SET checksum='tampered' WHERE id=1");
      if (kind === "gap") f.db.run("DELETE FROM schema_migrations WHERE id=4");
    } else if (kind === "foreign-key") {
      f.db.exec("PRAGMA foreign_keys=OFF");
      f.db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (1,999999,1,0)",
      );
    } else if (kind === "invalid-type") {
      f.db.exec("PRAGMA ignore_check_constraints=ON");
      f.db.run("UPDATE accounts SET type='unknown' WHERE id=?", [f.a]);
    } else if (kind === "unsafe-integer")
      f.db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (1,?,9007199254740993,0)",
        [f.a],
      );
    else if (kind === "unsafe-entry-id") {
      f.db.run(
        "INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES (9007199254740993,'unsafe','2026-01-01','synthetic','synthetic','unsafe')",
      );
      f.db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (9007199254740993,?,1,0)",
        [f.a],
      );
    } else if (kind === "infinity")
      f.db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (1,?,1e999,0)",
        [f.a],
      );
    else if (kind === "malformed-amount")
      f.db.run(
        "INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES (1,?,'not-money',0)",
        [f.a],
      );
    f.db.close();
    const file = join(f.company, "data/ledger.sqlite");
    if (kind === "symlink-db") {
      rmSync(file);
      symlinkSync(join(alpha.company, "data/ledger.sqlite"), file);
    }
    if (kind === "symlink-wal") {
      rmSync(file + "-wal", { force: true });
      symlinkSync(join(alpha.company, "data/ledger.sqlite-wal"), file + "-wal");
    }
    if (kind === "corrupt-db") writeFileSync(file, "not a database");
    const before = inventory(f.company);
    const result = invoke(f.company);
    assert.equal(result.status, 1, kind + JSON.stringify(result));
    assert.equal(result.value.ok, false);
    same(inventory(f.company), before);
    checks.push(`fail closed, unchanged source: ${kind}`);
  }
  const duplicate = join(temp, "independent-copy");
  cpSync(alpha.company, duplicate, { recursive: true });
  const originalBefore = inventory(alpha.company);
  same(invoke(duplicate, "2026-01-01", "2026-01-31").value.data, jan);
  same(inventory(alpha.company), originalBefore);
  checks.push("independent copied-snapshot recovery leaves original unchanged");
  const concurrent = fixture("concurrent");
  // A real SQLite writer runs while a separate Rust process captures DB/WAL/SHM.
  const process = Bun.spawn(
    [
      binary,
      "trial-balance",
      "--company",
      concurrent.company,
      "--from",
      "2026-01-01",
      "--to",
      "2026-12-31",
    ],
    { stdout: "pipe", stderr: "pipe" },
  );
  const writer = (async () => {
    for (let n = 0; n < 40; n++) {
      concurrent.db.transaction(() => {
        concurrent.entry(`W${n}`, "2026-06-01", [
          [concurrent.a, 0.01, 0],
          [concurrent.b, 0, 0.01],
        ]);
      })();
      await Bun.sleep(1);
    }
  })();
  const [exit, text, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    writer,
  ]);
  assert.equal(stderr, "");
  const observed = JSON.parse(text);
  if (exit === 0) {
    const n = observed.data.journalEntryCount - 3;
    assert(n >= 0 && n <= 40);
    assert.equal(observed.data.linesConsidered, 8 + 2 * n);
    assert.equal(observed.data.totalDebit, ((504 + n) / 100).toFixed(2));
    assert.equal(observed.data.balanced, true);
  } else {
    assert.equal(exit, 1);
    assert.equal(observed.code, "source_changed", text);
  }
  assert.equal(
    formatAmount(
      buildTrialBalance(concurrent.db, "2026-01-01", "2026-12-31").totalDebit,
    ),
    "5.44",
  );
  concurrent.db.close();
  checks.push(
    "actual concurrent filesystem/WAL capture yields coherent committed report or bounded refusal",
  );
  alpha.db.close();
  beta.db.close();
  console.log(
    JSON.stringify(
      {
        ok: true,
        scope: "synthetic current-schema read-only Rust trial-balance parity",
        checks: checks.length,
        passed: checks,
        limitations: [
          "No hosted authorization, full audit health, mutation, production migration or qualified Windows storage claimed.",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(temp, { recursive: true, force: true });
}
