import { Button, Input } from "../components/ui";
import { FormEvent, useEffect, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";

export function LoginView() {
  const { refresh } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [needsTotp, setNeedsTotp] = useState(false);
  const [verificationMethod, setVerificationMethod] = useState<"totp" | "backup">("totp");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try {
      const result = await authClient.signIn.email({ email, password, rememberMe: false });
      if (result.error) throw new Error("sign in failed");
      if (result.data && "twoFactorRedirect" in result.data && result.data.twoFactorRedirect) {
        setPassword("");
        setNeedsTotp(true);
      }
      else await refresh();
    } catch { setError("Kunne ikke logge ind. Kontrollér dine oplysninger og prøv igen."); }
    finally { setBusy(false); }
  };
  const verify = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try {
      const result = verificationMethod === "totp"
        ? await authClient.twoFactor.verifyTotp({ code, trustDevice: false })
        : await authClient.twoFactor.verifyBackupCode({ code, trustDevice: false });
      if (result.error) throw new Error("two factor verification failed");
      await refresh();
    }
    catch { setError("Koden kunne ikke bekræftes. Prøv igen."); }
    finally { setCode(""); setBusy(false); }
  };
  return <AuthPanel title={needsTotp ? "Bekræft din kode" : "Log ind"}>
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

const recoveryMessage = "Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.";

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
    try { await authClient.sendVerificationEmail({ email, callbackURL: window.location.origin }); }
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
  const [complete, setComplete] = useState(false);
  useEffect(() => { if (location.search) navigate("/reset-password", { replace: true }); }, [location.search, navigate]);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setError(null);
    if (!token) { setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link."); return; }
    try {
      const result = await authClient.resetPassword({ newPassword: password, token });
      if (result.error) throw new Error("reset failed");
      setPassword(""); setToken(""); setComplete(true);
    } catch { setPassword(""); setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link."); }
  };
  return <AuthPanel title="Vælg ny adgangskode">{complete ? <><p role="status">Din adgangskode er opdateret. Log ind igen.</p><p><Link to="/">Til login</Link></p></> : <><p>Vælg en ny adgangskode på mindst 12 tegn.</p>{error && <p className="banner error" role="alert">{error}</p>}<form onSubmit={submit}><label>Ny adgangskode<Input aria-label="Ny adgangskode" autoComplete="new-password" type="password" minLength={12} value={password} onChange={(event) => setPassword(event.target.value)} required /></label><Button className="btn" type="submit">Opdater adgangskode</Button></form></>}</AuthPanel>;
}

export function AuthPanel({ title, children }: { title: string; children: ReactNode }) {
  return <main className="auth-panel"><section className="card"><p className="eyebrow">Rentemester</p><h1>{title}</h1>{children}</section></main>;
}
