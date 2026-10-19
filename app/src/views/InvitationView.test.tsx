import { afterEach, describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvitationView } from "./InvitationView";
import { mockFetch } from "../test/fixtures";
import { renderAt } from "../test/render";
import { restoreGlobals } from "../test/globals";

afterEach(() => restoreGlobals());

describe("InvitationView", () => {
  test("consumes the fragment token once, clears it from the URL and never persists it", async () => {
    const token = "x".repeat(43);
    window.history.replaceState(null, "", `/invite#token=${token}`);
    mockFetch({
      "POST /api/invitations/claim": {
        accepted: true, accessReady: false, nextStep: "verify-email-and-enable-mfa",
      },
    });
    renderAt(<InvitationView />, { route: `/invite#token=${token}`, path: "/invite" });
    expect(window.location.hash).toBe("");
    await userEvent.type(screen.getByLabelText("Navn"), "Inviteret Bruger");
    await userEvent.type(screen.getByLabelText("Adgangskode"), "meget-hemmelig-adgangskode");
    await userEvent.click(screen.getByRole("button", { name: "Acceptér invitation" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Invitationen er accepteret");
    const call = (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.find(([url]) =>
      String(url) === "/api/invitations/claim"
    );
    expect(JSON.parse(String((call![1] as RequestInit).body))).toEqual({
      token,
      name: "Inviteret Bruger",
      password: "meget-hemmelig-adgangskode",
    });
    expect(sessionStorage.length).toBe(0);
    expect(localStorage.length).toBe(0);
  });

  test("fails locally without sending a request when the fragment token is absent", async () => {
    window.history.replaceState(null, "", "/invite");
    const mocked = mockFetch({});
    void mocked;
    renderAt(<InvitationView />, { route: "/invite", path: "/invite" });
    await userEvent.type(screen.getByLabelText("Navn"), "Inviteret Bruger");
    await userEvent.type(screen.getByLabelText("Adgangskode"), "meget-hemmelig-adgangskode");
    await userEvent.click(screen.getByRole("button", { name: "Acceptér invitation" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Invitationen er ugyldig eller udløbet");
    expect(fetch).not.toHaveBeenCalled();
  });
});

test("does not resend an unconfirmed claim and clears its bearer", async () => {
  window.history.replaceState(null, "", `/invite#token=${"x".repeat(43)}`);
  mockFetch({});
  (fetch as unknown as ReturnType<typeof vi.fn>).mockImplementation(async () => new Response(JSON.stringify({ok:false,code:"internal",errors:["outcome_unconfirmed"]}),{status:503,headers:{"content-type":"application/json"}}));
  renderAt(<InvitationView />, {route:"/invite",path:"/invite"});
  await userEvent.type(screen.getByLabelText("Navn"),"Synthetic Invited");
  await userEvent.type(screen.getByLabelText("Adgangskode"),"synthetic-password-456");
  await userEvent.click(screen.getByRole("button",{name:"Acceptér invitation"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("Accepten sendes ikke igen");
  expect(screen.getByRole("button",{name:"Acceptér invitation"})).toBeDisabled();
  expect(screen.getByLabelText("Adgangskode")).toHaveValue("");
});
