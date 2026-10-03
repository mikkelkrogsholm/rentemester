import { useMutationOutcome } from "../lib/useMutationOutcome";
import { parseDanishAmount } from "../lib/format";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Dialog, Button, Input, PageHeader, Select, MoneyInput } from "../components/ui";
// Anlægskartotek — the per-company fixed-asset view (#336).
//
// Renders `/api/companies/:slug/assets`: every capitalised asset with its
// posted/remaining periods, akkumuleret afskrivning, restværdi and status,
// plus the straksafskrivning history. Three actions live here:
//
//   * "Registrér anlæg" — POST /api/companies/:slug/assets. The owner picks
//     an existing bilag (purchase document), enters acquisition date, cost,
//     levetid and category; the server computes the deterministic linear
//     depreciation plan via the SAME `registerAsset` core the CLI uses.
//
//   * "Beregn afskrivning" pr. række — POST .../assets/:id/depreciate.
//     A confirm-gated one-click that posts the NEXT unposted period of the
//     asset's schedule through `postDepreciationPeriod`. The cockpit shows
//     the period number + amount before the owner confirms.
//
//   * "Straksafskriv" — POST .../assets/write-off. Books a small purchase as
//     a straksafskrivning via `postImmediateWriteOff`; the threshold-rule
//     reference is captured verbatim on the audit record.
//
// All depreciation arithmetic is computed server-side — this view never
// re-implements the schedule.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  AssetRow,
  AssetWriteOffRow,
  CompanyAssets,
  CompanyDocuments,
  DocumentRow,
} from "../lib/types";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";

const DEFAULT_EXPENSE_ACCOUNT = "3000";
const DEFAULT_THRESHOLD_RULE =
  "AL §6 stk. 1 nr. 2 — småanskaffelser (straksafskrivning)";

