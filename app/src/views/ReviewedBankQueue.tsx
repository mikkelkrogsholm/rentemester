import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { useParams } from "react-router-dom";
import { Button, ButtonLink, Input, PageHeader, Amount } from "../components/ui";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { request } from "../lib/api/_shared";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import type { AccountingDraft } from "../lib/types";
type Row = { bankTransactionId: number; date: string; text: string; amount: number; currency: string; bankAccountNo: string | null; suggestedAccount: string | null; sourceHash: string };
type Queue = { rows: Row[]; truncated: boolean };
export function ReviewedBankQueue() {
 const { slug = "" } = useParams(); const path = `/api/companies/${encodeURIComponent(slug)}/reviewed-bank`;
 const state = useAsync(signal => request<{ ok: true; queue: Queue }>(path, { signal }).then(x => x.queue), [slug]);
 const [selected, setSelected] = useState<Row | null>(null); const [account, setAccount] = useState(""); const [document, setDocument] = useState(""); const [draftId, setDraftId] = useState(""); const [confirming, setConfirming] = useState(false); const [created, setCreated] = useState<string | null>(null);
 const outcome = useMutationOutcome(state.reload);
 if (state.loading && !state.data) return <Loading label="Henter bankens arbejdskø…" />;
 if (state.error && !state.data) return <ErrorState message={state.error} onRetry={state.reload} />;
 async function create() {
   if (!selected || outcome.isBlocked()) return;
   const id = document.trim() ? Number(document) : undefined;
   if (id !== undefined && (!Number.isSafeInteger(id) || id <= 0)) throw new Error("Bilags-id skal være et positivt heltal.");
   const result = await request<{ accountingDraft: AccountingDraft }>(path, { method: "POST", body: JSON.stringify({ bankTransactionId: selected.bankTransactionId, expectedSourceHash: selected.sourceHash, accountNo: account, draftId, ...(id === undefined ? {} : { documentId: id }), confirm: true }) }).catch(outcome.reject);
   setCreated(result.accountingDraft.id); setSelected(null); state.reload();
 }
 return <section {...stylex.props(cockpitStyles.statement)}><PageHeader title="Bankens arbejdskø" actions={<><ButtonLink to={`/companies/${slug}/kladder`}>Gennemgå kladder</ButtonLink><ButtonLink to={`/companies/${slug}/posteringsregler`}>Posteringsregler</ButtonLink></>} />
   <p>Opret én kildebundet kladde ad gangen. Gennemgå linjerne, indsend den, og lad en anden bruger godkende og bogføre. Samlet batchanvendelse og fulde slutkontroller er endnu ikke tilgængelige her.</p>
   <p>Denne genvej håndterer DKK uden automatisk momsberegning. Vælg korrekt bilag og modkonto; moms eller valutaforhold kræver gennemgang af kladdens linjer.</p>
   {outcome.feedback}{state.error && <p role="alert">{state.error}</p>}{created && <p role="status">Kladde {created} er oprettet. <ButtonLink to={`/companies/${slug}/kladder`}>Åbn kladder og indsend til uafhængig godkendelse</ButtonLink></p>}
   {state.data?.truncated && <p role="alert">Udvalget er begrænset til 501 poster. Køen er ikke en fuldstændighedskontrol.</p>}
   <table {...stylex.props(...[cockpitStyles.tableData])}><thead><tr><th>Dato</th><th>Bankpost</th><th>Beløb</th><th>Konti</th><th>Handling</th></tr></thead><tbody>{state.data?.rows.map(row => <tr key={row.bankTransactionId}><td>{row.date}</td><td>#{row.bankTransactionId} {row.text}</td><td><Amount value={row.amount} currency={row.currency} /></td><td>Bank: {row.bankAccountNo ?? "Ikke tilknyttet"}<div>Regelforslag: {row.suggestedAccount ?? "Kræver vurdering"}</div></td><td><Button requiredPermission="company.draft.write" disabled={outcome.blocked || !row.bankAccountNo || row.currency !== "DKK"} onClick={() => { setSelected(row); setAccount(row.suggestedAccount ?? ""); setDocument(""); setDraftId(`bank-${row.bankTransactionId}`); }}>Opret bankkladde</Button></td></tr>)}</tbody></table>
   {state.data?.rows.length === 0 && <p>Ingen uafstemte bankposter.</p>}
   {selected && <form {...stylex.props(cockpitStyles.card, cockpitStyles.formGrid)} onSubmit={e => { e.preventDefault(); setConfirming(true); }}><h3>Gennemgå bankpost #{selected.bankTransactionId}</h3><p>{selected.date} · {selected.text} · <Amount value={selected.amount} currency={selected.currency} /></p><label>Kladde-id<Input required pattern="[a-z][a-z0-9-]{0,63}" value={draftId} onChange={e => setDraftId(e.target.value)} /></label><label>Modkonto<Input required value={account} onChange={e => setAccount(e.target.value)} /></label><label>Bilags-id (kræves ved udgift eller indtægt)<Input inputMode="numeric" value={document} onChange={e => setDocument(e.target.value)} /></label><Button type="submit" disabled={outcome.blocked}>Gennemgå oprettelse</Button><Button type="button" variant="secondary" onClick={() => setSelected(null)}>Annullér</Button></form>}
   {confirming && selected && <ConfirmDialog title="Opret kladde fra præcis bankpost" body={<p>Bankpost #{selected.bankTransactionId}: {selected.text}. Modkonto {account}, bilag {document || "intet"}. Dette bogfører ikke.</p>} confirmLabel="Opret kladde" onConfirm={create} onClose={() => setConfirming(false)} onRefresh={state.reload} operationKey={`bank-draft-${selected.sourceHash}-${draftId}`} />}
 </section>;
}
