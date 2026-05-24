// Moms — the per-company VAT return (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/vat?year=`: the VAT return for the quarter —
// output VAT (salgsmoms), input VAT (købsmoms), the resulting payable amount,
// AND the full SKAT TastSelv momsangivelse rubrics (rubrik A/B/C, foreign
// goods/services VAT) so the owner can file straight from the cockpit instead
// of dropping to the terminal (#257). All money fields are kroner —
// `formatKroner` is used throughout.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyVat, VatRubrikker } from "../lib/types";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";

export function VatView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyVat>(() => api.vat(slug, year), [slug, year]);
  // True while the close-period ConfirmDialog is open (#287).
  const [closing, setClosing] = useState(false);
  // True while the reopen-period ConfirmDialog is open (#301).
  const [reopening, setReopening] = useState(false);
  // #301: the second, explicit confirmation a future-end period close needs.
  const [futureEndAcknowledged, setFutureEndAcknowledged] = useState(false);
  // Set after a successful period close / reopen — surfaced as a success banner.
  const [closedNotice, setClosedNotice] = useState<string | null>(null);

  if (state.loading && !state.data) return <Loading label="Henter moms…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const v = state.data!;
  const currency = v.company.currency || "DKK";
  const payablePositive = v.payable >= 0;
  // #301: a period whose end date is still in the future has not ended yet —
  // closing it now is almost always a mistake, so the confirm dialog warns.
  const periodEndsInFuture = v.periodEnd > todayIso();
  // #301: a closed (not reported) period can be reopened from the cockpit.
  const canReopen = !v.archived && v.periodStatus === "closed";
  // #303: a momsangivelse is only filing-ready for a closed/reported period.
  const provisional = !v.archived && !v.momsangivelseReady;

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{v.company.name}</h2>
          <p className="muted">
            {v.company.cvr ? `CVR ${v.company.cvr} · ` : ""}
            {v.company.country} · {currency} · Moms
          </p>
        </div>
        <div className="row-actions">
          {/* #287: closing the VAT period is the prerequisite for a
              momsangivelse — hidden for an archived (read-only) year. Once the
              period is closed the action becomes a reopen instead (#301). */}
          {!v.archived && v.periodStatus === "open" && (
            <button
              type="button"
              className="btn"
              onClick={() => {
                setFutureEndAcknowledged(false);
                setClosing(true);
              }}
            >
              Luk momsperiode
            </button>
          )}
          {canReopen && (
            <button
              type="button"
              className="btn secondary"
              onClick={() => setReopening(true)}
            >
              Genåbn momsperiode
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={v.fiscalYears}
        selectedYear={v.selectedYear}
        onYearChange={setYear}
      />

      {closedNotice && <Banner kind="success">{closedNotice}</Banner>}

      {closing && (
        <ConfirmDialog
          title="Luk momsperiode"
          body={
            <>
              <p>
                Luk momsperioden <strong>{v.periodLabel}</strong> (
                {v.periodStart} – {v.periodEnd}). En lukket periode er en
                forudsætning for at indberette momsangivelsen, og bogføring i
                perioden låses bagefter.
              </p>
              {/* #301: a future period-end means the period is not over yet.
                  Warn clearly and require a second, explicit acknowledgement
                  before the close can go through. */}
              {periodEndsInFuture && (
                <>
                  <Banner kind="warning">
                    Denne momsperiode er <strong>ikke afsluttet endnu</strong> —
                    slutdatoen {v.periodEnd} ligger i fremtiden. Lukker du nu,
                    blokeres bogføring med dato i perioden, og tallene i
                    momsangivelsen kan stadig nå at ændre sig. Luk normalt først
                    perioden, når den er forbi.
                  </Banner>
                  <label className="confirm-ack">
                    <input
                      type="checkbox"
                      checked={futureEndAcknowledged}
                      onChange={(e) =>
                        setFutureEndAcknowledged(e.target.checked)
                      }
                    />
                    Jeg forstår at perioden ikke er afsluttet, og vil lukke den
                    alligevel.
                  </label>
                  <p className="muted">
                    Lukker du ved en fejl, kan perioden genåbnes igen herfra
                    (Genåbn momsperiode).
                  </p>
                </>
              )}
            </>
          }
          confirmLabel="Luk perioden"
          confirmKind="danger"
          onConfirm={async () => {
            // #301: a future-end period close requires the explicit extra
            // acknowledgement. Without it the close is blocked with a clear
            // message rather than going through silently.
            if (periodEndsInFuture && !futureEndAcknowledged) {
              throw {
                code: "bad_request",
                message:
                  "Bekræft først at du vil lukke en periode der ikke er afsluttet endnu — sæt flueben i feltet ovenfor.",
              };
            }
            await api.closePeriod(slug, {
              periodStart: v.periodStart,
              periodEnd: v.periodEnd,
              kind: "vat_quarter",
            });
            setClosedNotice(
              `Momsperioden er lukket — ${v.periodLabel} kan nu indberettes.`,
            );
            state.reload();
          }}
          onClose={() => setClosing(false)}
        />
      )}

      {reopening && (
        <ReopenDialog
          vat={v}
          onReopened={(label) => {
            setClosedNotice(
              `Momsperioden ${label} er genåbnet — bogføring i perioden er tilladt igen.`,
            );
            state.reload();
          }}
          onClose={() => setReopening(false)}
        />
      )}

      {v.archived ? (
        <ArchivedNotice year={v.selectedYear} />
      ) : (
        <>
          <p className="statement-asof muted">
            {v.periodLabel} · {v.periodStart} – {v.periodEnd}
          </p>

          {/* #303: for an OPEN period the figures are not final — say so,
              honestly, instead of presenting a ready-to-file momsangivelse. */}
          {provisional && (
            <Banner kind="warning">
              Åben periode — foreløbige tal. Momsperioden {v.periodLabel} er
              ikke lukket endnu, så tallene kan stadig ændre sig og udgør ikke
              en indberetningsklar momsangivelse. Luk perioden, når den er
              forbi, for at få de endelige tal.
            </Banner>
          )}
          <div className="card statement-card">
            <table className="data statement-table">
              <tbody>
                <tr>
                  <td>Salgsmoms (udgående moms)</td>
                  <td className="num">
                    {formatKroner(v.outputVat, currency)}
                  </td>
                </tr>
                {/* A bad-debt write-off (debitortab) claims back the output
                    VAT on a receivable that will never be paid. It is shown
                    on its own line so it never silently turns the salgsmoms
                    headline above negative (#271). */}
                {v.outputVatAdjustment !== 0 && (
                  <tr>
                    <td>Regulering for tab på debitorer (debitortab)</td>
                    <td className="num">
                      {formatKroner(v.outputVatAdjustment, currency)}
                    </td>
                  </tr>
                )}
                <tr>
                  <td>Købsmoms (indgående moms)</td>
                  <td className="num">
                    {formatKroner(v.inputVat, currency)}
                  </td>
                </tr>
                <tr
                  className={`statement-result ${
                    payablePositive ? "positive" : "negative"
                  }`}
                >
                  <td>{payablePositive ? "Moms at betale" : "Moms tilgode"}</td>
                  <td className="num">{formatKroner(v.payable, currency)}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div className="card statement-card vat-deadline">
            <div>
              <span className="vat-deadline-label">Angives og betales senest</span>
              <span className="vat-deadline-date">{v.deadline}</span>
            </div>
            <DeadlineCountdown days={v.daysRemaining} />
          </div>

          <RubrikkerCard
            rubrikker={v.rubrikker}
            currency={currency}
            provisional={provisional}
          />

          <p className="statement-check ok">
            {payablePositive
              ? "Salgsmoms minus købsmoms — beløbet skal afregnes til SKAT."
              : "Købsmoms overstiger salgsmoms — beløbet udbetales fra SKAT."}
          </p>
        </>
      )}
    </section>
  );
}

