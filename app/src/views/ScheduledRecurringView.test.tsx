import { afterEach, expect, test } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { restoreGlobals } from "../test/globals";
import { renderAt } from "../test/render";
import { ScheduledRecurringView } from "./ScheduledRecurringView";
import { MutationMemoryProvider } from "../lib/mutation-memory";
afterEach(() => { sessionStorage.clear(); restoreGlobals(); });
const row = { templateId: 31, templateHash: "a".repeat(64), stateHash: "b".repeat(64), state: "enabled", ownerUserId: "synthetic-owner", lastRun: null, nextOccurrence: { periodIndex: 2, issueDate: "2026-10-25" }, missedCount: 2, queued: [], error: null, history: [] };
const plan = { template: { id: 31, name: "Synthetic service", interval: "monthly", intervalCount: 1, firstIssueDate: "2026-08-25", paymentTermsDays: 14, deliveryPeriodMode: "issue_month", payload: { currency: "DKK", lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 100 }] } }, templateId: row.templateId, templateHash: row.templateHash, stateHash: row.stateHash, asOfDate: "2026-10-25", normalDue: row.nextOccurrence, missed: [{ periodIndex: 0, issueDate: "2026-08-25" }, { periodIndex: 1, issueDate: "2026-09-25" }], missedCount: 2, truncated: false, queued: [], planHash: "c".repeat(64) };
function setup(result: unknown = { scheduledRecurring: row }) {
  mockFetch({
    "GET /api/companies/a/scheduled-recurring": { scheduledRecurring: { asOfDate: plan.asOfDate, timeZone: "Europe/Copenhagen", localService: false, rows: [row] } },
    "GET /api/companies/a/scheduled-recurring/31/plan": { plan },
    "POST /api/companies/a/scheduled-recurring/catch-up": result,
  });
  renderAt(<ScheduledRecurringView slug="a" />);
}
const posts = () => (fetch as any).mock.calls.filter(([, init]: any[]) => init?.method === "POST");
async function selectTemplate() { await screen.findByRole("option", { name: /Skabelon 31/ }); await userEvent.selectOptions(screen.getByLabelText("Planlagt fakturaskabelon"), "31"); }
test("opening, selecting and refreshing status never queues or dispatches", async () => {
  setup(); await selectTemplate();
  await userEvent.click(screen.getByRole("button", { name: "Opdatér planlægningsstatus" }));
  await screen.findByText("Manglende forekomster");
  expect(posts()).toHaveLength(0);
});
test("catch-up requires reviewed exact server plan, selected occurrence and confirmation", async () => {
  setup(); await selectTemplate();
  await userEvent.selectOptions(screen.getByLabelText("Planlægningshandling"), "catch-up");
  const submit = screen.getByRole("button", { name: "Sæt valgte forekomster i kø" });
  expect(submit).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Gennemgå forekomster" }));
  await userEvent.click(await screen.findByRole("checkbox", { name: "2026-09-25 (periode 1)" }));
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg bekræfter/ }));
  await userEvent.click(submit);
  await waitFor(() => expect(posts()).toHaveLength(1));
  expect(JSON.parse(posts()[0][1].body)).toEqual({ templateId: 31, planHash: plan.planHash, periodIndexes: [1], confirm: true });
  expect(posts()[0][0]).toContain("/catch-up");
});
test("definite refusal remains visible without claiming issuance or changing the row", async () => {
  setup({ __error: { code: "validation", message: "Source changed; review again" } }); await selectTemplate();
  await userEvent.click(screen.getByRole("button", { name: "Gennemgå forekomster" }));
  await userEvent.click(await screen.findByRole("checkbox", { name: "2026-08-25 (periode 0)" }));
  await userEvent.selectOptions(screen.getByLabelText("Planlægningshandling"), "catch-up");
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg bekræfter/ }));
  await userEvent.click(screen.getByRole("button", { name: "Sæt valgte forekomster i kø" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Source changed; review again");
  expect(screen.queryByText(/Fakturaen er udstedt/)).toBeNull();
  expect(posts()).toHaveLength(1);
});
const restoreConsent = { required: true, boundaryHash: "d".repeat(64), historicalOwnerUserId: "historical-owner", historicalQueued: [{ periodIndex: 1, issueDate: "2026-09-25" }] };
const restored = { ...row, state: "paused", ownerUserId: null, restoreConsent };
const restoredPlan = { ...plan, normalDue: null, restoreConsent };
const consented = { ...row, ownerUserId: "fresh-owner", restoreConsent: { ...restoreConsent, required: false } };
function setupRestored(result: unknown = { scheduledRecurring: consented }, boundary = restoreConsent.boundaryHash) {
  mockFetch({
    "GET /api/companies/a/scheduled-recurring": { scheduledRecurring: { asOfDate: plan.asOfDate, timeZone: "Europe/Copenhagen", localService: false, rows: [restored] } },
    "GET /api/companies/a/scheduled-recurring/31/plan": { plan: { ...restoredPlan, restoreConsent: { ...restoreConsent, boundaryHash: boundary } } },
    "POST /api/companies/a/scheduled-recurring/reconsent": result,
  });
  return renderAt(<MutationMemoryProvider><ScheduledRecurringView slug="a" /></MutationMemoryProvider>);
}
async function reviewRestored() {
  await selectTemplate();
  await userEvent.click(screen.getByRole("button", { name: "Gennemgå forekomster" }));
  await screen.findByRole("heading", { name: "Synthetic service" });
  await userEvent.type(screen.getByLabelText("Planlægningsbegrundelse"), "Gennemgået efter gendannelse");
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg bekræfter/ }));
}
test("restored consent requires source review and reason; exact reconsent never queues or dispatches", async () => {
  setupRestored(); await selectTemplate();
  const submit = screen.getByRole("button", { name: "Godkend planlægning efter gendannelse" });
  expect(submit).toBeDisabled();
  expect(screen.getByText(/Tidligere køvalg, som ikke er aktive/)).toHaveTextContent("2026-09-25");
  expect(posts()).toHaveLength(0);
  await userEvent.click(screen.getByRole("button", { name: "Gennemgå forekomster" }));
  await screen.findByRole("heading", { name: "Synthetic service" });
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg bekræfter/ }));
  expect(submit).toBeDisabled();
  await userEvent.type(screen.getByLabelText("Planlægningsbegrundelse"), "Gennemgået efter gendannelse");
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg bekræfter/ }));
  await userEvent.click(submit);
  await screen.findByText(/Dit samtykke er gemt. Automatik er ikke startet/);
  expect(posts()).toHaveLength(1);
  expect(posts()[0][0]).toContain("/reconsent");
  expect(JSON.parse(posts()[0][1].body)).toEqual({ templateId: 31, planHash: plan.planHash, reason: "Gennemgået efter gendannelse", confirm: true });
});
test("a plan from another restore boundary cannot consent", async () => {
  setupRestored(undefined, "e".repeat(64)); await reviewRestored();
  expect(screen.getByRole("button", { name: "Godkend planlægning efter gendannelse" })).toBeDisabled();
  expect(posts()).toHaveLength(0);
});
test("malformed accepted reconsent remains blocked across reopening and status refresh", async () => {
  const view = setupRestored({ scheduledRecurring: { ...consented, ownerUserId: null } });
  await reviewRestored();
  await userEvent.click(screen.getByRole("button", { name: "Godkend planlægning efter gendannelse" }));
  await screen.findByText(/Serverens godkendelse kunne ikke bekræftes/);
  expect(posts()).toHaveLength(1);
  view.unmount();
  renderAt(<MutationMemoryProvider><ScheduledRecurringView slug="a" /></MutationMemoryProvider>);
  await selectTemplate();
  await userEvent.click(screen.getByRole("button", { name: "Opdatér planlægningsstatus" }));
  expect(screen.getByRole("button", { name: "Godkend planlægning efter gendannelse" })).toBeDisabled();
  expect(posts()).toHaveLength(1);
});
