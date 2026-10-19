import { afterEach, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ReviewedBankQueue } from "./ReviewedBankQueue";
import { PostingRulesView } from "./PostingRulesView";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { restoreGlobals } from "../test/globals";
afterEach(() => restoreGlobals());
const path = "/api/companies/acme-aps/reviewed-bank";
test("bank evidence creates only one explicitly confirmed draft and links to separate review", async () => {
 mockFetch({ [`GET ${path}`]: { queue: { rows: [{ bankTransactionId: 7, date: "2026-01-01", text: "Synthetic capital", amount: 100.01, currency: "DKK", bankAccountNo: "B", suggestedAccount: "E", sourceHash: "source-hash" }], truncated: false } }, [`POST ${path}`]: { accountingDraft: { id: "bank-7" } } });
 renderAt(<ReviewedBankQueue />, { route: "/companies/acme-aps/batchbogfoering", path: "/companies/:slug/batchbogfoering" });
 await userEvent.click(await screen.findByRole("button", { name: "Opret bankkladde" }));
 await userEvent.click(screen.getByRole("button", { name: "Gennemgå oprettelse" }));
 expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(0);
 await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Opret kladde" }));
 await screen.findByRole("status");
 const writes = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST"); expect(writes).toHaveLength(1);
 expect(JSON.parse(writes[0][1].body)).toEqual({ bankTransactionId: 7, expectedSourceHash: "source-hash", accountNo: "E", draftId: "bank-7", confirm: true });
 expect(screen.getByRole("link", { name: "Åbn kladder og indsend til uafhængig godkendelse" }).getAttribute("href")).toBe("/companies/acme-aps/kladder");
});
test("posting-rule authoring binds exact bank text and requires explicit confirmation", async () => {
 mockFetch({ "GET /api/companies/acme-aps/posting-rules": { postingRules: [] }, "POST /api/companies/acme-aps/posting-rules/propose": { postingRule: { ok: true } } });
 renderAt(<PostingRulesView />, { route: "/companies/acme-aps/posteringsregler", path: "/companies/:slug/posteringsregler" });
 await screen.findByText("Ingen posteringsregler endnu.");
 await userEvent.type(screen.getByLabelText("Regel-id"), "synthetic-capital"); await userEvent.type(screen.getByLabelText("Præcis banktekst"), "Synthetic capital"); await userEvent.type(screen.getByLabelText("Foreslået modkonto"), "E"); await userEvent.type(screen.getByLabelText("Begrundelse"), "Reviewed synthetic source");
 await userEvent.click(screen.getByRole("button", { name: "Gennemgå forslag" }));
 await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Bekræft" }));
 const writes = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST"); expect(writes).toHaveLength(1);
 const body = JSON.parse(writes[0][1].body); expect(body.confirm).toBe(true); expect(body.proposal.conditions).toEqual({ currency: "DKK", attributes: { bankText: "Synthetic capital" } }); expect(body.proposal.creator).toBeUndefined();
});

test("unconfirmed bank draft blocks another write even after a successful queue refresh", async () => {
 mockFetch({ [`GET ${path}`]: { queue: { rows: [{ bankTransactionId: 7, date: "2026-01-01", text: "Synthetic capital", amount: 100.01, currency: "DKK", bankAccountNo: "B", suggestedAccount: "E", sourceHash: "source-hash" }], truncated: false } }, [`POST ${path}`]: { __error: { code: "internal", message: "Resultatet kan ikke bekræftes" } } });
 renderAt(<ReviewedBankQueue />, { route: "/companies/acme-aps/batchbogfoering", path: "/companies/:slug/batchbogfoering" });
 await userEvent.click(await screen.findByRole("button", { name: "Opret bankkladde" })); await userEvent.click(screen.getByRole("button", { name: "Gennemgå oprettelse" }));
 await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Opret kladde" }));
 expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Opret kladde" })).toBeDisabled();
 const refresh = within(screen.getByRole("dialog")).getByRole("button", { name: /Kontrollér status/ }); await userEvent.click(refresh);
 expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Opret kladde" })).toBeDisabled();
 expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
});
