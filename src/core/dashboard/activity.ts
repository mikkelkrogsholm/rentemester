// "Seneste aktivitet" — the audit-log strip, translated to plain Danish.

import type { AuditLogRow } from "../audit-log";
import { escapeHtml, formatTimestampShort } from "./_shared";

// The audit log records events under terse machine codes (`journal_reverse`,
// `document_ingest`, ...). On the human-facing dashboard those read as the
// system's internals, not an overview of the business — so the "Seneste
// aktivitet" strip translates each event code to a plain-Danish label. (#233)
const ACTIVITY_EVENT_DA: Record<string, string> = {
  asset_depreciation_post: "Afskrivning bogført",
  asset_immediate_writeoff: "Straksafskrivning bogført",
  asset_register: "Aktiv registreret",
  authority_export: "Eksport til myndighed",
  backup_archive_created: "Backup-arkiv oprettet",
  backup_destination_added: "Backup-destination tilføjet",
  backup_destination_removed: "Backup-destination fjernet",
  backup_lock_configured: "Bogføringslås konfigureret",
  backup_placed: "Backup placeret eksternt",
  bank_account_add: "Bankkonto oprettet",
  bank_import: "Banktransaktioner importeret",
  company_cvr_sync: "Stamdata hentet fra CVR",
  credit_note_issue: "Kreditnota udstedt",
  customer_create: "Kunde oprettet",
  document_ingest: "Bilag indlæst",
  gdpr_erasure: "Persondata slettet (GDPR)",
  import_chart_reconcile: "Kontoplan afstemt ved import",
  import_company_reconcile: "Virksomhed afstemt ved import",
  invoice_bad_debt_writeoff: "Tab på debitor bogført",
  invoice_claim_payment_apply: "Betaling af krav registreret",
  invoice_compensation_post: "Kompensation bogført",
  invoice_compensation_register: "Kompensationskrav registreret",
  invoice_email_send: "Faktura sendt på email",
  invoice_interest_post: "Morarente bogført",
  invoice_interest_register: "Morarentekrav registreret",
  invoice_issue: "Faktura udstedt",
  invoice_payment_apply: "Fakturabetaling registreret",
  invoice_refund_apply: "Refundering til kunde bogført",
  invoice_reminder_post: "Rykker bogført",
  invoice_reminder_register: "Rykker registreret",
  invoice_render_pdf: "Faktura-PDF genereret",
  journal_post: "Finanspostering bogført",
  journal_reverse: "Finanspostering tilbageført",
  mileage_entry_create: "Kørselspost registreret",
  mileage_log_export: "Kørselsregnskab eksporteret",
  opening_balance_post: "Primobalance bogført",
  period_close: "Regnskabsperiode lukket",
  period_report: "Regnskabsperiode markeret indberettet",
  public_einvoice_oioubl_export: "OIOUBL e-faktura eksporteret",
  public_einvoice_peppol_submission: "PEPPOL e-faktura afsendt",
  recurring_invoice_generate: "Gentagende faktura genereret",
  recurring_invoice_template_create: "Fakturaskabelon oprettet",
  saft_export: "SAF-T-eksport",
  system_backup: "Backup oprettet",
  system_restore: "Backup gendannet",
  vendor_create: "Leverandør oprettet",
};

/** Translate an audit event code to a plain-Danish label, never an internal code. (#233) */
export function activityEventLabel(eventType: string): string {
  const known = ACTIVITY_EVENT_DA[eventType];
  if (known) return known;
  // Unknown code: humanise it (replace underscores, capitalise) rather than
  // showing the raw snake_case identifier.
  const words = eventType.replace(/_/g, " ").trim();
  return words.length > 0 ? words.charAt(0).toUpperCase() + words.slice(1) : "Aktivitet";
}

