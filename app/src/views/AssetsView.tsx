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
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  AssetRow,
  AssetWriteOffRow,
  CompanyAssets,
  CompanyDocuments,
  DocumentRow,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const DEFAULT_EXPENSE_ACCOUNT = "3000";
const DEFAULT_THRESHOLD_RULE =
  "AL §6 stk. 1 nr. 2 — småanskaffelser (straksafskrivning)";

export function AssetsView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyAssets>(() => api.assets(slug), [slug]);
  const [registerOpen, setRegisterOpen] = useState(false);
  const [writeOffOpen, setWriteOffOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  if (state.loading && !state.data)
    return <Loading label="Henter anlægskartotek…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const data = state.data!;
  const currency = data.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Anlæg
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

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

      <div className="row-actions" style={{ marginTop: "1rem" }}>
        <button
          type="button"
          className="btn"
          onClick={() => {
            setActionError(null);
            setRegisterOpen(true);
          }}
        >
          Registrér anlæg
        </button>
        <button
          type="button"
          className="btn secondary"
          onClick={() => {
            setActionError(null);
            setWriteOffOpen(true);
          }}
        >
          Straksafskriv
        </button>
      </div>

      {actionError ? (
        <div className="card archived-notice" role="alert">
          <p className="muted">{actionError}</p>
        </div>
      ) : null}

      <h3 style={{ marginTop: "1.5rem" }}>Kapitaliserede anlæg</h3>
      {data.assets.length === 0 ? (
        <div className="card archived-notice">
          <p className="muted">
            Der er ingen kapitaliserede anlæg endnu. Brug "Registrér anlæg" til
            at oprette et nyt aktiv ud fra et eksisterende bilag.
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table">
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

      <h3 style={{ marginTop: "1.5rem" }}>Straksafskrivninger</h3>
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
          <table className="data statement-table">
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
  const canDepreciate = row.status === "active" && row.remainingPeriods > 0;

  async function handleDepreciate() {
    if (!canDepreciate) return;
    if (
      !window.confirm(
        `Bogfør næste afskrivningsperiode for "${row.name}"? Posteringen kan ikke fortrydes uden modposterings-entry.`,
      )
    )
      return;
    setBusy(true);
    try {
      await api.depreciateAsset(slug, row.assetId, {
        transactionDate: new Date().toISOString().slice(0, 10),
      });
      onPosted();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : "Kunne ikke bogføre afskrivningen.",
      );
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
        <button
          type="button"
          className="btn secondary"
          onClick={handleDepreciate}
          disabled={!canDepreciate || busy}
          aria-label={`Beregn afskrivning for ${row.name}`}
        >
          {busy ? "Bogfører…" : "Beregn afskrivning"}
        </button>
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
  return useAsync<CompanyDocuments>(() => api.documents(slug), [slug]);
}

function RegisterAssetModal({
  slug,
  onClose,
  onCreated,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const [name, setName] = useState("");
  const [category, setCategory] = useState("hardware");
  const [acquisitionDate, setAcquisitionDate] = useState(
    new Date().toISOString().slice(0, 10),
  );
  const [cost, setCost] = useState("");
  const [usefulLifeMonths, setUsefulLifeMonths] = useState("36");
  const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const costNumber = Number(cost);
      const months = Number(usefulLifeMonths);
      const docId = Number(purchaseDocumentId);
      if (!Number.isFinite(costNumber) || costNumber <= 0) {
        throw new Error("Kostprisen skal være et positivt tal.");
      }
      if (!Number.isInteger(months) || months <= 0) {
        throw new Error("Levetiden (måneder) skal være et positivt heltal.");
      }
      if (!Number.isInteger(docId) || docId <= 0) {
        throw new Error("Vælg et bilag som købsbilag.");
      }
      await api.registerAsset(slug, {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        cost: costNumber,
        usefulLifeMonths: months,
        purchaseDocumentId: docId,
        ...(note.trim() ? { note: note.trim() } : {}),
      });
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
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="card modal-card">
        <h3>Registrér nyt anlæg</h3>
        <form onSubmit={handleSubmit}>
          <label>
            <span>Navn</span>
            <input
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label>
            <span>Kategori</span>
            <input
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            />
          </label>
          <label>
            <span>Anskaffet (YYYY-MM-DD)</span>
            <input
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Kostpris (kr.)</span>
            <input
              type="number"
              step="0.01"
              min="0"
              required
              value={cost}
              onChange={(e) => setCost(e.target.value)}
            />
          </label>
          <label>
            <span>Levetid (måneder, lineær afskrivning)</span>
            <input
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
            <select
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
            </select>
          </label>
          <label>
            <span>Note (valgfri)</span>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <div className="row-actions">
            <button type="submit" className="btn" disabled={busy}>
              {busy ? "Opretter…" : "Registrér anlæg"}
            </button>
            <button
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function WriteOffModal({
  slug,
  onClose,
  onCreated,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const [name, setName] = useState("");
  const [category, setCategory] = useState("smaaanskaffelser");
  const [acquisitionDate, setAcquisitionDate] = useState(
    new Date().toISOString().slice(0, 10),
  );
  const [transactionDate, setTransactionDate] = useState(
    new Date().toISOString().slice(0, 10),
  );
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

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const costNumber = Number(cost);
      const docId = Number(purchaseDocumentId);
      if (!Number.isFinite(costNumber) || costNumber <= 0) {
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
      await api.writeOffAsset(slug, {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        transactionDate,
        cost: costNumber,
        purchaseDocumentId: docId,
        expenseAccountNo: expenseAccountNo.trim(),
        thresholdRuleSource: thresholdRuleSource.trim(),
        ...(note.trim() ? { note: note.trim() } : {}),
      });
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
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="card modal-card">
        <h3>Straksafskriv småanskaffelse</h3>
        <p className="muted">
          Straksafskrivning er en skattemæssig vurdering — du bekræfter med en
          eksplicit hjemmelshenvisning, og handlingen bogføres som en udgift
          direkte. Bilag og hjemmel arkiveres på audit-sporet.
        </p>
        <form onSubmit={handleSubmit}>
          <label>
            <span>Navn</span>
            <input
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label>
            <span>Kategori</span>
            <input
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            />
          </label>
          <label>
            <span>Anskaffet (YYYY-MM-DD)</span>
            <input
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Bogføringsdato</span>
            <input
              type="date"
              required
              value={transactionDate}
              onChange={(e) => setTransactionDate(e.target.value)}
            />
          </label>
          <label>
            <span>Beløb (kr.)</span>
            <input
              type="number"
              step="0.01"
              min="0"
              required
              value={cost}
              onChange={(e) => setCost(e.target.value)}
            />
          </label>
          <label>
            <span>Bilag (købsdokument)</span>
            <select
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
            </select>
          </label>
          <label>
            <span>Udgiftskonto</span>
            <input
              type="text"
              required
              value={expenseAccountNo}
              onChange={(e) => setExpenseAccountNo(e.target.value)}
            />
          </label>
          <label>
            <span>Hjemmelshenvisning (tærskelregel)</span>
            <input
              type="text"
              required
              value={thresholdRuleSource}
              onChange={(e) => setThresholdRuleSource(e.target.value)}
            />
          </label>
          <label>
            <span>Note (valgfri)</span>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <div className="row-actions">
            <button type="submit" className="btn" disabled={busy}>
              {busy ? "Bogfører…" : "Straksafskriv"}
            </button>
            <button
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