/**
 * The full SKAT TastSelv momsangivelse rubrics — the exact numbers an owner
 * types into the momsangivelse on skat.dk. Surfacing these (#257) means the
 * cockpit's VAT view is filing-complete: rubrik A/B/C and the foreign
 * goods/services VAT no longer force a trip to the terminal's
 * `vat momsangivelse`.
 *
 * #303: for an OPEN period the terminal `vat momsangivelse` refuses to produce
 * a momsangivelse at all (it requires a closed/reported vat_quarter period).
 * The card must therefore NOT claim its figures match that command — instead
 * it marks them provisional and says a closed period is the prerequisite.
 */
/**
 * Format a kroner amount as a raw TastSelv-compatible number string: NO
 * thousand separator, NO currency suffix. Decimal øre — if any — are emitted
 * with a comma (the Danish convention TastSelv accepts). Whole-kroner amounts
 * are emitted as plain integers ("4457", not "4457,00") — that's exactly what
 * the owner needs to type into TastSelv Erhverv.
 *
 * #401: this is the load-bearing function for the rubrikker Kopier-buttons.
 * The formatted, display-only `formatKroner` output (`52.317,00 kr.`) must
 * NEVER end up on the clipboard — the `.` thousand separator would corrupt
 * the TastSelv field.
 */
