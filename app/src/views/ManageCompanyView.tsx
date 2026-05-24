// Company management — rename the display name, sync CVR stamdata, and
// archive/restore.
//
// Strictly non-destructive: there is intentionally no delete of ledger data.
// Archiving only flips a manifest flag; the ledger stays on disk untouched.

import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import type {
  CompanyEntry,
  CompanySettings,
  VatPeriodType,
} from "../lib/types";

/** The VAT-cadence options for the profile / create-company selectors (#300). */
const VAT_PERIOD_OPTIONS: Array<{ value: VatPeriodType; label: string }> = [
  { value: "month", label: "Måned (måneds-moms)" },
  { value: "quarter", label: "Kvartal (kvartals-moms)" },
  { value: "half-year", label: "Halvår (halvårs-moms)" },
];

export function ManageCompanyView() {
  const { slug = "" } = useParams();
  const navigate = useNavigate();
  const state = useAsync(async () => {
    const companies = await api.companies();
    const found = companies.find((c) => c.slug === slug);
    if (!found) throw new ApiError("not_found", "Virksomheden findes ikke.", 404);
    const settings = await api.companySettings(slug);
    return { found, settings };
  }, [slug]);

  if (state.loading) return <Loading />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  return (
    <ManageForm
      company={state.data!.found}
      settings={state.data!.settings}
      onArchivedAway={() => navigate("/")}
    />
  );
}

function ManageForm({
  company,
  settings,
  onArchivedAway,
}: {
  company: CompanyEntry;
  settings: CompanySettings;
  onArchivedAway: () => void;
}) {
  const [name, setName] = useState(company.name);
  // Local mirrors of the persisted state, so the form stays consistent after a
  // save without re-fetching (a re-fetch would unmount this form mid-notice).
  const [savedName, setSavedName] = useState(company.name);
  const [archived, setArchived] = useState(company.archived);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const renameDisabled =
    busy || name.trim().length === 0 || name.trim() === savedName;

  async function rename(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await api.updateCompany(company.slug, {
        name: name.trim(),
      });
      setSavedName(updated.name);
      setName(updated.name);
      setNotice("Visningsnavn opdateret.");
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Kunne ikke gemme navnet.");
    } finally {
      setBusy(false);
    }
  }

  async function toggleArchive() {
    const next = !archived;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.updateCompany(company.slug, { archived: next });
      if (next) {
        onArchivedAway();
      } else {
        setArchived(false);
        setNotice("Virksomheden er gendannet.");
      }
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Kunne ikke ændre arkivstatus.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <div className="page-head">
        <div>
          <h2>Administrér {savedName}</h2>
          <p className="muted">
            Slug <code>{company.slug}</code> · oprettet{" "}
            {company.createdAt.slice(0, 10)}
            {archived ? " · arkiveret" : ""}
          </p>
        </div>
        <Link className="btn secondary" to={`/companies/${company.slug}`}>
          Tilbage til regnskab
        </Link>
      </div>

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}

      <form className="form" onSubmit={rename} aria-label="Omdøb virksomhed">
        <label>
          Visningsnavn
          <input
            name="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <span className="field-hint">
            Ændrer kun det viste navn — slug og regnskabsdata berøres ikke.
          </span>
        </label>
        <div className="row-actions">
          <button className="btn" type="submit" disabled={renameDisabled}>
            Gem navn
          </button>
        </div>
      </form>

      <ProfileCard slug={company.slug} initial={settings} />

      <CvrCard slug={company.slug} initial={settings} />

      <AccountantExportCard slug={company.slug} />

      <div className="card" style={{ marginTop: 24, maxWidth: 460 }}>
        <h3 style={{ marginTop: 0 }}>
          {archived ? "Gendan virksomhed" : "Arkivér virksomhed"}
        </h3>
        <p className="muted">
          {archived
            ? "Virksomheden er arkiveret. Gendan den for at vise den i porteføljen igen."
            : "Arkivering skjuler virksomheden fra den aktive portefølje. Regnskabsdata slettes aldrig og kan gendannes."}
        </p>
        <button className="btn secondary" onClick={toggleArchive} disabled={busy}>
          {archived ? "Gendan virksomhed" : "Arkivér virksomhed"}
        </button>
      </div>
    </section>
  );
}

/**
 * The editable company profile + bank-details card (#284).
 *
 * A Cockpit owner must be able to record the company's own postal address,
 * default payment terms and — critically — its bank account. Without a bank
 * account every issued invoice goes out with no payment instructions, so the
 * card warns prominently when none is configured. Saving calls the same
 * `setCompanyProfile` core function the CLI's `company profile` command uses.
 */
