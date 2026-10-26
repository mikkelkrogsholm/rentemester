import { cockpitStyles } from "../design/cockpit.stylex";
import { AssetDisposalDialog } from "../components/AssetDisposalDialog";
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

import { useEffect, useRef, useState } from "react";
import { useOptionalAuth } from "../lib/auth-context";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationBlock } from "../lib/mutation-memory";
import { UnknownMutationNotice } from "../components/UnknownMutationNotice";
import type { AssetWriteOffInput, NativeAssetWriteOffReceipt } from "../lib/types/assets";
import { useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  AssetRow,
  AssetWriteOffRow,
  CompanyAssets,
  AssetNextDepreciation,
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
    <section {...stylex.props(cockpitStyles.statement)}>
      {state.error && <div {...stylex.props(cockpitStyles.banner)} role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Anlæg" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Anlæg
          </p>
        </div>

      </PageHeader>

      <p {...stylex.props(cockpitStyles.statementAsof, cockpitStyles.muted)}>
        Anlægskartoteket — kapitaliserede aktiver, deres afskrivninger og
        straksafskrivninger. Alle beløb i {currency}.
      </p>

      <div {...stylex.props(cockpitStyles.statusGrid, cockpitStyles.invoicesSummary)}>
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>Bogført kostpris</h3>
          <div {...stylex.props(cockpitStyles.statusFigure)}>
            {formatKroner(data.totals.cost, currency)}
          </div>
          <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
            {data.totals.activeCount} aktive ·{" "}
            {data.totals.fullyDepreciatedCount} fuldt afskrevne
          </p>
        </div>
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>Restværdi (netto)</h3>
          <div {...stylex.props(cockpitStyles.statusFigure)}>
            {formatKroner(data.totals.netBookValue, currency)}
          </div>
          <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
            Akkumuleret afskrevet{" "}
            {formatKroner(data.totals.accumulatedDepreciation, currency)}
          </p>
        </div>
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
          <h3 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.statusCardH3,
						)}>Straksafskrivninger</h3>
          <div {...stylex.props(cockpitStyles.statusFigure)}>
            {formatKroner(data.totals.writeOffTotal, currency)}
          </div>
          <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
            {data.totals.writeOffCount}{" "}
            {data.totals.writeOffCount === 1 ? "post" : "poster"}
          </p>
        </div>
      </div>

      <div {...stylex.props(...[cockpitStyles.rowActions, viewStyles.site0])} >
        <Button requiredPermission="company.ledger.post"
          type="button"

          onClick={() => {
            setActionError(null);
            setRegisterOpen(true);
          }} xstyle={[cockpitStyles.statementBtnComposition]}
        >
          Registrér anlæg
        </Button>
        {data.writeOffAvailable !== false && <Button requiredPermission="company.ledger.post" variant="secondary"
          type="button"

          onClick={() => {
            setActionError(null);
            setWriteOffOpen(true);
          }} xstyle={[cockpitStyles.statementBtnComposition]}
        >
          Straksafskriv
        </Button>}
      </div>

      {actionError ? (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)} role="alert">
          <p {...stylex.props(cockpitStyles.muted)}>{actionError}</p>
        </div>
      ) : null}

      <h3 {...stylex.props(viewStyles.site1)}>Kapitaliserede anlæg</h3>
      {data.assets.length === 0 ? (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
          <p {...stylex.props(cockpitStyles.muted)}>
            Der er ingen kapitaliserede anlæg endnu. Brug "Registrér anlæg" til
            at oprette et nyt aktiv ud fra et eksisterende bilag.
          </p>
        </div>
      ) : (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
          <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
            <thead {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableThead,
							)}>
              <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Navn</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Kategori</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Anskaffet</th>
                <th {...stylex.props(cockpitStyles.num)}>Kostpris</th>
                <th {...stylex.props(cockpitStyles.num)}>Akkumuleret afskrivning</th>
                <th {...stylex.props(cockpitStyles.num)}>Restværdi</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Status</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Handling</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTbody,
							)}>
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
              <tr {...stylex.props(...[cockpitStyles.statementResultTd])}>
                <td colSpan={3} {...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition8,
									)}>I alt</td>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(data.totals.cost, currency)}
                </td>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(
                    data.totals.accumulatedDepreciation,
                    currency,
                  )}
                </td>
                <td {...stylex.props(cockpitStyles.num)}>
                  {formatKroner(data.totals.netBookValue, currency)}
                </td>
                <td colSpan={2} {...stylex.props(
										cockpitStyles.statementTableScrollTableTdComposition8,
									)} />
              </tr>
            </tbody>
          </table>
        </div>
      )}

      <h3 {...stylex.props(viewStyles.site2)}>Straksafskrivninger</h3>
      {data.writeOffs.length === 0 ? (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
          <p {...stylex.props(cockpitStyles.muted)}>
            {data.writeOffAvailable === false ? "Straksafskrivning er endnu ikke tilgængelig." : 'Ingen straksafskrivninger endnu. Brug "Straksafskriv" når en småanskaffelse kan bogføres direkte som udgift.'}
          </p>
        </div>
      ) : (
        <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
          <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
            <thead {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableThead,
							)}>
              <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Navn</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Kategori</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Anskaffet</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Bogført</th>
                <th {...stylex.props(cockpitStyles.num)}>Beløb</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Konto</th>
                <th {...stylex.props(
										cockpitStyles.statementTableScrollTableThComposition5,
									)}>Hjemmel</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTbody,
							)}>
              {data.writeOffs.map((row) => (
                <WriteOffRow key={row.id} row={row} currency={currency} />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <p {...stylex.props(cockpitStyles.statementCheck)}>
        Afskrivninger bogføres med reference til købsbilaget og fremgår af regnskabet.
      </p>

      {registerOpen ? (
        <RegisterAssetModal
          nativeAccounts={data.nativeAssetAccounts === true}
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
          key={`${slug}:${data.nativeAssetAccounts === true}`}
          nativeAccounts={data.nativeAssetAccounts === true}
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
  const [preview, setPreview] = useState<AssetNextDepreciation | null>(null);
  const [disposalOpen, setDisposalOpen] = useState(false);
  const canDepreciate = row.status === "active" && row.remainingPeriods > 0;

  async function doDepreciate() {
    setBusy(true);
    try {
      await api.depreciateAsset(slug, row.assetId, {
        // Use the LOCAL date — `toISOString()` is UTC and would mis-date a
        // BOOKED afskrivning in Danish evening hours (UTC+1/+2 → off by one).
        transactionDate: todayIso(),
        ...(preview?.nextPeriodIndex ? { periodIndex: preview.nextPeriodIndex } : {}),
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
    <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.name}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.category}</td>
      <td {...stylex.props(cockpitStyles.entryDate)}>{row.acquisitionDate}</td>
      <td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.cost, currency)}</td>
      <td {...stylex.props(cockpitStyles.num)}>
        {formatKroner(row.accumulatedDepreciation, currency)}
      </td>
      <td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.netBookValue, currency)}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        <span
          {...stylex.props(...[cockpitStyles.flag, ...(row.status === "active" ? [cockpitStyles.flagOk] : [cockpitStyles.flagNeutral])])}
        >
          {row.status === "active"
            ? `${row.postedPeriods}/${row.usefulLifeMonths} afskrevet`
            : row.status === "disposed" ? "Afhændet" : "Fuldt afskrevet"}
        </span>
      </td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        {row.status === "disposed" ? <><p>Afhændet {row.disposalDate} · Postering {row.disposalJournalEntryNo ?? row.disposalJournalEntryId} · Ingen senere afskrivning</p><Button variant="secondary" onClick={() => setDisposalOpen(true)}>Hent afhændelseskvittering</Button></> : <Button requiredPermission="company.ledger.post" variant="secondary" onClick={() => setDisposalOpen(true)}>Afhænd {row.name}</Button>}
        {disposalOpen && <AssetDisposalDialog slug={slug} asset={row} currency={currency} onClose={() => setDisposalOpen(false)} onPosted={onPosted} />}
        <Button requiredPermission="company.ledger.post" variant="secondary"
          type="button"

          onClick={async () => {
            setBusy(true);
            try {
              const next = await api.assetNextDepreciation(slug, row.assetId);
              if (next.nextPeriodIndex === null) { onPosted(); throw new Error("Anlægget er allerede fuldt afskrevet."); }
              setPreview(next);
            }
            catch (err) { onError(err instanceof Error ? err.message : "Kunne ikke beregne afskrivningen."); }
            finally { setBusy(false); }
          }}
          disabled={!canDepreciate || busy}
          aria-label={`Beregn afskrivning for ${row.name}`}
        >
          {busy ? "Arbejder…" : "Beregn afskrivning"}
        </Button>
        {preview && (
          <ConfirmDialog
            title={`Bogfør afskrivning: ${row.name}`}
            body={
              <p {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                Bogfører periode {preview.nextPeriodIndex} af {preview.totalPeriods}: {formatKroner(preview.nextPeriodAmount ?? 0, currency)}. Posteringen
                gemmes med købsbilaget og revisionsspor.
              </p>
            }
            confirmLabel="Bogfør afskrivning"
            confirmKind="danger"
            onConfirm={async () => {
              await doDepreciate();
            }}
            operationKey={`asset-depreciation:${row.assetId}:${preview.nextPeriodIndex}`}
            onClose={() => setPreview(null)} onRefresh={onPosted}
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
    <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.name}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.category}</td>
      <td {...stylex.props(cockpitStyles.entryDate)}>{row.acquisitionDate}</td>
      <td {...stylex.props(cockpitStyles.entryDate)}>{row.writeOffDate}</td>
      <td {...stylex.props(cockpitStyles.num)}>{formatKroner(row.cost, currency)}</td>
      <td {...stylex.props(...[cockpitStyles.num])}>{row.expenseAccountNo}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.thresholdRuleSource}</td>
    </tr>
  );
}

function useDocumentPicker(slug: string) {
  return useAsync<CompanyDocuments>((signal) => api.documents(slug, { signal }), [slug]);
}

function RegisterAssetModal({
  slug,
  nativeAccounts,
  onClose: onDismiss,
  onRefresh,
  onCreated,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onRefresh: () => void;
  nativeAccounts: boolean;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const chart = useAsync((signal) => nativeAccounts ? api.accounts(slug, { signal }) : Promise.resolve(null), [slug, nativeAccounts]);
  const [assetAccount, setAssetAccount] = useState("");
  const [depreciationAccount, setDepreciationAccount] = useState("");
  const [accumulatedAccount, setAccumulatedAccount] = useState("");
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
  const guard = useDiscardGuard(Boolean(name || cost || purchaseDocumentId || note || assetAccount || depreciationAccount || accumulatedAccount) || category !== "hardware" || acquisitionDate !== todayIso() || usefulLifeMonths !== "36", onDismiss);
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
      await api.registerAsset(slug, {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        cost: costNumber,
        usefulLifeMonths: months,
        purchaseDocumentId: docId,
        ...(nativeAccounts ? { assetAccountNo: assetAccount, depreciationExpenseAccountNo: depreciationAccount, accumulatedDepreciationAccountNo: accumulatedAccount } : {}),
        ...(note.trim() ? { note: note.trim() } : {}),
      }).catch(outcome.reject);
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
    <Dialog title="Registrér nyt anlæg" onClose={onClose} busy={busy} xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}>
    {outcome.feedback}
      {guard.confirmation}


        {nativeAccounts && <p>Bogfør først købet på anlægskontoen med købsbilaget via Ny postering. Registreringen her opretter afskrivningsplanen uden at bogføre købet igen.</p>}
        <form onSubmit={handleSubmit} {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          {nativeAccounts && <>
            {chart.error && <p role="alert">{chart.error}</p>}
            {([
              ["Anlægskonto", assetAccount, setAssetAccount, "asset", "debit"],
              ["Afskrivningskonto", depreciationAccount, setDepreciationAccount, "expense", "debit"],
              ["Akkumulerede afskrivninger", accumulatedAccount, setAccumulatedAccount, "asset", "credit"],
            ] as const).map(([label,value,setValue,type,balance]) => <label key={label}>{label}<Select required disabled={outcome.blocked} value={value} onChange={(e) => setValue(e.target.value)}><option value="">Vælg konto</option>{(chart.data?.accounts ?? []).filter((a) => a.active !== false && a.type === type && a.normalBalance === balance).map((a) => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</Select></label>)}
          </>}

          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Navn</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kategori</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Anskaffet (YYYY-MM-DD)</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kostpris (kr.)</span>
            <MoneyInput disabled={outcome.blocked}
              required
              value={cost}
              onValueChange={(e) => setCost(e)}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Levetid (måneder, lineær afskrivning)</span>
            <Input disabled={outcome.blocked}
              type="number"
              step="1"
              min="1"
              required
              value={usefulLifeMonths}
              onChange={(e) => setUsefulLifeMonths(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bilag (købsdokument)</span>
            <Select disabled={outcome.blocked}
              required
              value={purchaseDocumentId}
              onChange={(e) => setPurchaseDocumentId(e.target.value)} xstyle={[cockpitStyles.selectComposition]}
            >
              <option value="" {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Vælg bilag…</option>
              {docRows.map((d) => (
                <option key={d.id} value={d.id} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                  #{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
                  {d.supplierName ?? "ukendt leverandør"}
                </option>
              ))}
            </Select>
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Note (valgfri)</span>
            <Input disabled={outcome.blocked}
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <div {...stylex.props(cockpitStyles.rowActions)}>
            <Button requiredPermission="company.ledger.post" type="submit"  disabled={outcome.blocked || (busy)}>
              {busy ? "Opretter…" : "Registrér anlæg"}
            </Button>
            <Button variant="secondary"
              type="button"

              onClick={onClose}
              disabled={busy} xstyle={[cockpitStyles.buttonComposition]}
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
  nativeAccounts,
  onClose: onDismiss,
  onRefresh,
  onCreated,
  onError,
}: {
  slug: string;
  nativeAccounts: boolean;
  onClose: () => void;
  onRefresh: () => void;
  onCreated: () => void;
  onError: (msg: string) => void;
}) {
  const docs = useDocumentPicker(slug);
  const chart = useAsync(signal => nativeAccounts ? api.accounts(slug, { signal }) : Promise.resolve(null), [slug, nativeAccounts]);
  const auth = useOptionalAuth();
  const { can } = useCapabilities(slug);
  const allowed = can("company.ledger.post");
  const currentAllowed = useRef(allowed);
  currentAllowed.current = allowed;
  const scope = `/companies/${slug}/assets`;
  // Observe the same persisted barrier owned by ConfirmDialog, including after remount.
  const sendLock = useMutationBlock(`${scope}:Bekræft straksafskrivning:native-writeoff-review`);
  const reviewedContext = useRef(auth?.context);
  const [review, setReview] = useState<AssetWriteOffInput | null>(null);
  const [receipt, setReceipt] = useState<NativeAssetWriteOffReceipt | null>(null);
  const active = useRef(true);
  const context = useRef(auth?.context);
  context.current = auth?.context;
  useEffect(() => { active.current = true; return () => { active.current = false; }; }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: permission or source snapshot changes invalidate the pending review.
  useEffect(() => { setReview(null); }, [auth?.context, allowed, docs.data, chart.data]);
  const nativeBlocked = nativeAccounts && (sendLock.blocked || !allowed || !!receipt);
  const refreshNative = async () => { await Promise.all([api.assets(slug), api.journal(slug)]); if (active.current) onRefresh(); };

  const [name, setName] = useState("");
  const [category, setCategory] = useState("smaaanskaffelser");
  // Default to the LOCAL date — `toISOString()` is UTC and is off-by-one in
  // Danish evening hours (UTC+1/+2), pre-filling tomorrow's date.
  const [acquisitionDate, setAcquisitionDate] = useState(todayIso());
  const [transactionDate, setTransactionDate] = useState(todayIso());
  const [cost, setCost] = useState("");
  const [purchaseDocumentId, setPurchaseDocumentId] = useState<string>("");
  const [expenseAccountNo, setExpenseAccountNo] = useState(
    nativeAccounts ? "" : DEFAULT_EXPENSE_ACCOUNT,
  );
  const [thresholdRuleSource, setThresholdRuleSource] = useState(
    nativeAccounts ? "" : DEFAULT_THRESHOLD_RULE,
  );
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(!receipt && (Boolean(name || cost || purchaseDocumentId || note) || category !== "smaaanskaffelser" || acquisitionDate !== todayIso() || transactionDate !== todayIso() || expenseAccountNo !== (nativeAccounts ? "" : DEFAULT_EXPENSE_ACCOUNT) || thresholdRuleSource !== (nativeAccounts ? "" : DEFAULT_THRESHOLD_RULE)), onDismiss);
  const { onClose } = guard;

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (outcome.isBlocked() || nativeBlocked) return;
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
      const input: AssetWriteOffInput = {
        name: name.trim(),
        category: category.trim(),
        acquisitionDate,
        transactionDate,
        cost: costNumber,
        purchaseDocumentId: docId,
        expenseAccountNo: expenseAccountNo.trim(),
        thresholdRuleSource: thresholdRuleSource.trim(),
        ...(note.trim() ? { note: note.trim() } : {}),
      };
      if (nativeAccounts) {
        if (!chart.data?.accounts.some(a => a.accountNo === expenseAccountNo && a.active === true && a.type === "expense" && a.normalBalance === "debit")) throw new Error("Vælg en aktiv udgiftskonto fra kontoplanen.");
        if (docs.error || chart.error || docs.loading || chart.loading) throw new Error("Opdatér bilag og kontoplan før gennemgang.");
        reviewedContext.current = context.current;
        setReview(input);
        return;
      }
      await api.writeOffAsset(slug, input).catch(outcome.reject);
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
    <Dialog title="Straksafskriv småanskaffelse" onClose={onClose} busy={busy} xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}>
    {outcome.feedback}
    {nativeAccounts && <>
      <p {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}>Betaling: virksomhedens aktuelt konfigurerede bankrolle. Mangler mappingen, afviser serveren: konfigurér bankrollen under Administrér før ny handling. Der vælges ingen standardkonto her.</p>
      {(docs.error || chart.error) && <p role="alert">{docs.error || chart.error}</p>}
      {sendLock.blocked && !review && <UnknownMutationNotice onRefresh={refreshNative} onRelease={sendLock.release} persistent={sendLock.persistent} verifyPersistence={sendLock.verifyPersistence} />}
      {receipt && <div role="status">Bogført: {receipt.entryNo} (#{receipt.entryId}) · Straksafskrivning #{receipt.writeOffId}<p>Journalhash: <code>{receipt.entryHash}</code></p></div>}
    </>}
      {guard.confirmation}


        <p {...stylex.props(cockpitStyles.muted)}>
          Straksafskrivning er en skattemæssig vurdering — du bekræfter med en
          eksplicit hjemmelshenvisning, og handlingen bogføres som en udgift
          direkte. Bilag og hjemmel arkiveres på audit-sporet.
        </p>
        <form onSubmit={handleSubmit} onChange={() => setReview(null)}>
          <fieldset disabled={nativeBlocked || !!review}>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Navn</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Kategori</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={category}
              onChange={(e) => setCategory(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Anskaffet (YYYY-MM-DD)</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={acquisitionDate}
              onChange={(e) => setAcquisitionDate(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bogføringsdato</span>
            <Input disabled={outcome.blocked}
              type="date"
              required
              value={transactionDate}
              onChange={(e) => setTransactionDate(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Beløb (kr.)</span>
            <MoneyInput disabled={outcome.blocked}
              required
              value={cost}
              onValueChange={(e) => setCost(e)}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Bilag (købsdokument)</span>
            <Select disabled={outcome.blocked}
              required
              value={purchaseDocumentId}
              onChange={(e) => setPurchaseDocumentId(e.target.value)} xstyle={[cockpitStyles.selectComposition]}
            >
              <option value="" {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>Vælg bilag…</option>
              {docRows.map((d) => (
                <option key={d.id} value={d.id} {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
                  #{d.id} · {d.documentNo ?? "uden nr."} ·{" "}
                  {d.supplierName ?? "ukendt leverandør"}
                </option>
              ))}
            </Select>
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Udgiftskonto</span>
            {nativeAccounts ? <Select required value={expenseAccountNo} onChange={e => setExpenseAccountNo(e.target.value)}><option value="">Vælg aktiv udgiftskonto</option>{(chart.data?.accounts ?? []).filter(a => a.active === true && a.type === "expense" && a.normalBalance === "debit").map(a => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} · {a.name}</option>)}</Select> : <Input disabled={outcome.blocked} type="text" required value={expenseAccountNo} onChange={e => setExpenseAccountNo(e.target.value)} />}
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Hjemmelshenvisning (tærskelregel)</span>
            <Input disabled={outcome.blocked}
              type="text"
              required
              value={thresholdRuleSource}
              onChange={(e) => setThresholdRuleSource(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          <label {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Note (valgfri)</span>
            <Input disabled={outcome.blocked}
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)} xstyle={[cockpitStyles.inputComposition]}
            />
          </label>
          </fieldset>
          <div {...stylex.props(cockpitStyles.rowActions)}>
            <Button requiredPermission="company.ledger.post" type="submit"  disabled={outcome.blocked || busy || nativeBlocked || !!review}>
              {busy ? "Bogfører…" : nativeAccounts ? "Gennemgå straksafskrivning" : "Straksafskriv"}
            </Button>
            <Button variant="secondary"
              type="button"

              onClick={onClose}
              disabled={busy} xstyle={[cockpitStyles.buttonComposition]}
            >
              {receipt ? "Færdig" : "Annullér"}
            </Button>
          </div>
        </form>
        {nativeAccounts && review && <ConfirmDialog title="Bekræft straksafskrivning" confirmLabel="Bekræft og bogfør straksafskrivning" confirmKind="danger" operationScope={scope} operationKey="native-writeoff-review" confirmDisabled={nativeBlocked} onRefresh={refreshNative} onClose={() => setReview(null)} body={<>
          <p>Virksomhed: {slug} · Bilag #{review.purchaseDocumentId} · {review.name} · {review.category}</p>
          <p>{formatKroner(review.cost)} · Anskaffet {review.acquisitionDate} · Bogføres {review.transactionDate}</p>
          <p>Udgiftskonto {review.expenseAccountNo} · Betaling: aktuel konfigureret bankrolle</p>
          <p>Hjemmel: {review.thresholdRuleSource} · Note: {review.note || "Ingen"}</p>
          <p>Jeg bekræfter straksafskrivning og bogføring. Serveren kontrollerer tærskel, moms og journal; dette er ikke en skatteretlig godkendelse.</p>
        </>} onConfirm={async () => {
          if (!active.current || !currentAllowed.current || context.current !== reviewedContext.current) throw new ApiError("access_denied", "Gennemgangen er ikke længere gyldig.", 403);
          const originalContext = reviewedContext.current;
          try {
            const result = await api.writeOffNativeAsset(slug, review, { confirm: true, confirmImmediateWriteOff: true });
            if (active.current && currentAllowed.current && context.current === originalContext) { setReceipt(result); onRefresh(); }
          } catch (err) {
            const failure = err as { status?: number; code?: string };
            if (active.current && currentAllowed.current && context.current === originalContext && !(failure.status && failure.status >= 400 && failure.status < 500 && failure.status !== 408 && failure.code !== "internal")) {
              // Read-only reconciliation never releases the pre-send lock or resends.
              void refreshNative().catch(() => {});
            }
            throw err;
          }
        }} />}

    </Dialog>
  );
}

const viewStyles = stylex.create({
site0: { marginTop: "1rem" },
site1: { marginTop: "1.5rem" },
site2: { marginTop: "1.5rem" }
});
