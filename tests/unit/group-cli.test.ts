import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createCompany } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { postJournalEntry } from "../../src/core/ledger";
import { companyPaths } from "../../src/core/paths";
import { companyRootForSlug, initWorkspace } from "../../src/core/workspace";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../../src/core/workspace-control";
import { createParty } from "../../src/core/party-registry";
import { ingestCorporateRecord } from "../../src/core/corporate-records";
import { approveIntercompanyDisposition, linkIntercompanyDispositionJournal, proposeIntercompanyDisposition } from "../../src/core/intercompany-dispositions";

function manifest(holding: string, operating: string) {
  return JSON.stringify({ version: 1, groups: [{ id: "synthetic-group", name: "Synthetic group", memberships: [
    { id: "holding-member", companySlug: holding, validFrom: "2026-01-01" },
    { id: "operating-member", companySlug: operating, validFrom: "2026-01-01" },
  ], ownership: [{ id: "holding-owns-operating", parentCompanySlug: holding, childCompanySlug: operating, basisPoints: 10000, evidenceRefs: ["evidence"], validFrom: "2026-01-01" }] }] });
}

async function run(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: { ...process.env, RENTEMESTER_ACTOR: "" } });
  const [stdout, stderr, exit] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (!stdout) throw new Error(`${args.slice(0, 2).join(" ")} exited ${exit}: ${stderr}`);
  return { exit, result: JSON.parse(stdout) as { ok: boolean; errors?: string[]; status?: string; mappingId?: string; mappingHash?: string; eliminationId?: string; payloadHash?: string; profileId?: string; profileHash?: string; consolidatedFigures?: unknown[] | null; rows?: unknown[] } };
}

