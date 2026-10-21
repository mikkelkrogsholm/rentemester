import { useRef, useState } from "react";
import { request } from "../lib/api/_shared";
import { Button, Input } from "./ui";

type Row = { id: number; eventType: string; entityType: string; entityId: string | null; message: string; actor: string; createdAt: string };
type View = { ok: true; slug: string; rows: Row[]; total: number; count: number; limit: number; offset: number; hasMore: boolean; nextOffset?: number };
type Filters = { fromDate: string; toDate: string; eventTypeLike: string; actorLike: string; limit: string };
const initial: Filters = { fromDate: "", toDate: "", eventTypeLike: "", actorLike: "", limit: "50" };
function verified(value: View, slug: string, offset: number): View {
  if (value?.ok !== true || value.slug !== slug || !Array.isArray(value.rows) || !Number.isSafeInteger(value.total) || value.total < 0
    || !Number.isSafeInteger(value.count) || value.count !== value.rows.length || !Number.isSafeInteger(value.limit) || value.limit < 1 || value.limit > 5000
    || value.count > value.limit || value.offset !== offset || value.total < value.count || typeof value.hasMore !== "boolean"
    || value.hasMore !== (offset + value.count < value.total)
    || (value.hasMore ? value.nextOffset !== offset + value.count || value.count === 0 : value.nextOffset !== undefined)
    || value.rows.some(row => !Number.isSafeInteger(row.id) || row.id < 1 || typeof row.eventType !== "string" || typeof row.entityType !== "string"
      || !(row.entityId === null || typeof row.entityId === "string") || typeof row.message !== "string" || typeof row.actor !== "string" || typeof row.createdAt !== "string")) {
    throw Error("Serverens revisionsspor kunne ikke bekræftes.");
  }
  return value;
}
export function NativeAuditLog({ slug }: { slug: string }) {
  const [form, setForm] = useState<Filters>(initial), [submitted, setSubmitted] = useState<Filters>(initial);
  const [value, setValue] = useState<View | null>(null), [error, setError] = useState(""), [busy, setBusy] = useState(false);
  const sending = useRef(false);
  async function load(filters: Filters, offset: number) {
    if (sending.current) return;
    sending.current = true; setBusy(true); setError(""); setValue(null);
    const query = new URLSearchParams({ limit: filters.limit, offset: String(offset) });
    for (const field of ["fromDate", "toDate", "eventTypeLike", "actorLike"] as const) if (filters[field]) query.set(field, filters[field]);
    try { const reply = await request<View>(`/api/companies/${encodeURIComponent(slug)}/audit-log?${query}`); setValue(verified(reply, slug, offset)); setSubmitted(filters); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Revisionsspor kunne ikke hentes."); }
    finally { sending.current = false; setBusy(false); }
  }
  function field(name: keyof Filters, label: string, type = "text") {
    return <label>{label}<Input type={type} value={form[name]} min={type === "number" ? 1 : undefined} max={type === "number" ? 5000 : undefined} maxLength={type === "text" ? 256 : undefined} onChange={event => setForm(previous => ({ ...previous, [name]: event.target.value }))} /></label>;
  }
  return <section className="card" aria-label="Revisionsspor">
    <h3>Revisionsspor</h3>
    <p className="muted">Find, hvem der har foretaget en handling. Filtre kombineres. Listen viser registrerede hændelser; den er ikke en fuld integritetskontrol.</p>
    <form onSubmit={event => { event.preventDefault(); void load({ ...form }, 0); }}>
      <div className="form-grid">{field("fromDate", "Fra dato", "date")}{field("toDate", "Til dato", "date")}{field("eventTypeLike", "Hændelse indeholder")}{field("actorLike", "Aktør indeholder")}{field("limit", "Rækker pr. side", "number")}</div>
      <Button type="submit" disabled={busy}>Hent revisionsspor</Button>
    </form>
    {error && <p role="alert">{error}</p>}
    {value && <>
      <p role="status">{value.total} hændelser matcher. Viser {value.count ? value.offset + 1 : 0}–{value.count ? value.offset + value.count : 0}.</p>
      {value.rows.length === 0 ? <p>Ingen hændelser matcher filtrene.</p> : <div className="table-wrap"><table><thead><tr><th>Id</th><th>Tidspunkt</th><th>Aktør</th><th>Hændelse</th><th>Reference</th><th>Besked</th></tr></thead><tbody>
        {value.rows.map(row => <tr key={row.id}><td>{row.id}</td><td>{row.createdAt}</td><td>{row.actor}</td><td>{row.eventType}</td><td>{row.entityType}{row.entityId === null ? "" : ` · ${row.entityId}`}</td><td style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{row.message}</td></tr>)}
      </tbody></table></div>}
      <div className="row-actions"><Button type="button" variant="secondary" disabled={busy || value.offset === 0} onClick={() => void load(submitted, Math.max(0, value.offset - value.limit))}>Forrige side</Button><Button type="button" variant="secondary" disabled={busy || !value.hasMore} onClick={() => void load(submitted, value.nextOffset!)}>Næste side</Button></div>
      <p className="muted">Hver side er et nyt opslag. Nye hændelser kan flytte rækkernes placering. % og _ i tekstfiltre fungerer som jokertegn.</p>
    </>}
  </section>;
}
