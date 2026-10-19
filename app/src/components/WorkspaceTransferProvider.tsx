import { useEffect, useRef, useState, type ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { useOptionalAuth } from "../lib/auth-context";
import { workspaceTransferApi, type Transfer } from "../lib/api/workspace-transfer";
import { clearWorkspaceWriteMode, setWorkspaceWriteMode } from "../lib/workspace-write-mode";
import { Button } from "./ui";
import { Context } from "../lib/workspace-transfer-context";
export { useWorkspaceTransfer } from "../lib/workspace-transfer-context";
import { useWorkspaceTransfer } from "../lib/workspace-transfer-context";
export function WorkspaceTransferProvider({ children }: { children: ReactNode }) {
  const auth = useOptionalAuth();
  const native = auth?.backend === "rust";
  const scope = `${window.location.origin}:${auth?.session?.id ?? ""}:${auth?.currentSessionId ?? ""}`;
  const [snapshot, setSnapshot] = useState<{ scope: string; transfer: Transfer | null; error: string; loading: boolean }>({ scope, transfer: null, error: "", loading: true });
  const latest = useRef(scope); latest.current = scope;
  const serial = useRef(0);
  const matches = snapshot.scope === scope;
  const transfer = matches ? snapshot.transfer : null;
  const error = matches ? snapshot.error : "";
  const writable = !native || Boolean(transfer?.mode === "active" && !error);
  if (native) setWorkspaceWriteMode(scope, writable ? "active" : error || !transfer ? "unknown" : transfer.mode);
  function adopt(value: Transfer) {
    if (latest.current !== scope) return;
    serial.current++;
    setWorkspaceWriteMode(scope, value.mode);
    setSnapshot({ scope, transfer: value, error: "", loading: false });
  }
  async function refresh() {
    if (!native) return;
    const request = ++serial.current;
    setSnapshot(old => ({ ...old, loading: true }));
    try {
      const value = await workspaceTransferApi.status();
      if (latest.current === scope && request === serial.current) setSnapshot({ scope, transfer: value, error: "", loading: false });
    } catch {
      if (latest.current === scope && request === serial.current) {
        setWorkspaceWriteMode(scope, "unknown");
        setSnapshot(old => ({ scope, transfer: old.scope === scope ? old.transfer : null, error: "Workspace-tilstanden kunne ikke kontrolleres. Skrivning og automatik er stoppet indtil ny kontrol.", loading: false }));
      }
    }
  }
  useEffect(() => {
    if (!native) return;
    void refresh();
    const timer = setInterval(() => void refresh(), 15_000);
    const focus = () => void refresh();
    window.addEventListener("focus", focus);
    return () => { serial.current++; clearInterval(timer); window.removeEventListener("focus", focus); clearWorkspaceWriteMode(scope); };
  }, [native, scope]);
  return <Context.Provider value={{ enabled: native, transfer, error, loading: !matches || snapshot.loading, writable, scope, refresh, adopt }}>{children}</Context.Provider>;
}
export function WorkspaceTransferBanner() {
  const lifecycle = useWorkspaceTransfer();
  if (!lifecycle?.enabled || lifecycle.writable) return null;
  const mode = lifecycle.transfer?.mode;
  return <aside role="status" className="banner">
    <strong>{mode === "moved" ? "Denne kilde er permanent skrivebeskyttet." : mode === "staged" ? "Målet er klargjort og endnu ikke aktivt." : mode === "frozen" ? "Workspace er frosset til flytning." : "Workspace-tilstanden kontrolleres."}</strong>
    <p>Rapporter og autoriserede læsninger er fortsat tilgængelige. Nye ændringer og automatisk kørsel er blokeret.</p>
    {mode === "moved" && <p>Frigivelsen autoriserer ét bestemt mål. Den beviser ikke, at målet er aktiveret.</p>}
    {lifecycle.error && <p role="alert">{lifecycle.error}</p>}
    <Link to="/flytning">Se flytning og dokumentation</Link>{" "}<Button variant="secondary" onClick={() => void lifecycle.refresh()}>Kontrollér workspace-tilstand</Button>
  </aside>;
}
export function WorkspaceReadOnlyGuard({ children }: { children: ReactNode }) {
  const lifecycle = useWorkspaceTransfer();
  const { pathname } = useLocation();
  if (!lifecycle?.enabled || lifecycle.writable || pathname === "/flytning") return <>{children}</>;
  return <fieldset disabled aria-label="Skrivebeskyttet workspace"><legend>Workspace er skrivebeskyttet. Rapportlinks kan fortsat åbnes.</legend>{children}</fieldset>;
}