function tastSelvNumber(kroner: number): string {
  // Round to 2 decimals to avoid floating-point noise like 4456.9999999.
  const rounded = Math.round(kroner * 100) / 100;
  if (Number.isInteger(rounded)) return String(rounded);
  // Two-decimal form with a comma separator (no thousand separator).
  const [intPart, fracPart = ""] = rounded.toFixed(2).split(".");
  return `${intPart},${fracPart}`;
}

/** All rubrikker as label/raw-number pairs, in the order shown in the UI. */
function rubrikkerCsvRows(
  rubrikker: VatRubrikker,
): Array<[string, string]> {
  const owedPositive = rubrikker.momstilsvar >= 0;
  return [
    ["Salgsmoms", tastSelvNumber(rubrikker.salgsmoms)],
    [
      "Moms af varekøb i udlandet",
      tastSelvNumber(rubrikker.momsAfVarekobUdland),
    ],
    [
      "Moms af ydelseskøb i udlandet",
      tastSelvNumber(rubrikker.momsAfYdelseskobUdland),
    ],
    ["Købsmoms", tastSelvNumber(rubrikker.kobsmoms)],
    [
      owedPositive ? "Momstilsvar" : "Negativt momstilsvar",
      tastSelvNumber(rubrikker.momstilsvar),
    ],
    [
      "Rubrik A - varer og ydelser købt i udlandet",
      tastSelvNumber(rubrikker.rubrikA),
    ],
    [
      "Rubrik B - varer og ydelser solgt til udlandet",
      tastSelvNumber(rubrikker.rubrikB),
    ],
    ["Rubrik C - øvrige momsfrie salg", tastSelvNumber(rubrikker.rubrikC)],
  ];
}

/**
 * One row's Kopier-button. Copies ONLY the raw TastSelv-format number (no
 * thousand separator, no "kr.") to the clipboard, then briefly shows
 * "Kopieret" without stealing focus. For a provisional (open-period) row the
 * button is disabled with a title explaining why — the owner must not paste
 * foreløbige tal into a real momsangivelse.
 */
