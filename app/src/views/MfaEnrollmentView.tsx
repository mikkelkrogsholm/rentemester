import { Button, Input } from "../components/ui";
import { FormEvent, useState } from "react";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";
import { AuthPanel } from "./LoginView";

export function VerificationRequiredView() {
  const { session } = useAuth();
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const resend = async () => { setError(null); try { const result = await authClient.sendVerificationEmail({ email: session?.email ?? "", callbackURL: "/" }); if (result.error) throw new Error("resend failed"); setSent(true); } catch { setError("Kunne ikke sende bekræftelsesmailen. Prøv igen senere."); } };
  return <AuthPanel title="Bekræft din e-mail"><p>Du skal bekræfte din e-mail, før du kan fortsætte.</p>{error && <p className="banner error" role="alert">{error}</p>}<Button className="btn" type="button" onClick={() => void resend()}>{sent ? "E-mail sendt" : "Send bekræftelsesmail igen"}</Button></AuthPanel>;
}

export function MfaEnrollmentView() {
  const { refresh } = useAuth();
  const [password, setPassword] = useState("");
  const [uri, setUri] = useState<string | null>(null);
  const [codes, setCodes] = useState<string[]>([]);
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const begin = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try {
      const result = await authClient.twoFactor.enable({ password, method: "totp", issuer: "Rentemester" });
      if (result.error) throw new Error("totp enable failed");
      if (!result.data || result.data.method !== "totp") throw new Error("TOTP setup was not returned");
      setUri(result.data.totpURI); setCodes(result.data.backupCodes); setPassword("");
    }
    catch { setError("Kunne ikke starte opsætningen. Prøv igen."); }
    finally { setBusy(false); }
  };
  const verify = async (event: FormEvent) => {
    event.preventDefault(); setBusy(true); setError(null);
    try { const result = await authClient.twoFactor.verifyTotp({ code, trustDevice: false }); if (result.error) throw new Error("totp verify failed"); setCodes([]); await refresh(); }
    catch { setError("Koden kunne ikke bekræftes. Prøv igen."); }
    finally { setBusy(false); }
  };
  return <AuthPanel title="Opsæt totrinsbekræftelse">
    {error && <p className="banner error" role="alert">{error}</p>}
    {!uri ? <form onSubmit={begin}><p>Tilføj Rentemester i din autentifikator-app.</p><label>Adgangskode<Input aria-label="Adgangskode" type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} required /></label><Button className="btn" type="submit" disabled={busy}>Start opsætning</Button></form>
      : <form onSubmit={verify}><p>Indtast denne opsætningsadresse i din autentifikator-app:</p><code className="totp-uri">{uri}</code>{codes.length > 0 && <section className="recovery-codes" aria-label="Recovery codes"><h3>Recovery codes</h3><p>Gem dem nu. De vises ikke igen.</p><ul>{codes.map((backup) => <li key={backup}>{backup}</li>)}</ul></section>}<label>Engangskode<Input aria-label="Engangskode" inputMode="numeric" autoComplete="one-time-code" value={code} onChange={(e) => setCode(e.target.value)} required /></label><Button className="btn" type="submit" disabled={busy}>Bekræft opsætning</Button></form>}
  </AuthPanel>;
}
