import { describe, expect, test, vi, beforeEach } from "bun:test";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, useLocation } from "react-router-dom";
import { stubGlobal } from "./test/globals";

const authMocks = {
  getSession: vi.fn(), signOut: vi.fn(), revokeSessions: vi.fn(),
  listSessions: vi.fn(), revokeSession: vi.fn(),
  changePassword: vi.fn(),
  signInEmail: vi.fn(), verifyTotp: vi.fn(), verifyBackupCode: vi.fn(), enable: vi.fn(), sendVerificationEmail: vi.fn(), requestPasswordReset: vi.fn(), resetPassword: vi.fn(),
};

vi.mock("./lib/auth-client", () => ({
  authClient: {
    getSession: authMocks.getSession,
    signOut: authMocks.signOut,
    revokeSessions: authMocks.revokeSessions,
    listSessions: authMocks.listSessions,
    revokeSession: authMocks.revokeSession,
    changePassword: authMocks.changePassword,
    signIn: { email: authMocks.signInEmail },
    twoFactor: { verifyTotp: authMocks.verifyTotp, verifyBackupCode: authMocks.verifyBackupCode, enable: authMocks.enable },
    sendVerificationEmail: authMocks.sendVerificationEmail,
    requestPasswordReset: authMocks.requestPasswordReset,
    resetPassword: authMocks.resetPassword,
  },
}));

import { App } from "./App";
import { request } from "./lib/api/_shared";
import { mockFetch } from "./test/fixtures";

function hostedFetch(extra: Record<string, unknown> = {}) {
  stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL) => {
    const path = String(input).replace(/^https?:\/\/[^/]+/, "").split("?")[0];
    if (path === "/api/health") return new Response(JSON.stringify({ ok: true, deploymentProfile: "hosted" }), { headers: { "content-type": "application/json" } });
    if (path === "/api/me") return new Response(JSON.stringify({ ok: true, user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true }, workspaceRole: "workspace_owner", companies: [{ slug: "allowed", name: "Allowed ApS", role: "owner", archived: false }], ...extra }), { headers: { "content-type": "application/json" } });
    if (path === "/api/portfolio") throw new Error("portfolio must not be fetched before auth is ready");
    throw new Error(`unexpected ${path}`);
  }));
}

function LocationPath() { const location = useLocation(); return <output data-testid="location-path">{`${location.pathname}${location.search}`}</output>; }
function renderApp(entry = "/") { return render(<MemoryRouter initialEntries={[entry]}><App /><LocationPath /></MemoryRouter>); }