function CopyRubrikButton({
  rawValue,
  label,
  provisional,
  onCopied,
  copied,
}: {
  rawValue: string;
  label: string;
  provisional: boolean;
  onCopied: () => void;
  copied: boolean;
}) {
  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(rawValue);
      onCopied();
    } catch {
      // Clipboard write can fail (insecure context, permission denied) —
      // silently ignore: the owner can still read the figure on screen.
    }
  }
  return (
    <span className="rubrik-copy">
      {copied && (
        <span className="rubrik-copy-feedback" aria-live="polite">
          Kopieret
        </span>
      )}
      <button
        type="button"
        className="rubrik-copy-btn"
        onClick={handleCopy}
        disabled={provisional}
        aria-label={`Kopier ${label}`}
        title={
          provisional
            ? "Periode ikke lukket — luk først for at få endelige tal"
            : `Kopier ${label} til udklipsholderen`
        }
      >
        Kopier
      </button>
    </span>
  );
}

function RubrikkerCard({
  rubrikker,
  currency,
  provisional,
}: {
  rubrikker: VatRubrikker;
  currency: string;
  provisional: boolean;
}) {
  const owedPositive = rubrikker.momstilsvar >= 0;
  // The label of the row most recently copied — drives the "Kopieret"
  // confirmation, scoped per row so two adjacent buttons don't share state.
  const [copiedLabel, setCopiedLabel] = useState<string | null>(null);
  // Cleared after a short interval so the confirmation does not linger.
  function markCopied(label: string) {
    setCopiedLabel(label);
    setTimeout(() => {
      setCopiedLabel((current) => (current === label ? null : current));
    }, 1500);
  }
  async function copyAllAsCsv() {
    const rows = rubrikkerCsvRows(rubrikker);
    const csv = rows.map(([label, value]) => `${label};${value}`).join("\n");
    try {
      await navigator.clipboard.writeText(csv);
      markCopied("__csv__");
    } catch {
      // ignore — see CopyRubrikButton.
    }
  }
  function rubrikRow(label: string, value: number, extraClass?: string) {
    return (
      <tr className={extraClass}>
        <td>{label}</td>
        <td className="num">
          <span className="rubrik-num">{formatKroner(value, currency)}</span>
          <CopyRubrikButton
            rawValue={tastSelvNumber(value)}
            label={label}
            provisional={provisional}
            onCopied={() => markCopied(label)}
            copied={copiedLabel === label}
          />
        </td>
      </tr>
    );
  }
  return (
    <div className="card statement-card">
      <div className="statement-card-head">
        <h3 className="statement-subhead">
          {provisional
            ? "SKAT-rubrikker (foreløbige — åben periode)"
            : "SKAT-rubrikker (momsangivelse)"}
        </h3>
        <button
          type="button"
          className="rubrik-copy-csv"
          onClick={copyAllAsCsv}
          disabled={provisional}
          title={
            provisional
              ? "Periode ikke lukket — luk først for at få endelige tal"
              : "Kopier alle rubrikker som CSV (label;beløb) til regneark"
          }
        >
          {copiedLabel === "__csv__" ? "Kopieret" : "Kopier alle som CSV"}
        </button>
      </div>
      <p className="muted statement-note">
        {provisional ? (
          <>
            De felter der hører til momsangivelsen på skat.dk (TastSelv
            Erhverv). Momsperioden er <strong>ikke lukket endnu</strong>, så
            tallene er foreløbige og kan stadig ændre sig. Når perioden er
            forbi og lukket, bliver tallene endelige og kan indberettes.
          </>
        ) : (
          <>
            Disse felter svarer 1:1 til momsangivelsen på skat.dk (TastSelv
            Erhverv) — udfyld dem som vist. Perioden er lukket, så tallene er
            endelige.
          </>
        )}
      </p>
      <table className="data statement-table">
        <tbody>
          {rubrikRow("Salgsmoms", rubrikker.salgsmoms)}
          {rubrikRow(
            "Moms af varekøb i udlandet (både EU og lande uden for EU)",
            rubrikker.momsAfVarekobUdland,
          )}
          {rubrikRow(
            "Moms af ydelseskøb i udlandet med omvendt betalingspligt",
            rubrikker.momsAfYdelseskobUdland,
          )}
          {rubrikRow("Købsmoms", rubrikker.kobsmoms)}
          {rubrikRow(
            owedPositive ? "Momstilsvar" : "Negativt momstilsvar",
            rubrikker.momstilsvar,
            `statement-result ${owedPositive ? "positive" : "negative"}`,
          )}
        </tbody>
      </table>
      <table className="data statement-table">
        <tbody>
          {rubrikRow(
            "Rubrik A — varer og ydelser købt i udlandet",
            rubrikker.rubrikA,
          )}
          {rubrikRow(
            "Rubrik B — varer og ydelser solgt til udlandet",
            rubrikker.rubrikB,
          )}
          {rubrikRow("Rubrik C — øvrige momsfrie salg", rubrikker.rubrikC)}
        </tbody>
      </table>
    </div>
  );
}

