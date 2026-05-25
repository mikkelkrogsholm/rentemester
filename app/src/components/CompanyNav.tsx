// Per-company chrome shared by every company view (cockpit-redesign it. 2).
//
// Two concerns live here so the four company views stay declarative:
//
//   * `useCompanyYear` — the selected fiscal year, carried in the URL as a
//     `?year=` query param. Carrying it in the route (not React state) means
//     the choice survives navigation between a company's sub-views and a page
//     reload, and every in-app link below preserves it automatically.
//
//   * `CompanyNav` — the sub-navigation bar plus the fiscal-year selector,
//     rendered at the top of each company view. The fourteen views are
//     arranged in four labelled groups (Regnskab · Bogføring · Salg ·
//     Historik) so the bar stays scannable and wraps tidily on a phone.

import { NavLink, useSearchParams } from "react-router-dom";
import type { FiscalYearEntry } from "../lib/types";

/**
 * The selected fiscal year as a URL query param. `year` is `undefined` until
 * the user picks one (the backend then defaults to the most recent live
 * year); `setYear` writes it back to the URL so it persists across views.
 */
export function useCompanyYear(): {
  year: string | undefined;
  setYear: (year: string) => void;
} {
  const [params, setParams] = useSearchParams();
  const year = params.get("year") ?? undefined;
  const setYear = (next: string) => {
    const updated = new URLSearchParams(params);
    updated.set("year", next);
    setParams(updated, { replace: true });
  };
  return { year, setYear };
}

/**
 * The route for a single account's postings — the Posteringer view filtered
 * to one account via `?account=`. The fiscal year is carried through so the
 * drill-down lands on the same year the statement was showing. Used by the
 * statement views (Resultatopgørelse · Balance · Saldobalance) to make every
 * account row a drill-down link.
 */
export function accountPostingsTo(
  slug: string,
  year: string,
  accountNo: string,
): string {
  const params = new URLSearchParams();
  if (year) params.set("year", year);
  params.set("account", accountNo);
  return `/companies/${slug}/posteringer?${params.toString()}`;
}

type NavTab = { to: string; label: string };

/**
 * The fourteen company views, arranged into four labelled groups. The grouping
 * keeps the bar scannable — and gives narrow viewports a deliberate wrap
 * boundary rather than an arbitrary one.
 */
const TAB_GROUPS: { name: string; tabs: NavTab[] }[] = [
  {
    name: "Regnskab",
    tabs: [
      { to: "", label: "Overblik" },
      { to: "resultatopgorelse", label: "Resultatopgørelse" },
      { to: "balance", label: "Balance" },
      { to: "saldobalance", label: "Saldobalance" },
      { to: "forpligtelser", label: "Forpligtelser" },
      { to: "likviditet", label: "Likviditet" },
      // #339: budget plan vs. faktiske bevægelser, side-om-side i en knap.
      { to: "budget", label: "Budget" },
    ],
  },
  {
    name: "Bogføring",
    tabs: [
      { to: "posteringer", label: "Posteringer" },
      { to: "bilag", label: "Bilag" },
      { to: "leverandoerfaktura", label: "Leverandørfaktura" },
      { to: "bank", label: "Bank" },
      { to: "anlaeg", label: "Anlæg" },
      { to: "moms", label: "Moms" },
      { to: "koersel", label: "Kørsel" },
      // Agent-forslag → menneskelig godkendelse (#346). Lever i Bogføring-
      // gruppen fordi en godkendelse her er sidste mile før en konkret
      // postering — selve den deterministiske postering laves derefter på
      // den linkede side (Anlæg, Leverandørfaktura, Posteringer, …).
      { to: "agent-forslag", label: "Agent-forslag" },
    ],
  },
  {
    name: "Salg",
    tabs: [
      { to: "fakturaer", label: "Fakturaer" },
      { to: "faktura-skabeloner", label: "Skabeloner" },
      { to: "kontakter", label: "Kontakter" },
    ],
  },
  {
    name: "Historik",
    tabs: [
      { to: "arkiv", label: "Arkiv" },
      { to: "fleraar", label: "Flerår" },
      // #343 — 5-års retention-status pr. data-domæne, så ejeren kan se hvad
      // der nærmer sig udløb af bogføringspligten.
      { to: "retention", label: "Retention" },
      // #333 — Integritet & backup: hash-kæde-status, backup-compliance og
      // backup-destinationer.
      { to: "integritet", label: "Integritet" },
      // #344 — Kontoplan: read-only liste over konti med søg + type-filter.
      { to: "kontoplan", label: "Kontoplan" },
    ],
  },
];

/**
 * The per-company sub-navigation. `slug` keys the links; the current `?year=`
 * is threaded through every tab so the chosen year follows the user across
 * views. `years`/`selectedYear`/`onYearChange` drive the fiscal-year selector.
 */
export function CompanyNav({
  slug,
  years,
  selectedYear,
  onYearChange,
}: {
  slug: string;
  years: FiscalYearEntry[];
  selectedYear: string;
  onYearChange: (year: string) => void;
}) {
  const [params] = useSearchParams();
  const query = params.toString();
  const suffix = query ? `?${query}` : "";

  return (
    <nav className="company-nav" aria-label="Virksomhedsvisninger">
      <div className="company-tabs">
        {TAB_GROUPS.map((group) => (
          <div
            key={group.name}
            className="company-tab-group"
            role="group"
            aria-label={group.name}
          >
            {group.tabs.map((tab) => {
              const path = tab.to
                ? `/companies/${slug}/${tab.to}`
                : `/companies/${slug}`;
              return (
                <NavLink key={tab.to} to={`${path}${suffix}`} end>
                  {tab.label}
                </NavLink>
              );
            })}
          </div>
        ))}
      </div>
      <YearSelector
        years={years}
        selected={selectedYear}
        onChange={onYearChange}
      />
    </nav>
  );
}

/** The fiscal-year dropdown — shared by every company view. */
export function YearSelector({
  years,
  selected,
  onChange,
}: {
  years: FiscalYearEntry[];
  selected: string;
  onChange: (year: string) => void;
}) {
  return (
    <label className="year-selector">
      <span className="ys-label">Regnskabsår</span>
      <select
        value={selected}
        onChange={(e) => onChange(e.target.value)}
        aria-label="Vælg regnskabsår"
      >
        {years.map((y) => (
          <option key={y.label} value={y.label}>
            {y.label}
            {y.source === "archive" ? " (arkiv)" : ""}
          </option>
        ))}
      </select>
    </label>
  );
}
