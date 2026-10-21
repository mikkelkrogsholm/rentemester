import { afterEach, beforeEach, expect, test, vi } from "bun:test";
import { StrictMode } from "react";
import { MemoryRouter } from "react-router-dom";
import { act, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderAt } from "../test/render";
import { stubGlobal } from "../test/globals";
import { OpeningBalancePanel } from "./OpeningBalancePanel";
import { ReportSourceNotice } from "./ReportSourceNotice";
import { openingBalanceApi } from "../lib/api/opening-balance";
import { archiveImportApi } from "../lib/api/archive-import";
import { api } from "../lib/api";
import { ApiError } from "../lib/api/_shared";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import type { OpeningInput, OpeningPlan, OpeningRecord } from "../lib/types/opening-balance";
const h = "a".repeat(64), sourceHash = "c".repeat(64);
const basis = { kind: "operator_supplied" as const, currency: "DKK", sourceArchives: [{ importId: 41, sourceHash, parserContract: "synthetic-parser/1", declaredCurrency: "DKK" }] };
let lastInput: OpeningInput, saved: OpeningRecord | null;
function record(): OpeningRecord { return { cutOverDate: "2024-07-01", journalEntryId: 7, journalEntryNo: "2024-7", journalEntryHash: "d".repeat(64), planHash: h, sourceBasis: basis, createdAt: "2026-10-20T12:00:00Z", actor: "user:synthetic" }; }
function plan(input: OpeningInput): OpeningPlan { return { planHash: h, input: structuredClone(input), ruleVersion: "synthetic-current-rules", sourceBasis: input.sourceArchiveImportIds.length ? basis : { ...basis, sourceArchives: [] }, totals: { debit: "100.00", credit: "100.00" }, existing: saved, blockers: saved ? ["opening_balance_exists"] : [], status: saved ? "blocked" : "ready" }; }
function setup() {
  saved = null;
  vi.spyOn(api, "fiscalYears").mockResolvedValue([{ label: "2024", source: "live", start: "2024-07-01", end: "2025-06-30" }]);
  vi.spyOn(api, "accounts").mockResolvedValue({ slug: "synthetic", nativeSetup: true, company: { name: "Synthetic", cvr: null, country: "DK", currency: "DKK" }, accounts: [
    { accountNo: "1000", name: "Synthetic asset", type: "asset", normalBalance: "debit", defaultVatCode: null, hasPostings: false, active: true },
    { accountNo: "2000", name: "Synthetic liability", type: "liability", normalBalance: "credit", defaultVatCode: null, hasPostings: false, active: true },
  ], byType: {}, accountRoles: { status: "complete", missing: [], ambiguous: [], resolutions: [], proposals: [], candidates: [], reasons: [] } });
  vi.spyOn(archiveImportApi, "list").mockResolvedValue({ ok: true, years: [{ fiscalYear: 2023, sourceSystem: "dinero", postingCount: 2, balanceCount: 2, importedAt: "2026-10-20", importId: 41, sourceHash, parserContract: "synthetic-parser/1", declaredCurrency: "DKK", hasBalances: true }], errors: [] });
  vi.spyOn(openingBalanceApi, "read").mockImplementation(async () => ({ ok: true, openingBalance: saved, errors: [] }));
  vi.spyOn(openingBalanceApi, "plan").mockImplementation(async (_slug, input) => { lastInput = structuredClone(input); return plan(input); });
  return vi.spyOn(openingBalanceApi, "apply").mockImplementation(async () => { saved = record(); return { ok: true, openingBalance: saved, errors: [], idempotency: { receiptId: 8, replayed: false, createdAt: "now", expiresAt: "later" } }; });
}
function mount(slug = "synthetic") { return renderAt(<StrictMode><MutationMemoryProvider><OpeningBalancePanel slug={slug} /></MutationMemoryProvider></StrictMode>, { route: `/companies/${slug}/manage` }); }
async function fill() {
  await within(screen.getByLabelText("Konto 1")).findByRole("option", { name: "1000 · Synthetic asset" });
  await userEvent.type(screen.getByLabelText("Skæringsdato"), "2024-07-01");
  await userEvent.selectOptions(screen.getByLabelText("Konto 1"), "1000");
  await userEvent.type(screen.getByLabelText("Debet 1"), "100.00");
  await userEvent.selectOptions(screen.getByLabelText("Konto 2"), "2000");
  await userEvent.type(screen.getByLabelText("Kredit 2"), "100.00");
  await userEvent.type(screen.getByLabelText("Note (valgfri)"), "Synthetic reviewed opening");
  await userEvent.click(screen.getByLabelText("Arkivimport 41"));
}
async function review() { await fill(); await userEvent.click(screen.getByRole("button", { name: "Gennemgå åbningsbalance" })); return screen.findByRole("dialog", { name: "Bekræft åbningsbalance" }); }
async function confirm(dialog: HTMLElement, retry = false) {
  await userEvent.click(within(dialog).getByRole("checkbox", { name: "Jeg bekræfter skæringsdato, konti og åbningsbeløb" }));
  await userEvent.click(within(dialog).getByRole("button", { name: retry ? "Hent oprindelig kvittering / prøv samme anmodning" : "Bogfør åbningsbalance" }));
}
beforeEach(() => sessionStorage.clear());
afterEach(() => { vi.restoreAllMocks(); sessionStorage.clear(); });

