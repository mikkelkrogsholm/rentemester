import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useRef, useState } from "react";
import { request } from "../lib/api/_shared";
import { Button } from "./ui";
type Component = { name: string; available: boolean; ok: boolean; checked: Record<string, number>; errors: unknown[] };
type View = { ok: true; audit: { format: string; slug: string; ok: boolean; available: boolean; entries: number; errors: string[]; components: Component[] } };
const labels: Record<string, string> = { ledger: "Posteringer og revisionsspor", bank: "Bankkoblinger", invoices: "Fakturagrundlag", originals: "Originalbilag", physical_schema: "Database og schema" };
function verified(value: View, slug: string): View {
  const audit = value?.audit;
  if (value?.ok !== true || audit?.format !== "rentemester-native-audit-verification/1" || audit.slug !== slug || !Number.isSafeInteger(audit.entries) || audit.entries < 0
    || !Array.isArray(audit.errors) || audit.errors.some(error => typeof error !== "string") || !Array.isArray(audit.components) || audit.components.length !== 5
    || new Set(audit.components.map(component => component.name)).size !== 5
    || audit.components.some(component => !Object.hasOwn(labels, component.name) || typeof component.available !== "boolean" || typeof component.ok !== "boolean" || !Array.isArray(component.errors)
      || !component.checked || typeof component.checked !== "object" || Object.values(component.checked).some(count => !Number.isSafeInteger(count) || count < 0)
      || component.ok !== (component.available && component.errors.length === 0) || (!component.available && component.errors.length === 0))
    || audit.available !== audit.components.every(component => component.available)
    || audit.ok !== audit.components.every(component => component.available && component.ok)
    || audit.errors.length !== audit.components.reduce((sum, component) => sum + component.errors.length, 0)) throw Error("Serverens regnskabskontrol kunne ikke bekræftes.");
  return value;
}
export function NativeAuditVerify({ slug }: { slug: string }) {
  const [value, setValue] = useState<View | null>(null), [busy, setBusy] = useState(false), [error, setError] = useState("");
  const sending = useRef(false);
  async function load() {
    if (sending.current) return;
    sending.current = true; setBusy(true); setError(""); setValue(null);
    try { setValue(verified(await request<View>(`/api/companies/${encodeURIComponent(slug)}/audit-verify`), slug)); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Regnskabskontrollen kunne ikke gennemføres."); }
    finally { sending.current = false; setBusy(false); }
  }
  return <section {...stylex.props(cockpitStyles.card)} aria-label="Regnskabskontrol">
    <h3>Regnskabskontrol</h3>
    <p {...stylex.props(cockpitStyles.muted)}>Sammenhold posteringer, bankkoblinger og fakturagrundlag med registrerede originalfiler. Kontrollen ændrer ingen regnskabsdata.</p>
    <Button type="button" disabled={busy} onClick={() => void load()}>Kontrollér regnskabet</Button>
    {error && <p role="alert">{error}</p>}
    {value && <>
      <p role="status" {...stylex.props(...[cockpitStyles.banner, ...(value.audit.ok ? [cockpitStyles.bannerSuccess] : (value.audit.available ? [cockpitStyles.bannerError] : [cockpitStyles.statusChipWarning]))])}>
        {value.audit.ok ? "Kontrollen fandt ingen afvigelser." : value.audit.available ? "Kontrollen fandt afvigelser." : "Kontrollen kunne ikke gennemføres fuldt ud."} Posteringer gennemgået: {value.audit.entries}.
      </p>
      {value.audit.components.map(component => <div key={component.name}>
        <h4>{labels[component.name]}</h4>
        <p>{component.ok ? "Ingen afvigelser fundet." : component.available ? "Afvigelser fundet." : "Kontrollen er ikke fuldført."}</p>
        {component.errors.length > 0 && <details><summary>Vis kontrollens detaljer</summary><pre {...stylex.props(cockpitStyles.wrappedEvidence)}>{JSON.stringify(component.errors, null, 2)}</pre></details>}
      </div>)}
      {!value.audit.ok && <p>Gennemgå afvigelser og manglende kontroller, før resultatet bruges som dokumentation.</p>}
    </>}
  </section>;
}
