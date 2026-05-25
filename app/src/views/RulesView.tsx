// Lovgrundlag-viewer (#347) — workspace-level read-only side der viser de
// danske regler Rentemester anvender, med klikbar SHA-256-citation pr.
// provision og deep-links til retsinformation.dk.
//
// Acceptkriterium fra #347: read-only (regler kan kun ændres via PR i
// `rules/dk/`), bruger `parseRuleBundle` + `readLegalSourceIds` via det nye
// `/api/rules`-endpoint, og citationerne er SHA-256-fingeraftryk pr. paragraf.

import { useMemo, useState } from "react";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  LegalSource,
  RuleSummary,
  RulesResponse,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

export function RulesView() {
  const state = useAsync<RulesResponse>(() => api.rules(), []);
  const [bundleFilter, setBundleFilter] = useState<string>("");
  const [search, setSearch] = useState("");

  const sources = useMemo(() => {
    const map = new Map<string, LegalSource>();
    for (const s of state.data?.legalSources ?? []) map.set(s.id, s);
    return map;
  }, [state.data]);

  const bundles = state.data?.ruleBundles ?? [];
  const allRules = state.data?.rules ?? [];

  const filteredRules = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return allRules.filter((r) => {
      if (bundleFilter !== "" && r.bundle !== bundleFilter) return false;
      if (needle === "") return true;
      return (
        r.ruleId.toLowerCase().includes(needle) ||
        r.name.toLowerCase().includes(needle) ||
        r.explanation.toLowerCase().includes(needle) ||
        r.provisions.some((p) => p.ref.toLowerCase().includes(needle))
      );
    });
  }, [allRules, bundleFilter, search]);

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;

  return (
    <section className="rules-view">
      <header className="page-head">
        <div>
          <h2>Lovgrundlag</h2>
          <p className="muted">
            Rentemester citerer linje for linje fra retsinformation.dk via
            SHA-256-fingeraftryk. Siden viser hvilke regler der p.t. styrer
            bogføringen — og hvilken paragraf hver regel hænger på. Read-only:
            regler kan kun ændres ved en PR i <code>rules/dk/</code>.
          </p>
        </div>
      </header>

      <section className="card">
        <h3>Aktive regelbundler</h3>
        <table className="table">
          <thead>
            <tr>
              <th>Bundle</th>
              <th>Version</th>
              <th>Antal regler</th>
              <th>Kilder</th>
              <th>VAT-koder</th>
            </tr>
          </thead>
          <tbody>
            {bundles.map((b) => (
              <tr key={b.name}>
                <td>{b.name}</td>
                <td>
                  <code>{b.version}</code>
                </td>
                <td>{b.ruleCount}</td>
                <td>{b.sources.join(", ")}</td>
                <td>{b.vatCodes.join(", ") || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="card">
        <h3>Regler ({filteredRules.length} af {allRules.length})</h3>
        <div className="filter-bar">
          <label>
            Bundle{" "}
            <select
              value={bundleFilter}
              onChange={(e) => setBundleFilter(e.target.value)}
            >
              <option value="">Alle</option>
              {bundles.map((b) => (
                <option key={b.name} value={b.name}>
                  {b.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Søg{" "}
            <input
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="regel-id, paragraf, ord …"
            />
          </label>
        </div>

        <ul className="rule-list">
          {filteredRules.map((r) => (
            <RuleEntry
              key={r.ruleId}
              rule={r}
              source={sources.get(r.sourceId) ?? null}
            />
          ))}
          {filteredRules.length === 0 && (
            <li className="muted">Ingen regler matcher filtret.</li>
          )}
        </ul>
      </section>
    </section>
  );
}

function RuleEntry({
  rule,
  source,
}: {
  rule: RuleSummary;
  source: LegalSource | null;
}) {
  return (
    <li className="rule-entry">
      <div className="rule-head">
        <span className="rule-id">
          <code>{rule.ruleId}</code>
        </span>
        <span className="rule-name">{rule.name}</span>
        <span className={`pill severity-${rule.severity || "info"}`}>
          {rule.severity || "info"}
        </span>
        <span className="pill">{rule.category || "—"}</span>
        <span className="pill">{rule.bundle}</span>
      </div>
      {rule.explanation && <p className="rule-explanation">{rule.explanation}</p>}
      <details>
        <summary>Citationer ({rule.provisions.length})</summary>
        {source && (
          <p className="muted">
            Kilde:{" "}
            <a href={source.url} target="_blank" rel="noopener noreferrer">
              {source.title}
            </a>{" "}
            ({source.authority})
          </p>
        )}
        <table className="table provisions">
          <thead>
            <tr>
              <th>Paragraf</th>
              <th>SHA-256-fingeraftryk</th>
            </tr>
          </thead>
          <tbody>
            {rule.provisions.map((p, i) => (
              <tr key={`${rule.ruleId}-${i}`}>
                <td>{p.ref}</td>
                <td>
                  <code>{p.textHash}</code>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </li>
  );
}