test("StrictMode: blank explicit inputs → readonly full source review → checked exact apply and canonical readback", async () => {
  const apply = setup(); mount(); expect(screen.getByLabelText("Skæringsdato")).toHaveValue(""); expect(screen.getByLabelText("Debet 1")).toHaveValue("");
  const dialog = await review(); expect(apply).not.toHaveBeenCalled(); expect(within(dialog).getByText(h)).toBeInTheDocument(); expect(within(dialog).getByText(sourceHash)).toBeInTheDocument(); expect(within(dialog).getByText("Serverens totaler")).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Bogfør åbningsbalance" })).toBeDisabled();
  await confirm(dialog); await screen.findByRole("region", { name: "Bogført åbningsbalance" });
  expect(apply).toHaveBeenCalledTimes(1); const body = apply.mock.calls[0]![1]; expect(body).toMatchObject({ input: lastInput, planHash: h, confirm: true });
  expect(body.input.lines).toEqual([{ accountNo: "1000", debitAmount: "100.00", creditAmount: "0.00", text: null }, { accountNo: "2000", debitAmount: "0.00", creditAmount: "100.00", text: null }]); expect(body.input.sourceArchiveImportIds).toEqual([41]); expect(body).not.toHaveProperty("actor");
  expect(screen.getByRole("link", { name: "Vis åbningspostering 2024-7" })).toHaveAttribute("href", "/companies/synthetic/posteringer?year=2024&source=live&journalEntryId=7"); expect(openingBalanceApi.read).toHaveBeenCalled();
});

test("server blockers stay visible and prevent confirmation; changed canonical source cannot be adopted", async () => {
  const apply = setup(); vi.spyOn(openingBalanceApi, "plan").mockImplementation(async (_slug, input) => ({ ...plan(input), status: "blocked", blockers: ["unbalanced_opening_source"] })); mount(); const dialog = await review(); await userEvent.click(within(dialog).getByRole("checkbox")); expect(within(dialog).getByText("unbalanced_opening_source")).toBeInTheDocument(); expect(within(dialog).getByRole("button", { name: "Bogfør åbningsbalance" })).toBeDisabled(); expect(apply).not.toHaveBeenCalled();
});

test("mismatched plan line is refused without any mutation", async () => {
  const apply = setup(); vi.spyOn(openingBalanceApi, "plan").mockImplementation(async (_slug, input) => { const value = plan(input); value.input.lines[0]!.debitAmount = "1.00"; return value; }); mount(); await fill(); await userEvent.click(screen.getByRole("button", { name: "Gennemgå åbningsbalance" })); await screen.findByText("Serverens plan stemmer ikke med de angivne kilder og linjer."); expect(screen.queryByRole("dialog")).toBeNull(); expect(apply).not.toHaveBeenCalled();
});

