import { expect, test, vi } from "bun:test";
import { act, screen } from "@testing-library/react";
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

test("an export completed after leaving its company view cannot start a download", async () => {
  sessionStorage.clear();
  let complete!: (response: Response) => void;
  stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { complete = resolve; })));
  const download = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const objectUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:synthetic");
  try {
    const view = renderAt(<MutationMemoryProvider><AccountantExportCard slug="old-company" /></MutationMemoryProvider>);
    await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
    view.unmount();
    renderAt(<MutationMemoryProvider><AccountantExportCard slug="new-company" /></MutationMemoryProvider>);
    await act(async () => {
      complete(new Response("synthetic archive", { headers: { "content-disposition": "attachment; filename*=UTF-8''old-company.tar" } }));
    });
    expect(download).not.toHaveBeenCalled();
    expect(objectUrl).not.toHaveBeenCalled();
    expect(screen.queryByText(/Hentede old-company/)).toBeNull();
  } finally {
    download.mockRestore();
    objectUrl.mockRestore();
  }
});

test("a completed export in its original view downloads exactly once and releases the object URL", async () => {
  sessionStorage.clear();
  stubGlobal("fetch", vi.fn(async () => new Response("synthetic archive", { headers: { "content-disposition": "attachment; filename*=UTF-8''synthetic.tar" } })));
  const download = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const objectUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:synthetic");
  const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  try {
    renderAt(<MutationMemoryProvider><AccountantExportCard slug="synthetic" /></MutationMemoryProvider>);
    await userEvent.click(screen.getByRole("button", { name: "Generér og download" }));
    await screen.findByText(/Hentede synthetic.tar/);
    expect(download).toHaveBeenCalledTimes(1);
    expect(revoke).toHaveBeenCalledWith("blob:synthetic");
  } finally {
    download.mockRestore();
    objectUrl.mockRestore();
    revoke.mockRestore();
  }
});
