import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AccountantExportCard } from "./AccountantExportCard";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { renderAt } from "../test/render";
import { stubGlobal } from "../test/globals";

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
