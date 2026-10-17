/** Real synthetic legacy writer -> Rust hash compatibility; no production paths/credentials. */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { Database } from "bun:sqlite";
import { openDb, migrate } from "../../src/core/db";
import { ensureCompanyDirs } from "../../src/core/paths";
import {
  postJournalEntry,
  reverseJournalEntry,
  verifyAuditChain,
  hashEntry,
} from "../../src/core/ledger";
import { roundDkk } from "../../src/core/money";

const root = resolve(import.meta.dir, "../..");
const temp = mkdtempSync(join(tmpdir(), "rentemester-journal-hash-parity-"));
const cargo = Bun.which("cargo") ?? join(homedir(), ".cargo/bin/cargo");
const build = Bun.spawnSync(
  [cargo, "build", "--locked", "-p", "rentemester-cli"],
  { cwd: root, stdout: "pipe", stderr: "pipe" },
);
assert.equal(build.exitCode, 0, build.stderr.toString());
const binary = join(root, "target/debug/rentemester-rust");
const fixed = JSON.parse(
  readFileSync(
    join(
      root,
      "docs/architecture/rust-target/implementation/journal-hashes/vectors.json",
    ),
    "utf8",
  ),
);
const passed: string[] = [];
let sequence = 0;
function inventory(dir: string): Record<string, string> {
  return Object.fromEntries(
    readdirSync(dir, { recursive: true, withFileTypes: true })
      .filter((e) => e.isFile())
      .map((e) => {
        const path = join(e.parentPath, e.name);
        return [
          path.slice(dir.length + 1),
          createHash("sha256").update(readFileSync(path)).digest("hex"),
        ];
      })
      .sort(([a], [b]) => a.localeCompare(b)),
  );
}
function invoke(company: string, extra: string[] = []) {
  const p = Bun.spawnSync(
    [binary, "verify-journal-hashes", "--company", company, ...extra],
    { stdout: "pipe", stderr: "pipe" },
  );
  assert.equal(p.stderr.length, 0, p.stderr.toString());
  assert.equal(p.stdout.toString().trim().split("\n").length, 1);
  return { exit: p.exitCode, body: JSON.parse(p.stdout.toString()) };
}
function dropGuards(db: Database, table: string) {
  const guards = db
    .query("SELECT name FROM sqlite_schema WHERE type='trigger' AND tbl_name=?")
    .all(table) as { name: string }[];
  for (const g of guards)
    db.exec(`DROP TRIGGER "${g.name.replaceAll('"', '""')}"`);
}
const seed = join(temp, "seed");
const db = openDb(ensureCompanyDirs(seed).db);
let closed = false;
try {
  migrate(db);
  db.exec("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0");
  db.run(
    "INSERT INTO accounts(account_no,name,type,normal_balance) VALUES ('A','Synthetic asset','asset','debit'),('L','Synthetic liability','liability','credit')",
  );
  const input = (amount: number, text: string) => ({
    transactionDate: "2026-01-10",
    text,
    createdBy: " agent:synthetic ",
    createdByProgram: " fixture-hash ",
    lines: [
      { accountNo: "A", debitAmount: amount },
      { accountNo: "L", creditAmount: amount, text: "" },
    ],
  });
  const results = [
    postJournalEntry(db, input(1.005, "Minimal")),
    postJournalEntry(db, {
      ...input(
        1.0049999999999997,
        'Unicode æ🦀𐀀\uE000\u2028\u2029 e\u0301 é / " \\ \b \f \n \r \t \u0000',
      ),
      lines: [
        {
          accountNo: "A",
          debitAmount: 1.0049999999999997,
          text: "line\u0000text",
        },
        { accountNo: "L", creditAmount: 1.0049999999999997, text: " 🦀 " },
      ],
    }),
    postJournalEntry(db, {
      ...input(0.01, "Tiny FX"),
      currency: " eur ",
      amountForeign: 10000,
      amountDkk: 0.01,
      fxRateToDkk: 0.000001,
    }),
    postJournalEntry(db, input(1e21, "Huge amount")),
  ];
  for (const r of results) assert.equal(r.ok, true, JSON.stringify(r));
  const reversal = reverseJournalEntry(db, {
    entryId: results[1].entryId!,
    transactionDate: "2026-02-01",
    reason: " reason ",
    createdBy: "agent:synthetic",
    createdByProgram: "fixture-hash",
  });
  assert.equal(reversal.ok, true, JSON.stringify(reversal));
  results.push(reversal);
  assert.deepEqual(
    results.map((r) => r.entryHash),
    fixed.entries.map((e: any) => e.entryHash),
  );
  const audit = verifyAuditChain(db, { companyRoot: seed });
  assert.deepEqual(audit, { ok: true, entries: 5, errors: [] });
  passed.push(
    "real legacy posting and reversal pass full legacy audit and fixed five-hash oracle",
  );
  const entries = db
    .query("SELECT * FROM journal_entries ORDER BY id")
    .all() as any[];
  for (const [index, entry] of entries.entries()) {
    const lines = db
      .query(
        "SELECT a.account_no,jl.debit_amount,jl.credit_amount,jl.vat_code,jl.text FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=? ORDER BY jl.id",
      )
      .all(entry.id) as any[];
    const canonical = {
      id: entry.id,
      entry_no: entry.entry_no,
      transaction_date: entry.transaction_date,
      text: entry.text,
      source_bank_transaction_id: entry.source_bank_transaction_id ?? null,
      document_id: entry.document_id ?? null,
      currency: entry.currency ?? "DKK",
      amount_foreign: entry.amount_foreign ?? null,
      amount_dkk: entry.amount_dkk ?? null,
      fx_rate_to_dkk: entry.fx_rate_to_dkk ?? null,
      rule_version: entry.rule_version,
      created_by: entry.created_by,
      created_by_program: entry.created_by_program,
      status: entry.status,
      reversal_of_entry_id: entry.reversal_of_entry_id ?? null,
      lines: lines.map((line, ordinal) => ({
        ordinal,
        account_no: line.account_no,
        debit_amount: roundDkk(line.debit_amount),
        credit_amount: roundDkk(line.credit_amount),
        vat_code: line.vat_code ?? null,
        text: line.text ?? null,
      })),
    };
    assert.equal(JSON.stringify(canonical), fixed.entries[index].canonicalJson);
    assert.equal(hashEntry(canonical, entry.previous_hash), entry.entry_hash);
    assert.equal(
      createHash("sha256")
        .update(fixed.entries[index].canonicalJson)
        .update(entry.previous_hash)
        .digest("hex"),
      entry.entry_hash,
    );
  }
  passed.push(
    "persisted canonical JSON property/line order, null/empty, Unicode/controls/NUL, FX and huge-number bytes match saved vectors",
  );
  const beforeWal = inventory(seed);
  const valid = invoke(seed);
  assert.deepEqual(valid, {
    exit: 0,
    body: {
      ok: true,
      format: "rentemester-journal-hashes/1",
      data: { ok: true, entriesChecked: 5, errors: [] },
    },
  });
  assert.deepEqual(invoke(seed), valid);
  assert.deepEqual(inventory(seed), beforeWal);
  passed.push(
    "Rust verifies real live-WAL legacy chain twice without source byte changes",
  );
  assert.throws(() =>
    assert.deepEqual(
      { ...valid.body.data, entriesChecked: 4 },
      valid.body.data,
    ),
  );
  assert.notEqual(
    hashEntry({ ...fixed.entries[0].canonical, text: "wrong" }, "GENESIS"),
    fixed.entries[0].entryHash,
  );
  const { id: _, ...omitted } = fixed.entries[0].canonical;
  assert.notEqual(hashEntry(omitted, "GENESIS"), fixed.entries[0].entryHash);
  assert.notEqual(
    createHash("sha256")
      .update(fixed.entries[0].canonicalJson + "\nGENESIS")
      .digest("hex"),
    fixed.entries[0].entryHash,
  );
  assert.notEqual(
    hashEntry(
      {
        ...fixed.entries[1].canonical,
        text: fixed.entries[1].canonical.text.normalize("NFC"),
      },
      fixed.entries[1].previousHash,
    ),
    fixed.entries[1].entryHash,
  );
  passed.push(
    "negative controls reject omitted entry, changed bytes, omitted field and extra hash separator",
  );
  db.close(true);
  closed = true;
  const baseline = join(temp, "baseline");
  cpSync(seed, baseline, { recursive: true });
  const beforeFrozen = inventory(baseline);
  const copy = () => {
    const p = join(temp, `case-${sequence++}`);
    cpSync(baseline, p, { recursive: true });
    return p;
  };
  for (const kind of [
    "text",
    "actor",
    "fx",
    "line-amount",
    "line-vat",
    "null-empty",
    "line-order",
    "previous-null",
    "previous-wrong",
    "hash",
    "no-lines",
    "nonfinite",
    "unsafe-entry-id",
    "unsafe-reference",
    "text-number",
    "blob-text",
    "invalid-utf8",
    "hash-column-drift",
    "history-tamper",
  ]) {
    const company = copy();
    const altered = openDb(join(company, "data/ledger.sqlite"));
    try {
      for (const table of [
        "journal_entries",
        "journal_lines",
        "schema_migrations",
      ])
        dropGuards(altered, table);
      if (kind === "text")
        altered.run("UPDATE journal_entries SET text='changed' WHERE id=1");
      if (kind === "actor")
        altered.run(
          "UPDATE journal_entries SET created_by='agent:other' WHERE id=1",
        );
      if (kind === "fx")
        altered.run(
          "UPDATE journal_entries SET fx_rate_to_dkk=0.0000001 WHERE id=3",
        );
      if (kind === "line-amount")
        altered.run("UPDATE journal_lines SET debit_amount=1.02 WHERE id=1");
      if (kind === "line-vat")
        altered.run("UPDATE journal_lines SET vat_code='synthetic' WHERE id=1");
      if (kind === "null-empty")
        altered.run("UPDATE journal_lines SET text='' WHERE id=1");
      if (kind === "line-order")
        altered.exec(
          "UPDATE journal_lines SET id=-1 WHERE id=1; UPDATE journal_lines SET id=1 WHERE id=2; UPDATE journal_lines SET id=2 WHERE id=-1",
        );
      if (kind === "previous-null")
        altered.run("UPDATE journal_entries SET previous_hash=NULL WHERE id=1");
      if (kind === "previous-wrong")
        altered.run(
          "UPDATE journal_entries SET previous_hash='wrong' WHERE id=2",
        );
      if (kind === "hash")
        altered.run("UPDATE journal_entries SET entry_hash='wrong' WHERE id=1");
      if (kind === "no-lines")
        altered.run("DELETE FROM journal_lines WHERE journal_entry_id=1");
      if (kind === "nonfinite")
        altered.run("UPDATE journal_entries SET amount_dkk=1e999 WHERE id=3");
      if (kind === "unsafe-entry-id")
        altered.run(
          "INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES (9007199254740993,'unsafe','2026-01-01','synthetic','synthetic','synthetic')",
        );
      if (kind === "unsafe-reference")
        altered.run(
          "UPDATE journal_entries SET reversal_of_entry_id=9007199254740993 WHERE id=5",
        );
      if (kind === "text-number")
        altered.run(
          "UPDATE journal_entries SET amount_dkk='not-a-number' WHERE id=3",
        );
      if (kind === "blob-text")
        altered.run("UPDATE journal_lines SET text=X'80' WHERE id=1");
      if (kind === "invalid-utf8")
        altered.run(
          "UPDATE journal_entries SET text=CAST(X'80' AS TEXT) WHERE id=1",
        );
      if (kind === "hash-column-drift")
        altered.exec(
          "ALTER TABLE journal_entries RENAME COLUMN created_by TO missing_created_by",
        );
      if (kind === "history-tamper")
        altered.run(
          "UPDATE schema_migrations SET checksum='tampered' WHERE id=1",
        );
    } finally {
      altered.close(true);
    }
    const before = inventory(company);
    const result = invoke(company);
    assert.equal(result.exit, 1, kind + JSON.stringify(result));
    assert.equal(result.body.ok, false, kind);
    if (
      [
        "nonfinite",
        "unsafe-entry-id",
        "unsafe-reference",
        "text-number",
        "blob-text",
        "invalid-utf8",
      ].includes(kind)
    ) {
      assert.equal(result.body.code, "invalid_ledger", kind);
      assert.equal(result.body.data, undefined);
    } else if (["hash-column-drift", "history-tamper"].includes(kind))
      assert.equal(result.body.code, "unsupported_schema", kind);
    else {
      assert.equal(result.body.format, "rentemester-journal-hashes/1");
      assert.equal(result.body.data.ok, false);
      assert(result.body.data.errors.length > 0);
    }
    if (kind === "no-lines")
      assert(
        result.body.data.errors.includes(
          "2026-00001: entry has no journal lines",
        ),
      );
    if (kind === "previous-null")
      assert.deepEqual(result.body.data.errors, [
        "2026-00001: previous_hash mismatch",
      ]);
    assert.deepEqual(inventory(company), before, kind);
    passed.push(`corruption/refusal, unchanged source: ${kind}`);
  }
  const empty = join(temp, "empty");
  const emptyDb = openDb(ensureCompanyDirs(empty).db);
  migrate(emptyDb);
  emptyDb.close(true);
  assert.deepEqual(invoke(empty), {
    exit: 0,
    body: {
      ok: true,
      format: "rentemester-journal-hashes/1",
      data: { ok: true, entriesChecked: 0, errors: [] },
    },
  });
  passed.push("empty current-schema hash chain is explicitly scoped success");
  const missing = join(temp, "must-not-create");
  assert.equal(invoke(missing).body.code, "source_unavailable");
  assert.equal(existsSync(missing), false);
  passed.push("missing company refused without creation");
  const link = join(temp, "linked");
  symlinkSync(baseline, link);
  for (const path of [link, link + "/", link + "/./"])
    assert.equal(invoke(path).body.code, "source_unavailable");
  passed.push("root symlink and trailing slash/dot variants refused");
  const concurrent = copy();
  const writerDb = openDb(join(concurrent, "data/ledger.sqlite"));
  try {
    const child = Bun.spawn(
      [binary, "verify-journal-hashes", "--company", concurrent],
      { stdout: "pipe", stderr: "pipe" },
    );
    const writer = (async () => {
      for (let n = 0; n < 12; n++) {
        const posted = postJournalEntry(
          writerDb,
          input(0.01, `Concurrent synthetic ${n}`),
        );
        assert.equal(posted.ok, true, JSON.stringify(posted));
        await Bun.sleep(1);
      }
    })();
    const [exit, text, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      writer,
    ]);
    assert.equal(stderr, "");
    const observed = JSON.parse(text);
    if (exit === 0) {
      assert.equal(observed.ok, true);
      assert(
        observed.data.entriesChecked >= 5 && observed.data.entriesChecked <= 17,
      );
      assert.deepEqual(observed.data.errors, []);
    } else {
      assert.equal(exit, 1);
      assert.equal(observed.code, "source_changed");
    }
    assert.deepEqual(verifyAuditChain(writerDb, { companyRoot: concurrent }), {
      ok: true,
      entries: 17,
      errors: [],
    });
    assert.equal(invoke(concurrent).body.data.entriesChecked, 17);
    passed.push(
      "concurrent real legacy writer: coherent whole chain or bounded source_changed; all 12 commits retained and verified",
    );
  } finally {
    writerDb.close(true);
  }
  for (const args of [
    [],
    ["--company"],
    ["--company", ""],
    ["--unknown", seed],
    ["--company", seed, "--company", seed],
    ["--company", "--help"],
  ]) {
    const proc = Bun.spawnSync([binary, "verify-journal-hashes", ...args], {
      stdout: "pipe",
      stderr: "pipe",
    });
    assert.equal(proc.exitCode, 2);
    assert.equal(proc.stderr.length, 0);
    assert.equal(JSON.parse(proc.stdout.toString()).code, "invalid_input");
  }
  const help = Bun.spawnSync([binary, "verify-journal-hashes", "--help"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  assert.equal(help.exitCode, 0);
  assert.equal(help.stderr.length, 0);
  assert.match(
    JSON.parse(help.stdout.toString()).scope,
    /not full audit integrity/,
  );
  const rootHelp = Bun.spawnSync([binary, "--help"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  assert.equal(rootHelp.exitCode, 0);
  assert.equal(rootHelp.stderr.length, 0);
  assert.deepEqual(
    JSON.parse(rootHelp.stdout.toString()).commands.map((c: any) => c.name),
    ["trial-balance", "verify-journal-hashes"],
  );
  passed.push("strict usage JSON/exit2 and explicit hash-only help/exit0");
  assert.deepEqual(inventory(baseline), beforeFrozen);
  passed.push(
    "frozen synthetic oracle remains byte-identical after all damaged-copy executions",
  );
  console.log(
    JSON.stringify(
      {
        ok: true,
        scope:
          "legacy canonical journal hash compatibility on private snapshots",
        checks: passed.length,
        passed,
        legacyAudit: audit,
        fixedVectorHashes: fixed.entries.map((entry: any) => entry.entryHash),
        limits: [
          "Hash agreement is not full audit/accounting integrity or audit_log completeness",
          "No mutation, hosted authority, external checkpoint, live migration or whole-issue completion claimed",
        ],
      },
      null,
      2,
    ),
  );
} finally {
  if (!closed) db.close(true);
  rmSync(temp, { recursive: true, force: true });
}
