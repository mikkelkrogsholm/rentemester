import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
import { renderAt } from "../test/render";
const user = { id: "synthetic-owner", email: "owner@example.invalid", emailVerified: true, twoFactorEnabled: true };
mock.module("../lib/auth-client", () => ({ authClient: { getSession: async () => ({ data: { user, session: { id: "synthetic-session" } }, error: null }) } }));
const { AuthProvider, useAuth } = await import("../lib/auth-context");
const { MutationMemoryProvider } = await import("../lib/mutation-memory");
const { ScheduledRecurringProvider, useScheduledRecurringAutomation } = await import("./ScheduledRecurringProvider");
afterEach(() => { sessionStorage.clear(); restoreGlobals(); });
beforeEach(() => sessionStorage.clear());
const row = { templateId: 31, templateHash: "a".repeat(64), stateHash: "b".repeat(64), state: "enabled", ownerUserId: user.id, error: null, missedCount: 0, queued: [] };
const plan = { template: { id: 31, name: "Synthetic service", interval: "monthly", intervalCount: 1, firstIssueDate: "2026-08-25", paymentTermsDays: 14, deliveryPeriodMode: "issue_month", payload: { currency: "DKK", lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 100 }] } }, templateId: row.templateId, templateHash: row.templateHash, stateHash: row.stateHash, asOfDate: "2026-10-25", normalDue: { periodIndex: 2, issueDate: "2026-10-25" }, missedCount: 0, missed: [], truncated: false, queued: [], planHash: "c".repeat(64) };
function Start() { const auth = useAuth(), controls = useScheduledRecurringAutomation(); return <button disabled={!auth.context} onClick={() => controls!.start("a", 31, row.templateHash)}>Start synthetic</button>; }
function mount() { return renderAt(<AuthProvider hosted backend="rust"><MutationMemoryProvider><ScheduledRecurringProvider><Start /></ScheduledRecurringProvider></MutationMemoryProvider></AuthProvider>); }
function setup(result: unknown = { dispatch: { status: "succeeded" }, generation: { documentId: 101, templateId: 31, periodIndex: 2 } }, missed = false) {
  mockFetch({
    "GET /api/me": { user, workspaceRole: "member", companies: [{ slug: "a", name: "Synthetic", role: "owner", archived: false }] },
    "GET /api/companies/a/scheduled-recurring": { scheduledRecurring: { rows: [row] } },
    "GET /api/companies/a/scheduled-recurring/31/plan": { plan: { ...plan, missedCount: missed ? 1 : 0 } },
    "POST /api/companies/a/scheduled-recurring/dispatch": result,
  });
}
const posts = () => (fetch as any).mock.calls.filter(([, init]: any[]) => init?.method === "POST");
async function start() { const button = await screen.findByRole("button", { name: "Start synthetic" }); await waitFor(() => expect(button).not.toBeDisabled()); await userEvent.click(button); }
test("only explicit session activation dispatches today's reviewed occurrence; reopening stays inert", async () => {
  setup(); const view = mount();
  await waitFor(() => expect(screen.getByRole("button", { name: "Start synthetic" })).not.toBeDisabled());
  expect(posts()).toHaveLength(0);
  await start(); await waitFor(() => expect(posts()).toHaveLength(1));
  expect(JSON.parse(posts()[0][1].body)).toEqual({ templateId: 31, planHash: plan.planHash, periodIndex: 2, confirm: true });
  view.unmount(); mount();
  await waitFor(() => expect(screen.getByRole("button", { name: "Start synthetic" })).not.toBeDisabled());
  expect(posts()).toHaveLength(1);
});
test("missed periods stop automation for review instead of dispatching", async () => {
  setup(undefined, true); mount(); await start();
  await screen.findByText(/Gennemgå manglende eller køsatte forekomster/);
  expect(posts()).toHaveLength(0);
});
test("unconfirmed success persists a payload-free lock and explicit restart cannot resend", async () => {
  setup({ dispatch: { status: "succeeded" } }); mount(); await start();
  await screen.findByText(/Kontrollér kørselsstatus og adgang/);
  expect(posts()).toHaveLength(1);
  const saved = sessionStorage.getItem("rentemester:uncertain-operations:v1")!;
  expect(JSON.parse(saved)).toHaveLength(1); expect(saved).not.toContain("synthetic");
  await start(); await screen.findByText(/En tidligere handling har uafklaret udfald/);
  expect(posts()).toHaveLength(1);
});
