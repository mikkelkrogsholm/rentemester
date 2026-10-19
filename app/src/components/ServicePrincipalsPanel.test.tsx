import { afterEach, expect, test, vi } from "bun:test";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { stubGlobal, restoreGlobals } from "../test/globals";
import { ServicePrincipalsPanel } from "./ServicePrincipalsPanel";
afterEach(() => { sessionStorage.clear(); restoreGlobals(); });
const secret = `rms_${"a".repeat(64)}`;
const posts = () => (fetch as any).mock.calls.filter(([, init]: any[]) => init?.method === "POST");
async function create() {
  await screen.findByRole("heading", { name: "Servicekonti" });
  await userEvent.type(screen.getByLabelText("Navn på servicekonto"), "Synthetic worker");
  await userEvent.click(screen.getByRole("button", { name: "Opret servicekonto" }));
  await userEvent.click(screen.getByRole("button", { name: "Gennemfør servicehandling" }));
}
test("create shows a one-time secret without persisting or granting access; explicit activation is separate", async () => {
  const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
    const body = init?.body ? JSON.parse(String(init.body)) : {};
    return new Response(JSON.stringify(init?.method === "POST" ? { ok: true, operationId: body.operationId, serviceAccountId: "svc", credentialId: "key", secret } : { ok: true, principals: [] }), { status: init?.method === "POST" ? 201 : 200, headers: { "content-type": "application/json" } });
  });
  stubGlobal("fetch", fetchMock);
  const view = renderAt(<ServicePrincipalsPanel onChanged={() => {}} />);
  await create();
  await waitFor(() => expect(screen.getByLabelText("Engangsnøgle")).toHaveValue(secret));
  expect(posts()).toHaveLength(1);
  expect(JSON.parse(posts()[0][1].body)).toMatchObject({ displayName: "Synthetic worker", confirm: true });
  expect(sessionStorage.getItem("rentemester:service-operation:v1")).toBeNull();
  expect(JSON.stringify(sessionStorage)).not.toContain(secret);
  view.unmount(); renderAt(<ServicePrincipalsPanel onChanged={() => {}} />);
  await waitFor(() => expect(screen.queryByText("Henter servicekonti…")).toBeNull());
  expect(screen.queryByLabelText("Engangsnøgle")).toBeNull();
});
test("lost creation response survives remount as an operation ID and only explicit recovery is sent", async () => {
  stubGlobal("fetch", vi.fn(async (_url: string, init?: RequestInit) => {
    if (init?.method === "POST") throw new Error("synthetic lost response");
    return new Response(JSON.stringify({ ok: true, principals: [] }), { headers: { "content-type": "application/json" } });
  }));
  const view = renderAt(<ServicePrincipalsPanel onChanged={() => {}} />); await create();
  await screen.findByText(/En servicehandling kræver afklaring/);
  const operationId = sessionStorage.getItem("rentemester:service-operation:v1")!;
  expect(operationId).toMatch(/^[0-9a-f-]{36}$/);
  expect(posts()).toHaveLength(1);
  view.unmount();
  mockFetch({ "GET /api/workspace/service-principals": { principals: [] }, "POST /api/workspace/service-principals/recover": { operationId, status: "completed", recovered: false, serviceAccountId: "svc", credentialId: "key" } });
  renderAt(<ServicePrincipalsPanel onChanged={() => {}} />);
  expect(screen.getByRole("button", { name: "Opret servicekonto" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Afklar servicehandling" }));
  await userEvent.click(screen.getByRole("button", { name: "Gennemfør servicehandling" }));
  await screen.findByText(/Operationsstatus: completed/);
  expect(posts()).toHaveLength(1); expect(posts()[0][0]).toContain("/recover");
  expect(JSON.parse(posts()[0][1].body)).toEqual({ operationId, confirm: true });
  expect(sessionStorage.getItem("rentemester:service-operation:v1")).toBeNull();
});
test("native 429 refusal clears an unissued operation without retrying or creating an unknown lock", async () => {
  stubGlobal("fetch", vi.fn(async (_url: string, init?: RequestInit) => new Response(JSON.stringify(init?.method === "POST" ? { message: "Request refused" } : { ok: true, principals: [] }), { status: init?.method === "POST" ? 429 : 200, headers: { "content-type": "application/json" } })));
  renderAt(<ServicePrincipalsPanel onChanged={() => {}} />); await create();
  await screen.findByText(/Der er foretaget for mange servicehandlinger/);
  expect(sessionStorage.getItem("rentemester:service-operation:v1")).toBeNull();
  expect(screen.queryByText(/En servicehandling kræver afklaring/)).toBeNull();
  expect(posts()).toHaveLength(1);
  await userEvent.click(screen.getByRole("button", { name: "Annullér" }));
  expect(screen.getByRole("button", { name: "Opret servicekonto" })).not.toBeDisabled();
  expect(posts()).toHaveLength(1);
});
