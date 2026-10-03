import { Select } from "./ui";
import { useLocation, useNavigate } from "react-router-dom";
import { useAuth } from "../lib/auth-context";

const roleLabels: Record<string, string> = {
  owner: "Ejer",
  bookkeeper: "Bogholder",
  reviewer: "Godkender",
  reader: "Læseadgang",
};

export function CompanySwitcher() {
  const { context } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  if (!context || context.companies.length === 0) return null;
  const current = /^\/companies\/([^/]+)/.exec(location.pathname)?.[1] ?? "";
  return <label className="company-switcher">
    <span>Virksomhed</span>
    <Select aria-label="Skift virksomhed" value={current} onChange={(event) => navigate(`/companies/${event.target.value}`)}>
      <option value="">Vælg virksomhed</option>
      {context.companies.map((company) => <option key={company.slug} value={company.slug}>{company.name} — {roleLabels[company.role] ?? "Rolle ukendt"}</option>)}
    </Select>
  </label>;
}