describe("group CLI workspace-wide authorization", () => {
  test("disposition lifecycle authorizes both companies and preserves both legal ledgers", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-disposition-lifecycle-cli-"));
    initWorkspace(workspace);
    const left = createCompany(workspace, { name: "Synthetic Left", onboardingActor: "agent:group-test" });
    const right = createCompany(workspace, { name: "Synthetic Right", onboardingActor: "agent:other" });
    const db = openWorkspaceControlDb(workspace);
    try {
      const party = createParty(db, { partyId: "synthetic-party", kind: "organization", name: "Synthetic parties", source: "synthetic", observedAt: "2026-01-01T00:00:00Z", reviewAssertion: "reviewed", actor: "user:maker" });
      const record = ingestCorporateRecord(db, { recordId: "synthetic-evidence", type: "intercompany_agreement", bytes: new TextEncoder().encode("Synthetic agreement"), filename: "agreement.txt", source: "synthetic", receivedAt: "2026-01-01T00:00:00Z", uploader: "synthetic", actor: "user:maker" });
      const input = { dispositionId: "synthetic-disposition", type: "loan", economicDate: "2026-02-01", amount: 100, currency: "DKK", partyIds: [party.partyId], evidenceRecordIds: [record.recordId],
        left: { companySlug: left.slug, role: "lender", expectedSide: "receivable" },
        right: { companySlug: right.slug, role: "borrower", expectedSide: "payable" } };
      const proposed = proposeIntercompanyDisposition(db, input, { actor: "user:maker", principal: { kind: "user", id: "maker" } });
      const approved = approveIntercompanyDisposition(db, input.dispositionId, proposed.payloadHash, { actor: "user:review", principal: { kind: "user", id: "review" } });
      proposeIntercompanyDisposition(db, { ...input, dispositionId: "synthetic-replacement" }, { actor: "user:maker", principal: { kind: "user", id: "maker" } });
      for (const [side, company, lines] of [
        ["left", left, [{ accountNo: "1100", debitAmount: 100 }, { accountNo: "5000", creditAmount: 100 }]],
        ["right", right, [{ accountNo: "7000", creditAmount: 100 }, { accountNo: "5000", debitAmount: 100 }]],
      ] as const) {
        const ledger = openDb(companyPaths(companyRootForSlug(workspace, company.slug)).db);
        try {
          const journal = postJournalEntry(ledger, { transactionDate: input.economicDate, text: "Synthetic loan", lines: [...lines] });
          expect(journal.ok).toBe(true);
          linkIntercompanyDispositionJournal(db, workspace, { dispositionId: input.dispositionId, payloadHash: approved.payloadHash, side, journalEntryId: journal.entryId!, expectedLedgerHeadHash: journal.entryHash!, actor: "user:link", principal: { kind: "user", id: "link" } });
          // Finish fixture writes before asserting that lifecycle inspection leaves source bytes untouched.
          ledger.run("PRAGMA wal_checkpoint(TRUNCATE)");
        } finally { ledger.close(); }
      }
      const hashes = () => [left, right].map(company => {
        const hash = createHash("sha256");
        const path = companyPaths(companyRootForSlug(workspace, company.slug)).db;
        for (const suffix of ["", "-wal", "-shm", "-journal"]) {
          hash.update(`${suffix}:${existsSync(path + suffix)}\0`);
          if (existsSync(path + suffix)) hash.update(readFileSync(path + suffix));
        }
        return hash.digest("hex");
      });
      const before = hashes();
      const events = () => db.query("SELECT count(*) AS count FROM rm_intercompany_disposition_lifecycle_events").get();
      const beforeDenied = events();
      const args = (command: string) => ["group", command, "--workspace", workspace, "--disposition-id", input.dispositionId, "--payload-hash", approved.payloadHash,
        "--confirm", "yes", "--actor", "agent:group-test", "--format", "json",
        ...(command === "settle-disposition" ? ["--settlement-evidence-json", JSON.stringify([record.recordId])]
          : command === "supersede-disposition" ? ["--replacement-disposition-id", "synthetic-replacement", "--reason", "Synthetic replacement"]
          : ["--reason", "Synthetic reopening"])];
      for (const command of ["settle-disposition", "reopen-disposition", "supersede-disposition"]) {
        const denied = await run(args(command));
        expect(denied.exit).toBe(1);
        expect(denied.result.errors?.join(" ")).toContain(right.slug);
        expect(events()).toEqual(beforeDenied);
        expect(hashes(), `ledgers after denied ${command}`).toEqual(before);
      }
      writeFileSync(join(workspace, right.slug, "config", "policy.yaml"), "actor_allowlist:\n  agents:\n    - agent:group-test\n");
      const settled = await run(args("settle-disposition"));
      expect(settled).toMatchObject({ exit: 0, result: { ok: true, status: "settled" } });
      expect(hashes(), "ledgers after successful settlement").toEqual(before);
      const afterSettlement = events();
      expect(await run(args("settle-disposition"))).toMatchObject({ exit: 0, result: { status: "settled" } });
      expect(events()).toEqual(afterSettlement);
      expect(await run(args("reopen-disposition"))).toMatchObject({ exit: 0, result: { status: "posted" } });
      const afterReopen = events();
      expect((await run(args("reopen-disposition"))).exit).toBe(1);
      expect(events()).toEqual(afterReopen);
      expect(await run(args("supersede-disposition"))).toMatchObject({ exit: 0, result: { status: "superseded" } });
      const afterSupersede = events();
      expect((await run(args("supersede-disposition"))).exit).toBe(1);
      expect(events()).toEqual(afterSupersede);
      expect(hashes()).toEqual(before);
    } finally { db.close(); rmSync(workspace, { recursive: true, force: true }); }
  }, 30000);

  for (const command of ["settle-disposition", "reopen-disposition", "supersede-disposition"]) {
    test(`${command} validates actor identity and exact confirmation before disposition access`, async () => {
      const workspace = mkdtempSync(join(tmpdir(), "rentemester-disposition-gates-"));
      try {
        initWorkspace(workspace);
        openWorkspaceControlDb(workspace).close();
        const base = ["bun", "run", "src/cli.ts", "group", command, "--workspace", workspace,
          "--disposition-id", "missing", "--payload-hash", "0".repeat(64), "--format", "json"];
        for (const [extra, expectedExit, expectedText] of [
          [["--confirm", "yes", "--actor", "malformed"], 2, "explicit actor must use canonical format"],
          [["--confirm", "yes"], 2, "actor required for mutations"],
          [["--confirm", "YES", "--actor", "agent:group-test"], 1, "--confirm yes required"],
          [["--confirm", "true", "--actor", "agent:group-test"], 1, "--confirm yes required"],
        ] as const) {
          const proc = Bun.spawn([...base, ...extra], { cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
            env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "" } });
          const stdout = await new Response(proc.stdout).text();
          const stderr = await new Response(proc.stderr).text();
          expect(await proc.exited).toBe(expectedExit);
          expect(expectedExit === 2 ? stderr : JSON.parse(stdout).errors.join(" ")).toContain(expectedText);
        }
      } finally { rmSync(workspace, { recursive: true, force: true }); }
    });

    test(`${command} resolves the workspace disposition without a synthetic company`, async () => {
      const workspace = mkdtempSync(join(tmpdir(), "rentemester-disposition-cli-"));
      try {
        initWorkspace(workspace);
        openWorkspaceControlDb(workspace).close();
        const extra = command === "settle-disposition"
          ? ["--settlement-evidence-json", "[]"]
          : command === "supersede-disposition"
            ? ["--replacement-disposition-id", "missing-replacement", "--reason", "Synthetic correction"]
            : ["--reason", "Synthetic correction"];
        const proc = Bun.spawn(["bun", "run", "src/cli.ts", "group", command,
          "--workspace", workspace, "--disposition-id", "missing-disposition",
          "--payload-hash", "0".repeat(64), "--confirm", "yes", "--actor", "agent:group-test",
          "--format", "json", ...extra], {
          cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
          env: { ...process.env, RENTEMESTER_COMPANY: "", RENTEMESTER_ACTOR: "" },
        });
        const stdout = await new Response(proc.stdout).text();
        const stderr = await new Response(proc.stderr).text();
        expect(await proc.exited).toBe(1);
        expect(stderr).toBe("");
        expect(JSON.parse(stdout)).toMatchObject({ ok: false, errors: ["disposition not found"] });
        const db = openWorkspaceControlReadOnlyDb(workspace);
        try {
          expect(db.query("SELECT count(*) AS count FROM rm_intercompany_disposition_events").get()).toMatchObject({ count: 0 });
        } finally { db.close(); }
      } finally { rmSync(workspace, { recursive: true, force: true }); }
    });
  }

  test("fails closed until every referenced active company explicitly allowlists the actor", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-group-cli-"));
    try {
      initWorkspace(workspace);
      const holding = createCompany(workspace, { name: "Holding", onboardingActor: "agent:group-test" });
      const operating = createCompany(workspace, { name: "Operating", onboardingActor: "agent:other" });
      const path = join(workspace, "manifest.json");
      writeFileSync(path, manifest(holding.slug, operating.slug));
      const base = ["group", "apply-manifest", "--workspace", workspace, "--manifest", path, "--policy-company", holding.slug, "--confirm", "yes", "--actor", "agent:group-test", "--format", "json"];
      const denied = await run(base);
      expect(denied.exit).toBe(1);
      expect(denied.result).toMatchObject({ ok: false });
      expect(denied.result.errors?.join(" ")).toContain(operating.slug);

      writeFileSync(join(workspace, operating.slug, "config", "policy.yaml"), "actor_allowlist:\n  agents:\n    - agent:group-test\n");
      const applied = await run(base);
      expect(applied.exit).toBe(0);
      expect(applied.result).toMatchObject({ ok: true, status: "applied" });
      const db = openWorkspaceControlReadOnlyDb(workspace);
      expect(db.query("SELECT count(*) AS count FROM rm_group_manifest_events").get()).toMatchObject({ count: 1 });
      db.close();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  }, 15000);

  test("proposes, independently approves and reconciles an explicit mapping", async () => {
    const workspace = mkdtempSync(join(tmpdir(), "rentemester-group-mapping-cli-"));
    try {
      initWorkspace(workspace);
      const holding = createCompany(workspace, { name: "Synthetic Holding", onboardingActor: "agent:proposer" });
      const operating = createCompany(workspace, { name: "Synthetic Operating", onboardingActor: "agent:proposer" });
      for (const slug of [holding.slug, operating.slug]) writeFileSync(join(workspace, slug, "config", "policy.yaml"), "actor_allowlist:\n  agents:\n    - agent:proposer\n    - agent:reviewer\n");
      const structurePath = join(workspace, "manifest.json");
      writeFileSync(structurePath, manifest(holding.slug, operating.slug));
      const structure = await run(["group", "apply-manifest", "--workspace", workspace, "--manifest", structurePath, "--policy-company", holding.slug, "--confirm", "yes", "--actor", "agent:proposer", "--format", "json"]);
      expect(structure.exit).toBe(0);
      const mappingPath = join(workspace, "mapping.json");
      writeFileSync(mappingPath, JSON.stringify({ id: "synthetic-reciprocal", groupId: "synthetic-group", leftCompanySlug: holding.slug, rightCompanySlug: operating.slug, leftAccountNos: ["1100"], rightAccountNos: ["7000"], leftPosition: "receivable", rightPosition: "payable", evidenceRefs: ["synthetic-contract"], validFrom: "2026-01-01" }));
      const proposed = await run(["group", "propose-mapping", "--workspace", workspace, "--mapping", mappingPath, "--confirm", "yes", "--actor", "agent:proposer", "--format", "json"]);
      expect(proposed.result).toMatchObject({ ok: true, status: "proposed", mappingId: "synthetic-reciprocal" });
      const selfApproval = await run(["group", "approve-mapping", "--workspace", workspace, "--mapping-id", proposed.result.mappingId!, "--mapping-hash", proposed.result.mappingHash!, "--confirm", "yes", "--actor", "agent:proposer", "--format", "json"]);
      expect(selfApproval.result.errors?.join(" ")).toContain("distinct reviewer");
      const approved = await run(["group", "approve-mapping", "--workspace", workspace, "--mapping-id", proposed.result.mappingId!, "--mapping-hash", proposed.result.mappingHash!, "--confirm", "yes", "--actor", "agent:reviewer", "--format", "json"]);
      expect(approved.result).toMatchObject({ ok: true, status: "approved" });
      for (const [slug, lines] of [[holding.slug, [{ accountNo: "1100", debitAmount: 100 }, { accountNo: "5000", creditAmount: 100 }]], [operating.slug, [{ accountNo: "7000", creditAmount: 100 }, { accountNo: "5000", debitAmount: 100 }]]] as const) {
        const ledger = openDb(companyPaths(companyRootForSlug(workspace, slug)).db);
        expect(postJournalEntry(ledger, { transactionDate: "2026-02-01", text: "Synthetic reciprocal", createdBy: "agent:test", createdByProgram: "unit-test", lines: [...lines] }).ok).toBe(true);
        ledger.close();
      }
      const reconciled = await run(["group", "reconcile", "--workspace", workspace, "--as-of", "2026-12-31", "--format", "json"]);
      expect(reconciled.result).toMatchObject({ ok: true, rows: [{ mappingId: "synthetic-reciprocal", status: "matched", difference: 0 }] });
      const eliminationPath = join(workspace, "elimination.json");
      writeFileSync(eliminationPath, JSON.stringify({ id: "synthetic-elimination", mappingId: "synthetic-reciprocal", asOf: "2026-12-31", evidenceRefs: ["synthetic-review-pack"] }));
      const elimination = await run(["group", "propose-elimination", "--workspace", workspace, "--elimination", eliminationPath, "--confirm", "yes", "--actor", "agent:proposer", "--format", "json"]);
      expect(elimination.result).toMatchObject({ ok: true, status: "proposed" });
      const eliminationApproved = await run(["group", "approve-elimination", "--workspace", workspace, "--elimination-id", elimination.result.eliminationId!, "--payload-hash", elimination.result.payloadHash!, "--confirm", "yes", "--actor", "agent:reviewer", "--format", "json"]);
      expect(eliminationApproved.result).toMatchObject({ ok: true, status: "approved" });
      const eliminationApplied = await run(["group", "apply-elimination", "--workspace", workspace, "--elimination-id", elimination.result.eliminationId!, "--payload-hash", elimination.result.payloadHash!, "--confirm", "yes", "--actor", "agent:reviewer", "--format", "json"]);
      expect(eliminationApplied.result).toMatchObject({ ok: true, status: "applied" });
      const profilePath = join(workspace, "consolidation-profile.json");
      writeFileSync(profilePath, JSON.stringify({ version: 1, id: "synthetic-profile", groupId: "synthetic-group", currency: "DKK", validFrom: "2026-01-01", evidenceRefs: ["synthetic-reporting-policy"], reportingLines: [
        { id: "assets", label: "Assets", section: "asset", displayOrder: 10 }, { id: "liabilities", label: "Liabilities", section: "liability", displayOrder: 20 }, { id: "equity", label: "Equity", section: "equity", displayOrder: 30 }, { id: "current-result", label: "Current result", section: "equity", role: "current-result", displayOrder: 40 }, { id: "income", label: "Income", section: "income", displayOrder: 50 }, { id: "expenses", label: "Expenses", section: "expense", displayOrder: 60 },
      ], accountMappings: [
        { id: "holding-receivable", companySlug: holding.slug, accountNo: "1100", reportingLineId: "assets", validFrom: "2026-01-01" }, { id: "holding-equity", companySlug: holding.slug, accountNo: "5000", reportingLineId: "equity", validFrom: "2026-01-01" }, { id: "operating-payable", companySlug: operating.slug, accountNo: "7000", reportingLineId: "liabilities", validFrom: "2026-01-01" }, { id: "operating-equity", companySlug: operating.slug, accountNo: "5000", reportingLineId: "equity", validFrom: "2026-01-01" },
      ] }));
      const profile = await run(["group", "propose-profile", "--workspace", workspace, "--profile", profilePath, "--confirm", "yes", "--actor", "agent:proposer", "--format", "json"]);
      expect(profile.result).toMatchObject({ ok: true, status: "proposed", profileId: "synthetic-profile" });
      const profileApproved = await run(["group", "approve-profile", "--workspace", workspace, "--profile-id", profile.result.profileId!, "--profile-hash", profile.result.profileHash!, "--confirm", "yes", "--actor", "agent:reviewer", "--format", "json"]);
      expect(profileApproved.result).toMatchObject({ ok: true, status: "approved" });
      const report = await run(["group", "consolidated-report", "--workspace", workspace, "--profile-id", "synthetic-profile", "--from", "2026-01-01", "--as-of", "2026-12-31", "--format", "json"]);
      expect(report.result).toMatchObject({ ok: true, status: "ready" });
      expect(report.result.consolidatedFigures).not.toBeNull();
    } finally { rmSync(workspace, { recursive: true, force: true }); }
  }, 30000);
});
