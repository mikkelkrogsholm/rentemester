import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeAuditVerify } from "./NativeAuditVerify";
function report() { return { ok: true, audit: { format: "rentemester-native-audit-verification/1", slug: "synthetic", ok: true, available: true, entries: 2, errors: [] as string[], components: ["ledger", "bank", "invoices", "originals", "physical_schema"].map(name => ({ name, ok: true, available: true, checked: {}, errors: [] as unknown[] })) } }; }
test("explicit whole audit read displays failed checks and never retains an earlier green result on refusal", async () => {
  let calls = 0;
  stubGlobal("fetch", vi.fn(async () => { calls++; if (calls === 3) return Response.json({ ok: false, code: "access_denied", errors: ["access_denied"] }, { status: 403 }); const value = report(); if (calls === 2) { value.audit.ok = false; value.audit.errors = ["originals: synthetic missing original"]; value.audit.components[3]!.ok = false; value.audit.components[3]!.errors = [{ error: "<script>inert finding</script>" }]; } return Response.json(value); }));
  renderAt(<NativeAuditVerify slug="synthetic" />); expect(calls).toBe(0);
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér regnskabet" })); await screen.findByText(/Kontrollen fandt ingen afvigelser/);
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér regnskabet" })); await screen.findByText(/Kontrollen fandt afvigelser/); expect(screen.queryByText(/Kontrollen fandt ingen afvigelser/)).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér regnskabet" })); await screen.findByRole("alert"); expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
test("an omitted required component cannot produce a green whole audit", async () => {
  const value = report(); value.audit.components.pop(); stubGlobal("fetch", vi.fn(async () => Response.json(value))); renderAt(<NativeAuditVerify slug="synthetic" />);
  await userEvent.click(screen.getByRole("button", { name: "Kontrollér regnskabet" })); await screen.findByRole("alert"); expect(screen.queryByText(/Kontrollen fandt ingen afvigelser/)).not.toBeInTheDocument();
});
