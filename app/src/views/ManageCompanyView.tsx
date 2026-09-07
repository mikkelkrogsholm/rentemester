// Company management — rename the display name, sync CVR stamdata, and
// archive/restore.
//
// Strictly non-destructive: there is intentionally no delete of ledger data.
// Archiving only flips a manifest flag; the ledger stays on disk untouched.

import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatDateDa } from "../lib/format";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { AccountantExportCard } from "../components/AccountantExportCard";
import { ConfirmDialog } from "../components/ConfirmDialog";
import type {
  CompanyEntry,
  CompanySettings,
  VatPeriodType,
} from "../lib/types";

// #UI-3 — the cockpit serves no /docs/* routes, so the old `/docs/cvr-opsaetning`
// link was a dead end. CVR-login setup is covered by the public installation
// guide; link there with an absolute URL, matching HelpView's DOCS_BASE pattern.
const CVR_SETUP_GUIDE_URL = "https://rentemester.dk/docs/installation";

/**
 * VAT-cadence options for the profile selector (#300). `"none"` is the
 * form-level sentinel for a NOT VAT-registered company; it is submitted to the
 * PATCH-profile endpoint as `null`, which deregisters the company. Without this
 * option an owner of a non-registered holding ApS could never keep — and would
 * silently re-register on — the not-registered state when saving the profile.
 */
const VAT_PERIOD_OPTIONS: Array<{ value: VatPeriodType | "none"; label: string }> = [
  { value: "month", label: "Måned (måneds-moms)" },
  { value: "quarter", label: "Kvartal (kvartals-moms)" },
  { value: "half-year", label: "Halvår (halvårs-moms)" },
  { value: "none", label: "Ikke momsregistreret" },
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
  // #UI-17 — archiving navigates the owner away from this view, so it must not
  // fire on a single stray click. Gate it behind a confirm dialog. (Restoring
  // is non-destructive and stays on the page, so it stays one-click.)
  const [confirmingArchive, setConfirmingArchive] = useState(false);
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

  // Restoring is non-destructive and keeps the owner on the page — one click.
  async function restoreCompany() {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.updateCompany(company.slug, { archived: false });
      setArchived(false);
      setNotice("Virksomheden er gendannet.");
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Kunne ikke ændre arkivstatus.",
      );
    } finally {
      setBusy(false);
    }
  }

  // #UI-17 — archiving runs from the confirm dialog. It rejects on failure so
  // the dialog surfaces the error (and the backup-lock conflict) inline rather
  // than navigating away on a half-finished write.
  async function confirmArchive() {
    await api.updateCompany(company.slug, { archived: true });
    onArchivedAway();
  }

  return (
    <section data-cockpit-page="manage" data-evidence-issue="655">
      <div className="page-head">
        <div>
          <h2>Administration</h2>
          <p className="muted">
            Hold virksomhedens profil og den daglige opsætning på plads.
          </p>
        </div>
        <Link className="btn secondary" to={`/companies/${company.slug}`}>
          Tilbage til regnskab
        </Link>
      </div>

      {error && <Banner kind="error">{error}</Banner>}
      {notice && <Banner kind="success">{notice}</Banner>}

      <section aria-labelledby="virksomhedsprofil-heading">
      <h3 id="virksomhedsprofil-heading">Virksomhedsprofil</h3>
      <p className="muted">Redigér navn, stamdata og betalingsoplysninger. Regnskabsdata påvirkes ikke.</p>
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
      </section>

      <section className="card" aria-labelledby="daglig-opsætning-heading">
        <h3 id="daglig-opsætning-heading">Daglig opsætning</h3>
        <p>Vælg det område, du vil gøre klar til den daglige bogføring.</p>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${company.slug}/kontoplan`}>Kontoplan</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/dimensioner`}>Dimensioner</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/bankkonti`}>Bankkonti</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/bilagsmail`}>Bilagsmail</Link>
        </div>
      </section>

      <section className="card" aria-labelledby="advanced-security-heading">
        <h3 id="advanced-security-heading">Avanceret og sikkerhed</h3>
        <p className="muted">Kontrol, opbevaring og særlige arbejdsgange er adskilt fra den almindelige profilredigering.</p>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${company.slug}/integritet`}>Integritet og backup</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/retention`}>Opbevaring</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/gdpr`}>GDPR</Link>
          <Link className="btn secondary" to={`/companies/${company.slug}/arkiv`}>Arkiv</Link>
        </div>
      </section>

      <details className="card">
        <summary>System- og livscyklusindstillinger</summary>
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
        <button
          className="btn secondary"
          type="button"
          onClick={
            archived ? restoreCompany : () => setConfirmingArchive(true)
          }
          disabled={busy}
        >
          {archived ? "Gendan virksomhed" : "Arkivér virksomhed"}
        </button>
      </div>

      {confirmingArchive && (
        <ConfirmDialog
          title="Arkivér virksomhed"
          body={
            <p>
              Arkivér <strong>{savedName}</strong>? Virksomheden skjules fra den
              aktive portefølje, og du sendes tilbage til oversigten.
              Regnskabsdata slettes aldrig og kan gendannes herfra igen.
            </p>
          }
          confirmLabel="Arkivér virksomhed"
          confirmKind="danger"
          onConfirm={confirmArchive}
          onClose={() => setConfirmingArchive(false)}
        />
      )}
      </details>
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
  // #300: the VAT settlement cadence is editable from the cockpit, including
  // turning registration OFF (`"none"` → submitted as `null`). Initialise the
  // form state from `initial.vatPeriodType` so a not-VAT-registered company
  // (null) shows "Ikke momsregistreret" selected — defaulting to `quarter`
  // here would silently re-register the company on the next profile save.
  const [vatPeriodType, setVatPeriodType] = useState<VatPeriodType | "none">(
    initial.vatPeriodType ?? "none",
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
        // `"none"` → null so the server deregisters the company; a real
        // cadence is sent verbatim.
        vatPeriodType: vatPeriodType === "none" ? null : vatPeriodType,
        payment: {
          bankName: bankName.trim(),
          registrationNo: registrationNo.trim(),
          accountNo: accountNo.trim(),
          iban: iban.trim(),
        },
      });
      setSettings(updated);
      setVatPeriodType(updated.vatPeriodType ?? "none");
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
              setVatPeriodType(e.target.value as VatPeriodType | "none")
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
            Momsperioder og -frister følger dette valg. Vælg «Ikke
            momsregistreret» for en virksomhed uden momsregistrering (fx
            holdingselskab) — momsfrister og momsangivelse slås så fra.
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
          <a
            href={CVR_SETUP_GUIDE_URL}
            target="_blank"
            rel="noreferrer noopener"
          >
            installationsguiden
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
            value={
              settings.cvrSyncedAt
                ? formatDateDa(settings.cvrSyncedAt.slice(0, 10))
                : "Aldrig"
            }
          />
        </dl>
      )}

      <button
        className="btn secondary"
        type="button"
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
      "installationsguiden på rentemester.dk/docs/installation) og prøv igen."
    );
  }
  return raw;
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
