import { expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { stubGlobal } from "../test/globals";
import { renderAt } from "../test/render";
import { NativeBackupRegistrations } from "./NativeBackupRegistrations";
const storage = "rentemester:backup-registrations:synthetic-workspace:synthetic";
const initial = () => ({ ok: true, workspaceId: "synthetic-workspace", slug: "synthetic", destinations: [] as any[], history: [] as any[], backups: [], operatorTargets: [], errors: [] });
async function fill() {
  await userEvent.type(screen.getByLabelText("Destinationsnavn"), "Synthetic SSH");
  await userEvent.type(screen.getByLabelText("Placering eller beskrivelse"), "Synthetic external store");
  await userEvent.type(screen.getByLabelText("Attesteret af"), "Synthetic owner");
  await userEvent.click(screen.getByRole("checkbox", { name: "Jeg har gennemgået registreringen." }));
  await userEvent.click(screen.getByRole("button", { name: "Registrér destination" }));
}
test("lost accepted registration reply reconciles exact operation after remount without automatic POST", async () => {
  localStorage.clear(); let intent: any; let operation: any;
  const mock = vi.fn(async (path: string | URL, init?: RequestInit) => {
    if (init?.method === "POST") {
      intent = JSON.parse(String(init.body)); operation = { format: "rentemester-native-backup-registration-operation/1", ...intent, actor: "user:synthetic-owner", createdAt: "2026-10-21T00:00:00Z", result: { ...intent, active: true } };
      throw Error("Synthetic accepted response lost");
    }
    if (String(path).includes("/operations/")) return Response.json({ ok: true, workspaceId: "synthetic-workspace", slug: "synthetic", operation });
    const value = initial(); if (operation) { value.destinations.push(operation.result); value.history.push(operation); } return Response.json(value);
  }); stubGlobal("fetch", mock);
  const first = renderAt(<NativeBackupRegistrations slug="synthetic" />);
  expect(mock).toHaveBeenCalledTimes(0);
  await userEvent.click(screen.getByRole("button", { name: "Hent registrerede destinationer" }));
  await fill(); await screen.findByRole("alert");
  expect(JSON.parse(localStorage.getItem(storage)!).operationId).toBe(intent.operationId);
  expect(intent.inEeaOrEu).toBe(false); expect(intent.nonRelatedParty).toBe(false);
  first.unmount(); renderAt(<NativeBackupRegistrations slug="synthetic" />);
  expect(mock.mock.calls.filter(([, options]) => options?.method === "POST")).toHaveLength(1);
  await userEvent.click(screen.getByRole("button", { name: "Hent registrerede destinationer" }));
  await userEvent.click(screen.getByRole("button", { name: "Hent registreringens resultat" }));
  await screen.findByRole("status");
  expect(localStorage.getItem(storage)).toBeNull();
  expect(mock.mock.calls.filter(([, options]) => options?.method === "POST")).toHaveLength(1);
});
test("missing operation can be retained and manually reopened without losing its identity", async () => {
  localStorage.clear(); const intent = { action: "remove", operationId: "synthetic-operation-116", destinationId: "synthetic-destination-116", confirm: true };
  localStorage.setItem(storage, JSON.stringify(intent));
  const mock = vi.fn(async (path: string | URL) => Response.json(String(path).includes("/operations/") ? { ok: true, workspaceId: "synthetic-workspace", slug: "synthetic", operation: null } : initial()));
  stubGlobal("fetch", mock); renderAt(<NativeBackupRegistrations slug="synthetic" />);
  await userEvent.click(screen.getByRole("button", { name: "Hent registrerede destinationer" }));
  await userEvent.click(screen.getByRole("button", { name: "Hent registreringens resultat" }));
  await userEvent.click(screen.getByRole("button", { name: "Bevar kontrol-id og frigiv formularen" }));
  expect(JSON.parse(localStorage.getItem(`${storage}:history`)!).at(-1)).toEqual(intent);
  await userEvent.click(screen.getByRole("button", { name: `Åbn gemt registrering ${intent.operationId}` }));
  expect(JSON.parse(localStorage.getItem(storage)!)).toEqual(intent);
  expect(mock).toHaveBeenCalledTimes(2);
});