function ProfileCard({
  slug,
  initial,
}: {
  slug: string;
  initial: CompanySettings;
}) {
  const [settings, setSettings] = useState(initial);
  const [address, setAddress] = useState(initial.address ?? "");
  const [postalCode, setPostalCode] = useState(initial.postalCode ?? "");
  const [city, setCity] = useState(initial.city ?? "");
  // #300: the VAT settlement cadence is editable from the cockpit.
  const [vatPeriodType, setVatPeriodType] = useState<VatPeriodType>(
    initial.vatPeriodType,
  );
  const [bankName, setBankName] = useState(initial.payment?.bankName ?? "");
  const [registrationNo, setRegistrationNo] = useState(
    initial.payment?.registrationNo ?? "",
  );
  const [accountNo, setAccountNo] = useState(
    initial.payment?.accountNo ?? "",
  );
  const [iban, setIban] = useState(initial.payment?.iban ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const hasPayment = Boolean(
    settings.payment &&
      (settings.payment.bankName ||
        settings.payment.registrationNo ||
        settings.payment.accountNo ||
        settings.payment.iban),
  );

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updated = await api.updateCompanyProfile(slug, {
        address: address.trim(),
        postalCode: postalCode.trim(),
        city: city.trim(),
        vatPeriodType,
        payment: {
          bankName: bankName.trim(),
          registrationNo: registrationNo.trim(),
          accountNo: accountNo.trim(),
          iban: iban.trim(),
        },
      });
      setSettings(updated);
      setVatPeriodType(updated.vatPeriodType);
      setNotice("Stamdata opdateret.");
    } catch (err) {
      setError(
        err instanceof ApiError
          ? err.message
          : "Kunne ikke gemme stamdata.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="card" style={{ marginTop: 24, maxWidth: 460 }}>
      <h3 style={{ marginTop: 0 }}>Stamdata og bankoplysninger</h3>
      <p className="muted">
        Virksomhedens egen adresse og bankkonto. Bankkontoen vises som
        betalingsoplysninger på alle fakturaer du udsteder.
      </p>

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}
      {!hasPayment && (
        <Banner kind="warning">
          Der er ingen bankkonto registreret. Fakturaer udstedes uden
          betalingsoplysninger, indtil du tilføjer en konto her.
        </Banner>
      )}

      <form className="form" onSubmit={save} aria-label="Rediger stamdata">
        <label>
          Adresse
          <input
            name="address"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="Vej 1"
          />
        </label>
        <label>
          Postnummer
          <input
            name="postalCode"
            value={postalCode}
            onChange={(e) => setPostalCode(e.target.value)}
            placeholder="1000"
          />
        </label>
        <label>
          By
          <input
            name="city"
            value={city}
            onChange={(e) => setCity(e.target.value)}
            placeholder="København"
          />
        </label>
        <label>
          Momsperiode
          <select
            name="vatPeriodType"
            value={vatPeriodType}
            onChange={(e) =>
              setVatPeriodType(e.target.value as VatPeriodType)
            }
          >
            {VAT_PERIOD_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <span className="field-hint">
            Den momsperiode virksomheden er registreret for hos SKAT.
            Momsperioder og -frister følger dette valg.
          </span>
        </label>
        <label>
          Pengeinstitut
          <input
            name="bankName"
            value={bankName}
            onChange={(e) => setBankName(e.target.value)}
            placeholder="Danske Bank"
          />
        </label>
        <label>
          Registreringsnummer
          <input
            name="registrationNo"
            value={registrationNo}
            onChange={(e) => setRegistrationNo(e.target.value)}
            placeholder="1234"
          />
        </label>
        <label>
          Kontonummer
          <input
            name="accountNo"
            value={accountNo}
            onChange={(e) => setAccountNo(e.target.value)}
            placeholder="0001234567"
          />
        </label>
        <label>
          IBAN (valgfrit)
          <input
            name="iban"
            value={iban}
            onChange={(e) => setIban(e.target.value)}
            placeholder="DK0000000000000000"
          />
          <span className="field-hint">
            Bruges til betalinger fra udlandet.
          </span>
        </label>
        <div className="row-actions">
          <button className="btn" type="submit" disabled={busy}>
            {busy ? "Gemmer…" : "Gem stamdata"}
          </button>
        </div>
      </form>
    </div>
  );
}

/**
 * The CVR-stamdata card: shows the company's registered address / branche /
 * status and a "Hent fra CVR" button that refreshes it from the CVR register.
 * The lookup runs server-side, so the CVR credentials never reach the browser.
 */
function CvrCard({ slug, initial }: { slug: string; initial: CompanySettings }) {
  const [settings, setSettings] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fiscalWarning, setFiscalWarning] = useState<string | null>(null);
  // #402 — `null` = "we haven't checked yet", so we don't flicker the
  // "CVR-login mangler"-banner during the initial fetch.
  const [cvrLoginConfigured, setCvrLoginConfigured] = useState<boolean | null>(
    null,
  );

  const hasCvr = Boolean(settings.cvr);
  const loginMissing = cvrLoginConfigured === false;

  // #402 — find out if the server has CVR_USERNAME / CVR_PASSWORD before the
  // owner clicks anything. An owner who lacks the login should *see* that fact
  // — and the path to fix it — instead of clicking a button that throws a raw
  // 401 their way and leaves them guessing.
  useEffect(() => {
    let cancelled = false;
    api
      .cvrStatus()
      .then((status) => {
        if (!cancelled) setCvrLoginConfigured(status.configured);
      })
      .catch(() => {
        // A failure to *check* status shouldn't block the owner from trying —
        // treat it as "unknown" and let the sync call surface any real issue.
        if (!cancelled) setCvrLoginConfigured(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function sync() {
    setBusy(true);
    setError(null);
    setNotice(null);
    setFiscalWarning(null);
    try {
      const result = await api.syncCvr(slug);
      if (!result.ok) {
        setError(translateCvrError(result.errors[0]));
        return;
      }
      const fresh = await api.companySettings(slug);
      setSettings(fresh);
      const changed = result.updatedFields ?? [];
      setNotice(
        changed.length > 0
          ? `Hentet fra CVR. Opdaterede felter: ${changed.join(", ")}.`
          : "Hentet fra CVR. Stamdata var allerede opdateret.",
      );
      const fy = result.fiscalYearStartMonth;
      if (fy && !fy.matches && fy.cvr !== null) {
        setFiscalWarning(
          `CVR har regnskabsår der starter i måned ${fy.cvr}, men virksomheden ` +
            `er sat op med måned ${fy.current}. Regnskabsåret ændres aldrig ` +
            `automatisk — ret det manuelt hvis det er forkert.`,
        );
      }
    } catch (err) {
      setError(
        err instanceof ApiError
          ? translateCvrError(err.message)
          : "Kunne ikke hente fra CVR.",
      );
    } finally {
      setBusy(false);
    }
  }

  const buttonDisabled = busy || !hasCvr || loginMissing;
  const buttonTitle = loginMissing
    ? "CVR-login mangler på serveren — se forklaringen ovenfor."
    : !hasCvr
      ? "Tilføj først et CVR-nummer på virksomheden."
      : undefined;

  return (
    <div className="card" style={{ marginTop: 24, maxWidth: 460 }}>
      <h3 style={{ marginTop: 0 }}>CVR-stamdata</h3>

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}
      {fiscalWarning && <Banner kind="warning">{fiscalWarning}</Banner>}

      {/*
        #402 — Owner-facing explanation when the server has no virk.dk-login.
        We say *what* is missing and *how* to fix it, in plain Danish, without
        sending the owner to the terminal for an environment variable they
        won't recognise.
      */}
      {loginMissing && (
        <Banner kind="warning">
          Cockpittet mangler dit virk.dk-login. Indtil det er sat op, kan
          stamdata ikke hentes automatisk fra CVR. Se{" "}
          <a href="/docs/cvr-opsaetning" target="_blank" rel="noreferrer">
            CVR-opsætningsguiden
          </a>{" "}
          for hvordan du konfigurerer det.
        </Banner>
      )}

      {!hasCvr && (
        <p className="muted">
          Der er ikke registreret et CVR-nummer på virksomheden. Tilføj et
          CVR-nummer for at kunne hente stamdata fra CVR-registret.
        </p>
      )}

      {hasCvr && (
        <dl className="cvr-facts">
          <Fact label="CVR-nummer" value={settings.cvr} />
          <Fact label="Adresse" value={cvrAddressLine(settings)} />
          <Fact label="Virksomhedsform" value={settings.companyForm} />
          <Fact
            label="Branche"
            value={
              settings.industryText && settings.industryCode
                ? `${settings.industryCode} — ${settings.industryText}`
                : settings.industryText
            }
          />
          <Fact label="Status" value={settings.cvrStatus} />
          <Fact
            label="Revision fravalgt"
            value={
              settings.auditWaived === null
                ? null
                : settings.auditWaived
                  ? "Ja"
                  : "Nej"
            }
          />
          <Fact
            label="Sidst hentet"
            value={settings.cvrSyncedAt ? settings.cvrSyncedAt.slice(0, 10) : "Aldrig"}
          />
        </dl>
      )}

      <button
        className="btn secondary"
        onClick={sync}
        disabled={buttonDisabled}
        title={buttonTitle}
      >
        {busy ? "Henter…" : "Hent fra CVR"}
      </button>
      <p className="field-hint">
        Kræver dit virk.dk-login. Konfigurér det én gang under CVR-login —{" "}
        regnskabsåret ændres aldrig automatisk.
      </p>
    </div>
  );
}

/**
 * #402 — Map raw CVR-register error messages to something an owner can act
 * on. The CVR core surfaces "kræver miljøvariablerne CVR_USERNAME og
 * CVR_PASSWORD" when credentials are missing; that's developer-speak, so we
 * translate it into a plain-Danish call to action. Everything else passes
 * through verbatim.
 */
function translateCvrError(raw: string | undefined): string {
  const fallback = "CVR-opslaget mislykkedes.";
  if (!raw) return fallback;
  const lower = raw.toLowerCase();
  if (
    lower.includes("cvr_username") ||
    lower.includes("cvr_password") ||
    lower.includes("http 401")
  ) {
    return (
      "Cockpittet mangler CVR-login. Konfigurér dit virk.dk-login (se " +
      "CVR-opsætningsguiden under /docs/cvr-opsaetning) og prøv igen."
    );
  }
  return raw;
}

/**
 * Revisor-eksport — generates the accountant-handoff package and triggers a
 * single .tar download. Calls the same core export the CLI's
 * `system export-accountant` uses, packed by the server and streamed back.
 */
function AccountantExportCard({ slug }: { slug: string }) {
  const today = new Date().toISOString().slice(0, 10);
  const yearStart = `${today.slice(0, 4)}-01-01`;
  const [periodStart, setPeriodStart] = useState(yearStart);
  const [periodEnd, setPeriodEnd] = useState(today);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<{
    filename: string;
    journalEntryCount: number;
    documentCount: number;
    bankTransactionCount: number;
  } | null>(null);

  async function generate() {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const res = await api.accountantExport(slug, { periodStart, periodEnd });
      // Trigger a browser download from the blob — the response is the only
      // copy of the package that leaves the server.
      const url = URL.createObjectURL(res.blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = res.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      setDone({
        filename: res.filename,
        journalEntryCount: res.journalEntryCount,
        documentCount: res.documentCount,
        bankTransactionCount: res.bankTransactionCount,
      });
    } catch (err) {
      const e = err as { message?: string };
      setError(e?.message ?? "Eksporten kunne ikke gennemføres.");
    } finally {
      setBusy(false);
    }
  }

  const disabled =
    busy ||
    periodStart.length !== 10 ||
    periodEnd.length !== 10 ||
    periodStart > periodEnd;

  return (
    <div className="card" style={{ marginTop: 24, maxWidth: 460 }}>
      <h3 style={{ marginTop: 0 }}>Revisor-eksport</h3>
      <p className="muted">
        Pakker journal, bilag, banktransaktioner og audit-log for perioden i én
        .tar-fil, du kan sende til din revisor. Genereres deterministisk og
        forlader ikke din maskine før du klikker «Generér».
      </p>
      <label>
        Fra
        <input
          type="date"
          value={periodStart}
          onChange={(e) => setPeriodStart(e.target.value)}
          disabled={busy}
        />
      </label>
      <label>
        Til
        <input
          type="date"
          value={periodEnd}
          onChange={(e) => setPeriodEnd(e.target.value)}
          disabled={busy}
        />
      </label>
      {error && <Banner kind="error">{error}</Banner>}
      {done && (
        <Banner kind="success">
          Hentede {done.filename} — {done.journalEntryCount} posteringer,{" "}
          {done.documentCount} bilag, {done.bankTransactionCount}{" "}
          banktransaktioner.
        </Banner>
      )}
      <div className="row-actions">
        <button
          className="btn"
          onClick={generate}
          disabled={disabled}
          type="button"
        >
          {busy ? "Genererer…" : "Generér og download"}
        </button>
      </div>
    </div>
  );
}

function Fact({ label, value }: { label: string; value: string | null | undefined }) {
  return (
    <div className="cvr-fact">
      <dt className="muted">{label}</dt>
      <dd>{value && value.length > 0 ? value : "—"}</dd>
    </div>
  );
}

function cvrAddressLine(settings: CompanySettings): string | null {
  const cityLine = [settings.postalCode, settings.city].filter(Boolean).join(" ");
  const full = [settings.address, cityLine].filter((p) => p && p.length > 0).join(", ");
  return full.length > 0 ? full : null;
}
