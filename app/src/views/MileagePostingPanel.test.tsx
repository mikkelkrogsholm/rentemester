import { expect, test } from "bun:test";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { AuthProvider } from "../lib/auth-context";
import { renderAt } from "../test/render";
import { mileage } from "../test/fixtures";
import { MileagePostingPanel } from "./MileagePostingPanel";
const rate = { version: "synthetic-v1", validFrom: "2026-01-01", validTo: "2026-12-31", vehicle: "car", ratePerKm: "2.00", basis: "Synthetic basis", source: "Synthetic source", expenseAccountNo: "4100", liabilityAccountNo: "6800", actor: "user:test", createdAt: "2026-01-01" };
const preview = { mileageEntryId: mileage().entries[0]!.id, entryNo: "MIL-test", tripDate: "2026-05-10", vehicle: "car", kilometers: 12.35, ratePerKm: "2.00", amount: 24.70, rateVersion: rate.version, expenseAccountNo: "4100", liabilityAccountNo: "6800", documentId: 7, documentHash: "b".repeat(64), previewHash: "a".repeat(64), appliedRules: ["NATIVE-MILEAGE-POST-1"] };
const receipt = { ...preview, journalEntryId: 12, journalEntryNo: "J-12", replayed: false };
function setup(mode = "normal") {
  const calls: Array<{ path: string; body: Record<string, unknown> | null }> = [];
  let posted = false;
  globalThis.fetch = (async (url, init) => {
    const path = String(url).split("/").at(-1)!;
    calls.push({ path, body: init?.body ? JSON.parse(String(init.body)) : null });
    if (path === "post" && mode === "unknown") { posted = true; throw new TypeError("Lost response"); }
    if (path === "post" && mode === "stale") return Response.json({ ok: false, code: "mileage_posting_refused", errors: ["Stale preview"] }, { status: 422 });
    if (path === "post") posted = true;
    const value = path === "rate-versions" ? { rateVersions: [rate] } : path === "postings" ? { postings: posted ? [receipt] : [] } : path === "post-preview" ? preview : receipt;
    return Response.json({ ok: true, mileagePosting: value });
  }) as typeof fetch;
  renderAt(<AuthProvider hosted={false} backend="rust"><MileagePostingPanel slug="synthetic" entries={mileage().entries} /></AuthProvider>);
  return calls;
}
async function select() {
  await screen.findByRole("option", { name: /synthetic-v1/ });
  await act(async () => {
  fireEvent.change(screen.getByLabelText("Kørsel"), { target: { value: String(preview.mileageEntryId) } });
  fireEvent.change(screen.getByLabelText("Takstversion"), { target: { value: rate.version } });
  fireEvent.change(screen.getByLabelText("Bilags-id"), { target: { value: "7" } });
  });
}
async function confirm() {
  fireEvent.click(screen.getByRole("button", { name: "Forhåndsvis bogføring" }));
  fireEvent.click(await screen.findByRole("button", { name: "Bogfør kørsel" }));
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Bekræft bogføring" }));
}
test("missing input cannot preview or post; changed input invalidates preview", async () => {
  const calls = setup();
  await screen.findByRole("option", { name: /synthetic-v1/ });
  expect(screen.getByRole("button", { name: "Forhåndsvis bogføring" })).toBeDisabled();
  await select();
  fireEvent.click(screen.getByRole("button", { name: "Forhåndsvis bogføring" }));
  await screen.findByRole("button", { name: "Bogfør kørsel" });
  await act(async () => { fireEvent.change(screen.getByLabelText("Bilags-id"), { target: { value: "8" } }); });
  expect(screen.queryByRole("button", { name: "Bogfør kørsel" })).toBeNull();
  expect(calls.filter(c => c.path === "post")).toHaveLength(0);
});
test("server preview then explicit confirmation and canonical posted receipt", async () => {
  const calls = setup(); await select();
  fireEvent.click(screen.getByRole("button", { name: "Forhåndsvis bogføring" }));
  fireEvent.click(await screen.findByRole("button", { name: "Bogfør kørsel" }));
  expect(calls.filter(c => c.path === "post")).toHaveLength(0);
  fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Bekræft bogføring" }));
  await screen.findByText(/journal J-12/);
  expect(calls.find(c => c.path === "post")!.body).toMatchObject({ expectedPreviewHash: preview.previewHash, confirm: true, documentId: 7 });
  expect(screen.queryByRole("button", { name: "Bogfør kørsel" })).toBeNull();
});
test("stale refusal is not success", async () => {
  setup("stale"); await select(); await confirm();
  await screen.findByText("Stale preview");
  expect(screen.queryByText(/journal J-12/)).toBeNull();
});
test("unknown post outcome offers readback and never retries", async () => {
  const calls = setup("unknown"); await select(); await confirm();
  await waitFor(() => expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Bekræft bogføring" })).toBeDisabled());
  fireEvent.click(screen.getByRole("button", { name: "Annullér" }));
  fireEvent.click(screen.getByRole("button", { name: "Opdatér bogføringsstatus" }));
  await screen.findByText(/journal J-12/);
  expect(calls.filter(c => c.path === "post")).toHaveLength(1);
});
