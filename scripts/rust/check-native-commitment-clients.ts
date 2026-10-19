// Build host CLI/MCP binaries first (or RM_NATIVE_CLI_BIN/RM_NATIVE_MCP_BIN).
// The helper uses the real native server and leaves shared build caches alone.

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-commitment-clients.json";
export async function checkNativeCommitmentClients(page: Page, base: string, restarted = false) {
  const temp = mkdtempSync("/tmp/rm-native-commitment-clients-"),
    cookieFile = join(temp, "session.cookie");
  const cookies = (await page.context().cookies(base))
    .map((c) => `${c.name}=${c.value}`)
    .join("; ");
  writeFileSync(cookieFile, cookies, { mode: 0o600, flag: "wx" });
  const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile],
    cli = process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"),
    mcp = process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp");
  function cliCall(command: string, body?: string, flags: string[] = [], allowFailure = false) {
    const child = spawnSync(
      cli,
      ["remote", command, ...common, "--company", "synthetic", ...flags],
      { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
    );
    if (child.error)
      throw new Error(
        "Host Rust CLI unavailable; build rentemester-cli or configure RM_NATIVE_CLI_BIN",
      );
    if (!allowFailure && child.status !== 0)
      throw new Error(`CLI ${command} failed: ${child.stdout.slice(0, 1000)}`);
    const value = JSON.parse(child.stdout);
    return { status: child.status, value };
  }
  function rpc(name: string, args: Record<string, unknown>) {
    const input =
      [
        {
          jsonrpc: "2.0",
          id: 1,
          method: "initialize",
          params: {
            protocolVersion: "2025-11-25",
            capabilities: {},
            clientInfo: { name: "synthetic-native-commitment-client-check", version: "1" },
          },
        },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        {
          jsonrpc: "2.0",
          id: 2,
          method: name === "tools/list" ? name : "tools/call",
          params: name === "tools/list" ? {} : { name, arguments: args },
        },
      ]
        .map((v) => JSON.stringify(v))
        .join("\n") + "\n";
    const child = spawnSync(mcp, common, {
      input,
      encoding: "utf8",
      maxBuffer: 16 * 1024 * 1024,
    });
    if (child.error)
      throw new Error(
        "Host Rust MCP unavailable; build rentemester-mcp or configure RM_NATIVE_MCP_BIN",
      );
    if (child.status !== 0) throw new Error("MCP subprocess failed");
    const replies = child.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    const result = replies.find((r) => r.id === 2);
    if (result?.error || result?.result?.isError)
      throw new Error(`MCP ${name} failed: ${JSON.stringify(result).slice(0, 1000)}`);
    return name === "tools/list" ? result.result : result.result.structuredContent.data;
  }

  let evidence: {
    id: string;
    date: string;
    doc: number;
    original: string;
    vendor: number;
    approvalHash: string;
  };
  try {
    const list = () =>
      cliCall("supplier-commitments", undefined, ["--start-date", evidence.date]).value.data
        .supplierCommitments;
    const own = () =>
      list().commitments.find((c: { commitmentId: string }) => c.commitmentId === evidence.id);
    const noEffects = async () => {
      const docs = await (
        await page.request.get(`${base}/api/companies/synthetic/documents`)
      ).json();
      if (
        docs.documents.documents.some(
          (d: { id: number; journalEntryId: number | null }) =>
            d.id === evidence.doc && d.journalEntryId !== null,
        )
      )
        throw new Error("Commitment source was journal-posted");
      if (
        cliCall("payables", undefined, ["--status", "all"]).value.data.payables.rows.some(
          (p: { documentId: number }) => p.documentId === evidence.doc,
        )
      )
        throw new Error("Commitment source became payable");
    };
    if (!restarted) {
      const year = new Date()
        .toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" })
        .slice(0, 4);
      evidence = {
        id: "synthetic-client-commitment",
        date: `${year}-01-31`,
        doc: 0,
        vendor: 0,
        approvalHash: "",
        original: "Synthetic client supplier commitment12.34DKK monthly source. Planning only.\n",
      };
      evidence.vendor = cliCall(
        "create-vendor",
        JSON.stringify({ name: "Synthetic Commitment Client Supplier" }),
        ["--confirm"],
      ).value.data.vendor.id;
      const upload = await page.request.post(`${base}/api/companies/synthetic/documents/ingest`, {
        headers: { origin: base, "content-type": "application/json" },
        data: {
          confirm: true,
          fileName: "synthetic-client-commitment.txt",
          fileBase64: Buffer.from(evidence.original).toString("base64"),
          metadata: {
            source: "Synthetic commitment client original",
            documentType: "cash_register_receipt",
            issueDate: evidence.date,
            currency: "DKK",
            amountIncVat: 12.34,
          },
        },
      });
      if (!upload.ok()) throw new Error(await upload.text());
      evidence.doc = (await upload.json()).document.id;
      const baseline = JSON.stringify(cliCall("journal").value.data.journal.entries),
        bankBaseline = JSON.stringify(cliCall("bank").value.data.bank.transactions),
        payableBaseline = JSON.stringify(
          cliCall("payables", undefined, ["--status", "all"]).value.data.payables.rows,
        );
      const commitment = {
        commitmentId: evidence.id,
        vendorPartyId: `vendor:${evidence.vendor}`,
        type: "subscription",
        description: "Synthetic client service",
        businessPurpose: "Synthetic reviewed client planning",
        amount: 12.34,
        currency: "DKK",
        frequency: "monthly",
        nextDate: evidence.date,
        evidenceRefs: [`document:${evidence.doc}`],
      };
      const schemas = rpc("tools/list", {}).tools;
      for (const name of [
        "supplier_commitments",
        "supplier_commitment_matches",
        "plan_supplier_commitment",
        "approve_supplier_commitment",
        "change_supplier_commitment",
        "match_supplier_commitment",
      ]) {
        const tool = schemas.find((t: { name: string }) => t.name === name);
        if (!tool || tool.inputSchema.additionalProperties !== false)
          throw new Error(`Missing strict tool ${name}`);
      }
      if (
        schemas.find((t: { name: string }) => t.name === "plan_supplier_commitment").annotations
          .readOnlyHint !== true
      )
        throw new Error("Plan not declared read-only");
      const planned = cliCall("plan-supplier-commitment", JSON.stringify({ commitment })).value.data
        .plan;
      evidence.approvalHash = planned.payloadHash;
      const readPlan = rpc("plan_supplier_commitment", {
        company: "synthetic",
        body: JSON.stringify({ commitment }),
      }).plan;
      if (readPlan.payloadHash !== planned.payloadHash || own())
        throw new Error("Read-only plans differed or created an approval");
      const approve = JSON.stringify({
        commitment,
        payloadHash: planned.payloadHash,
        idempotencyKey: "synthetic-client-commitment-reviewed-approval",
        confirm: true,
      });
      rpc("approve_supplier_commitment", { company: "synthetic", body: approve, confirm: true });
      const replay = cliCall("approve-supplier-commitment", approve, ["--confirm"]).value.data
        .supplierCommitment;
      if (
        !replay.idempotent ||
        list().history.filter(
          (h: { commitmentId: string; event: string }) =>
            h.commitmentId === evidence.id && h.event === "approved",
        ).length !== 1
      )
        throw new Error("Known successful approval replay duplicated evidence");
      const projected = () =>
        list()
          .forecast.periods.flatMap(
            (p: { sources: Array<{ reference: string; amount: number }> }) => p.sources,
          )
          .filter((s: { reference: string }) => s.reference === evidence.id)
          .reduce((n: number, s: { amount: number }) => n + Math.round(s.amount * 100), 0);
      if (projected() !== -4936)
        throw new Error("Approved client commitment missing exact forecast");
      const matched = cliCall(
        "match-supplier-commitment",
        JSON.stringify({
          commitmentId: evidence.id,
          occurrenceDate: evidence.date,
          evidence: { kind: "canonical_document", id: String(evidence.doc) },
          confirm: true,
        }),
        ["--confirm"],
      ).value.data.supplierCommitmentMatch;
      if (
        matched.variance.amount !== 0 ||
        matched.variance.party.status !== "missing" ||
        projected() !== -3702
      )
        throw new Error("Canonical client match/identity/duplicate exclusion wrong");
      const matches = rpc("supplier_commitment_matches", { company: "synthetic" }).matches.rows;
      if (
        matches.filter((m: { commitmentId: string }) => m.commitmentId === evidence.id).length !== 1
      )
        throw new Error("Cross-client match read missing");
      cliCall(
        "change-supplier-commitment",
        JSON.stringify({
          commitmentId: evidence.id,
          action: "paused",
          reason: "Synthetic client reviewed pause",
          expectedPayloadHash: own().payloadHash,
          confirm: true,
        }),
        ["--confirm"],
      );
      if (own().status !== "paused" || projected() !== 0)
        throw new Error("Client pause did not withdraw forecast");
      rpc("change_supplier_commitment", {
        company: "synthetic",
        body: JSON.stringify({
          commitmentId: evidence.id,
          action: "ended",
          reason: "Synthetic client reviewed end",
          expectedPayloadHash: own().payloadHash,
          confirm: true,
        }),
        confirm: true,
      });
      const current = rpc("supplier_commitments", {
        company: "synthetic",
        startDate: evidence.date,
      }).supplierCommitments;
      if (
        current.commitments.find((c: { commitmentId: string }) => c.commitmentId === evidence.id)
          ?.status !== "ended" ||
        current.forecast.completeness.status !== "incomplete"
      )
        throw new Error("Current read lost terminal state or disclosed incomplete coverage");
      if (
        JSON.stringify(cliCall("journal").value.data.journal.entries) !== baseline ||
        JSON.stringify(cliCall("bank").value.data.bank.transactions) !== bankBaseline ||
        JSON.stringify(
          cliCall("payables", undefined, ["--status", "all"]).value.data.payables.rows,
        ) !== payableBaseline
      )
        throw new Error("Client planning changed accounting/bank facts");
      await noEffects();
      mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
      writeFileSync(saved, JSON.stringify(evidence));
    } else evidence = JSON.parse(readFileSync(saved, "utf8"));
    const current = list();
    if (
      own()?.status !== "ended" ||
      current.history.filter((h: { commitmentId: string }) => h.commitmentId === evidence.id)
        .length !== 3 ||
      current.matches.filter((m: { commitmentId: string }) => m.commitmentId === evidence.id)
        .length !== 1 ||
      current.forecast.completeness.status !== "incomplete"
    )
      throw new Error("Client commitment lifecycle did not persist");
    const approved = current.history.find(
      (h: { commitmentId: string; event: string }) =>
        h.commitmentId === evidence.id && h.event === "approved",
    );
    if (approved?.payloadHash !== evidence.approvalHash)
      throw new Error("Original approval hash changed");
    await noEffects();
    const original = await page.request.get(
      `${base}/api/companies/synthetic/documents/${evidence.doc}/file`,
    );
    if (!original.ok() || (await original.body()).toString() !== evidence.original)
      throw new Error("Client source original changed");
    console.log(
      restarted
        ? "PASS: supplier commitment CLI/MCP proof, canonical match, terminal history, source and incomplete forecast persist after restart"
        : "PASS: distinct CLI/MCP supplier source→read-only plans→confirmed exact approval/replay→canonical match→reasoned pause/end/current reads; exact forecast, no accounting effects",
    );
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
