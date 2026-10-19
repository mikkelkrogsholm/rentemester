import { afterEach, expect, test } from "bun:test";
import { clearWorkspaceWriteMode, ordinaryWorkspaceWriteAllowed as allowed, setWorkspaceWriteMode } from "./workspace-write-mode";
import { request } from "./api/_shared";
afterEach(() => clearWorkspaceWriteMode("test"));
test("legacy unaffected; inactive and unknown native modes refuse ordinary writes before fetch", async () => {
 expect(allowed("/api/companies/a/journal", "POST")).toBe(true);
 for (const mode of ["frozen","moved","staged","unknown"] as const) { setWorkspaceWriteMode("test",mode); await expect(request("/api/companies/a/journal", {method:"POST",body:"{}"})).rejects.toMatchObject({status:423,code:"workspace_not_active"}); }
});
test("exact read/auth/handoff/backup/receipt-map exemptions reject lookalike writes", () => {
 setWorkspaceWriteMode("test","frozen");
 for (const path of ["/api/auth/sign-out","/api/workspace/backup","/api/workspace/transfer/release","/api/companies/a/restore/receipts/reconcile"]) expect(allowed(path,"POST")).toBe(true);
 expect(allowed("/api/reports","GET")).toBe(true); expect(allowed("/api/preview","POST",true)).toBe(true);
 expect(allowed("/api/workspace/transfer/release/extra","POST")).toBe(false); expect(allowed("/api/companies/a/restore/receipts/replay","POST")).toBe(false);
});
test("stale scope cleanup cannot clear a current fence", () => { setWorkspaceWriteMode("test","staged"); clearWorkspaceWriteMode("old"); expect(allowed("/api/journal","POST")).toBe(false); });