/**
 * The reopen-period confirm dialog (#301). Reopening is a controlled,
 * audit-logged action — it requires a free-text reason, recorded verbatim in
 * the audit log. The dialog reuses `ConfirmDialog`'s note field for that
 * reason and blocks the action until a reason is given.
 */
function ReopenDialog({
  vat,
  onReopened,
  onClose,
}: {
  vat: CompanyVat;
  onReopened: (label: string) => void;
  onClose: () => void;
}) {
  return (
    <ConfirmDialog
      title="Genåbn momsperiode"
      body={
        <>
          <p>
            Genåbn momsperioden <strong>{vat.periodLabel}</strong> (
            {vat.periodStart} – {vat.periodEnd}). Bogføring med dato i perioden
            bliver tilladt igen.
          </p>
          <p className="muted">
            Genåbningen er en kontrolleret, fuldt revisionssporet handling — den
            tilføjes som en ny linje i revisionssporet med din begrundelse.
            Selve periode-rækken ændres aldrig. En allerede indberettet
            (reported) periode kan ikke genåbnes.
          </p>
        </>
      }
      confirmLabel="Genåbn perioden"
      confirmKind="danger"
      noteLabel="Begrundelse (påkrævet)"
      notePlaceholder="Hvorfor genåbnes perioden? (fx 'bilag bogført for sent')"
      onConfirm={async (reason) => {
        if (reason.trim().length === 0) {
          throw {
            code: "bad_request",
            message:
              "Angiv en begrundelse — en genåbning skal kunne spores i revisionssporet.",
          };
        }
        await api.reopenPeriod(vat.slug, {
          periodStart: vat.periodStart,
          periodEnd: vat.periodEnd,
          kind: "vat_quarter",
          reason: reason.trim(),
        });
        onReopened(vat.periodLabel);
      }}
      onClose={onClose}
    />
  );
}

/**
 * The "X dage tilbage" countdown to the VAT filing deadline. Turns critical
 * once the deadline is near or passed, so an owner sees the urgency at a
 * glance — the momsangivelse is easy to forget.
 */
function DeadlineCountdown({ days }: { days: number }) {
  if (days < 0) {
    return (
      <span className="flag critical">
        Fristen er overskredet {Math.abs(days)}{" "}
        {Math.abs(days) === 1 ? "dag" : "dage"}
      </span>
    );
  }
  if (days === 0) return <span className="flag critical">Frist i dag</span>;
  const tone = days <= 30 ? "warning" : "ok";
  return (
    <span className={`flag ${tone}`}>
      {days} {days === 1 ? "dag" : "dage"} tilbage
    </span>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Moms er ikke tilgængelig for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Momsopgørelsen beregnes fra den
        aktive ledgers bogførte momskonti, og en momsangivelse kan ikke
        rekonstrueres for et arkiveret år — den vises derfor ikke.
        Resultatopgørelse, balance, saldobalance og posteringer for {year} er
        tilgængelige.
      </p>
    </div>
  );
}
