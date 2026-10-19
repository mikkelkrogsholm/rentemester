import { type FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";
import { Button, ButtonLink, Dialog, Input, PageHeader, Select } from "../components/ui";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type {
  CommitmentInput,
  CommitmentProposal,
  SupplierCommitmentRow,
  SupplierCommitmentsResponse,
} from "../lib/types/commitments";
import { useAsync } from "../lib/useAsync";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

type Data = SupplierCommitmentsResponse["supplierCommitments"];
const cadence=(value:string|null)=>({weekly:"Ugentlig",monthly:"Månedlig",quarterly:"Kvartalsvis",yearly:"Årlig"}[value??""]??value??"Ukendt");
const statusLabel=(value:string)=>({active:"Aktiv",approved:"Godkendt",paused:"Pauseret",ended:"Afsluttet",superseded:"Erstattet",unresolved:"Uafklaret"}[value]??value);
const evidenceLabel=(value:string)=>({canonical_document:"Bilag",payable:"Kreditor",bank_transaction:"Bankpost"}[value]??value);
const sourceLabel=(value:string)=>({receivables:"Tilgodehavender",payables:"Kreditorer",commitments:"Godkendte forpligtelser",obligations:"Momsforpligtelser",scenarios:"Åben moms (antagelse)",undated_budget:"Udateret budget"}[value]??value);

const dateToday = () => new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
function Author({
  slug,
  data,
  close,
  reload,
}: {
  slug: string;
  data: Data;
  close: () => void;
  reload: () => void;
}) {
  const [values, setValues] = useState({
    id: "",
    vendor: "",
    type: "",
    description: "",
    purpose: "",
    amount: "",
    currency: data.company.currency,
    frequency: "monthly" as CommitmentInput["frequency"],
    nextDate: data.startDate,
    startDate: "",
    endDate: "",
    renewalDate: "",
    noticeDate: "",
    document: "",
    vatProposal: "",
  });
  const [review, setReview] = useState<{
      input: CommitmentInput;
      plan: CommitmentProposal;
      key: string;
    } | null>(null),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const markSaved = useUnsavedChanges(
    Boolean(
      values.id ||
        values.vendor ||
        values.type ||
        values.description ||
        values.purpose ||
        values.amount ||
        values.document,
    ) && !review,
  );
  const set = (key: keyof typeof values, value: string) =>
    setValues((v) => ({ ...v, [key]: value }));
  async function plan(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const input: CommitmentInput = {
        commitmentId: values.id,
        vendorPartyId: `vendor:${values.vendor}`,
        type: values.type,
        description: values.description,
        businessPurpose: values.purpose,
        amount: Number(values.amount),
        currency: values.currency,
        frequency: values.frequency,
        nextDate: values.nextDate,
        evidenceRefs: [`document:${values.document}`],
        ...(values.startDate ? { startDate: values.startDate } : {}),
        ...(values.endDate ? { endDate: values.endDate } : {}),
        ...(values.renewalDate ? { renewalDate: values.renewalDate } : {}),
        ...(values.noticeDate ? { noticeDate: values.noticeDate } : {}),
        ...(values.vatProposal ? { vatProposal: values.vatProposal } : {}),
      };
      setReview({
        input,
        plan: await api.supplierCommitmentPlan(slug, input),
        key: crypto.randomUUID(),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  if (review)
    return (
      <ConfirmDialog
        title="Godkend forpligtelse"
        operationKey={review.plan.payloadHash}
        body={
          <>
            <p>
              {review.plan.vendorSnapshot} · {review.plan.businessPurpose} ·{" "}
              {formatKroner(review.plan.amount, review.plan.currency)} · {cadence(review.plan.frequency)}
            </p>
            <p>Planlægning alene. Opretter ingen kreditor, betaling eller bogføring.</p>
            <table className="data">
              <caption>Forventede betalinger i planens første 13 uger</caption>
              <thead>
                <tr>
                  <th>Dato</th>
                  <th>Beløb</th>
                </tr>
              </thead>
              <tbody>
                {review.plan.schedule.map((s) => (
                  <tr key={s.date}>
                    <td>{s.date}</td>
                    <td>{formatKroner(s.amount, s.currency)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p>Kilde: {review.input.evidenceRefs.map(ref=>data.documents.find(d=>`document:${d.id}`===ref)?.documentNo??ref).join(", ")}</p>
            {review.plan.warnings.map((w) => (
              <p key={w}>{w}</p>
            ))}
          </>
        }
        confirmLabel="Godkend planen"
        onClose={() => {
          setReview(null);
        }}
        onRefresh={reload}
        onConfirm={async () => {
          await api.supplierCommitmentApply(slug, {
            commitment: review.input,
            payloadHash: review.plan.payloadHash,
            idempotencyKey: review.key,
          });
          markSaved();
          reload();
          close();
        }}
      />
    );
  return (
    <Dialog title="Ny leverandørforpligtelse" onClose={close}>
      <form aria-label="Planlæg leverandørforpligtelse" onSubmit={(e) => void plan(e)}>
        <fieldset disabled={busy}>
          <label>
            Reference{" "}
            <Input
              required
              maxLength={96}
              value={values.id}
              onChange={(e) => set("id", e.target.value)}
            />
          </label>
          <label>
            Leverandør{" "}
            <Select aria-label="Leverandør" required value={values.vendor} onChange={(e) => set("vendor", e.target.value)}>
              <option value="">Vælg eksisterende leverandør</option>
              {data.vendors.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name}
                </option>
              ))}
            </Select>
          </label>
          <p>Leverandørens aktuelle navn og identitet gemmes med planen.</p>
          <label>
            Type{" "}
            <Input required value={values.type} onChange={(e) => set("type", e.target.value)} />
          </label>
          <label>
            Beskrivelse{" "}
            <Input
              required
              value={values.description}
              onChange={(e) => set("description", e.target.value)}
            />
          </label>
          <label>
            Forretningsformål{" "}
            <Input
              required
              value={values.purpose}
              onChange={(e) => set("purpose", e.target.value)}
            />
          </label>
          <label>
            Beløb pr. betaling{" "}
            <Input
              type="number"
              min="0.01"
              step="0.01"
              required
              value={values.amount}
              onChange={(e) => set("amount", e.target.value)}
            />
          </label>
          <label>
            Valuta{" "}
            <Input
              required
              pattern="[A-Z]{3}"
              maxLength={3}
              value={values.currency}
              onChange={(e) => set("currency", e.target.value)}
            />
          </label>
          <label>
            Frekvens{" "}
            <Select aria-label="Frekvens" value={values.frequency} onChange={(e) => set("frequency", e.target.value)}>
              <option value="weekly">Ugentlig</option>
              <option value="monthly">Månedlig</option>
              <option value="quarterly">Kvartalsvis</option>
              <option value="yearly">Årlig</option>
            </Select>
          </label>
          {(
            [
              ["nextDate", "Næste betaling"],
              ["startDate", "Startdato (valgfri)"],
              ["endDate", "Slutdato (valgfri)"],
              ["renewalDate", "Fornyelse (valgfri)"],
              ["noticeDate", "Opsigelsesfrist (valgfri)"],
            ] as const
          ).map(([key, label]) => (
            <label key={key}>
              {label}
              <Input
                type="date"
                required={key === "nextDate"}
                value={values[key]}
                onChange={(e) => set(key, e.target.value)}
              />
            </label>
          ))}
          <label>
            Originalt kildebilag{" "}
            <Select
              aria-label="Originalt kildebilag"
              required
              value={values.document}
              onChange={(e) => set("document", e.target.value)}
            >
              <option value="">Vælg bilag</option>
              {data.documents.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.documentNo ?? d.filename ?? d.id}
                </option>
              ))}
            </Select>
          </label>
          <label>
            Momsforslag (valgfrit, ikke bogføring){" "}
            <Input
              value={values.vatProposal}
              onChange={(e) => set("vatProposal", e.target.value)}
            />
          </label>
          <Button type="submit">Vis plan til review</Button>
        </fieldset>
        {error && <p role="alert">{error}</p>}
      </form>
    </Dialog>
  );
}
function MatchForm({ slug, data, reload }: { slug: string; data: Data; reload: () => void }) {
  const [selected, setSelected] = useState(""),
    [evidence, setEvidence] = useState(""),
    [confirm, setConfirm] = useState(false);
  const occurrence = data.occurrences.find((o) => `${o.commitmentId}:${o.date}` === selected),
    actual = data.evidenceOptions.find((o) => `${o.kind}:${o.id}` === evidence);
  return (
    <>
      <form
        aria-label="Match forventet betaling"
        onSubmit={(e) => {
          e.preventDefault();
          setConfirm(true);
        }}
      >
        <label>
          Forventet betaling{" "}
          <Select aria-label="Forventet betaling" required value={selected} onChange={(e) => setSelected(e.target.value)}>
            <option value="">Vælg umatchet forekomst</option>
            {data.occurrences.map((o) => (
              <option key={`${o.commitmentId}:${o.date}`} value={`${o.commitmentId}:${o.date}`}>
                {o.commitmentId} · {o.date} · {formatKroner(o.amount, o.currency)}
              </option>
            ))}
          </Select>
        </label>
        <label>
          Faktisk kilde{" "}
          <Select aria-label="Faktisk kilde" required value={evidence} onChange={(e) => setEvidence(e.target.value)}>
            <option value="">Vælg bilag, kreditor eller udgående bankpost</option>
            {data.evidenceOptions.map((o) => (
              <option key={`${o.kind}:${o.id}`} value={`${o.kind}:${o.id}`}>
                {evidenceLabel(o.kind)} {o.id} · {o.label} · {o.date} · {formatKroner(o.amount, o.currency)}
              </option>
            ))}
          </Select>
        </label>
        <p>
          Seneste 500 pr. evidenstype. Serveren læser beløb, dato, valuta og identitet igen. Et
          match fjerner kun forventningen fra prognosen.
        </p>
        <Button
          requiredPermission="company.draft.write"
          type="submit"
          disabled={!occurrence || !actual}
        >
          Gennemgå match
        </Button>
      </form>
      {confirm && occurrence && actual && (
        <ConfirmDialog
          title="Bekræft evidensmatch"
          operationKey={`${selected}:${evidence}`}
          body={
            <p>
              {occurrence.commitmentId} · {occurrence.date}: forventet{" "}
              {formatKroner(occurrence.amount, occurrence.currency)}. Valgt {actual.kind}{" "}
              {actual.id} · {actual.date} · {formatKroner(actual.amount, actual.currency)}. Opretter
              ingen betaling eller bogføring.
            </p>
          }
          confirmLabel="Bekræft match"
          onClose={() => setConfirm(false)}
          onRefresh={reload}
          onConfirm={async () => {
            await api.supplierCommitmentMatch(slug, {
              commitmentId: occurrence.commitmentId,
              occurrenceDate: occurrence.date,
              evidence: { kind: actual.kind, id: actual.id },
            });
            setSelected("");
            setEvidence("");
            reload();
          }}
        />
      )}
    </>
  );
}
export function NativeLiquidityView() {
  const { slug = "" } = useParams(),
    { setYear } = useCompanyYear();
  const [asOf, setAsOf] = useState(dateToday),
    [creating, setCreating] = useState(false),
    [pending, setPending] = useState<{
      row: SupplierCommitmentRow;
      action: "paused" | "ended";
    } | null>(null);
  const state = useAsync((signal) => api.supplierCommitments(slug, asOf, { signal }), [slug, asOf]);
  if (state.loading && !state.data) return <Loading label="Henter likviditetsprognose…" />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!,
    currency = data.company.currency,
    f = data.forecast;
  const amount = (n: number | null) =>
    n === null ? "Ikke tilgængelig" : formatKroner(n, currency);
  return (
    <section className="statement">
      <PageHeader
        title="Likviditet"
        actions={
          <Button requiredPermission="company.draft.write" onClick={() => setCreating(true)}>
            Ny forpligtelse
          </Button>
        }
      >
        <p>
          {data.company.name} · {currency}
        </p>
      </PageHeader>
      <CompanyNav
        slug={slug}
        years={data.fiscalYears}
        selectedYear={data.selectedYear}
        onYearChange={setYear}
      />
      <p>
        <ButtonLink to={`/companies/${slug}/bank`}>Se faktiske bankposter og afstemning</ButtonLink>
      </p>
      <h2>13 ugers likviditetsprognose</h2>
      <label>
        Prognose fra{" "}
        <Input
          type="date"
          value={asOf}
          onChange={(e) => {
            if (e.target.value) setAsOf(e.target.value);
          }}
        />
      </label>
      <p>
        Bogført bank pr. {asOf}: {amount(f.openingCash)} · laveste basepunkt: {amount(f.lowestPoint)}.
        Forventninger er ikke betalingstilsagn.
      </p>
      <p role="status">
        Ufuldstændig dækning — gennemgå kilder og manglende oplysninger før beslutninger.
      </p>
      <table className="data" aria-label="13 ugers prognose">
        <thead>
          <tr>
            <th>Uge</th>
            <th>Tilgodehavender</th>
            <th>Kreditorer</th>
            <th>Forpligtelser</th>
            <th>Momsforpligtelser</th>
            <th>Base ultimo</th>
            <th>Scenarie ultimo</th>
          </tr>
        </thead>
        <tbody>
          {f.periods.map((p) => (
            <tr key={p.weekStart}>
              <td>{p.weekStart}</td>
              <td>{amount(p.receivables)}</td>
              <td>{amount(p.payables)}</td>
              <td>{amount(p.commitments)}</td>
              <td>{amount(p.obligations)}</td>
              <td>{amount(p.closingCash)}</td>
              <td>{amount(p.scenarioClosingCash)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <details>
        <summary>Kilder, antagelser og udeladelser</summary>
        <h3>Medtaget</h3>
        <ul>
          {f.completeness.included.map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
        <h3>Udeladt eller usikkert</h3>
        <ul>
          {f.completeness.excluded.map((s) => (
            <li key={s}>{s}</li>
          ))}
        </ul>
        {f.periods.map((p) => (
          <div key={p.weekStart}>
            <h4>{p.weekStart}</h4>
            {p.sources.map((s, i) => (
              <p key={`${s.reference}:${i}`}>
                {sourceLabel(s.source)} · {formatKroner(s.amount, currency)} · {s.reference}
                {s.assumption ? " · antagelse" : ""}
                {s.settlementStatus === "unknown" ? " · betalingsstatus ukendt" : ""}
              </p>
            ))}
            {p.excluded.map((s, i) => (
              <p key={`${s.source}:${i}`}>
                Udeladt {sourceLabel(s.source)} · {formatKroner(s.amount, s.currency)} · {s.reason}
              </p>
            ))}
          </div>
        ))}
      </details>
      <h2>Leverandørforpligtelser</h2>
      {!data.commitments.length ? (
        <p>Ingen godkendte forpligtelser.</p>
      ) : (
        <table className="data" aria-label="Leverandørforpligtelser">
          <thead>
            <tr>
              <th>Reference</th>
              <th>Leverandør og formål</th>
              <th>Beløb</th>
              <th>Næste og status</th>
              <th>Kilde</th>
              <th>Handling</th>
            </tr>
          </thead>
          <tbody>
            {data.commitments.map((c) => (
              <tr key={c.commitmentId}>
                <td>{c.commitmentId}</td>
                <td>
                  {c.vendor} · {c.purpose}
                </td>
                <td>
                  {c.amount === null ? "Ukendt" : formatKroner(c.amount, c.currency ?? currency)}
                </td>
                <td>
                  {c.nextDate} · {cadence(c.frequency)} · {statusLabel(c.status)}
                </td>
                <td>{c.evidenceRefs.join(", ")}</td>
                <td>
                  {c.status === "active" && (
                    <Button
                      requiredPermission="company.draft.write"
                      variant="secondary"
                      onClick={() => setPending({ row: c, action: "paused" })}
                    >
                      Pause
                    </Button>
                  )}
                  {c.status !== "ended" && (
                    <Button
                      requiredPermission="company.draft.write"
                      variant="secondary"
                      onClick={() => setPending({ row: c, action: "ended" })}
                    >
                      Afslut
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {data.alerts.map((a) => (
        <p key={`${a.commitmentId}:${a.kind}:${a.date}`}>
          {a.commitmentId} · {a.kind} · {a.date}
        </p>
      ))}
      {data.occurrences.length > 0 && (
        <details>
          <summary>Match faktisk bilag, kreditor eller bankpost</summary>
          <MatchForm slug={slug} data={data} reload={state.reload} />
        </details>
      )}
      {data.matches.length > 0 && (
        <section>
          <h3>Faktisk mod forventet</h3>
          {data.matches.map((m) => (
            <p key={`${m.commitmentId}:${m.occurrenceDate}`}>
              {m.commitmentId} · {m.occurrenceDate} ·{" "}
              {m.variance.amount === null
                ? "Valuta kan ikke sammenlignes"
                : amount(m.variance.amount)}{" "}
              · {m.variance.dateDays} dage · bilag {m.variance.documentation} · part{" "}
              {m.variance.party?.status ?? "ukendt"}
            </p>
          ))}
        </section>
      )}
      <details>
        <summary>Uforanderlig historik</summary>
        {data.history.map((h) => (
          <p key={`${h.commitmentId}:${h.payloadHash}:${h.event}`}>
            {h.createdAt} · {h.commitmentId} · {statusLabel(h.event)} · {h.actor ?? ""} ·{" "}
            {h.payload.reason ?? "Godkendt plan"}
          </p>
        ))}
      </details>
      {creating && (
        <Author slug={slug} data={data} close={() => setCreating(false)} reload={state.reload} />
      )}{" "}
      {pending && (
        <ConfirmDialog
          title={pending.action === "paused" ? "Pause forpligtelse" : "Afslut forpligtelse"}
          operationKey={pending.row.payloadHash}
          body={
            <p>
              {pending.row.commitmentId}: ændringen føjes til historikken og fjerner fremtidige
              forventninger fra prognosen.
            </p>
          }
          noteLabel="Begrundelse"
          confirmLabel={pending.action === "paused" ? "Pause" : "Afslut"}
          onClose={() => setPending(null)}
          onRefresh={state.reload}
          onConfirm={async (reason) => {
            if (!reason.trim()) throw new Error("Begrundelse er påkrævet.");
            await api.supplierCommitmentChange(slug, {
              commitmentId: pending.row.commitmentId,
              action: pending.action,
              reason,
              expectedPayloadHash: pending.row.payloadHash,
            });
            state.reload();
          }}
        />
      )}
    </section>
  );
}
