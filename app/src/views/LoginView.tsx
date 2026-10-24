import { loginAttempts, type LoginAttempt } from "../lib/login-attempt";
import { Button, Input } from "../components/ui";
import { FormEvent, useEffect, useState, useRef, useSyncExternalStore, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";
import { beginNativeEnrollment, confirmNativeEnrollment, NativeEnrollmentFailure } from "../lib/native-enrollment";

export function LoginView() {
  const location = useLocation();
  const navigate = useNavigate();
  const [verificationNotice] = useState(() => {
    const code = new URLSearchParams(location.search).get("error");
    if (code === "VERIFICATION_UNCONFIRMED") return "Bekræftelsens resultat kunne ikke fastslås. Prøv at logge ind for at kontrollere status. Du kan også anmode om en ny bekræftelsesmail.";
    if (code === "INVALID_TOKEN" || code === "TOKEN_EXPIRED") return "Bekræftelseslinket er ugyldigt, udløbet eller allerede brugt. Prøv at logge ind, eller anmod om en ny bekræftelsesmail.";
    return null;
  });
  useEffect(() => {
    const params = new URLSearchParams(location.search);
    if (!verificationNotice || !params.has("error")) return;
    params.delete("error");
    navigate({ pathname: location.pathname, search: params.size ? `?${params}` : "" }, { replace: true });
  }, [location.pathname, location.search, navigate, verificationNotice]);
  const { refresh } = useAuth();
  const attempt = useRef<LoginAttempt | null>(null);
  const status = useSyncExternalStore(loginAttempts.subscribe, loginAttempts.snapshot);
  useEffect(() => () => { if (attempt.current) void loginAttempts.cancel(attempt.current); }, []);
  const cancel = () => {
    const current = attempt.current; attempt.current = null;
    setPassword(""); setCode(""); setNeedsTotp(false); setNativeEnrollment(false); setError(null); setBusy(false);
    if (current) void loginAttempts.cancel(current);
  };
  const adopt = async (current: LoginAttempt) => {
    loginAttempts.ready(current);
    await refresh();
    // Only AuthProvider may finish after both session and workspace identity agree.
    if (!loginAttempts.current(current)) attempt.current = null;
  };
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [needsTotp, setNeedsTotp] = useState(false);
  const [nativeEnrollment, setNativeEnrollment] = useState(false);
  const [verificationMethod, setVerificationMethod] = useState<"totp" | "backup">("totp");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); if (busy || status !== "idle") return; setBusy(true); setError(null);
    const current = loginAttempts.start(email); attempt.current = current;
    try {
      const result = await loginAttempts.run(current, () => authClient.signIn.email({ email, password, rememberMe: false }, { retry: 0 }));
      if (!loginAttempts.current(current)) return;
      loginAttempts.remember(current, (result.data as { user?: { id: string; email: string } } | null)?.user);
      if (result.error?.code === "MFA_ENROLLMENT_REQUIRED" && result.error.status === 409) {
        setPassword(""); setNativeEnrollment(true); return;
      }
      if (result.error) throw new Error("sign in failed");
      if (result.data && "twoFactorRedirect" in result.data && result.data.twoFactorRedirect) {
        setPassword("");
        setNeedsTotp(true);
      }
      else await adopt(current);
    } catch { if (loginAttempts.current(current)) { cancel(); setError("Kunne ikke logge ind. Kontrollér dine oplysninger og prøv igen."); } }
    finally { if (loginAttempts.current(current)) { setPassword(""); setBusy(false); } }
  };
  const verify = async (event: FormEvent) => {
    event.preventDefault(); const current = attempt.current; if (busy || !current || !loginAttempts.current(current)) return; setBusy(true); setError(null);
    try {
      const result = await loginAttempts.run(current, () => verificationMethod === "totp"
        ? authClient.twoFactor.verifyTotp({ code, trustDevice: false }, { retry: 0 })
        : authClient.twoFactor.verifyBackupCode({ code, trustDevice: false }, { retry: 0 }));
      if (!loginAttempts.current(current)) return;
      if (result.error) {
        if (result.error.status && [400, 401, 403, 429].includes(result.error.status)) { setError("Koden kunne ikke bekræftes. Prøv igen."); return; }
        throw new Error("two factor verification unconfirmed");
      }
      loginAttempts.remember(current, (result.data as { user?: { id: string; email: string } } | null)?.user);
      await adopt(current);
    }
    catch { if (loginAttempts.current(current)) { cancel(); setError("Koden kunne ikke bekræftes. Prøv igen."); } }
    finally { if (loginAttempts.current(current)) { setCode(""); setBusy(false); } }
  };
  if (status === "cleaning" || status === "blocked") return <AuthPanel title="Log ind"><p role="status">{status === "cleaning" ? "Annullering afventer svar og sikker logout. Ingen nye login kan sendes endnu." : "Logout kunne ikke bekræftes. Login er blokeret."}</p>{status === "blocked" && <Button onClick={() => void loginAttempts.cancel()}>Kontrollér logout</Button>}</AuthPanel>;
  if (nativeEnrollment && attempt.current) return <NativeEnrollmentView email={email} attempt={attempt.current} onLogin={cancel} onAdopt={() => adopt(attempt.current!)} />;
  return <AuthPanel title={needsTotp ? "Bekræft din kode" : "Log ind"}>
    {(busy || needsTotp) && <Button type="button" onClick={cancel}>Annullér</Button>}
    {verificationNotice && <p className="banner error" role="alert">{verificationNotice}</p>}
    {error && <p className="banner error" role="alert">{error}</p>}
    {needsTotp ? <form onSubmit={verify}>
      <p>{verificationMethod === "totp" ? "Indtast koden fra din autentifikator-app." : "Indtast én af dine recovery codes."}</p>
      <label>{verificationMethod === "totp" ? "Engangskode" : "Recovery code"}<Input aria-label={verificationMethod === "totp" ? "Engangskode" : "Recovery code"} inputMode={verificationMethod === "totp" ? "numeric" : "text"} autoComplete="one-time-code" value={code} onChange={(e) => setCode(e.target.value)} required /></label>
      <Button className="btn" type="submit" disabled={busy}>Bekræft</Button>
      <Button variant="secondary" className="btn secondary" type="button" disabled={busy} onClick={() => { setCode(""); setError(null); setVerificationMethod((current) => current === "totp" ? "backup" : "totp"); }}>{verificationMethod === "totp" ? "Brug recovery code" : "Brug autentifikator-kode"}</Button>
    </form>
      : <form onSubmit={submit}><label>E-mail<Input aria-label="E-mail" autoComplete="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required /></label><label>Adgangskode<Input aria-label="Adgangskode" autoComplete="current-password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} required /></label><Button className="btn" type="submit" disabled={busy}>Log ind</Button><p><Link to="/forgot-password">Glemt adgangskode?</Link></p><p><Link to="/verify-email">Send bekræftelsesmail igen</Link></p></form>}
  </AuthPanel>;
}

