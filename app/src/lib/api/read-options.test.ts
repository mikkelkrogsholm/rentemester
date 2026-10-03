import { describe, expect, it, vi } from "bun:test";
import { api } from "../api";
import { workspaceInboxApi } from "./workspace-inbox";
import { stubGlobal } from "../../test/globals";

function response(body: object) {
  return new Response(JSON.stringify({ ok: true, ...body }), { headers: { "content-type": "application/json" } });
}

describe("API read cancellation options", () => {
  it("accepts options as the first argument for parameterless reads", async () => {
    const controller = new AbortController();
    const fetchMock = vi.fn(async () => response({ service: "rentemester-cockpit" }));
    stubGlobal("fetch", fetchMock);
    await api.health({ signal: controller.signal });
    expect(fetchMock).toHaveBeenCalledWith("/api/health", expect.objectContaining({ signal: controller.signal }));
  });

  it("keeps all date bounds when options are the last of five arguments", async () => {
    const controller = new AbortController();
    const report = { company: { name: "Synthetic company" } };
    const fetchMock = vi.fn(async () => response({ annualReport: report }));
    stubGlobal("fetch", fetchMock);
    const received = await api.annualReport("synthetic/aps", "2026-01-01", "2026-12-31", undefined, { signal: controller.signal });
    expect(received.company.name).toBe(report.company.name);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/companies/synthetic%2Faps/annual-report?fiscalYearStart=2026-01-01&fiscalYearEnd=2026-12-31",
      expect.objectContaining({ signal: controller.signal }),
    );
  });

  it("preserves a standalone inbox reader's path and response envelope", async () => {
    const controller = new AbortController();
    const source = { sourceId: "source/one", filename: "synthetic.pdf" };
    const fetchMock = vi.fn(async () => response({ source }));
    stubGlobal("fetch", fetchMock);
    const received = await workspaceInboxApi.inspect("synthetic", source.sourceId, { signal: controller.signal });
    expect(received.source.sourceId).toBe(source.sourceId);
    expect(received.source.filename).toBe(source.filename);
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/companies/synthetic/workspace-inbox/source%2Fone",
      expect.objectContaining({ signal: controller.signal }),
    );
  });

  it("adds cancellation to read-only POST preflight without changing its input", async () => {
    const controller = new AbortController();
    const input = { journalLineId: 7, allocations: [{ dimensionId: "project", memberId: "one", amountMinor: 500, currency: "DKK" }] };
    const fetchMock = vi.fn(async () => response({ plan: { planHash: "synthetic-hash" } }));
    stubGlobal("fetch", fetchMock);
    await api.planDimensionAssignment("synthetic", input, { signal: controller.signal });
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/companies/synthetic/dimensions/plan",
      expect.objectContaining({ method: "POST", body: JSON.stringify(input), signal: controller.signal }),
    );
  });

  it("does not share a cancelled read's signal with a subsequent mutation", async () => {
    const controller = new AbortController();
    controller.abort();
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, _init?: RequestInit) => response({}));
    stubGlobal("fetch", fetchMock);
    await api.health({ signal: controller.signal });
    const scope = { companyId: 1, accountingFrom: "2026-01-01", accountingTo: "2026-12-31", bankFrom: "2026-01-01", bankTo: "2026-12-31", runKey: "synthetic-run" };
    await api.bookkeepingBatchPersist("synthetic", scope);
    const mutationInit = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(mutationInit.signal).toBeUndefined();
    expect(mutationInit.method).toBe("POST");
    expect(mutationInit.body).toBe(JSON.stringify({ ...scope, confirm: true }));
  });
});
