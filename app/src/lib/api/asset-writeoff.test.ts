import { describe, expect, test, vi } from "bun:test";
import manifest from "../../../../protocol/http-v1.json";
import { withProtocol } from "../../test/protocol";
import { stubGlobal } from "../../test/globals";
import type { NativeAssetWriteOffReceipt } from "../types/assets";
import { assetsApi } from "./assets";

const input = { name: "Synthetic keyboard", category: "equipment", acquisitionDate: "2026-10-24", transactionDate: "2026-10-24", cost: 1234.56, purchaseDocumentId: 7, expenseAccountNo: "EX", thresholdRuleSource: "Owner/advisor judgment", paymentAccountNo: "BANK", note: "Reviewed" };
const confirmation = { confirm: true, confirmImmediateWriteOff: true } as const;
const receipt: NativeAssetWriteOffReceipt = { ok: true, entryId: 12, entryNo: "2026-12", entryHash: "a".repeat(64), writeOffId: 3, cost: input.cost, thresholdDkk: 36000, appliedRules: ["DK-ASSET-WRITEOFF-001"], errors: [] };
function install(body: unknown, status = 200, truncated = false) {
  const transport = vi.fn(withProtocol(async () => new Response(truncated ? "{" : JSON.stringify(body), { status }), manifest.operations.map(op => [op.method, op.path] as const)));
  stubGlobal("fetch", transport);
  return transport;
}
function posts(transport: ReturnType<typeof install>) { return transport.mock.calls.filter(([, init]) => init?.method === "POST"); }

describe("original Rust immediate write-off binding", () => {
  test("whitelists the exact native DTO and validates the flat receipt without another POST", async () => {
    const transport = install(receipt);
    expect(await assetsApi.writeOffNativeAsset("synthetic", { ...input, actor: "never", config: {} } as typeof input, confirmation)).toEqual(receipt);
    expect(posts(transport)).toHaveLength(1);
    const [path, init] = posts(transport)[0]!;
    expect(path).toBe("/api/companies/synthetic/asset/original/write-off");
    expect(JSON.parse(String(init?.body))).toEqual({ name: input.name, category: input.category, acquisitionDate: input.acquisitionDate, date: input.transactionDate, cost: input.cost, documentId: 7, expenseAccount: "EX", paymentAccount: "BANK", thresholdRuleSource: input.thresholdRuleSource, note: input.note, ...confirmation });
    expect(init).not.toHaveProperty("decodeRefusal422");
  });
  test("recognizes only the exact parsed source false422; exceptions may remain", async () => {
    const transport = install({ ok: false, appliedRules: ["rule"], errors: ["missing bill"] }, 422);
    await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "source_refused", status: 422, message: expect.stringContaining("undtagelse") });
    expect(posts(transport)).toHaveLength(1);
  });
  for (const body of [{ ok: false, errors: ["no rules"] }, { ok: false, appliedRules: [], errors: [] }, { ok: false, appliedRules: [1], errors: ["bad"] }, { ok: false, appliedRules: [], errors: ["bad"], extra: true }]) {
    test(`malformed422 stays unknown: ${JSON.stringify(body)}`, async () => {
      const transport = install(body, 422);
      await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "internal", status: 422 });
      expect(posts(transport)).toHaveLength(1);
    });
  }
  for (const patch of [{ entryId: null }, { entryId: Number.MAX_SAFE_INTEGER + 1 }, { entryNo: "" }, { entryHash: "bad" }, { writeOffId: 0 }, { cost: 12 }, { thresholdDkk: null }, { thresholdDkk: input.cost - 0.01 }, { appliedRules: ["unrelated-rule"] }, { appliedRules: [] }, { errors: ["bad"] }]) {
    test(`invalid success stays unknown: ${JSON.stringify(patch)}`, async () => {
      const transport = install({ ...receipt, ...patch });
      await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "internal" });
      expect(posts(transport)).toHaveLength(1);
    });
  }
  test("truncated success and truncated422 are unknown", async () => {
    for (const status of [200, 422]) {
      const transport = install(null, status, true);
      await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "internal" });
      expect(posts(transport)).toHaveLength(1);
    }
  });
  test("5xx source false is not decoded as definite refusal", async () => {
    const transport = install({ ok: false, appliedRules: ["rule"], errors: ["unknown"] }, 503);
    await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "internal", status: 503 });
    expect(posts(transport)).toHaveLength(1);
  });
  test("network uncertainty never retries or falls back", async () => {
    const transport = vi.fn(withProtocol(async () => { throw new TypeError("offline"); }, manifest.operations.map(op => [op.method, op.path] as const)));
    stubGlobal("fetch", transport);
    await expect(assetsApi.writeOffNativeAsset("synthetic", input, confirmation)).rejects.toMatchObject({ code: "network" });
    expect(posts(transport)).toHaveLength(1);
  });
  test("legacy Bun branch keeps the legacy route, fields and nested response", async () => {
    const summary = { writeOffId: 3, entryId: 12, cost: input.cost, thresholdDkk: 36000 };
    const transport = vi.fn(withProtocol(async () => Response.json({ ok: true, writeOff: summary }), [["POST", "/api/companies/{company}/assets/write-off"]]));
    stubGlobal("fetch", transport);
    expect(await assetsApi.writeOffAsset("synthetic", input)).toEqual(summary);
    expect(posts(transport)[0]?.[0]).toBe("/api/companies/synthetic/assets/write-off");
    const body = JSON.parse(String(posts(transport)[0]?.[1]?.body));
    expect(body.transactionDate).toBe(input.transactionDate);
    expect(body.purchaseDocumentId).toBe(7);
    expect(body).not.toHaveProperty("confirmImmediateWriteOff");
  });
});
