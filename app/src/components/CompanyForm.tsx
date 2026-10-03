import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Input, Select } from "./ui";
// The "add company" form — POSTs to /api/companies. It is reused verbatim by
// the first-run onboarding flow and the standalone add-company route, so it
// owns input state + submit, and reports the created slug via `onCreated`.

import { useState } from "react";
import { api, ApiError } from "../lib/api";
import type { VatPeriodType } from "../lib/types";
import { Banner } from "./Feedback";

/**
 * VAT-cadence options for the create-company selector (#300). `"none"` creates
 * a NOT VAT-registered company (a holding ApS, frivilligt momsfritaget, or a
 * microbusiness under the § 48 threshold); it is submitted as `null`.
 */
const VAT_PERIOD_OPTIONS: Array<{ value: VatPeriodType | "none"; label: string }> = [
  { value: "month", label: "Måned (måneds-moms)" },
  { value: "quarter", label: "Kvartal (kvartals-moms)" },
  { value: "half-year", label: "Halvår (halvårs-moms)" },
  { value: "none", label: "Ikke momsregistreret" },
];

export function CompanyForm({
  onCreated,
  submitLabel = "Opret virksomhed",
}: {
  onCreated: (slug: string) => void;
  submitLabel?: string;
}) {
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [cvr, setCvr] = useState("");
  const [fiscalMonth, setFiscalMonth] = useState("1");
  // #300: the VAT settlement cadence — defaults to the historical `quarter`.
  // `"none"` creates a NOT VAT-registered company (submitted as null).
  const [vatPeriodType, setVatPeriodType] = useState<VatPeriodType | "none">("quarter");
  // #284: optional bank/payment details — captured at creation so the very
  // first invoice already carries payment instructions.
  const [bankName, setBankName] = useState("");
  const [registrationNo, setRegistrationNo] = useState("");
  const [accountNo, setAccountNo] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState(false);
  const [foundCompanies, setFoundCompanies] = useState<Array<{ slug: string; name: string }> | null>(null);
  const outcome = useMutationOutcome(async () => {
    const companies = await api.companies();
    setFoundCompanies(companies.filter(company => slug.trim() ? company.slug === slug.trim() : company.name === name.trim()));
  });
  const markSaved = useUnsavedChanges(!created && Boolean(name || slug || cvr || bankName || registrationNo || accountNo || fiscalMonth !== "1" || vatPeriodType !== "quarter"));

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (submitting || outcome.isBlocked()) return;
    if (name.trim().length === 0) {
      setError("Angiv et virksomhedsnavn.");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const payment =
        bankName.trim() || registrationNo.trim() || accountNo.trim()
          ? {
              bankName: bankName.trim() || undefined,
              registrationNo: registrationNo.trim() || undefined,
              accountNo: accountNo.trim() || undefined,
            }
          : undefined;
      const created = await api.createCompany({
        name: name.trim(),
        slug: slug.trim() || undefined,
        cvr: cvr.trim() || undefined,
        fiscalYearStartMonth: fiscalMonth.trim() || undefined,
        // `"none"` → null so the server creates a not-VAT-registered company.
        vatPeriodType: vatPeriodType === "none" ? null : vatPeriodType,
        ...(payment ? { payment } : {}),
      }).catch(outcome.reject);
      setCreated(true);
      markSaved();
      onCreated(created.slug);
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Kunne ikke oprette virksomheden.",
      );
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form className="form" onSubmit={handleSubmit} aria-label="Opret virksomhed">
      {error && <Banner kind="error">{error}</Banner>}
      {outcome.feedback}
      {foundCompanies && <div role="status">
        <p>{foundCompanies.length ? "Statuskontrollen fandt følgende virksomhed. Åbn den for at gennemgå resultatet." : "Statuskontrollen fandt ingen virksomhed med de indtastede oplysninger. Oprettelsen er fortsat blokeret, fordi resultatet er uafklaret."}</p>
        {foundCompanies.map(company => <Button key={company.slug} variant="secondary" onClick={() => { setCreated(true); markSaved(); onCreated(company.slug); }}>Åbn {company.name}</Button>)}
      </div>}

      <label>
        Virksomhedsnavn
        <Input
          disabled={submitting || outcome.blocked}
          name="name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Eksempel ApS"
          autoFocus
          required
        />
      </label>

      <details className="advanced-fold">
        <summary>Avancerede indstillinger</summary>
        <label>
          Slug (valgfrit)
          <Input
          disabled={submitting || outcome.blocked}
            name="slug"
            value={slug}
            onChange={(e) => setSlug(e.target.value)}
            placeholder="udledes-af-navnet"
          />
          <span className="field-hint">
            Mappenavnet for regnskabet på disk. Udledes automatisk fra
            navnet ovenfor hvis du lader feltet stå tomt — du behøver
            sjældent at angive det selv.
          </span>
        </label>
      </details>

      <label>
        CVR-nummer (valgfrit)
        <Input
          disabled={submitting || outcome.blocked}
          name="cvr"
          value={cvr}
          onChange={(e) => setCvr(e.target.value)}
          placeholder="12345678"
        />
      </label>

      <label>
        Regnskabsår starter i måned
        <Select
          disabled={submitting || outcome.blocked}
          name="fiscalYearStartMonth"
          value={fiscalMonth}
          onChange={(e) => setFiscalMonth(e.target.value)}
        >
          {MONTHS.map((m, i) => (
            <option key={m} value={String(i + 1)}>
              {m}
            </option>
          ))}
        </Select>
      </label>

      <label>
        Momsperiode
        <Select
          disabled={submitting || outcome.blocked}
          name="vatPeriodType"
          value={vatPeriodType}
          onChange={(e) => setVatPeriodType(e.target.value as VatPeriodType | "none")}
        >
          {VAT_PERIOD_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
        <span className="field-hint">
          Den momsperiode virksomheden er registreret for hos SKAT — månedlig,
          kvartalsvis eller halvårlig. Vælg «Ikke momsregistreret» for fx et
          holdingselskab. Kan ændres senere under Administrér.
        </span>
      </label>

      <label>
        Bank (valgfrit)
        <Input
          disabled={submitting || outcome.blocked}
          name="bankName"
          value={bankName}
          onChange={(e) => setBankName(e.target.value)}
          placeholder="Danske Bank"
        />
      </label>

      <label>
        Registreringsnummer (valgfrit)
        <Input
          disabled={submitting || outcome.blocked}
          name="registrationNo"
          value={registrationNo}
          onChange={(e) => setRegistrationNo(e.target.value)}
          placeholder="1234"
        />
      </label>

      <label>
        Kontonummer (valgfrit)
        <Input
          disabled={submitting || outcome.blocked}
          name="accountNo"
          value={accountNo}
          onChange={(e) => setAccountNo(e.target.value)}
          placeholder="0001234567"
        />
        <span className="field-hint">
          Bankkontoen vises som betalingsoplysninger på dine fakturaer. Kan
          også tilføjes senere under Administrér.
        </span>
      </label>

      <div className="row-actions">
        <Button className="btn" type="submit" disabled={submitting || outcome.blocked}>
          {submitting ? "Opretter…" : submitLabel}
        </Button>
      </div>
    </form>
  );
}

const MONTHS = [
  "Januar",
  "Februar",
  "Marts",
  "April",
  "Maj",
  "Juni",
  "Juli",
  "August",
  "September",
  "Oktober",
  "November",
  "December",
];
