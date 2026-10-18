import { useState } from "react";
import { api } from "../lib/api";
import type { AccountRole, CompanyAccounts } from "../lib/types";
import { useCapabilities } from "../lib/useCapabilities";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { Button, Input, Select } from "./ui";

const types = { asset: "Aktiv", liability: "Passiv", equity: "Egenkapital", income: "Indtægt", expense: "Omkostning", vat: "Moms" };
const roles: Record<AccountRole, [string, string, string]> = {
  bank: ["Bankkonto", "asset", "debit"], debtors: ["Debitorer", "asset", "debit"],
  creditors: ["Kreditorer", "liability", "credit"], output_vat: ["Salgsmoms", "vat", "credit"],
  input_vat: ["Købsmoms", "vat", "debit"], reverse_charge_vat: ["Omvendt betalingspligt-moms", "vat", "credit"],
  vat_settlement: ["Momsafregning", "liability", "credit"], operational_default: ["Standard driftskonto", "expense", "debit"],
};

export function NativeChartSetup({ slug, data, onSaved }: { slug: string; data: CompanyAccounts; onSaved: () => void }) {
  const capabilities = useCapabilities(slug);
  const [accountNo, setAccountNo] = useState("");
  const [name, setName] = useState("");
  const [type, setType] = useState("expense");
  const [normalBalance, setNormalBalance] = useState("debit");
  const [vatCode, setVatCode] = useState("");
  const [role, setRole] = useState<AccountRole>("input_vat");
  const [roleAccount, setRoleAccount] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const outcome = useMutationOutcome(onSaved, "chart-setup", `company:${slug}`);
  if (!capabilities.can("company.admin")) return null;
  const compatible = data.accounts.filter(a => a.active !== false && a.type === roles[role][1] && a.normalBalance === roles[role][2]);
  async function save(action: () => Promise<unknown>) {
    if (busy || outcome.isBlocked()) return;
    setBusy(true); setError("");
    try { await action().catch(outcome.reject); onSaved(); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "Ændringen kunne ikke gemmes."); }
    finally { setBusy(false); }
  }
  return <section className="card" aria-labelledby="chart-setup-heading">
    <h3 id="chart-setup-heading">Tilpas kontoplan</h3>
    {outcome.feedback}
    {error && <p role="alert" className="warning">{error}</p>}
    <fieldset disabled={busy || outcome.blocked}>
      <legend>Opret konto</legend>
      <form aria-label="Opret konto" onSubmit={e => { e.preventDefault(); void save(() => api.createAccount(slug, { accountNo: accountNo.trim(), name: name.trim(), type, normalBalance, defaultVatCode: vatCode.trim() || null })); }}>
        <div className="modal-field-grid">
          <label>Kontonummer<Input required maxLength={64} value={accountNo} onChange={e => setAccountNo(e.target.value)} /></label>
          <label>Kontonavn<Input required maxLength={256} value={name} onChange={e => setName(e.target.value)} /></label>
          <label>Kontotype<Select value={type} onChange={e => setType(e.target.value)}>{Object.entries(types).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></label>
          <label>Normal saldo<Select value={normalBalance} onChange={e => setNormalBalance(e.target.value)}><option value="debit">Debet</option><option value="credit">Kredit</option></Select></label>
          <label>Standard momskode (valgfri)<Input value={vatCode} onChange={e => setVatCode(e.target.value)} placeholder="fx DK_PURCHASE_25" /></label>
        </div>
        <Button type="submit">Opret konto</Button>
      </form>
    </fieldset>
    <fieldset disabled={busy || outcome.blocked}>
      <legend>Bekræft kontorolle</legend>
      <p className="muted">Vælg hvilken konto bogføringen skal bruge til rollen. Kun aktive konti med passende type og normal saldo vises.</p>
      <form aria-label="Bekræft kontorolle" onSubmit={e => { e.preventDefault(); void save(() => api.confirmAccountRole(slug, role, roleAccount)); }}>
        <div className="modal-field-grid">
          <label>Kontorolle<Select value={role} onChange={e => { setRole(e.target.value as AccountRole); setRoleAccount(""); }}>{Object.entries(roles).map(([value, [label]]) => <option key={value} value={value}>{label}</option>)}</Select></label>
          <label>Konto til rollen<Select required value={roleAccount} onChange={e => setRoleAccount(e.target.value)}><option value="">Vælg konto</option>{compatible.map(a => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} — {a.name}</option>)}</Select></label>
        </div>
        <Button type="submit" disabled={!roleAccount}>Bekræft kontorolle</Button>
      </form>
    </fieldset>
  </section>;
}
