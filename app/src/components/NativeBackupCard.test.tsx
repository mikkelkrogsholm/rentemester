import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { MutationMemoryProvider } from "../lib/mutation-memory";
import { NativeBackupCard } from "./NativeBackupCard";

function renderCard() {
  return renderAt(<MutationMemoryProvider><NativeBackupCard /></MutationMemoryProvider>);
}
test("workspace backup requires review and downloads the exact server blob without claiming restore verification", async () => {
  sessionStorage.clear();
  const bytes = '{"synthetic":"signed archive bytes"}';
  const fetchMock = vi.fn(async () => new Response(bytes, {headers:{"content-type":"application/vnd.rentemester.native-backup+json"}}));
  stubGlobal("fetch", fetchMock);
  const create = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:synthetic-backup");
  const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  try {
    renderCard();
    const button = screen.getByRole("button", {name:"Dan og download backup"});
    expect(button).toBeDisabled();
    expect(fetchMock).toHaveBeenCalledTimes(0);
    await userEvent.click(screen.getByText("Hvis computeren eller adgangen mistes"));
    expect(screen.getByText(/En kopi på den mistede disk/)).toBeVisible();
    expect(screen.getByText(/Ved lokal drift er du selv operatør/)).toBeVisible();
    expect(screen.getByText(/En ny nøgle eller en nøgle fra selve arkivet/)).toBeVisible();
    expect(fetchMock).toHaveBeenCalledTimes(0);
    await userEvent.click(screen.getByRole("checkbox"));
    await userEvent.click(button);
    await screen.findByText(/Downloadet er ikke en gendannelsesprøve/);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [path, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(path).toBe("/api/workspace/backup");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toEqual({confirm:true});
    expect(await (create.mock.calls[0]![0] as Blob).text()).toBe(bytes);
    expect(click).toHaveBeenCalledTimes(1);
    expect(revoke).toHaveBeenCalledWith("blob:synthetic-backup");
    expect(button).toBeDisabled();
  } finally { create.mockRestore(); revoke.mockRestore(); click.mockRestore(); }
});

test("missing operator key is visible and never retried automatically", async () => {
  sessionStorage.clear();
  const fetchMock = vi.fn(async () => Response.json({ok:false,code:"backup_unavailable",errors:["Synthetic backup unavailable"]},{status:503}));
  stubGlobal("fetch", fetchMock);
  renderCard();
  await userEvent.click(screen.getByRole("checkbox"));
  await userEvent.click(screen.getByRole("button", {name:"Dan og download backup"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("Synthetic backup unavailable");
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

test("an interrupted backup request remains blocked after leaving and returning", async () => {
  sessionStorage.clear();
  const fetchMock = vi.fn(async () => { throw new TypeError("Synthetic disconnected response"); });
  stubGlobal("fetch", fetchMock);
  const view = renderCard();
  await userEvent.click(screen.getByRole("checkbox"));
  await userEvent.click(screen.getByRole("button", {name:"Dan og download backup"}));
  await screen.findByText(/En ny skrivning er blokeret/);
  view.unmount(); renderCard();
  expect(screen.getByRole("button", {name:"Dan og download backup"})).toBeDisabled();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});