export function AssetsView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyAssets>((signal) => api.assets(slug, { signal }), [slug]);
  const [registerOpen, setRegisterOpen] = useState(false);
  const [writeOffOpen, setWriteOffOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  if (state.loading && !state.data)
    return <Loading label="Henter anlægskartotek…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const data = state.data!;
  const currency = data.company.currency || "DKK";

  return (
    <section className="statement" data-cockpit-page="assets" data-evidence-issue="655">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Anlæg" actions={<><div className="row-actions">
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Anlæg
          </p>
        </div>

      </PageHeader>

      <p className="statement-asof muted">
        Anlægskartoteket — kapitaliserede aktiver, deres afskrivninger og
        straksafskrivninger. Alle beløb i {currency}.
      </p>

      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Bogført kostpris</h3>
          <div className="status-figure">
            {formatKroner(data.totals.cost, currency)}
          </div>
          <p className="muted status-note">
            {data.totals.activeCount} aktive ·{" "}
            {data.totals.fullyDepreciatedCount} fuldt afskrevne
          </p>
        </div>
        <div className="card status-card">
          <h3>Restværdi (netto)</h3>
          <div className="status-figure">
            {formatKroner(data.totals.netBookValue, currency)}
          </div>
          <p className="muted status-note">
            Akkumuleret afskrevet{" "}
            {formatKroner(data.totals.accumulatedDepreciation, currency)}
          </p>
        </div>
        <div className="card status-card">
          <h3>Straksafskrivninger</h3>
          <div className="status-figure">
            {formatKroner(data.totals.writeOffTotal, currency)}
          </div>
          <p className="muted status-note">
            {data.totals.writeOffCount}{" "}
            {data.totals.writeOffCount === 1 ? "post" : "poster"}
          </p>
        </div>
      </div>

      <div className={["row-actions", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
        <Button requiredPermission="company.ledger.post"
          type="button"
          className="btn"
          onClick={() => {
            setActionError(null);
            setRegisterOpen(true);
          }}
        >
          Registrér anlæg
        </Button>
        <Button requiredPermission="company.ledger.post" variant="secondary"
          type="button"
          className="btn secondary"
          onClick={() => {
            setActionError(null);
            setWriteOffOpen(true);
          }}
        >
          Straksafskriv
        </Button>
      </div>

      {actionError ? (
        <div className="card archived-notice" role="alert">
          <p className="muted">{actionError}</p>
        </div>
      ) : null}

      <h3 {...stylex.props(viewStyles.site1)}>Kapitaliserede anlæg</h3>
      {data.assets.length === 0 ? (
        <div className="card archived-notice">
          <p className="muted">
            Der er ingen kapitaliserede anlæg endnu. Brug "Registrér anlæg" til
            at oprette et nyt aktiv ud fra et eksisterende bilag.
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table responsive-table" aria-label="Kapitaliserede anlæg">
            <thead>
              <tr>
                <th>Navn</th>
                <th>Kategori</th>
                <th>Anskaffet</th>
                <th className="num">Kostpris</th>
                <th className="num">Akkumuleret afskrivning</th>
                <th className="num">Restværdi</th>
                <th>Status</th>
                <th>Handling</th>
              </tr>
            </thead>
            <tbody>
              {data.assets.map((row) => (
                <AssetRowView
                  key={row.assetId}
                  row={row}
                  slug={slug}
                  currency={currency}
                  onPosted={() => state.reload()}
                  onError={setActionError}
                />
              ))}
              <tr className="statement-result">
                <td colSpan={3}>I alt</td>
                <td className="num">
                  {formatKroner(data.totals.cost, currency)}
                </td>
                <td className="num">
                  {formatKroner(
                    data.totals.accumulatedDepreciation,
                    currency,
                  )}
                </td>
                <td className="num">
                  {formatKroner(data.totals.netBookValue, currency)}
                </td>
                <td colSpan={2} />
              </tr>
            </tbody>
          </table>
        </div>
      )}

      <h3 {...stylex.props(viewStyles.site2)}>Straksafskrivninger</h3>
      {data.writeOffs.length === 0 ? (
        <div className="card archived-notice">
          <p className="muted">
            Ingen straksafskrivninger endnu. Brug "Straksafskriv" når en
            småanskaffelse er under det skattemæssige minimum og bogføres
            direkte som udgift.
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table responsive-table" aria-label="Straksafskrivninger">
            <thead>
              <tr>
                <th>Navn</th>
                <th>Kategori</th>
                <th>Anskaffet</th>
                <th>Bogført</th>
                <th className="num">Beløb</th>
                <th>Konto</th>
                <th>Hjemmel</th>
              </tr>
            </thead>
            <tbody>
              {data.writeOffs.map((row) => (
                <WriteOffRow key={row.id} row={row} currency={currency} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <p className="statement-check ok">
        Alle afskrivninger bogføres via den deterministiske kerne (linje:
        debit afskrivnings-udgift, kredit akkumuleret afskrivning) — det er
        samme kode CLI'en og MCP-kald bruger, så posteringerne er identiske
        på tværs af kanaler.
      </p>

      {registerOpen ? (
        <RegisterAssetModal
          slug={slug}
          onClose={() => setRegisterOpen(false)}
          onRefresh={state.reload}
          onCreated={() => {
            setRegisterOpen(false);
            setActionError(null);
            state.reload();
          }}
          onError={setActionError}
        />
      ) : null}

      {writeOffOpen ? (
        <WriteOffModal
          slug={slug}
          onClose={() => setWriteOffOpen(false)}
          onRefresh={state.reload}
          onCreated={() => {
            setWriteOffOpen(false);
            setActionError(null);
            state.reload();
          }}
          onError={setActionError}
        />
      ) : null}
    </section>
  );
}

function AssetRowView({
  row,
  slug,
  currency,
  onPosted,
  onError,
}: {
  row: AssetRow;
  slug: string;
  currency: string;
  onPosted: () => void;
  onError: (msg: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState(false);
  const canDepreciate = row.status === "active" && row.remainingPeriods > 0;

  async function doDepreciate() {
    setBusy(true);
    try {
      await api.depreciateAsset(slug, row.assetId, {
        // Use the LOCAL date — `toISOString()` is UTC and would mis-date a
        // BOOKED afskrivning in Danish evening hours (UTC+1/+2 → off by one).
        transactionDate: todayIso(),
      });
      onPosted();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : "Kunne ikke bogføre afskrivningen.",
      );
      throw err;
    } finally {
      setBusy(false);
    }
  }

  return (
    <tr>
      <td>{row.name}</td>
      <td>{row.category}</td>
      <td className="entry-date">{row.acquisitionDate}</td>
      <td className="num">{formatKroner(row.cost, currency)}</td>
      <td className="num">
        {formatKroner(row.accumulatedDepreciation, currency)}
      </td>
      <td className="num">{formatKroner(row.netBookValue, currency)}</td>
      <td>
        <span
          className={`flag ${row.status === "active" ? "ok" : "neutral"}`}
        >
          {row.status === "active"
            ? `${row.postedPeriods}/${row.usefulLifeMonths} afskrevet`
            : "Fuldt afskrevet"}
        </span>
      </td>
      <td>
        <Button requiredPermission="company.ledger.post" variant="secondary"
          type="button"
          className="btn secondary"
          onClick={() => setPending(true)}
          disabled={!canDepreciate || busy}
          aria-label={`Beregn afskrivning for ${row.name}`}
        >
          {busy ? "Bogfører…" : "Beregn afskrivning"}
        </Button>
        {pending && (
          <ConfirmDialog
            title={`Bogfør afskrivning: ${row.name}`}
            body={
              <p>
                Bogfører næste afskrivningsperiode for anlægget. Posteringen
                kan ikke fortrydes — kun rettes ved at lave en modpostering
                bagefter.
              </p>
            }
            confirmLabel="Bogfør afskrivning"
            confirmKind="danger"
            onConfirm={async () => {
              await doDepreciate();
            }}
            onClose={() => setPending(false)} onRefresh={onPosted}
          />
        )}
      </td>
    </tr>
  );
}

function WriteOffRow({
  row,
  currency,
}: {
  row: AssetWriteOffRow;
  currency: string;
}) {
  return (
    <tr>
      <td>{row.name}</td>
      <td>{row.category}</td>
      <td className="entry-date">{row.acquisitionDate}</td>
      <td className="entry-date">{row.writeOffDate}</td>
      <td className="num">{formatKroner(row.cost, currency)}</td>
      <td className="account-no">{row.expenseAccountNo}</td>
      <td>{row.thresholdRuleSource}</td>
    </tr>
  );
}

function useDocumentPicker(slug: string) {
  return useAsync<CompanyDocuments>((signal) => api.documents(slug, { signal }), [slug]);
}

function RegisterAssetModal({
  slug,
  onClose: onDismiss,
  onRefresh,
  onCreated,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onRefresh: () => void;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const [name, setName] = useState("");
  const [category, setCategory] = useState("hardware");
  // Default to the LOCAL date — `toISOString()` is UTC and is off-by-one in
  // Danish evening hours (UTC+1/+2), pre-filling tomorrow's date.
  const [acquisitionDate, setAcquisitionDate] = useState(todayIso());
  const [cost, setCost] = useState("");
  const [usefulLifeMonths, setUsefulLifeMonths] = useState("36");
  const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(Boolean(name || cost || purchaseDocumentId || note) || category !== "hardware" || acquisitionDate !== todayIso() || usefulLifeMonths !== "36", onDismiss);
  const { onClose } = guard;

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (outcome.isBlocked()) return;
    setBusy(true);
    try {
      const costNumber = parseDanishAmount(cost);
      const months = Number(usefulLifeMonths);
      const docId = Number(purchaseDocumentId);
      if (costNumber === null || costNumber <= 0) {
        throw new Error("Kostprisen skal være et positivt tal.");
      }
      if (!Number.isInteger(months) || months <= 0) {
        throw new Error("Levetiden (måneder) skal være et positivt heltal.");
      }
      if (!Number.isInteger(docId) || docId <= 0) {
        throw new Error("Vælg et bilag som købsbilag.");
      }
      await outcome.run(() => api.registerAsset(slug, {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        cost: costNumber,
        usefulLifeMonths: months,
        purchaseDocumentId: docId,
        ...(note.trim() ? { note: note.trim() } : {}),
      }));
      guard.dismiss();
      onCreated();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : err instanceof Error
            ? err.message
            : "Kunne ikke registrere anlæg.",
      );
    } finally {
      setBusy(false);
    }
  }

  const docRows: DocumentRow[] = docs.data?.documents ?? [];

  return (
    <Dialog title="Registrér nyt anlæg" onClose={onClose} busy={busy}>
    {outcome.feedback}
      {guard.confirmation}


        <form onSubmit={handleSubmit}>
          <label>
            <span>Navn</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label>
            <span>Kategori</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            />
          </label>
          <label>
            <span>Anskaffet (YYYY-MM-DD)</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Kostpris (kr.)</span>
            <MoneyInput disabled={outcome.blocked}
              required
              value={cost}
              onValueChange={(e) => setCost(e)}
            />
          </label>
          <label>
            <span>Levetid (måneder, lineær afskrivning)</span>
            <Input disabled={outcome.blocked}
              type="number"
              step="1"
              min="1"
              required
              value={usefulLifeMonths}
              onChange={(e) => setUsefulLifeMonths(e.target.value)}
            />
          </label>
          <label>
            <span>Bilag (købsdokument)</span>
            <Select disabled={outcome.blocked}
              required
              value={purchaseDocumentId}
              onChange={(e) => setPurchaseDocumentId(e.target.value)}
            >
              <option value="">Vælg bilag…</option>
              {docRows.map((d) => (
                <option key={d.id} value={d.id}>
                  #{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
                  {d.supplierName ?? "ukendt leverandør"}
                </option>
              ))}
            </Select>
          </label>
          <label>
            <span>Note (valgfri)</span>
            <Input disabled={outcome.blocked}
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <div className="row-actions">
            <Button requiredPermission="company.ledger.post" type="submit" className="btn" disabled={outcome.blocked || (busy)}>
              {busy ? "Opretter…" : "Registrér anlæg"}
            </Button>
            <Button variant="secondary"
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </Button>
          </div>
        </form>

    </Dialog>
  );
}

function WriteOffModal({
  slug,
  onClose: onDismiss,
  onRefresh,
  onCreated,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onRefresh: () => void;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const [name, setName] = useState("");
  const [category, setCategory] = useState("smaaanskaffelser");
  // Default to the LOCAL date — `toISOString()` is UTC and is off-by-one in
  // Danish evening hours (UTC+1/+2), pre-filling tomorrow's date.
  const [acquisitionDate, setAcquisitionDate] = useState(todayIso());
  const [transactionDate, setTransactionDate] = useState(todayIso());
  const [cost, setCost] = useState("");
  const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
  const [expenseAccountNo, setExpenseAccountNo] = useState(
    DEFAULT_EXPENSE_ACCOUNT,
  );
  const [thresholdRuleSource, setThresholdRuleSource] = useState(
    DEFAULT_THRESHOLD_RULE,
  );
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(Boolean(name || cost || purchaseDocumentId || note) || category !== "smaaanskaffelser" || acquisitionDate !== todayIso() || transactionDate !== todayIso() || expenseAccountNo !== DEFAULT_EXPENSE_ACCOUNT || thresholdRuleSource !== DEFAULT_THRESHOLD_RULE, onDismiss);
  const { onClose } = guard;

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (outcome.isBlocked()) return;
    setBusy(true);
    try {
      const costNumber = parseDanishAmount(cost);
      const docId = Number(purchaseDocumentId);
      if (costNumber === null || costNumber <= 0) {
        throw new Error("Beløbet skal være positivt.");
      }
      if (!Number.isInteger(docId) || docId <= 0) {
        throw new Error("Vælg et bilag som købsbilag.");
      }
      if (!thresholdRuleSource.trim()) {
        throw new Error(
          "Hjemmelshenvisningen (tærskelregel) er obligatorisk for straksafskrivning.",
        );
      }
      await outcome.run(() => api.writeOffAsset(slug, {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        transactionDate,
        cost: costNumber,
        purchaseDocumentId: docId,
        expenseAccountNo: expenseAccountNo.trim(),
        thresholdRuleSource: thresholdRuleSource.trim(),
        ...(note.trim() ? { note: note.trim() } : {}),
      }));
      guard.dismiss();
      onCreated();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : err instanceof Error
            ? err.message
            : "Kunne ikke straksafskrive.",
      );
    } finally {
      setBusy(false);
    }
  }

  const docRows: DocumentRow[] = docs.data?.documents ?? [];

  return (
    <Dialog title="Straksafskriv småanskaffelse" onClose={onClose} busy={busy}>
    {outcome.feedback}
      {guard.confirmation}


        <p className="muted">
          Straksafskrivning er en skattemæssig vurdering — du bekræfter med en
          eksplicit hjemmelshenvisning, og handlingen bogføres som en udgift
          direkte. Bilag og hjemmel arkiveres på audit-sporet.
        </p>
        <form onSubmit={handleSubmit}>
          <label>
            <span>Navn</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label>
            <span>Kategori</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            />
          </label>
          <label>
            <span>Anskaffet (YYYY-MM-DD)</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Bogføringsdato</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={transactionDate}
              onChange={(e) => setTransactionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Beløb (kr.)</span>
            <MoneyInput disabled={outcome.blocked}
              required
              value={cost}
              onValueChange={(e) => setCost(e)}
            />
          </label>
          <label>
            <span>Bilag (købsdokument)</span>
            <Select disabled={outcome.blocked}
              required
              value={purchaseDocumentId}
              onChange={(e) => setPurchaseDocumentId(e.target.value)}
            >
              <option value="">Vælg bilag…</option>
              {docRows.map((d) => (
                <option key={d.id} value={d.id}>
                  #{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
                  {d.supplierName ?? "ukendt leverandør"}
                </option>
              ))}
            </Select>
          </label>
          <label>
            <span>Udgiftskonto</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={expenseAccountNo}
              onChange={(e) => setExpenseAccountNo(e.target.value)}
            />
          </label>
          <label>
            <span>Hjemmelshenvisning (tærskelregel)</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={thresholdRuleSource}
              onChange={(e) => setThresholdRuleSource(e.target.value)}
            />
          </label>
          <label>
            <span>Note (valgfri)</span>
            <Input disabled={outcome.blocked}
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <div className="row-actions">
            <Button requiredPermission="company.ledger.post" type="submit" className="btn" disabled={outcome.blocked || (busy)}>
              {busy ? "Bogfører…" : "Straksafskriv"}
            </Button>
            <Button variant="secondary"
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </Button>
          </div>
        </form>

    </Dialog>
  );
}

const viewStyles = stylex.create({
site0: { marginTop: "1rem" },
site1: { marginTop: "1.5rem" },
site2: { marginTop: "1.5rem" }
});
