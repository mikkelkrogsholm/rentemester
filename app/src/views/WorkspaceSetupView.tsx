import { useEffect, useState, type FormEvent } from "react";
import { Button, Field, Input } from "../components/ui";
import { AuthPanel } from "./LoginView";

/** The local operator supplies the installation link; this is not public registration. */
export function WorkspaceSetupView({ onComplete }: { onComplete: () => void }) {
  const [token, setToken] = useState(() => new URLSearchParams(window.location.hash.slice(1)).get("setup") ?? "");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { if (window.location.hash) window.history.replaceState(null, "", window.location.pathname + window.location.search); }, []);
  const submit = async (event: FormEvent) => {
    event.preventDefault(); setError(null);
    if (password !== repeat) { setError("Adgangskoderne er ikke ens."); return; }
    setBusy(true);
    try {
      const response = await fetch("/api/setup/first-owner", { method: "POST", credentials: "omit", cache: "no-store", redirect: "error",
        headers: { "content-type": "application/json", "x-rentemester-install-token": token }, body: JSON.stringify({ name, email, password }) });
      const body = await response.json();
      if (!response.ok || body?.ok !== true) throw new Error("setup not confirmed");
      setToken(""); setDone(true);
    } catch { setError("Opsætningen kunne ikke bekræftes. Prøv samme e-mail igen. Hvis opsætningen allerede er gennemført, kan du gå til login."); }
    finally { setPassword(""); setRepeat(""); setBusy(false); }
  };
  if (done) return <AuthPanel title="Første ejer er oprettet"><p>Bekræft din e-mail med bekræftelseslinket. Ved lokal installation ligger linket i workspacets private bekræftelsesfiler.</p><p>Derefter kan du logge ind og opsætte totrinsbekræftelse.</p><Button onClick={onComplete}>Til login</Button></AuthPanel>;
  return <AuthPanel title="Opret workspace-ejer">
    {!token ? <p>Åbn det private installationslink fra serverens terminal for at oprette den første ejer.</p> : <form onSubmit={submit}>
      <p>Opret den første ejer af dette workspace. E-mailbekræftelse og totrinsbekræftelse følger efter oprettelsen.</p>
      {error && <p role="alert">{error}</p>}
      <Field label="Dit navn"><Input required autoComplete="name" maxLength={200} value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="E-mail"><Input required type="email" autoComplete="email" value={email} onChange={(e) => setEmail(e.target.value)} /></Field>
      <Field label="Ny adgangskode"><Input required type="password" autoComplete="new-password" minLength={12} maxLength={128} value={password} onChange={(e) => setPassword(e.target.value)} /></Field>
      <Field label="Gentag adgangskode"><Input required type="password" autoComplete="new-password" minLength={12} maxLength={128} value={repeat} onChange={(e) => setRepeat(e.target.value)} /></Field>
      <Button type="submit" disabled={busy}>Opret ejer</Button>
    </form>}
    <Button variant="secondary" disabled={busy} onClick={onComplete}>Kontrollér login</Button>
  </AuthPanel>;
}