test("unknown outcome retains exact body/key and requires canonical state read before explicit original-receipt replay", async () => {
  const apply = setup().mockRejectedValueOnce(new ApiError("network", "Synthetic lost response", 0)); mount(); const dialog = await review(); await confirm(dialog); await screen.findByText("Synthetic lost response"); const sent = structuredClone(apply.mock.calls[0]![1]);
  const stored = Object.entries(sessionStorage).find(([key]) => key.includes("opening-balance-attempt"))?.[1] ?? ""; expect(stored).toContain(sent.idempotencyKey); expect(stored).not.toContain("Synthetic reviewed opening"); expect(stored).not.toContain("100.00");
  expect(within(dialog).getByRole("button", { name: "Hent oprindelig kvittering / prøv samme anmodning" })).toBeDisabled(); saved = record(); await userEvent.click(within(dialog).getByRole("button", { name: "Kontrollér aktuel åbningsbalance" })); await screen.findByText(/Den bogførte åbningsbalance svarer til planen/); expect(apply).toHaveBeenCalledTimes(1); await confirm(dialog, true); await screen.findByRole("region", { name: "Bogført åbningsbalance" }); expect(apply.mock.calls[1]![1]).toEqual(sent);
});

test("reopen reconstructs identical source and old key/hash even when current singleton blocks a fresh plan", async () => {
  const apply = setup().mockRejectedValueOnce(new ApiError("internal", "Synthetic unknown", 503)); const first = mount(); await confirm(await review()); await screen.findByText("Synthetic unknown"); const sent = structuredClone(apply.mock.calls[0]![1]); first.unmount(); saved = record(); vi.spyOn(openingBalanceApi, "plan").mockImplementation(async (_slug, input) => ({ ...plan(input), planHash: "b".repeat(64) })); mount(); const dialog = await review(); expect(within(dialog).getByText(h)).toBeInTheDocument(); await userEvent.click(within(dialog).getByRole("button", { name: "Kontrollér aktuel åbningsbalance" })); await screen.findByText(/Den bogførte åbningsbalance svarer til planen/); await confirm(dialog, true); await screen.findByRole("region", { name: "Bogført åbningsbalance" }); expect(apply.mock.calls[1]![1]).toEqual(sent);
});

test("known 422 releases attempt, while malformed receipt remains uncertain without false success", async () => {
  const apply = setup().mockRejectedValueOnce(new ApiError("closed_period", "Synthetic closed period", 422)); mount(); const dialog = await review(); await confirm(dialog); await screen.findByText("Synthetic closed period"); expect(within(dialog).queryByRole("button", { name: "Kontrollér aktuel åbningsbalance" })).toBeNull();
  apply.mockResolvedValueOnce({ ok: true, openingBalance: { ...record(), planHash: "b".repeat(64) }, errors: [], idempotency: { receiptId: 8, replayed: false, createdAt: "now", expiresAt: "later" } }); await confirm(dialog); await screen.findByText(/Kvitteringen kunne ikke bekræftes/); expect(within(dialog).getByRole("button", { name: "Hent oprindelig kvittering / prøv samme anmodning" })).toBeDisabled(); expect(screen.queryByRole("region", { name: "Bogført åbningsbalance" })).toBeNull();
});

test("existing opening is read-only and a company switch cannot expose a late plan", async () => {
  const apply = setup(); let resolve!: (value: OpeningPlan) => void; vi.spyOn(openingBalanceApi, "plan").mockImplementation(async (_slug, input) => { lastInput = input; return new Promise(done => { resolve = done; }); }); const view = mount(); await fill(); await userEvent.click(screen.getByRole("button", { name: "Gennemgå åbningsbalance" })); await waitFor(() => expect(resolve).toBeDefined()); saved = record(); await act(async () => { view.rerender(<MemoryRouter><StrictMode><MutationMemoryProvider><OpeningBalancePanel slug="another" /></MutationMemoryProvider></StrictMode></MemoryRouter>); resolve(plan(lastInput)); }); await screen.findByRole("region", { name: "Bogført åbningsbalance" }); expect(screen.queryByLabelText("Skæringsdato")).toBeNull(); expect(screen.queryByRole("dialog")).toBeNull(); expect(apply).not.toHaveBeenCalled();
});

