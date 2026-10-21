import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeBackupRemote } from "./NativeBackupRemote";

const props = { workspace: "synthetic-workspace", destination: "synthetic-drive", placementOperationId: "synthetic-placement-114" };
const receipt = (intent: any) => ({ ...intent, destinationId: props.destination, archiveSha256: "a".repeat(64), archiveSizeBytes: 456, observedAt: "2026-10-21T00:00:00Z", remoteSyncVerified: true });
async function reviewed() {
  await userEvent.type(screen.getByRole("textbox", { name: "Drive-filens id" }), "synthetic_drive_object");
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg vil kontrollere/ }));
  await userEvent.click(screen.getByRole("button", { name: /^Kontrollér fjernkopien$/ }));
}
test("lost remote verification reply restores exact intent without automatic mutation", async () => {
  localStorage.clear(); let intent: any;
  const mock = vi.fn(async (input: string | URL, init?: RequestInit) => {
    if (init?.method === "POST") { intent = JSON.parse(String(init.body)); throw Error("Synthetic reply lost"); }
    expect(String(input).endsWith(`/remote-verifications/${intent.operationId}`)).toBe(true);
    return Response.json({ ok: true, verification: receipt(intent) });
  });
  stubGlobal("fetch", mock);
  const first = renderAt(<NativeBackupRemote {...props} />);
  await reviewed(); await screen.findByRole("alert");
  expect(intent.placementOperationId).toBe(props.placementOperationId);
  first.unmount(); renderAt(<NativeBackupRemote {...props} placementOperationId="new-placement-114" />);
  expect(mock).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole("button", { name: "Hent fjernkopiens kvittering" }));
  await screen.findByText(/Serveren verificerede fjernkopiens bytes/);
  expect(mock.mock.calls.filter(([, init]) => init?.method === "POST")).toHaveLength(1);
  await userEvent.click(screen.getByRole("button", { name: "Klargør ny fjernkontrol" }));
  expect(localStorage.getItem(`rentemester-remote-backup:${props.workspace}:${props.destination}`)).toBeNull();
});
test("missing receipt requires explicit same-id retry", async () => {
  localStorage.clear(); const posts: any[] = [];
  const mock = vi.fn(async (_input: string | URL, init?: RequestInit) => {
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body)); posts.push(body);
      if (posts.length === 1) throw Error("Synthetic reply lost");
      return Response.json({ ok: true, verification: receipt(body) });
    }
    return Response.json({ ok: true, verification: null });
  });
  stubGlobal("fetch", mock); renderAt(<NativeBackupRemote {...props} />);
  await reviewed(); await screen.findByRole("alert");
  expect(screen.queryByRole("button", { name: "Gentag kontrollen med samme id" })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Hent fjernkopiens kvittering" }));
  await screen.findByText(/Der er endnu ingen kvittering/);
  expect(posts).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Gentag kontrollen med samme id" })).toBeDisabled();
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg vil kontrollere/ }));
  await userEvent.click(screen.getByRole("button", { name: "Gentag kontrollen med samme id" }));
  await screen.findByText(/Serveren verificerede fjernkopiens bytes/);
  expect(posts[1]).toEqual(posts[0]);
});
test("wrong object can be corrected while retaining the prior operation for reconciliation", async () => {
  localStorage.clear(); const posts: any[] = [];
  stubGlobal("fetch", vi.fn(async (_input: string | URL, init?: RequestInit) => {
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body)); posts.push(body);
      return posts.length === 1
        ? Response.json({ ok: false, errors: ["Wrong synthetic object"], outcomeUnknown: false }, { status: 422 })
        : Response.json({ ok: true, verification: receipt(body) });
    }
    return Response.json({ ok: true, verification: null });
  }));
  renderAt(<NativeBackupRemote {...props} />);
  await reviewed(); await screen.findByRole("alert");
  await userEvent.click(screen.getByRole("button", { name: "Hent fjernkopiens kvittering" }));
  await userEvent.click(await screen.findByRole("button", { name: "Gem kontrol-id og ret fil-id" }));
  expect(JSON.parse(localStorage.getItem(`rentemester-remote-backup:${props.workspace}:${props.destination}:history`) ?? "[]")[0].operationId).toBe(posts[0].operationId);
  await userEvent.type(screen.getByRole("textbox", { name: "Drive-filens id" }), "corrected_synthetic_object");
  await userEvent.click(screen.getByRole("checkbox", { name: /Jeg vil kontrollere/ }));
  await userEvent.click(screen.getByRole("button", { name: /^Kontrollér fjernkopien$/ }));
  await screen.findByText(/Serveren verificerede fjernkopiens bytes/);
  expect(posts[1].remoteObjectId).toBe("corrected_synthetic_object");
  expect(posts[1].operationId).not.toBe(posts[0].operationId);
  await userEvent.click(screen.getByRole("button", { name: "Klargør ny fjernkontrol" }));
  await userEvent.click(screen.getByRole("button", { name: `Åbn tidligere kontrol ${posts[0].operationId}` }));
  expect(JSON.parse(localStorage.getItem(`rentemester-remote-backup:${props.workspace}:${props.destination}`) ?? "null").operationId).toBe(posts[0].operationId);
  expect(posts).toHaveLength(2);
});
test("disabled provider preserves manual receipt reconciliation without a new POST", async () => {
  localStorage.clear();
  const intent = { operationId: "synthetic-remote-114", placementOperationId: props.placementOperationId, remoteObjectId: "synthetic_drive_object" };
  localStorage.setItem(`rentemester-remote-backup:${props.workspace}:${props.destination}`, JSON.stringify(intent));
  const mock = vi.fn(async () => Response.json({ ok: true, verification: receipt(intent) }));
  stubGlobal("fetch", mock); renderAt(<NativeBackupRemote {...props} enabled={false} />);
  expect(mock).toHaveBeenCalledTimes(0);
  expect(screen.queryByRole("button", { name: /^Kontrollér fjernkopien$/ })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Hent fjernkopiens kvittering" }));
  await screen.findByText(/Serveren verificerede fjernkopiens bytes/);
  expect(mock).toHaveBeenCalledTimes(1);
});
