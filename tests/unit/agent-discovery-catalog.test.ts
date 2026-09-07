import { describe, expect, test } from "bun:test";
import {
  AGENT_CAPABILITIES,
  AGENT_CATALOGUE_HASH,
  AGENT_CATALOGUE_SCHEMA_VERSION,
  AGENT_WORKFLOWS,
  describeWorkflow,
  searchCapabilities,
  type LiveTool,
} from "../../src/agent-discovery-catalog";
import { handleRequest } from "../../src/server/router";
import { COMMAND_SPECS } from "../../src/cli-meta";
import { MUTATING_COMMANDS } from "../../src/cli-actor";
import { config, makeWorkspace, rmSync } from "./server-api/_shared";

const REQUIRED_WORKFLOWS = [
  "company-workspace-setup",
  "document-mail-intake",
  "bank-reconciliation-batch",
  "supplier-expense-booking",
  "supplier-payable-handling",
  "customer-invoice-lifecycle",
  "vat-preparation",
  "exceptions-corrections",
  "period-close-reopen",
  "backup-health-audit",
  "group-intercompany",
  "digisense-nemhandel",
  "imports-dinero",
] as const;

describe("agent runtime catalogue (#584)", () => {
  test("publishes stable capabilities and every required initial workflow with the complete contract", () => {
    expect(AGENT_CATALOGUE_SCHEMA_VERSION).toBe("rentemester-agent-discovery-v1");
    expect(AGENT_CATALOGUE_HASH).toMatch(/^[a-f0-9]{64}$/);
    expect(new Set(AGENT_CAPABILITIES.map((item) => item.id)).size).toBe(AGENT_CAPABILITIES.length);
    expect(new Set(AGENT_WORKFLOWS.map((item) => item.id)).size).toBe(AGENT_WORKFLOWS.length);
    expect(AGENT_WORKFLOWS.map((item) => item.id)).toEqual(expect.arrayContaining(REQUIRED_WORKFLOWS));
    const capabilityIds = new Set(AGENT_CAPABILITIES.map((item) => item.id));
    for (const capability of AGENT_CAPABILITIES) {
      expect(capability.outcomes.length).toBeGreaterThan(0);
      expect(capability.workflowIds.length).toBeGreaterThan(0);
      expect(capability.canonicalState.length).toBeGreaterThan(0);
      for (const workflowId of capability.workflowIds) expect(AGENT_WORKFLOWS.some((item) => item.id === workflowId)).toBe(true);
    }
    for (const workflow of AGENT_WORKFLOWS) {
      expect(capabilityIds.has(workflow.capabilityId)).toBe(true);
      expect(workflow.prerequisites.length).toBeGreaterThan(0);
      expect(workflow.blockers.length).toBeGreaterThan(0);
      expect(workflow.recovery.length).toBeGreaterThan(0);
      expect(workflow.stopConditions.length).toBeGreaterThan(0);
      expect(workflow.nonGoals.length).toBeGreaterThan(0);
      expect(workflow.steps.length).toBeGreaterThan(0);
      for (const workflowStep of workflow.steps) {
        expect(workflowStep.id).not.toBe("");
        expect(workflowStep.purpose).not.toBe("");
        expect(workflowStep.retryClass).not.toBe("");
        expect(workflowStep.expectedSafety).toMatch(/^(read|write|destructive)$/);
      }
    }
  });

  test("finds representative business outcomes by bounded token search", () => {
    const expected = new Map([
      ["reconcile bank", "bank-bookkeeping"],
      ["book supplier invoice", "supplier-purchases"],
      ["prepare VAT", "vat"],
      ["close period", "period-management"],
      ["import from Dinero", "imports"],
      ["send e-invoice", "digisense-nemhandel"],
      ["identify bank row counterparty", "document-party-resolution"],
    ]);
    for (const [query, capabilityId] of expected) {
      expect(searchCapabilities(query, 0, 10).items.map((item) => item.id)).toContain(capabilityId);
    }
    expect(searchCapabilities("submit VAT authority return", 0, 10)).toMatchObject({ total: 0, items: [] });
    expect(searchCapabilities("correct balance without bank movement", 0, 10).items.map((item) => item.id)).toContain("non-cash-balance-corrections");
    expect(describeWorkflow("non-cash-balance-correction", { tools: [], commands: [], routes: [] })?.workflow.steps.map((step) => step.operation.name)).toEqual(["documents_ingest", "documents_list", "journal_dry_run", "journal_post", "journal_list"]);
    const partyWorkflow=AGENT_WORKFLOWS.find(workflow=>workflow.id==="document-party-resolution")!;
    expect(partyWorkflow.steps.find(step=>step.id==="coverage-plan")?.purpose).toContain("exact transaction");
    expect(partyWorkflow.unsupportedBoundaries).toContain("A bank-row decision applies only to its exact transaction and is never projected onto sibling rows sharing a document.");
  });

  test("is deterministic and paginated for identical build inputs", () => {
    const first = searchCapabilities(undefined, 0, 5);
    expect(searchCapabilities(undefined, 0, 5)).toEqual(first);
    expect(first.hasMore).toBe(true);
    const second = searchCapabilities(undefined, first.nextCursor!, 50);
    expect(second.items.length).toBe(AGENT_CAPABILITIES.length - first.items.length);
    expect(new Set([...first.items, ...second.items].map((item) => item.id)).size).toBe(AGENT_CAPABILITIES.length);
  });

  test("fails closed for missing operations and derives MCP safety/retry facts from live annotations", () => {
    const absent = describeWorkflow("bank-reconciliation-batch", { tools: [], commands: [], routes: [] });
    expect(absent).not.toBeNull();
    expect(absent!.workflow.live).toBe(false);
    expect(absent!.workflow.unresolvedOperations).toContain("mcp:reconcile_bank");

    const tools: LiveTool[] = [
      { name: "bank_import", annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false } },
      { name: "bank_suggest_matches", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
      { name: "reconcile_bank", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_dry_run", annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_plan", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_persist", annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_approve", annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_apply", annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true } },
      { name: "bookkeeping_batch_status", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
      { name: "journal_list", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
      { name: "bank_list", annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true } },
    ];
    const resolved = describeWorkflow("bank-reconciliation-batch", { tools });
    expect(resolved!.workflow.live).toBe(true);
    const report = resolved!.workflow.steps.find((item) => item.id === "reconciliation-report")!;
    expect(report.operation).toMatchObject({ resolved: true, safety: "read", idempotent: true });
    expect(report.boundary).toBe("read");
    const apply = resolved!.workflow.steps.find((item) => item.id === "batch-apply")!;
    expect(apply.operation).toMatchObject({ resolved: true, safety: "write", idempotent: true });
    expect(apply.retryClass).toBe("natural-idempotent");
  });

  test("publishes exact CLI actor, confirmation and mode boundaries", () => {
    for (const workflow of AGENT_WORKFLOWS) {
      for (const workflowStep of workflow.steps.filter((item) => item.operation.surface === "cli")) {
        const command = COMMAND_SPECS.find((item) => item.key === workflowStep.operation.key);
        expect(command, `missing CLI command ${workflowStep.operation.key}`).toBeDefined();
        expect(workflowStep.requiresActor).toBe(MUTATING_COMMANDS.has(workflowStep.operation.key));
        expect(workflowStep.requiresConfirmation).toBe(command!.allowedFlags.includes("--confirm"));
      }
    }
    const imported = AGENT_WORKFLOWS.find((item) => item.id === "imports-dinero")!;
    expect(imported.steps.find((item) => item.id === "dry-run")?.requiredArguments).toEqual(["--dry-run"]);
    expect(imported.steps.find((item) => item.id === "apply-import")?.requiredArguments).toEqual(["--apply"]);
  });

  test("HTTP publishes the same catalogue while explicitly deferring MCP liveness to tools/list", async () => {
    const workspace = makeWorkspace("catalogue-http", ["Synthetic Company"]);
    try {
      const cfg = config({ workspaceRoot: workspace });
      const search = await handleRequest(new Request("http://localhost/api/agent-capabilities?query=prepare%20VAT&limit=1"), cfg);
      const searchBody = await search.json() as { catalogue: { hash: string }; items: Array<{ id: string }> };
      expect(search.status).toBe(200);
      expect(searchBody.catalogue.hash).toBe(AGENT_CATALOGUE_HASH);
      expect(searchBody.items[0]?.id).toBe("vat");
      const detail = await handleRequest(new Request("http://localhost/api/agent-workflows/vat-preparation"), cfg);
      const detailBody = await detail.json() as { workflow: { live: boolean; unresolvedOperations: string[]; steps: Array<{ operation: { surface: string; resolved: boolean | null; reason?: string } }> } };
      expect(detail.status).toBe(200);
      expect(detailBody.workflow.live).toBe(true);
      expect(detailBody.workflow.unresolvedOperations).toEqual([]);
      expect(detailBody.workflow.steps.filter((item) => item.operation.surface === "mcp").every((item) => item.operation.resolved === null && item.operation.reason?.includes("tools/list"))).toBe(true);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