test("storage failure prevents POST", async () => {
  const apply = setup(); mount(); const dialog = await review(); const original = sessionStorage; stubGlobal("sessionStorage", { getItem: (key: string) => original.getItem(key), setItem: (key: string, value: string) => { if (key.includes("opening-balance-attempt")) throw Error("Synthetic storage denied"); original.setItem(key, value); }, removeItem: (key: string) => original.removeItem(key), clear: () => original.clear() }); await confirm(dialog); await screen.findByText("Synthetic storage denied"); expect(apply).not.toHaveBeenCalled();
});

test("report notice uses canonical cutover/partial coverage and never claims full-year or archived carry-forward", () => {
  renderAt(<ReportSourceNotice slug="synthetic" report={{ fiscalYears: [{ label: "2024", source: "live", start: "2024-01-01", end: "2024-12-31" }], ledgerBasis: "native_ledger_only", openingBalanceBasis: { basis: "live", cutOverDate: "2024-07-01", openingJournalEntryId: 7, archiveAmountsIncluded: false }, liveCoverage: { status: "partial", reason: "native_history_begins_at_explicit_cutover", periodStart: "2024-01-01", periodEnd: "2024-12-31", cutOverDate: "2024-07-01" } }} />);
  const notice = screen.getByRole("status", { name: "Aktivt regnskab efter skæringsdato" }); expect(notice).toHaveTextContent("kun aktiv dækning fra 2024-07-01"); expect(notice).toHaveTextContent("Arkivbeløb er ikke medregnet"); expect(screen.getByRole("link", { name: "Vis bogført åbningsbalance" })).toHaveAttribute("href", "/companies/synthetic/posteringer?year=2024&source=live&journalEntryId=7");
});

test("reopened uncertainty refuses changed input instead of creating a replacement key or plan", async () => {
  const apply = setup().mockRejectedValueOnce(new ApiError("network", "Synthetic interrupted", 0)); const first = mount(); await confirm(await review()); await screen.findByText("Synthetic interrupted"); first.unmount(); mount(); await fill(); await userEvent.type(screen.getByLabelText("Note (valgfri)"), " altered"); await userEvent.click(screen.getByRole("button", { name: "Gennemgå åbningsbalance" })); await screen.findByText(/Det uafklarede forsøg kræver præcis samme dato/); expect(openingBalanceApi.plan).toHaveBeenCalledTimes(1); expect(apply).toHaveBeenCalledTimes(1); expect(screen.queryByRole("dialog")).toBeNull();
});

test("confirmed receipt with unavailable readback stays pending and a different singleton cannot enable replay", async () => {
  const apply = setup(); vi.spyOn(openingBalanceApi, "read").mockImplementation(async () => { if (saved) throw new ApiError("forbidden", "Synthetic readback refused", 403); return { ok: true, openingBalance: null, errors: [] }; }); mount(); const dialog = await review(); await confirm(dialog); await screen.findByText("Synthetic readback refused"); expect(apply).toHaveBeenCalledTimes(1); expect(screen.queryByRole("region", { name: "Bogført åbningsbalance" })).toBeNull(); expect(within(dialog).getByRole("button", { name: "Hent oprindelig kvittering / prøv samme anmodning" })).toBeDisabled(); vi.spyOn(openingBalanceApi, "read").mockResolvedValue({ ok: true, openingBalance: { ...record(), planHash: "e".repeat(64) }, errors: [] }); await userEvent.click(within(dialog).getByRole("button", { name: "Kontrollér aktuel åbningsbalance" })); await screen.findByText("Den aktuelle åbningsbalance tilhører en anden plan. Forsøget kan ikke gentages her."); expect(within(dialog).getByRole("button", { name: "Hent oprindelig kvittering / prøv samme anmodning" })).toBeDisabled(); expect(apply).toHaveBeenCalledTimes(1);
});


test("imported synthetic CSV opening uses the same read-only recorded-opening view", async () => {
  const apply = setup(); saved = { ...record(), sourceBasis: { kind: "synthetic_csv", currency: "DKK", sourceArchives: [] } }; mount();
  await screen.findByRole("region", { name: "Bogført åbningsbalance" });
  await userEvent.click(screen.getByText("Åbningsbalancens dokumentation", { exact: true }));
  expect(screen.getByText("CSV-kilden er bevaret i åbningsposteringens auditdokumentation.")).toBeVisible();
  expect(screen.queryByLabelText("Skæringsdato")).toBeNull(); expect(apply).not.toHaveBeenCalled();
});
