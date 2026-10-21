import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeAuditLog } from "./NativeAuditLog";
function page(offset: number) {
  return { ok: true, slug: "synthetic", rows: [{ id: 2 - offset, eventType: "journal_post", entityType: "journal_entry", entityId: "1", message: "<script>inert audit text</script>", actor: "user:synthetic", createdAt: "2026-10-21T00:00:00Z" }], total: 2, count: 1, limit: 1, offset, hasMore: offset === 0, ...(offset === 0 ? { nextOffset: 1 } : {}) };
}
test("explicit filter read, plain audit text and pagination keep submitted filters", async () => {
  const urls: URL[] = [];
  stubGlobal("fetch", vi.fn(async (path: string | URL, init?: RequestInit) => { expect(init?.method).not.toBe("POST"); const url = new URL(String(path), "http://localhost"); urls.push(url); return Response.json(page(Number(url.searchParams.get("offset")))); }));
  renderAt(<NativeAuditLog slug="synthetic" />); expect(urls).toHaveLength(0);
  await userEvent.type(screen.getByLabelText("Aktør indeholder"), "synthetic");
  await userEvent.clear(screen.getByLabelText("Rækker pr. side")); await userEvent.type(screen.getByLabelText("Rækker pr. side"), "1");
  await userEvent.click(screen.getByRole("button", { name: "Hent revisionsspor" }));
  await screen.findByText("<script>inert audit text</script>");
  expect(document.querySelector("script")).toBeNull();
  await userEvent.clear(screen.getByLabelText("Aktør indeholder")); await userEvent.type(screen.getByLabelText("Aktør indeholder"), "unsent");
  await userEvent.click(screen.getByRole("button", { name: "Næste side" }));
  await screen.findByText("2 hændelser matcher. Viser 2–2.");
  expect(urls[1]!.searchParams.get("actorLike")).toBe("synthetic"); expect(urls[1]!.searchParams.get("offset")).toBe("1");
  expect(screen.getByRole("button", { name: "Næste side" })).toBeDisabled();
});
test("a refused read clears earlier events instead of presenting stale history", async () => {
  let calls = 0; stubGlobal("fetch", vi.fn(async () => ++calls === 1 ? Response.json(page(0)) : Response.json({ ok: false, code: "access_denied", errors: ["access_denied"] }, { status: 403 })));
  renderAt(<NativeAuditLog slug="synthetic" />);
  await userEvent.click(screen.getByRole("button", { name: "Hent revisionsspor" })); await screen.findByText("user:synthetic");
  await userEvent.click(screen.getByRole("button", { name: "Hent revisionsspor" })); await screen.findByRole("alert");
  expect(screen.queryByText("user:synthetic")).not.toBeInTheDocument();
});