describe("hosted cockpit auth shell", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    authMocks.signOut.mockResolvedValue({ data: {} });
    authMocks.revokeSessions.mockResolvedValue({ data: {} });
    authMocks.listSessions.mockResolvedValue({ data: [] });
    authMocks.revokeSession.mockResolvedValue({ data: { status: true } });
    authMocks.changePassword.mockResolvedValue({ data: { user: { id: "u1" } } });
    authMocks.requestPasswordReset.mockResolvedValue({ data: { status: true } });
    authMocks.sendVerificationEmail.mockResolvedValue({ data: { status: true } });
    authMocks.resetPassword.mockResolvedValue({ data: { status: true } });
  });

  test("no hosted session shows login and never fetches portfolio", async () => {
    authMocks.getSession.mockResolvedValue({ data: null }); hostedFetch(); renderApp();
    expect(await screen.findByRole("heading", { name: "Log ind" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /opret|sign up/i })).not.toBeInTheDocument();
  });

  test("forgot-password returns the same generic response for an existing or absent e-mail", async () => {
    authMocks.getSession.mockResolvedValue({ data: null }); hostedFetch(); renderApp("/forgot-password");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "missing@example.test");
    await user.click(screen.getByRole("button", { name: "Send reset-link" }));
    expect(authMocks.requestPasswordReset).toHaveBeenCalledWith({ email: "missing@example.test", redirectTo: `${window.location.origin}/reset-password` });
    expect(await screen.findByText("Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.")).toBeInTheDocument();
    authMocks.requestPasswordReset.mockResolvedValueOnce({ data: null, error: { message: "provider detail" } });
    await user.type(screen.getByLabelText("E-mail"), "owner@example.test");
    await user.click(screen.getByRole("button", { name: "Send reset-link" }));
    expect(screen.getByText("Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.")).toBeInTheDocument();
    expect(screen.queryByText("provider detail")).not.toBeInTheDocument();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test("verification resend is sessionless and has a generic result for failures", async () => {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.sendVerificationEmail.mockResolvedValue({ data: null, error: { message: "provider detail" } }); hostedFetch(); renderApp("/verify-email");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "missing@example.test");
    await user.click(screen.getByRole("button", { name: "Send bekræftelsesmail" }));
    expect(authMocks.sendVerificationEmail).toHaveBeenCalledWith({ email: "missing@example.test", callbackURL: window.location.origin }, { retry: 0 });
    expect(await screen.findByText("Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.")).toBeInTheDocument();
    expect(screen.queryByText("provider detail")).not.toBeInTheDocument();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test.each(["success", "rejected", "transport"])("authenticated verification resend stays generic for %s and never claims delivery", async (outcome) => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: false, twoFactorEnabled: false } } });
    if (outcome === "rejected") authMocks.sendVerificationEmail.mockResolvedValueOnce({ data: null, error: { message: "private delivery detail" } });
    if (outcome === "transport") authMocks.sendVerificationEmail.mockRejectedValueOnce(new Error("private delivery detail"));
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Send bekræftelsesmail igen" }));
    expect(await screen.findByText("Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.")).toHaveAttribute("role", "status");
    expect(screen.queryByText("E-mail sendt")).not.toBeInTheDocument();
    expect(screen.queryByText("private delivery detail")).not.toBeInTheDocument();
    expect(authMocks.sendVerificationEmail).toHaveBeenCalledTimes(1);
    expect(authMocks.sendVerificationEmail).toHaveBeenCalledWith({ email: "owner@example.test", callbackURL: "/" }, { retry: 0 });
    expect(authMocks.signInEmail).not.toHaveBeenCalled();
  });

  test.each(["VERIFICATION_UNCONFIRMED", "INVALID_TOKEN", "TOKEN_EXPIRED"])("verification callback %s gives recovery without claiming success or taking action", async (code) => {
    authMocks.getSession.mockResolvedValue({ data: null }); hostedFetch(); renderApp(`/?error=${code}`);
    expect(await screen.findByRole("alert")).toHaveTextContent(code === "VERIFICATION_UNCONFIRMED" ? "resultat kunne ikke fastslås" : "ugyldigt, udløbet eller allerede brugt");
    await waitFor(() => expect(screen.getByTestId("location-path").textContent).toBe("/"));
    expect(authMocks.signInEmail).not.toHaveBeenCalled();
    expect(authMocks.sendVerificationEmail).not.toHaveBeenCalled();
    expect(screen.getByRole("link", { name: "Send bekræftelsesmail igen" })).toHaveAttribute("href", "/verify-email");
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test("reset-password consumes the token from the link and removes it from the route", async () => {
    authMocks.getSession.mockResolvedValue({ data: null }); hostedFetch(); renderApp("/reset-password?token=one-time-token");
    const user = userEvent.setup();
    await waitFor(() => {
      expect(screen.getByTestId("location-path")).toHaveTextContent("/reset-password");
      expect(screen.getByTestId("location-path")).not.toHaveTextContent("one-time-token");
    });
    await user.type(await screen.findByLabelText("Ny adgangskode"), "new-password-123");
    await user.click(screen.getByRole("button", { name: "Opdater adgangskode" }));
    expect(authMocks.resetPassword).toHaveBeenCalledWith({ newPassword: "new-password-123", token: "one-time-token" }, { retry: 0 });
    expect(await screen.findByText("Din adgangskode er opdateret. Log ind igen.")).toBeInTheDocument();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test("reset-password failure clears the entered secret and remains generic", async () => {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.resetPassword.mockResolvedValue({ data: null, error: { code: "INVALID_TOKEN", message: "provider detail" } }); hostedFetch(); renderApp("/reset-password?token=expired");
    const user = userEvent.setup();
    const input = await screen.findByLabelText("Ny adgangskode");
    await user.type(input, "new-password-123");
    await user.click(screen.getByRole("button", { name: "Opdater adgangskode" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Linket er ugyldigt eller udløbet");
    expect(input).toHaveValue("");
    expect(screen.queryByText("provider detail")).not.toBeInTheDocument();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test.each(["unconfirmed", "transport loss", "malformed acknowledgement"])("%s reset offers recovery without claiming success or retrying", async (scenario) => {
    authMocks.getSession.mockResolvedValue({ data: null });
    if (scenario === "transport loss") authMocks.resetPassword.mockRejectedValue(new Error("private provider detail"));
    else if (scenario === "malformed acknowledgement") authMocks.resetPassword.mockResolvedValue({ data: null, error: null });
    else authMocks.resetPassword.mockResolvedValue({ data: null, error: { code: "RESET_UNCONFIRMED", message: "private provider detail" } });
    hostedFetch(); renderApp("/reset-password?token=uncertain-token");
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Ny adgangskode"), "new-password-123");
    await user.click(screen.getByRole("button", { name: "Opdater adgangskode" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Vi kunne ikke bekræfte, om adgangskoden blev ændret");
    expect(screen.queryByText("Din adgangskode er opdateret. Log ind igen.")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Ny adgangskode")).not.toBeInTheDocument();
    expect(screen.queryByText("private provider detail")).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Til login" })).toHaveAttribute("href", "/");
    expect(screen.getByRole("link", { name: "Anmod om et nyt reset-link" })).toHaveAttribute("href", "/forgot-password");
    expect(authMocks.resetPassword).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("location-path")).not.toHaveTextContent("uncertain-token");
    await user.click(screen.getByRole("link", { name: "Anmod om et nyt reset-link" }));
    expect(await screen.findByRole("button", { name: "Send reset-link" })).toBeInTheDocument();
    expect(authMocks.resetPassword).toHaveBeenCalledTimes(1);
  });

  async function nativeSetup(reply: (path: string, init?: RequestInit) => Promise<Response>) {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.signInEmail.mockResolvedValue({ data: null, error: { code: "MFA_ENROLLMENT_REQUIRED", status: 409 } });
    hostedFetch(); const base = globalThis.fetch;
    stubGlobal("fetch", vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path.startsWith("/api/auth/mfa-enrollment/")) return reply(path, init);
      return base(input, init);
    }));
    renderApp(); const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "owner@example.test");
    await user.type(screen.getByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Log ind" }));
    await screen.findByRole("heading", { name: "Opsæt totrinsbekræftelse" });
    expect(screen.getByLabelText("Adgangskode")).toHaveValue("");
    await user.type(screen.getByLabelText("Adgangskode"), "reentered-password");
    await user.click(screen.getByRole("button", { name: "Start opsætning" }));
    return user;
  }
  const setupUri = "otpauth://totp/Rentemester:owner?secret=ABCDEFGHIJKLMNOP&issuer=Rentemester&digits=6&period=30";
  const recoveryCodes = Array.from({length:10},(_,i)=>`AAAA${i}-BBBB${i}`);
  const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), {status,headers:{"content-type":"application/json"}});
  test("native setup shows recovery codes after confirmation and refreshes only after acknowledgement", async () => {
    const calls: string[] = [];
    const user = await nativeSetup(async (path, init) => {
      calls.push(path); expect(init?.credentials).toBe("same-origin"); expect(init?.cache).toBe("no-store");
      if (path.endsWith("begin")) { expect(JSON.parse(String(init?.body))).toEqual({email:"owner@example.test",password:"reentered-password"}); return json({method:"totp",totpURI:setupUri}); }
      expect(JSON.parse(String(init?.body))).toEqual({code:"123456"}); return json({status:true,backupCodes:recoveryCodes});
    });
    expect(await screen.findByLabelText("Opsætningsnøgle")).toBeInTheDocument();
    expect(screen.queryByText("Gem dine recovery codes")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Engangskode"),"123456"); await user.click(screen.getByRole("button",{name:"Bekræft opsætning"}));
    expect(await screen.findByText("Gem dine recovery codes")).toBeInTheDocument();
    expect(screen.queryByLabelText("Opsætningsnøgle")).not.toBeInTheDocument();
    for(const code of recoveryCodes) expect(screen.getByText(code)).toBeInTheDocument();
    expect(authMocks.getSession).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button",{name:"Jeg har gemt koderne"}));
    await waitFor(()=>expect(authMocks.getSession).toHaveBeenCalledTimes(2));
    expect(calls).toEqual(["/api/auth/mfa-enrollment/begin","/api/auth/mfa-enrollment/confirm"]);
    expect(authMocks.enable).not.toHaveBeenCalled(); expect(authMocks.verifyTotp).not.toHaveBeenCalled();
  });
  for(const outcome of ["unknown","transport","malformed"] as const) test(`native setup ${outcome} removes secret form without retry`, async () => {
    let confirms=0;
    const user=await nativeSetup(async(path)=>{
      if(path.endsWith("begin"))return json({method:"totp",totpURI:setupUri});
      confirms++; if(outcome==="transport")throw new Error("private provider detail");
      return outcome==="unknown"?json({code:"AUTH_UNCONFIRMED",message:"private provider detail"},503):json({status:true,backupCodes:[]});
    });
    await screen.findByLabelText("Engangskode"); await user.type(screen.getByLabelText("Engangskode"),"123456");
    await user.click(screen.getByRole("button",{name:"Bekræft opsætning"}));
    expect(await screen.findByText(/Vi kunne ikke bekræfte resultatet/)).toHaveTextContent("Opsætningen kan være gennemført");
    expect(screen.queryByLabelText("Opsætningsnøgle")).not.toBeInTheDocument(); expect(screen.queryByLabelText("Engangskode")).not.toBeInTheDocument();
    expect(screen.queryByText("private provider detail")).not.toBeInTheDocument(); expect(authMocks.getSession).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button",{name:"Tilbage til login"}));
    expect(await screen.findByRole("heading",{name:"Log ind"})).toBeInTheDocument(); expect(confirms).toBe(1);
  });
  test("native setup bad code keeps ceremony and clears the input", async()=>{
    const user=await nativeSetup(async path=>path.endsWith("begin")?json({method:"totp",totpURI:setupUri}):json({code:"INVALID_CODE"},401));
    await screen.findByLabelText("Engangskode"); await user.type(screen.getByLabelText("Engangskode"),"123456");
    await user.click(screen.getByRole("button",{name:"Bekræft opsætning"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("Kontrollér koden"); expect(screen.getByLabelText("Engangskode")).toHaveValue("");
    expect(screen.getByLabelText("Opsætningsnøgle")).toBeInTheDocument(); expect(authMocks.getSession).toHaveBeenCalledTimes(1);
  });

  test("native setup failed begin clears password and permits an explicit password-bound restart", async()=>{
    let begins=0;
    const user=await nativeSetup(async()=>{ begins++; return begins===1?json({code:"AUTH_UNCONFIRMED"},503):json({method:"totp",totpURI:setupUri}); });
    expect(await screen.findByRole("alert")).toHaveTextContent("Opsætningen kunne ikke bekræftes");
    expect(screen.getByLabelText("Adgangskode")).toHaveValue(""); expect(begins).toBe(1);
    await user.type(screen.getByLabelText("Adgangskode"),"new-password-proof");
    await user.click(screen.getByRole("button",{name:"Start opsætning"}));
    expect(await screen.findByLabelText("Opsætningsnøgle")).toBeInTheDocument(); expect(begins).toBe(2);
    await user.click(screen.getByRole("button",{name:"Start opsætning forfra"}));
    expect(screen.queryByLabelText("Opsætningsnøgle")).not.toBeInTheDocument(); expect(screen.getByLabelText("Adgangskode")).toHaveValue("");
    expect(begins).toBe(2); expect(authMocks.getSession).toHaveBeenCalledTimes(1);
  });

  test("enrollment error code with a different HTTP status cannot enter native setup", async()=>{
    authMocks.getSession.mockResolvedValue({data:null}); authMocks.signInEmail.mockResolvedValue({error:{code:"MFA_ENROLLMENT_REQUIRED",status:401}});
    hostedFetch(); renderApp(); const user=userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"),"owner@example.test"); await user.type(screen.getByLabelText("Adgangskode"),"not-stored-password");
    await user.click(screen.getByRole("button",{name:"Log ind"})); expect(await screen.findByRole("alert")).toHaveTextContent("Kunne ikke logge ind");
    expect(screen.queryByRole("heading",{name:"Opsæt totrinsbekræftelse"})).not.toBeInTheDocument();
  });

  test("login two-factor redirect requires a TOTP code without trusted device", async () => {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.signInEmail.mockResolvedValue({ data: { twoFactorRedirect: true } });
    authMocks.verifyTotp.mockResolvedValue({ data: {} }); hostedFetch(); renderApp();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "owner@example.test");
    await user.type(screen.getByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Log ind" }));
    await screen.findByRole("heading", { name: "Bekræft din kode" });
    await user.type(screen.getByLabelText("Engangskode"), "123456");
    await user.click(screen.getByRole("button", { name: "Bekræft" }));
    expect(authMocks.verifyTotp).toHaveBeenCalledWith({ code: "123456", trustDevice: false }, { retry: 0 });
  });

  test("TOTP result errors remain generic and do not enter the cockpit", async () => {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.signInEmail.mockResolvedValue({ data: { twoFactorRedirect: true } });
    authMocks.verifyTotp.mockResolvedValue({ data: null, error: { message: "provider detail" } }); hostedFetch(); renderApp();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "owner@example.test");
    await user.type(screen.getByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Log ind" }));
    await user.type(await screen.findByLabelText("Engangskode"), "123456");
    await user.click(screen.getByRole("button", { name: "Bekræft" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Koden kunne ikke bekræftes");
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test("recovery-code verification uses Better Auth with no trusted device and enters only after success", async () => {
    authMocks.getSession
      .mockResolvedValueOnce({ data: null })
      .mockResolvedValueOnce({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    authMocks.signInEmail.mockResolvedValue({ data: { twoFactorRedirect: true } });
    authMocks.verifyBackupCode.mockResolvedValue({ data: {} }); hostedFetch(); renderApp();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "owner@example.test");
    await user.type(screen.getByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Log ind" }));
    await user.click(await screen.findByRole("button", { name: "Brug recovery code" }));
    expect(screen.getByRole("heading", { name: "Bekræft din kode" })).toBeInTheDocument();
    expect(screen.queryByText("owner@example.test")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Recovery code"), "one-time-code");
    await user.click(screen.getByRole("button", { name: "Bekræft" }));
    expect(authMocks.verifyBackupCode).toHaveBeenCalledWith({ code: "one-time-code", trustDevice: false }, { retry: 0 });
    expect(await screen.findByText("owner@example.test")).toBeInTheDocument();
  });

  test("recovery-code failures stay generic and can switch back to TOTP", async () => {
    authMocks.getSession.mockResolvedValue({ data: null });
    authMocks.signInEmail.mockResolvedValue({ data: { twoFactorRedirect: true } });
    authMocks.verifyBackupCode.mockResolvedValue({ data: null, error: { message: "provider detail" } }); hostedFetch(); renderApp();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("E-mail"), "owner@example.test");
    await user.type(screen.getByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Log ind" }));
    await user.click(await screen.findByRole("button", { name: "Brug recovery code" }));
    await user.type(screen.getByLabelText("Recovery code"), "used-or-invalid");
    await user.click(screen.getByRole("button", { name: "Bekræft" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Koden kunne ikke bekræftes");
    expect(screen.queryByText("provider detail")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Brug autentifikator-kode" }));
    expect(await screen.findByLabelText("Engangskode")).toBeInTheDocument();
    expect(screen.queryByText("owner@example.test")).not.toBeInTheDocument();
  });

  test("verified user without MFA is routed to enrollment", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: false } } });
    hostedFetch(); renderApp();
    expect(await screen.findByRole("heading", { name: "Opsæt totrinsbekræftelse" })).toBeInTheDocument();
  });

  test("MFA enrollment result errors remain generic and do not reveal setup data", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: false } } });
    authMocks.enable.mockResolvedValue({ data: null, error: { message: "provider detail" } }); hostedFetch(); renderApp();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Adgangskode"), "not-stored-password");
    await user.click(screen.getByRole("button", { name: "Start opsætning" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Kunne ikke starte opsætningen");
    expect(screen.queryByText(/otpauth:/i)).not.toBeInTheDocument();
  });

  test("account menu signs out and revoke-all is confirmation gated", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await screen.findByText("owner@example.test");
    await user.click(screen.getByRole("button", { name: "Log ud" }));
    expect(authMocks.signOut).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("heading", { name: "Log ind" })).toBeInTheDocument();
  });

  test("revoke-all is not called when cancelled, and clears only after success", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await screen.findByText("owner@example.test");
    await user.click(screen.getByRole("button", { name: "Sessioner" }));
    await user.click(screen.getByRole("button", { name: "Log ud på alle enheder" }));
    expect(authMocks.revokeSessions).not.toHaveBeenCalled();
    await user.click(within(screen.getByRole("dialog", { name: "Log ud på alle enheder?" })).getByRole("button", { name: "Annullér" }));
    await user.click(screen.getByRole("button", { name: "Log ud på alle enheder" }));
    await user.click(within(screen.getByRole("dialog", { name: "Log ud på alle enheder?" })).getByRole("button", { name: "Log ud" }));
    expect(authMocks.revokeSessions).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("heading", { name: "Log ind" })).toBeInTheDocument();
  });

  test("revoke-all error keeps the current shell and shows a generic failure", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    authMocks.revokeSessions.mockResolvedValue({ data: null, error: { message: "provider detail" } });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await screen.findByText("owner@example.test");
    await user.click(screen.getByRole("button", { name: "Sessioner" }));
    await user.click(screen.getByRole("button", { name: "Log ud på alle enheder" }));
    await user.click(within(screen.getByRole("dialog", { name: "Log ud på alle enheder?" })).getByRole("button", { name: "Log ud" }));
    expect(await screen.findByText("Kunne ikke logge ud på alle enheder. Din nuværende session er stadig aktiv.")).toBeInTheDocument();
    expect(screen.getByText("owner@example.test")).toBeInTheDocument();
  });

  test("shows safe role and session metadata and confirmation-gates one-session revocation", async () => {
    authMocks.getSession.mockResolvedValue({ data: {
      user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true },
      session: { id: "current-session" },
    } });
    authMocks.listSessions.mockResolvedValue({ data: [
      { id: "current-session", token: "never-render-current-token", createdAt: "2026-08-23T10:00:00Z", userAgent: "Safari secret raw agent" },
      { id: "other-session", token: "never-render-other-token", createdAt: "2026-08-22T09:00:00Z", userAgent: "Chrome/140 secret raw agent" },
    ] });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    expect(await screen.findByText("Workspace-ejer")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Sessioner" }));
    expect(await screen.findByText("Denne enhed")).toBeInTheDocument();
    expect(screen.getByText("Chrome")).toBeInTheDocument();
    expect(screen.queryByText(/never-render|secret raw agent/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Afslut" }));
    await user.click(within(screen.getByRole("dialog", { name: "Afslut session?" })).getByRole("button", { name: "Afslut session" }));
    expect(authMocks.revokeSession).toHaveBeenCalledWith({ token: "never-render-other-token" });
    expect(screen.queryByText("Chrome")).not.toBeInTheDocument();
  });

  test("authenticated password change replaces every old session and clears entered secrets", async () => {
    authMocks.getSession.mockResolvedValue({ data: {
      user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true },
      session: { id: "current-session" },
    } });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await screen.findByText("owner@example.test");
    await user.click(screen.getByRole("button", { name: "Skift adgangskode" }));
    const panel = screen.getByRole("dialog", { name: "Skift adgangskode" });
    await user.type(screen.getByLabelText("Nuværende adgangskode"), "current-password-secret");
    await user.type(screen.getByLabelText("Ny adgangskode"), "new-password-secret");
    await user.type(screen.getByLabelText("Gentag ny adgangskode"), "new-password-secret");
    await user.click(within(panel).getByRole("button", { name: "Skift adgangskode" }));
    expect(authMocks.changePassword).toHaveBeenCalledWith({
      currentPassword: "current-password-secret",
      newPassword: "new-password-secret",
      revokeOtherSessions: true,
    });
    expect(await screen.findByText(/alle tidligere sessioner er afsluttet/)).toBeInTheDocument();
    expect(screen.queryByDisplayValue(/password-secret/)).not.toBeInTheDocument();
  });

  test("account dialogs relate to their triggers and dismiss password fields without retaining secrets", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    hostedFetch(); renderApp(); const user = userEvent.setup();
    await screen.findByText("owner@example.test");
    const sessions = screen.getByRole("button", { name: "Sessioner" });
    expect(sessions).toHaveAttribute("aria-expanded", "false");
    await user.click(sessions);
    const sessionsDialog = await screen.findByRole("dialog", { name: "Aktive sessioner" });
    expect(sessions).toHaveAttribute("aria-controls", sessionsDialog.id);
    expect(sessions).toHaveAttribute("aria-expanded", "true");
    await user.click(within(sessionsDialog).getByRole("button", { name: "Luk" }));
    expect(sessions).toHaveFocus();
    const password = screen.getByRole("button", { name: "Skift adgangskode" });
    await user.click(password);
    const dialog = screen.getByRole("dialog", { name: "Skift adgangskode" });
    expect(password).toHaveAttribute("aria-controls", dialog.id);
    expect(password).toHaveAttribute("aria-expanded", "true");
    await user.type(screen.getByLabelText("Nuværende adgangskode"), "ephemeral-secret");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(password).toHaveFocus();
    expect(password).toHaveAttribute("aria-expanded", "false");
    await user.click(password);
    expect(screen.getByLabelText("Nuværende adgangskode")).toHaveValue("");
    expect(authMocks.changePassword).not.toHaveBeenCalled();
  });

  test("company switcher contains only server-authorized memberships", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    hostedFetch(); renderApp();
    const switcher = await screen.findByLabelText("Skift virksomhed");
    expect(switcher).toHaveTextContent("Allowed ApS — Ejer");
    expect(switcher).not.toHaveTextContent("Hidden ApS");
  });

  test("a normal API 401 after the cockpit loaded clears stale UI to login", async () => {
    authMocks.getSession.mockResolvedValue({ data: { user: { id: "u1", email: "owner@example.test", emailVerified: true, twoFactorEnabled: true } } });
    hostedFetch(); renderApp(); await screen.findByText("owner@example.test");
    stubGlobal("fetch", vi.fn(async () => new Response("unauthorized", { status: 401 })));
    await act(async () => { await expect(request("/api/companies")).rejects.toMatchObject({ code: "unauthorized" }); });
    expect(await screen.findByRole("heading", { name: "Log ind" })).toBeInTheDocument();
    expect(screen.queryByText("owner@example.test")).not.toBeInTheDocument();
  });

  test("health failure or unknown profile never starts auth or cockpit", async () => {
    stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ok: true, deploymentProfile: "unknown" }), { headers: { "content-type": "application/json" } })));
    renderApp();
    expect(await screen.findByRole("alert")).toHaveTextContent("Kunne ikke bekræfte");
    expect(authMocks.getSession).not.toHaveBeenCalled();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });

  test("local-container opens the local cockpit without hosted auth", async () => {
    mockFetch({
      "GET /api/health": {
        service: "rentemester-cockpit",
        workspace: "/workspace",
        authRequired: false,
        deploymentProfile: "local-container",
        build: {},
        provenance: {},
        routes: [],
      },
      "GET /api/portfolio": {
        portfolio: {
          workspace: "/workspace",
          asOf: "2026-08-24",
          companyCount: 0,
          rollup: { resultat: 0, liquidity: 0, vatPayable: 0, openTaskCount: 0 },
          totals: {},
          companies: [],
        },
      },
    });
    renderApp();
    expect(await screen.findByRole("form", { name: /Opret virksomhed/i })).toBeInTheDocument();
    expect(authMocks.getSession).not.toHaveBeenCalled();
    expect(screen.queryByRole("heading", { name: "Log ind" })).not.toBeInTheDocument();
    expect(screen.queryByText(/Kunne ikke bekræfte/)).not.toBeInTheDocument();
  });

  test("failed health never starts auth or cockpit", async () => {
    stubGlobal("fetch", vi.fn(async () => new Response("unavailable", { status: 503 })));
    renderApp();
    expect(await screen.findByRole("alert")).toHaveTextContent("Kunne ikke bekræfte");
    expect(authMocks.getSession).not.toHaveBeenCalled();
    expect(screen.queryByText("Portefølje")).not.toBeInTheDocument();
  });
});
