import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { AccountantExportCard } from "./AccountantExportCard";

test("an interrupted accountant export cannot be repeated through another click or its other placement", async () => {
  sessionStorage.clear();
  const fetchMock = vi.fn(async () => { throw new TypeError("stream lost after export"); });
  stubGlobal("fetch", fetchMock);
  const view = <MutationMemoryProvider><AccountantExportCard slug="synthetic" /></MutationMemoryProvider>;
  const first = renderAt(view, { route: "/companies/synthetic/manage" });
  await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
  await screen.findByText(/En ny skrivning er blokeret/);
  expect(screen.getByRole("button", { name: "Generér og download" })).toBeDisabled();
  first.unmount();
  renderAt(view, { route: "/companies/synthetic/overblik" });
  expect(screen.getByRole("button", { name: "Generér og download" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

test("a known export rejection allows a corrected request", async () => {
  sessionStorage.clear();
  const fetchMock = vi.fn(async () => Response.json({ ok: false, code: "bad_request", errors: ["Synthetic invalid period"] }, { status: 400 }));
  stubGlobal("fetch", fetchMock);
  renderAt(<MutationMemoryProvider><AccountantExportCard slug="synthetic" /></MutationMemoryProvider>);
  await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
  await screen.findByText("Synthetic invalid period");
  expect(screen.getByRole("button", { name: "Generér og download" })).toBeEnabled();
  await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
  expect(fetchMock).toHaveBeenCalledTimes(2);
});

test("SAF-T action sends only the reviewed period and confirmation to native export", async () => {
  sessionStorage.clear();
  const fetchMock = vi.fn(async () => Response.json({ ok: false, errors: ["Synthetic source original unavailable"] }, { status: 422 }));
  stubGlobal("fetch", fetchMock);
  renderAt(<MutationMemoryProvider><AccountantExportCard slug="synthetic" /></MutationMemoryProvider>);
  await userEvent.click(screen.getByRole("button", { name: "SAF-T v3 — generér og download" }));
  await screen.findByText("Synthetic source original unavailable");
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const call = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
  expect(call[0]).toBe("/api/companies/synthetic/saft-export");
  expect(call[1].method).toBe("POST");
  const body = JSON.parse(call[1].body as string);
  expect(Object.keys(body).sort()).toEqual(["confirm", "periodEnd", "periodStart"]);
  expect(body.confirm).toBe(true);
  expect(body.periodStart).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(body.periodEnd).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  expect(screen.getByText(/Ingen officiel skemavalidering eller indberetning/)).toBeInTheDocument();
});
