import { invoiceStatusDa, FALLBACK_UKENDT } from "../core/messages";
import {
  asArray,
  asCount,
  asText,
  appendWarnings,
  buildHumanError,
  asStringArray,
  formatKroner,
  formatPercent,
} from "./common";

// --- Invoice status --------------------------------------------------------

export function renderInvoiceStatus(result: Record<string, unknown>): string {
  const lines: string[] = [];
  const number = asText(result.invoiceNumber);
  lines.push(`Status for faktura ${number}`);
  lines.push("");

  const statusCode = typeof result.status === "string" ? result.status : "";
  const statusDa = invoiceStatusDa(statusCode) ?? FALLBACK_UKENDT;
  const open = typeof result.openBalance === "number" ? result.openBalance : Number(result.openBalance);

  if (statusCode === "paid") {
    lines.push("Fakturaen er fuldt betalt.");
  } else if (statusCode === "credited") {
    lines.push("Fakturaen er krediteret.");
  } else if (statusCode === "refunded") {
    lines.push("Fakturaen er refunderet.");
  } else if (statusCode === "written_off") {
    lines.push("Fakturaen er afskrevet som tab på debitorer.");
  } else if (statusCode === "overpaid") {
    lines.push(`Fakturaen er overbetalt med ${formatKroner(Number.isFinite(open) ? -open : open)}.`);
  } else if (Number.isFinite(open) && open > 0) {
    lines.push(`Der mangler at blive betalt ${formatKroner(open)} på fakturaen.`);
  } else {
    lines.push(`Fakturaen er ${statusDa}.`);
  }

  if (result.isOverdue === true) {
    const days = asCount(result.overdueDays);
    lines.push(`Fakturaen er forfalden — ${days} dage over forfaldsdato.`);
  }

  // One decisive bottom-line: an invoice may be part-paid, part-credited and
  // part-refunded at once, and the owner cannot read "still owes anything" off
  // four separate amounts. claimOpenBalance is the total still outstanding,
  // including reminder fees / interest / compensation. (#233)
  const claimOpen =
    typeof result.claimOpenBalance === "number"
      ? result.claimOpenBalance
      : Number(result.claimOpenBalance);
  const outstanding = Number.isFinite(claimOpen) ? claimOpen : open;
  if (Number.isFinite(outstanding) && outstanding > 0) {
    lines.push(`→ Udestående i alt: ${formatKroner(outstanding)} — kunden skylder stadig.`);
  } else if (Number.isFinite(outstanding) && outstanding < 0) {
    lines.push(`→ Afsluttet — kunden har ${formatKroner(-outstanding)} til gode.`);
  } else {
    lines.push("→ Afsluttet — intet udestående.");
  }

  lines.push("");
  lines.push(`  Fakturabeløb (inkl. moms):     ${formatKroner(result.grossAmount)}`);
  if (asCount(result.creditedAmount) !== 0) {
    lines.push(`  Krediteret:                    ${formatKroner(result.creditedAmount)}`);
  }
  lines.push(`  Betalt:                        ${formatKroner(result.paidAmount)}`);
  lines.push(`  Åben saldo:                    ${formatKroner(result.openBalance)}`);
  if (asCount(result.claimOpenBalance) !== 0) {
    lines.push(`  Åben saldo på krav (gebyrer):  ${formatKroner(result.claimOpenBalance)}`);
  }
  lines.push("");
  lines.push(`  Status:                        ${statusDa}`);
  if (result.effectiveDueDate) {
    lines.push(`  Forfaldsdato:                  ${asText(result.effectiveDueDate)}`);
  }
  if (result.asOfDate) {
    lines.push(`  Opgjort pr.:                   ${asText(result.asOfDate)}`);
  }

  const payments = asArray(result.payments);
  if (payments.length > 0) {
    lines.push("");
    lines.push(`Indbetalinger (${payments.length}):`);
    for (const payment of payments) {
      lines.push(
        `  - ${asText(payment.paymentDate)}: ${formatKroner(payment.amount)}` +
          (payment.note ? ` (${asText(payment.note)})` : ""),
      );
    }
  }

  const creditNotes = asArray(result.creditNotes);
  if (creditNotes.length > 0) {
    lines.push("");
    lines.push(`Kreditnotaer (${creditNotes.length}):`);
    for (const note of creditNotes) {
      lines.push(
        `  - ${asText(note.creditNoteNumber)} (${asText(note.issueDate)}): ${formatKroner(note.amount)}`,
      );
    }
  }

  const reminders = asArray(result.reminders);
  if (reminders.length > 0) {
    lines.push("");
    lines.push(`Rykkere (${reminders.length}):`);
    for (const reminder of reminders) {
      lines.push(
        `  - ${asText(reminder.reminderDate)}: gebyr ${formatKroner(reminder.feeAmount)}`,
      );
    }
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Invoice late interest (morarente) -------------------------------------

/**
 * Render an `invoice interest` / `invoice claim-interest` payload as Danish
 * human text. The payload is the `CalculateInvoiceLateInterestResult` from
 * `calculateInvoiceLateInterest` (optionally with the `claimId` /
 * `claimOpenBalance` registration fields). Shows the reference rate, the
 * statutory annual rate, the overdue window and the computed interest amount
 * — the figures that previously only appeared under `--format json`. (#250)
 */
export function renderInvoiceInterest(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`Morarente for faktura ${asText(result.invoiceNumber)}`);
  lines.push("");

  const overdueDays = asCount(result.overdueDays);
  const interest =
    typeof result.accruedInterestAmount === "number"
      ? result.accruedInterestAmount
      : Number(result.accruedInterestAmount);
  const registered = result.claimId != null;

  if (overdueDays > 0 && Number.isFinite(interest) && interest > 0) {
    lines.push(
      `Fakturaen er ${overdueDays} dage forfalden — der er påløbet ${formatKroner(interest)} i morarente.`,
    );
  } else if (overdueDays > 0) {
    lines.push(
      `Fakturaen er ${overdueDays} dage forfalden, men der er ingen morarente at opkræve` +
        " (ingen åben saldo at forrente).",
    );
  } else {
    lines.push("Fakturaen er ikke forfalden — der er ingen morarente at opkræve.");
  }
  lines.push("");

  lines.push(`  Forfaldsdato:                  ${asText(result.effectiveDueDate)}`);
  lines.push(`  Opgjort pr.:                   ${asText(result.asOfDate)}`);
  lines.push(`  Antal forfaldne dage:          ${overdueDays}`);
  lines.push(`  Åben hovedstol:                ${formatKroner(result.principalOpenBalance)}`);
  lines.push(`  Referencesats (Nationalbanken): ${formatPercent(result.referenceRatePercent)}`);
  lines.push(
    `  Morarentesats (reference + 8 %): ${formatPercent(result.annualInterestRatePercent)}`,
  );
  lines.push(`  Påløbet morarente:             ${formatKroner(result.accruedInterestAmount)}`);

  if (registered) {
    lines.push("");
    lines.push(`Morarentekravet er registreret (krav-id ${asText(result.claimId)}).`);
    if (result.claimDate) {
      lines.push(`  Kravdato:                      ${asText(result.claimDate)}`);
    }
    if (result.claimOpenBalance !== undefined) {
      lines.push(`  Åben saldo på krav:            ${formatKroner(result.claimOpenBalance)}`);
    }
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

// --- Invoice late compensation (kompensationsbeløb) ------------------------

/**
 * Render an `invoice compensation` / `invoice claim-compensation` payload as
 * Danish human text. The payload is the `CalculateInvoiceLateCompensationResult`
 * from `calculateInvoiceLateCompensation` (optionally with the registration
 * fields). Shows whether the invoice is eligible for the statutory fixed
 * compensation for late commercial payment, the amount and a clear reason
 * — figures that previously only appeared under `--format json`. (#250)
 */
export function renderInvoiceCompensation(result: Record<string, unknown>): string {
  const lines: string[] = [];
  lines.push(`Kompensation for sen betaling — faktura ${asText(result.invoiceNumber)}`);
  lines.push("");

  const eligible = result.eligible === true;
  const amount =
    typeof result.compensationAmountDkk === "number"
      ? result.compensationAmountDkk
      : Number(result.compensationAmountDkk);
  const registered = result.claimId != null;

  if (eligible && Number.isFinite(amount) && amount > 0) {
    lines.push(
      `Fakturaen er berettiget til ${formatKroner(amount)} i fast kompensation for sen betaling.`,
    );
  } else {
    lines.push("Fakturaen er IKKE berettiget til kompensation for sen betaling.");
    lines.push(`  Årsag: ${reasonInDanish(result.reason)}`);
  }
  lines.push("");

  lines.push(`  Forfaldsdato:                  ${asText(result.effectiveDueDate)}`);
  lines.push(`  Opgjort pr.:                   ${asText(result.asOfDate)}`);
  lines.push(`  Antal forfaldne dage:          ${asCount(result.overdueDays)}`);
  lines.push(`  Åben hovedstol:                ${formatKroner(result.principalOpenBalance)}`);
  lines.push(
    `  Erhvervshandel (CVR/VAT):      ${result.isCommercialTransaction === true ? "ja" : "nej"}`,
  );
  lines.push(`  Berettiget:                    ${eligible ? "ja" : "nej"}`);
  lines.push(`  Kompensationsbeløb:            ${formatKroner(result.compensationAmountDkk)}`);

  if (registered) {
    lines.push("");
    lines.push(`Kompensationskravet er registreret (krav-id ${asText(result.claimId)}).`);
    if (result.claimDate) {
      lines.push(`  Kravdato:                      ${asText(result.claimDate)}`);
    }
    if (result.claimOpenBalance !== undefined) {
      lines.push(`  Åben saldo på krav:            ${formatKroner(result.claimOpenBalance)}`);
    }
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

/**
 * Translate the machine-readable `reason` from the compensation core into a
 * plain-Danish explanation for the human renderer. Falls back to the raw
 * reason string for any reason without a dedicated translation.
 */
function reasonInDanish(reason: unknown): string {
  const text = typeof reason === "string" ? reason : "";
  if (text === "eligible" || text === "registered") {
    return "fakturaen er berettiget";
  }
  if (/commercial transaction not proven/i.test(text)) {
    return "købers CVR/VAT-nummer mangler — handlen kan ikke dokumenteres som erhvervshandel";
  }
  if (/no collectible open balance/i.test(text)) {
    return "fakturaen har ingen åben saldo at opkræve kompensation for";
  }
  if (/not overdue/i.test(text)) {
    return "fakturaen er ikke forfalden på den valgte dato";
  }
  if (/predates statutory compensation start date/i.test(text)) {
    return "fakturaen ligger før den lovbestemte ikrafttrædelsesdato for kompensation (1. marts 2013)";
  }
  return text.length > 0 ? text : "ukendt";
}

// --- Invoice payload validation --------------------------------------------

/**
 * Render an `invoice validate` payload as Danish human text. The payload is
 * the `InvoiceValidationResult` from `validateInvoice`. Both verdicts are
 * rendered in full: a valid payload states it plainly, an invalid payload
 * lists every concrete rejection reason — instead of a blank command
 * description with no result. (#250)
 */
export function renderInvoiceValidate(result: Record<string, unknown>): string {
  const lines: string[] = [];
  const valid = result.ok !== false;
  lines.push("Fakturavalidering");
  lines.push("");

  if (valid) {
    lines.push("Fakturaen er gyldig og kan udstedes.");
  } else {
    const errors = asStringArray(result.errors);
    lines.push(
      `Fakturaen er IKKE gyldig — ${errors.length} ${errors.length === 1 ? "fejl" : "fejl"} skal rettes:`,
    );
    if (errors.length === 0) {
      lines.push("  → Valideringen fejlede uden en specifik fejlbesked.");
    } else {
      for (const error of errors) lines.push(`  → ${error}`);
    }
  }
  lines.push("");

  const invoiceTypeDa =
    result.invoiceType === "simplified"
      ? "forenklet faktura"
      : result.invoiceType === "full"
        ? "fuld faktura"
        : asText(result.invoiceType);
  const vatTreatmentDa = VAT_TREATMENT_DA[String(result.vatTreatment)] ?? asText(result.vatTreatment);
  lines.push(`  Fakturatype:                   ${invoiceTypeDa}`);
  lines.push(`  Momsbehandling:                ${vatTreatmentDa}`);

  const rules = asStringArray(result.appliedRules);
  if (rules.length > 0) {
    lines.push(`  Anvendte regler:               ${rules.join(", ")}`);
  }
  appendWarnings(lines, result);
  return lines.join("\n");
}

/** Danish labels for the invoice VAT treatments. */
const VAT_TREATMENT_DA: Record<string, string> = {
  standard: "standardmoms",
  domestic_reverse_charge: "omvendt betalingspligt (indenlandsk)",
  foreign_reverse_charge: "omvendt betalingspligt (udenlandsk)",
};

// --- Invoice create (write command, guided path) ---------------------------

/**
 * Render an `invoice create` result as Danish human text. The payload is the
 * `IssueInvoiceResult` enriched with `computed` (line totals, net, VAT, gross)
 * from `src/cli/invoice.ts`. (#266, #268)
 *
 * Two bugs are addressed here:
 *  - #268: the write command used to fall through to the generic renderer,
 *    which printed the long command DESCRIPTION as the heading and English
 *    field labels ("Invoice Number", "Document Id") and dropped the figures.
 *    This renderer gives a short Danish heading, Danish labels and the
 *    amounts/VAT/dates/paths that matter.
 *  - #266: `invoice create` builds and issues the invoice but does NOT post it
 *    to the ledger — no revenue / output-VAT posting. The output must make
 *    that gap impossible to miss, so it states plainly that `invoice post` is
 *    still required.
 */
export function renderInvoiceCreate(result: Record<string, unknown>): string {
  if (result.ok === false) {
    return buildHumanError("Faktura kunne ikke udstedes", result).join("\n");
  }
  const computed = (typeof result.computed === "object" && result.computed !== null
    ? result.computed
    : {}) as Record<string, unknown>;
  const number = asText(result.invoiceNumber);
  const lines: string[] = [];
  lines.push(`Faktura ${number} er udstedt`);
  lines.push("");

  lines.push(`  Fakturanummer:                 ${number}`);
  if (result.documentId != null) {
    lines.push(`  Bilags-id:                     ${asText(result.documentId)}`);
  }
  lines.push(`  Nettobeløb (ekskl. moms):      ${formatKroner(computed.netAmount)}`);
  const ratePercent = computed.vatRatePercent ?? computed.vatRate;
  if (ratePercent != null) {
    const pct =
      typeof computed.vatRatePercent === "number"
        ? computed.vatRatePercent
        : typeof computed.vatRate === "number"
          ? computed.vatRate * 100
          : Number(ratePercent);
    lines.push(`  Momssats:                      ${formatPercent(pct)}`);
  }
  lines.push(`  Momsbeløb:                     ${formatKroner(computed.vatAmount)}`);
  lines.push(`  Fakturabeløb (inkl. moms):     ${formatKroner(computed.grossAmount)}`);
  if (result.storedPath) {
    lines.push(`  Gemt faktura (JSON):           ${asText(result.storedPath)}`);
  }
  if (result.pdfStoredPath) {
    lines.push(`  Faktura-PDF:                   ${asText(result.pdfStoredPath)}`);
  }
  lines.push("");
  // #266: the decisive line — the invoice is issued but NOT yet booked.
  lines.push(
    `Fakturaen er udstedt, men endnu IKKE bogført — kør \`invoice post ${number}\``,
  );
  lines.push(
    "for at få den i regnskabet (salgsindtægt og udgående moms). Indtil da indgår",
  );
  lines.push("den ikke i din moms eller dit resultat.");
  appendWarnings(lines, result);
  return lines.join("\n");
}