function NativeEnrollmentView({ email, attempt, onLogin, onAdopt }: { email: string; attempt: LoginAttempt; onLogin: () => void; onAdopt: () => Promise<void> }) {
  const [password, setPassword] = useState("");
  const [setupKey, setSetupKey] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [codes, setCodes] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [unconfirmed, setUnconfirmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const begin = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try { const key = await loginAttempts.run(attempt, () => beginNativeEnrollment(email, password)); if (loginAttempts.current(attempt)) setSetupKey(key); }
    catch (error) {
      if (!loginAttempts.current(attempt)) return;
      if (!(error instanceof NativeEnrollmentFailure) || error.unconfirmed) { setUnconfirmed(true); return; }
      setError(error instanceof NativeEnrollmentFailure && !error.unconfirmed
        ? "Kunne ikke starte opsætningen. Kontrollér dine oplysninger."
        : "Opsætningen kunne ikke bekræftes. Den kan være startet. Indtast din adgangskode for at starte igen, eller gå tilbage til login.");
    }
    finally { setPassword(""); setBusy(false); }
  };
  const confirm = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try { const result = await loginAttempts.run(attempt, () => confirmNativeEnrollment(code)); if (!loginAttempts.current(attempt)) return; setCodes(result); setSetupKey(null); }
    catch (error) {
      if (!loginAttempts.current(attempt)) return;
      if (!(error instanceof NativeEnrollmentFailure) || error.unconfirmed) {
        setSetupKey(null); setPassword(""); setUnconfirmed(true);
      } else setError("Koden kunne ikke bekræftes. Kontrollér koden og prøv igen.");
    } finally { setCode(""); setBusy(false); }
  };
  const acknowledge = async () => {
    setCodes([]); setBusy(true);
    try { await onAdopt(); } finally { setBusy(false); }
  };
  return <AuthPanel title="Opsæt totrinsbekræftelse">
    {error && <p role="alert" className="banner error">{error}</p>}
    {unconfirmed ? <><p role="status">Vi kunne ikke bekræfte resultatet. Opsætningen kan være gennemført. Gå til login og brug din autentifikator-app.</p><Button type="button" onClick={onLogin}>Tilbage til login</Button></>
      : codes.length ? <section aria-label="Recovery codes"><h3>Gem dine recovery codes</h3><p>Opsætningen er gennemført. Gem koderne sikkert nu. De vises kun denne ene gang.</p><ul>{codes.map((backup) => <li key={backup}>{backup}</li>)}</ul><Button type="button" disabled={busy} onClick={() => void acknowledge()}>Jeg har gemt koderne</Button></section>
      : setupKey ? <form onSubmit={confirm}><p>Åbn din autentifikator-app, vælg Tilføj konto og derefter Indtast opsætningsnøgle. Vælg en tidsbaseret kode (TOTP).</p><p>Konto: {email}. Tjeneste: Rentemester.</p><label>Opsætningsnøgle<Input aria-label="Opsætningsnøgle" readOnly value={setupKey} autoComplete="off" spellCheck={false} /></label><p>Brug 6 cifre og et interval på 30 sekunder, hvis appen spørger.</p><label>Engangskode<Input aria-label="Engangskode" inputMode="numeric" autoComplete="one-time-code" value={code} onChange={(e) => setCode(e.target.value)} required /></label><Button type="submit" disabled={busy}>Bekræft opsætning</Button><Button type="button" disabled={busy} onClick={() => { setSetupKey(null); setCode(""); setError(null); }}>Start opsætning forfra</Button></form>
      : <form onSubmit={begin}><p>Bekræft din adgangskode for at tilføje Rentemester i din autentifikator-app.</p><label>Adgangskode<Input aria-label="Adgangskode" type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required /></label><Button type="submit" disabled={busy}>Start opsætning</Button></form>}
    {!unconfirmed && <Button type="button" onClick={() => { setPassword(""); setCode(""); setSetupKey(null); setCodes([]); onLogin(); }}>Tilbage til login</Button>}
  </AuthPanel>;
}