// The audit log persists its detail messages in English ("Created customer
// ...", "Rendered invoice PDF ...", "Company volume initialized"). The event
// headings are already translated (#233), but the detail text below each one
// still leaked English onto the Danish-facing dashboard. The patterns below
// translate each known message template to plain Danish, preserving the
// variable part (customer name, invoice number, ...) verbatim. An unknown
// message falls through untouched so no information is ever lost. (#286)
const ACTIVITY_MESSAGE_PATTERNS: Array<{ re: RegExp; da: (m: RegExpMatchArray) => string }> = [
  { re: /^Company volume initialized$/, da: () => "Virksomhed oprettet" },
  { re: /^Created customer (.+)$/s, da: (m) => `Kunde oprettet: ${m[1]}` },
  { re: /^Created vendor (.+)$/s, da: (m) => `Leverandør oprettet: ${m[1]}` },
  { re: /^Created full backup (.+)$/s, da: (m) => `Fuld backup oprettet: ${m[1]}` },
  { re: /^Created recurring invoice template (.+)$/s, da: (m) => `Fakturaskabelon oprettet: ${m[1]}` },
  { re: /^Re-rendered invoice PDF (.+)$/s, da: (m) => `Faktura-PDF gendannet: ${m[1]}` },
  { re: /^Rendered invoice PDF (.+)$/s, da: (m) => `Faktura-PDF genereret: ${m[1]}` },
  { re: /^Ingested supporting document (\S+) \((.+)\)$/s, da: (m) => `Bilag ${m[1]} indlæst (${m[2]})` },
  { re: /^Ingested supporting document (.+)$/s, da: (m) => `Bilag ${m[1]} indlæst` },
  { re: /^Issued invoice (.+)$/s, da: (m) => `Faktura udstedt: ${m[1]}` },
  { re: /^Issued credit note (.+?) for (.+)$/s, da: (m) => `Kreditnota ${m[1]} udstedt for ${m[2]}` },
  { re: /^Posted journal entry (.+)$/s, da: (m) => `Finanspostering bogført: ${m[1]}` },
  { re: /^Reversed journal entry (.+?) with (.+)$/s, da: (m) => `Finanspostering ${m[1]} tilbageført med ${m[2]}` },
  { re: /^Added bank account (.+)$/s, da: (m) => `Bankkonto oprettet: ${m[1]}` },
  { re: /^Imported (\d+) bank transactions from (.+)$/s, da: (m) => `${m[1]} banktransaktioner importeret fra ${m[2]}` },
  { re: /^Applied payment (.+?) to invoice (.+)$/s, da: (m) => `Betaling ${m[1]} registreret på faktura ${m[2]}` },
  { re: /^Applied refund (.+?) to invoice (.+)$/s, da: (m) => `Refundering ${m[1]} bogført på faktura ${m[2]}` },
  { re: /^Applied claim receipt (.+?) to invoice (.+?) via combined settlement$/s, da: (m) => `Indbetaling på krav ${m[1]} registreret på faktura ${m[2]} via samlet afregning` },
  { re: /^Applied claim receipt (.+?) to invoice (.+)$/s, da: (m) => `Indbetaling på krav ${m[1]} registreret på faktura ${m[2]}` },
  { re: /^Wrote off bad debt (.+?) on invoice (.+)$/s, da: (m) => `Tab på debitor ${m[1]} bogført på faktura ${m[2]}` },
  { re: /^Registered asset (.+)$/s, da: (m) => `Aktiv registreret: ${m[1]}` },
  { re: /^Posted opening balance \(primobalance\) pr\. (.+?) as (.+)$/s, da: (m) => `Primobalance pr. ${m[1]} bogført som ${m[2]}` },
  { re: /^Restored from backup (.+)$/s, da: (m) => `Gendannet fra backup ${m[1]}` },
];

/**
 * Render an audit-log detail message in plain Danish. The audit log itself
 * stores English templates (immutable history); the dashboard translates them
 * for display only. Unknown messages pass through unchanged. (#286)
 */
export function activityMessageDanish(message: string): string {
  const text = message ?? "";
  for (const { re, da } of ACTIVITY_MESSAGE_PATTERNS) {
    const m = re.exec(text);
    if (m) return da(m);
  }
  return text;
}

export function activityList(rows: AuditLogRow[]): string {
  if (rows.length === 0) {
    return `<div class="empty-state">Ingen aktivitet endnu</div>`;
  }
  const items = rows.slice(0, 10).map((row) =>
    `  <div class="time">${escapeHtml(formatTimestampShort(row.createdAt))}</div>
  <div class="actor">${escapeHtml(row.actor)}</div>
  <div class="event">${escapeHtml(activityEventLabel(row.eventType))}</div>
  <div class="message">${escapeHtml(activityMessageDanish(row.message))}</div>`
  ).join("\n");
  return `<div class="activity-log">
${items}
</div>`;
}