export const recoveryMessage = "Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.";

export function ForgotPasswordView() {
  const [email, setEmail] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try { await authClient.requestPasswordReset({ email, redirectTo: `${window.location.origin}/reset-password` }); }
    catch { /* Deliberately identical response to prevent account enumeration. */ }
    finally { setEmail(""); setSubmitted(true); }
  };
  return <AuthPanel title="Nulstil adgangskode"><p>Indtast din e-mailadresse, så sender vi et reset-link, hvis den kan bruges.</p>{submitted && <p role="status">{recoveryMessage}</p>}<form onSubmit={submit}><label>E-mail<Input aria-label="E-mail" autoComplete="email" type="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label><Button className="btn" type="submit">Send reset-link</Button></form><p><Link to="/">Tilbage til login</Link></p></AuthPanel>;
}

export function VerificationRecoveryView() {
  const [email, setEmail] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    try { await authClient.sendVerificationEmail({ email, callbackURL: window.location.origin }, { retry: 0 }); }
    catch { /* Deliberately identical response to prevent account enumeration. */ }
    finally { setEmail(""); setSubmitted(true); }
  };
  return <AuthPanel title="Bekræft din e-mail"><p>Indtast din e-mailadresse for at få en ny bekræftelsesmail.</p>{submitted && <p role="status">{recoveryMessage}</p>}<form onSubmit={submit}><label>E-mail<Input aria-label="E-mail" autoComplete="email" type="email" value={email} onChange={(event) => setEmail(event.target.value)} required /></label><Button className="btn" type="submit">Send bekræftelsesmail</Button></form><p><Link to="/">Tilbage til login</Link></p></AuthPanel>;
}

export function ResetPasswordView() {
  const location = useLocation();
  const navigate = useNavigate();
  const [token, setToken] = useState(() => new URLSearchParams(location.search).get("token") ?? "");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<"editing" | "unconfirmed" | "complete">("editing");
  useEffect(() => { if (location.search) navigate("/reset-password", { replace: true }); }, [location.search, navigate]);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setError(null);
    if (!token) { setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link."); return; }
    try {
      const result = await authClient.resetPassword({ newPassword: password, token }, { retry: 0 });
      if (result.error || result.data?.status !== true) {
        setPassword("");
        if (["INVALID_TOKEN", "PASSWORD_TOO_SHORT", "PASSWORD_TOO_LONG"].includes(result.error?.code?.toString() ?? "")) {
          setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link."); return;
        }
        setToken(""); setOutcome("unconfirmed"); return;
      }
      setPassword(""); setToken(""); setOutcome("complete");
    } catch { setPassword(""); setToken(""); setOutcome("unconfirmed"); }
  };
  return <AuthPanel title="Vælg ny adgangskode">{outcome === "complete" ? <><p role="status">Din adgangskode er opdateret. Log ind igen.</p><p><Link to="/">Til login</Link></p></> : outcome === "unconfirmed" ? <><p role="alert">Vi kunne ikke bekræfte, om adgangskoden blev ændret. Prøv at logge ind med den nye adgangskode, eller anmod om et nyt reset-link.</p><p><Link to="/">Til login</Link></p><p><Link to="/forgot-password">Anmod om et nyt reset-link</Link></p></> : <><p>Vælg en ny adgangskode på mindst 12 tegn.</p>{error && <p className="banner error" role="alert">{error}</p>}<form onSubmit={submit}><label>Ny adgangskode<Input aria-label="Ny adgangskode" autoComplete="new-password" type="password" minLength={12} value={password} onChange={(event) => setPassword(event.target.value)} required /></label><Button className="btn" type="submit">Opdater adgangskode</Button></form></>}</AuthPanel>;
}

export function AuthPanel({ title, children }: { title: string; children: ReactNode }) {
  return <main className="auth-panel"><section className="card"><p className="eyebrow">Rentemester</p><h1>{title}</h1>{children}</section></main>;
}
