import type { Database } from "bun:sqlite";
import { postJournalEntry, postJournalEntryInCurrentTransaction, type JournalPostResult } from "./ledger";
import { getCompanySettings } from "./company";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { requireCachedViesValidation, normalizeEuVatNumber } from "./vies";
import { addDkk, compareDkk, fromOre, percentOfDkk, roundDkk, subtractDkk, sumDkk, toOre } from "./money";
import { resolveAccountRole } from "./account-roles";
import { emptyVatRubric, projectVatRubric, type VatRubric } from "./vat-rubric";
import {
  loadVatAccountSemantics,
  VAT_LINE_CODES,
} from "./vat-account-semantics";
import {
  HISTORICAL_IMPORT_PROGRAM,
  isPersistedHistoricalImportProgram,
} from "./import-provenance";
import {
  inferLegacyVatEvidence,
  type LegacyVatEvidenceRow,
} from "./vat-legacy-evidence";
import { deductibleDanishPurchaseSupplierErrors, resolvePersistedSupplierIdentity, type SupplierIdentityResolution } from "./supplier-identity";
import { parsePurchaseVatLinesPayload } from "./documents";
import { vatPeriodWindowFor, type VatPeriodType } from "./periods";

// Backward-compatible export; the implementation lives in the canonical
// cadence engine (`periods.ts`). New callers should import it there.
export { vatFilingDeadline } from "./periods";

/** Absolute difference between two DKK amounts, expressed in whole øre. */
function oreDifference(left: number, right: number): number {
  const delta = toOre(left) - toOre(right);
  return Number(delta < 0n ? -delta : delta);
}

export type VatPeriodReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  /** Registered cadence, null for a non-registered company. */
  vatPeriodType: VatPeriodType | null;
  /** Deadline only when the requested bounds equal one canonical VAT period. */
  filingDeadline: string | null;
  outputVat: number;
  inputVat: number;
  netVatPayable: number;
  purchaseBase25: number;
  salesBase25: number;
  /**
   * Combined value of all VAT-exempt reverse-charge sales (domestic + foreign).
   * Kept for backwards compatibility and human display ("Salg med omvendt
   * betalingspligt"). For the momsangivelse the two must be split — see
   * foreignReverseChargeSalesBase / domesticReverseChargeSalesBase below.
   */
  reverseChargeSalesBase: number;
  /**
   * JUR-2/KODE-2: value of FOREIGN reverse-charge sales — cross-border EU B2B
   * sales without Danish VAT (vat_code REVERSE_CHARGE_EXEMPT). These feed rubrik
   * B of the momsangivelse and are cross-checked against the EU sales list (VIES).
   */
  foreignReverseChargeSalesBase: number;
  /**
   * JUR-2/KODE-2: value of DOMESTIC reverse-charge sales — Danish §46 omvendt
   * betalingspligt (mobiltelefoner, CPU'er, metalskrot; vat_code
   * DOMESTIC_REVERSE_CHARGE_EXEMPT). These carry no Danish output VAT and feed
   * rubrik C ("værdi af andet salg uden moms"), NOT rubrik B, and must never
   * appear on the VIES EU sales list. Source: SKAT Den juridiske vejledning
   * A.B.3.3.1.5.
   */
  domesticReverseChargeSalesBase: number;
  /** EU service reverse-charge purchase base only. This feeds rubrik A;
   * non-EU services are kept separate below. */
  reverseChargePurchaseBase: number;
  /** EU goods acquisitions (§11), kept separate from service purchases (§46). */
  euGoodsAcquisitionPurchaseBase: number;
  /** Output VAT arising from §11 goods acquisitions, for the goods rubric. */
  euGoodsAcquisitionOutputVat: number;
  /** Service-purchase base from suppliers outside the EU. It contributes to
   * foreign-service reverse-charge VAT, but never to EU-only rubrik A. */
  nonEuServiceReverseChargePurchaseBase: number;
  /**
   * Output VAT allocated to foreign-service reverse charge (EU and non-EU).
   * A dedicated, source-proven VAT account supplies its booked amount; ledgers
   * where ordinary and reverse-charge output VAT share one account use the
   * øre-rounded VAT expected from each reverse-charge base line. The report
   * reconciles this category independently before it can be filed.
   */
  reverseChargePurchaseOutputVat: number;
  representationPurchaseBase: number;
  badDebtReliefBase25: number;
  /**
   * Value of VAT-exempt domestic sales (momsloven §13 — vat_code
   * DK_SALE_EXEMPT). These carry no output VAT and feed rubrik C of the
   * momsangivelse; they are kept out of the standard 25% sales base.
   */
  exemptSalesBase: number;
  /**
   * Value of digital-service sales to EU consumers handled under the OSS
   * scheme (vat_code OSS_EU_CONSUMER). These carry no Danish output VAT and
   * are reported via a separate OSS return — they are kept out of every
   * standard momsangivelse rubrik. See src/core/vat-oss.ts.
   */
  ossConsumerSalesBase: number;
  /** Number of journal entries that include an OSS_EU_CONSUMER line. */
  ossConsumerSalesEntryCount: number;
  journalEntryCount: number;
  reversedJournalEntryCount: number;
  reversalJournalEntryCount: number;
  totalJournalEntryCount: number;
  linesConsidered: number;
  reversedLinesConsidered: number;
  reversalLinesConsidered: number;
  totalLinesConsidered: number;
  /** Canonical SKAT rubric projection shared by report, filing and exports. */
  rubrikker: VatRubric;
  warnings: string[];
  errors: string[];
};

const RULE_ID = "DK-VAT-REPORT-001";
const REVERSE_CHARGE_RULE_ID = "DK-VAT-REVERSE-CHARGE-001";
const NON_EU_REVERSE_CHARGE_RULE_ID = "DK-VAT-NON-EU-SERVICE-REVERSE-CHARGE-001";
const REPRESENTATION_RULE_ID = "DK-VAT-REPRESENTATION-001";
const REGISTRATION_RULE_ID = "DK-VAT-REGISTRATION-001";

/**
 * True when the single company row is VAT-registered (a cadence is set). A
 * null `vat_period_type` marks a NOT VAT-registered company (Momsloven § 47 /
 * § 48). Reads through `getCompanySettings`, the canonical reader, so the
 * notion of "registered" is byte-identical to every other surface.
 */
function companyIsVatRegistered(db: Database): boolean {
  return getCompanySettings(db).vatPeriodType !== null;
}

// EU service reverse charge for a NOT VAT-registered company is NOT a deductible
// net-zero booking: under Momsloven § 46, stk. 1, nr. 3, jf. § 50 b the buyer
// becomes separately liable to register and pay Danish erhvervelsesmoms "fra
// første krone" with NO input-VAT deduction (§ 37). That registration + filing
// flow is out of scope (parallel to the § 11 EU-goods erhvervelsesmoms
// limitation), so we must refuse rather than silently book a forbidden 4000
// deduction (or silently absorb VAT the company actually owes SKAT).
const NON_REGISTERED_EU_SERVICE_MSG =
  "selskabet er ikke momsregistreret — EU-ydelseskøb med omvendt betalingspligt udløser særskilt registreringspligt for erhvervelsesmoms (momsloven § 46, stk. 1, nr. 3, jf. § 50 b) fra første krone uden fradrag (§ 37); det håndterer Rentemester ikke endnu. Registrér selskabet for erhvervelsesmoms og søg rådgivning — bogfør ikke købet som reverse charge her";
const NON_REGISTERED_NON_EU_SERVICE_MSG =
  "selskabet er ikke momsregistreret — ydelseskøb fra en leverandør uden for EU med omvendt betalingspligt kræver særskilt momsbehandling uden automatisk fradrag; det håndterer Rentemester ikke endnu. Registrér selskabet og søg rådgivning før bogføring";

// Representation for a NOT VAT-registered company: the § 42 partial deduction
// is a registered-business relief, and § 37 grants no deduction at all, so the
// full VAT is simply a cost. The correct booking is gross absorption via
// `expense book --vat-treatment non_deductible`, not this partial-deduction path.
const NON_REGISTERED_REPRESENTATION_MSG =
  "selskabet er ikke momsregistreret — repræsentationsmoms kan ikke fradrages (momsloven § 37); bogfør bilaget brutto med 'expense book --vat-treatment non_deductible', så hele momsen absorberes i udgiften";

export type ReverseChargePurchaseInput = {
  transactionDate: string;
  text: string;
  documentId: number;
  netAmount: number;
  expenseAccountNo: string;
  paymentAccountNo?: string;
  sourceBankTransactionId?: number;
  currency?: string;
  amountForeign?: number;
  amountDkk?: number;
  fxRateToDkk?: number;
  createdBy?: string;
  createdByProgram?: string;
};

export type RepresentationPurchaseInput = {
  transactionDate: string;
  text: string;
  documentId: number;
  netAmount: number;
  expenseAccountNo?: string;
  paymentAccountNo?: string;
  sourceBankTransactionId?: number;
  currency?: string;
  amountForeign?: number;
  amountDkk?: number;
  fxRateToDkk?: number;
  createdBy?: string;
  createdByProgram?: string;
};

function reverseChargeInputErrors(input: ReverseChargePurchaseInput): string[] {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (typeof input.text !== "string" || input.text.trim().length === 0) errors.push("text is required");
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isFinite(input.netAmount) || input.netAmount <= 0) errors.push("netAmount must be a positive number");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  return errors;
}

function postServiceReverseChargeLines(
  db: Database,
  input: ReverseChargePurchaseInput,
  vatCode: "EU_SERVICE_REVERSE_CHARGE" | "NON_EU_SERVICE_REVERSE_CHARGE",
  ruleId: string,
  sourceLabel: string,
  inCurrentTransaction = false,
): JournalPostResult {
  const vatAmount = percentOfDkk(input.netAmount, 25);
  const inputVat = resolveAccountRole(db, "input_vat");
  const outputVat = resolveAccountRole(db, "reverse_charge_vat");
  const bank = input.paymentAccountNo ? { ok: true as const, accountNo: input.paymentAccountNo } : resolveAccountRole(db, "bank");
  if (!inputVat.ok) return { ok: false, appliedRules: [ruleId], errors: [inputVat.error] };
  if (!outputVat.ok) return { ok: false, appliedRules: [ruleId], errors: [outputVat.error] };
  if (!bank.ok) return { ok: false, appliedRules: [ruleId], errors: [bank.error] };
  const post = inCurrentTransaction ? postJournalEntryInCurrentTransaction : postJournalEntry;
  const result = post(db, {
    transactionDate: input.transactionDate,
    text: input.text.trim(),
    documentId: input.documentId,
    sourceBankTransactionId: input.sourceBankTransactionId,
    currency: input.currency,
    amountForeign: input.amountForeign,
    amountDkk: input.amountDkk,
    fxRateToDkk: input.fxRateToDkk,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
    lines: [
      { accountNo: input.expenseAccountNo, debitAmount: roundDkk(input.netAmount), vatCode, text: `${sourceLabel} service purchase base` },
      { accountNo: inputVat.accountNo, debitAmount: vatAmount, text: "Deductible reverse-charge input VAT" },
      { accountNo: bank.accountNo, creditAmount: roundDkk(input.netAmount), text: "Payment / liability" },
      { accountNo: outputVat.accountNo, creditAmount: vatAmount, text: "Reverse-charge output VAT" },
    ],
  });
  return {
    ...result,
    appliedRules: result.ok ? [...new Set([...(result.appliedRules ?? []), ruleId])] : [...new Set([ruleId, ...(result.appliedRules ?? [])])],
  };
}

/** §11 acquisition VAT for goods bought from another EU member state. */
function postEuGoodsAcquisitionPurchaseInternal(db: Database, input: ReverseChargePurchaseInput, inCurrentTransaction: boolean): JournalPostResult {
  const errors = reverseChargeInputErrors(input);
  if (errors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors };
  if (!companyIsVatRegistered(db)) return { ok: false, appliedRules: [REGISTRATION_RULE_ID, REVERSE_CHARGE_RULE_ID], errors: [NON_REGISTERED_EU_SERVICE_MSG] };
  const identity = resolveDocumentSupplierIdentity(db, input.documentId);
  if (!identity?.ok || identity.identifierKind !== "eu_vat") return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: ["EU goods acquisition requires documented non-Danish EU supplier VAT identity"] };
  const viesCheck = requireCachedViesValidation(db, identity.identifier, "document sender_vat_cvr");
  if (!viesCheck.ok) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID, ...viesCheck.appliedRules], errors: viesCheck.errors };
  const vatAmount = percentOfDkk(input.netAmount, 25);
  const inputVat = resolveAccountRole(db, "input_vat");
  const outputVat = resolveAccountRole(db, "reverse_charge_vat");
  const bank = input.paymentAccountNo
    ? { ok: true as const, accountNo: input.paymentAccountNo }
    : resolveAccountRole(db, "bank");
  if (!inputVat.ok) {
    return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [inputVat.error] };
  }
  if (!outputVat.ok) {
    return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [outputVat.error] };
  }
  if (!bank.ok) {
    return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [bank.error] };
  }
  const post = inCurrentTransaction ? postJournalEntryInCurrentTransaction : postJournalEntry;
  const result = post(db, { transactionDate: input.transactionDate, text: input.text, documentId: input.documentId, sourceBankTransactionId: input.sourceBankTransactionId, createdBy: input.createdBy, createdByProgram: input.createdByProgram, lines: [
    { accountNo: input.expenseAccountNo, debitAmount: roundDkk(input.netAmount), vatCode: "EU_GOODS_ACQUISITION", text: "EU goods acquisition base" },
    { accountNo: inputVat.accountNo, debitAmount: vatAmount, text: "Deductible acquisition input VAT" },
    { accountNo: bank.accountNo, creditAmount: roundDkk(input.netAmount), text: "Payment / liability" },
    { accountNo: outputVat.accountNo, creditAmount: vatAmount, text: "Acquisition output VAT" },
  ] });
  return { ...result, appliedRules: [...new Set([REVERSE_CHARGE_RULE_ID, ...result.appliedRules])] };
}

export function postEuGoodsAcquisitionPurchaseInCurrentTransaction(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return postEuGoodsAcquisitionPurchaseInternal(db, input, true);
}

export function postEuGoodsAcquisitionPurchase(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return db.transaction(() => postEuGoodsAcquisitionPurchaseInternal(db, input, true)).immediate();
}

function resolveDocumentSupplierIdentity(
  db: Database,
  documentId: number,
): SupplierIdentityResolution | null {
  const row = db.query(
    `SELECT sender_vat_cvr, supplier_country_code, supplier_identifier_kind, supplier_identity_status
       FROM documents
      WHERE id = ?`,
  ).get(documentId) as {
    sender_vat_cvr: string | null;
    supplier_country_code: string | null;
    supplier_identifier_kind: string | null;
    supplier_identity_status: string | null;
  } | null;
  if (!row) return null;
  return resolvePersistedSupplierIdentity({
    supplierVatOrCvr: row.sender_vat_cvr,
    supplierCountryCode: row.supplier_country_code,
    supplierIdentifierKind: row.supplier_identifier_kind,
    supplierIdentityStatus: row.supplier_identity_status,
  });
}

function unsupportedStructuredPurchaseLinesErrors(
  db: Database,
  documentId: number,
  treatment: "reverse_charge" | "representation",
): string[] {
  const row = db.query(
    `SELECT amount_inc_vat, vat_amount, payload_json
       FROM documents
      WHERE id = ?`,
  ).get(documentId) as {
    amount_inc_vat: number | null;
    vat_amount: number | null;
    payload_json: string | null;
  } | null;
  if (!row) return [`documentId ${documentId} does not exist`];
  const parsed = parsePurchaseVatLinesPayload(row.payload_json, {
    amountIncVat: row.amount_inc_vat,
    vatAmount: row.vat_amount,
  });
  if (parsed.status === "invalid") {
    return parsed.errors.map((error) => `document ${documentId} has invalid persisted purchaseVatLines: ${error}`);
  }
  if (parsed.status === "valid") {
    return [`${treatment} purchase posting does not support structured purchaseVatLines; use a dedicated unsplit document or human resolution`];
  }
  return [];
}

function documentDanishInputVatSupplierErrors(db: Database, documentId: number): string[] {
  const row = db.query(
    `SELECT sender_vat_cvr, supplier_country_code, supplier_identifier_kind, supplier_identity_status
       FROM documents
      WHERE id = ?`,
  ).get(documentId) as {
    sender_vat_cvr: string | null;
    supplier_country_code: string | null;
    supplier_identifier_kind: string | null;
    supplier_identity_status: string | null;
  } | null;
  if (!row) return [`documentId ${documentId} does not exist`];
  return deductibleDanishPurchaseSupplierErrors({
    supplierVatOrCvr: row.sender_vat_cvr,
    supplierCountryCode: row.supplier_country_code,
    supplierIdentifierKind: row.supplier_identifier_kind,
    supplierIdentityStatus: row.supplier_identity_status,
  });
}

/**
 * A non-EU invoice may be ingested without an EU VAT identifier (#529), but
 * automatic input-VAT deduction is a separate, higher-evidence decision. The
 * invoice must retain the supplier's home-country registration number, the
 * Danish buyer VAT identifier, and explicit reverse-charge wording. Otherwise
 * the purchase remains available for human resolution without creating a VAT
 * journal entry.
 */
/** Shared documentary gate for both the read-only purchase preflight and
 * posting. Keep this source-of-truth here: preflight must never present a
 * green purchase-eligibility result that booking will immediately reject. */
export function nonEuReverseChargeEvidenceErrors(db: Database, documentId: number): string[] {
  const row = db.query(
    `SELECT sender_vat_cvr, recipient_vat_cvr, payload_json
       FROM documents
      WHERE id = ?`,
  ).get(documentId) as {
    sender_vat_cvr: string | null;
    recipient_vat_cvr: string | null;
    payload_json: string | null;
  } | null;
  if (!row) return [`documentId ${documentId} does not exist`];

  const errors: string[] = [];
  if (!row.sender_vat_cvr?.trim()) {
    errors.push("non-EU reverse-charge input-VAT deduction requires the supplier's home-country registration number on the invoice");
  }
  const buyerVat = row.recipient_vat_cvr?.trim().toUpperCase().replace(/\s/g, "") ?? "";
  if (!/^DK\d{8}$/.test(buyerVat)) {
    errors.push("non-EU reverse-charge input-VAT deduction requires the Danish buyer VAT identifier (DK + 8 digits) on the invoice");
  } else {
    let configuredCompanyVat: string | null = null;
    try {
      configuredCompanyVat = getCompanySettings(db).cvr;
    } catch {
      configuredCompanyVat = null;
    }
    if (!configuredCompanyVat) {
      errors.push("non-EU reverse-charge input-VAT deduction requires the ledger company's own CVR/VAT identifier to be configured");
    } else if (buyerVat !== configuredCompanyVat) {
      errors.push(`invoice buyer VAT identifier ${buyerVat} does not match the ledger company's configured VAT identifier ${configuredCompanyVat}`);
    }
  }
  let wordingConfirmed = false;
  try {
    const payload = row.payload_json ? JSON.parse(row.payload_json) as unknown : null;
    const record = payload && typeof payload === "object" && !Array.isArray(payload)
      ? payload as Record<string, unknown>
      : null;
    const evidence = record?.reverseChargeWordingEvidence;
    const sourceEvidence = evidence && typeof evidence === "object" && !Array.isArray(evidence)
      ? evidence as Record<string, unknown>
      : null;
    // A historical boolean may remain in immutable metadata, but it is not
    // documentary evidence. Only a cited source statement can unlock the
    // same booking gate that preflight reports.
    wordingConfirmed = Boolean(
      payload
      && typeof sourceEvidence?.excerpt === "string" && sourceEvidence.excerpt.trim()
      && typeof sourceEvidence.location === "string" && sourceEvidence.location.trim(),
    );
  } catch {
    wordingConfirmed = false;
  }
  if (!wordingConfirmed) {
    errors.push("non-EU reverse-charge input-VAT deduction requires confirmed reverse-charge wording evidenced by a source excerpt/location on the invoice");
  }
  return errors;
}


export function postEuServiceReverseChargePurchase(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  const errors = reverseChargeInputErrors(input);
  if (errors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors };
  const splitErrors = unsupportedStructuredPurchaseLinesErrors(db, input.documentId, "reverse_charge");
  if (splitErrors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: splitErrors };

  // A non-registered company cannot book deductible reverse-charge VAT (§ 37);
  // the purchase instead triggers a § 50 b erhvervelsesmoms registration that
  // is out of scope. Refuse rather than book a forbidden 4000 line.
  if (!companyIsVatRegistered(db)) {
    return { ok: false, appliedRules: [REGISTRATION_RULE_ID, REVERSE_CHARGE_RULE_ID], errors: [NON_REGISTERED_EU_SERVICE_MSG] };
  }

  const identity = resolveDocumentSupplierIdentity(db, input.documentId);
  if (!identity) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [`documentId ${input.documentId} does not exist`] };
  // EU service reverse charge (momsloven §46) applies only to suppliers in
  // *other* EU member states. A Danish supplier is a domestic purchase and
  // must not be booked as reverse charge.
  if (!identity.ok) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: ["document supplier identity requires human resolution before reverse-charge booking"] };
  if (identity.identifierKind === "dk_cvr") {
    return {
      ok: false,
      appliedRules: [REVERSE_CHARGE_RULE_ID],
      errors: ["document supplier identity is Danish — EU service reverse charge applies only to other EU member states; book this as a domestic DK_PURCHASE_25 expense"],
    };
  }
  if (identity.identifierKind === "non_eu") return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: ["document supplier identity is non-EU — use the applicable non-EU purchase treatment; do not fabricate an EU VAT ID"] };
  const viesCheck = requireCachedViesValidation(db, identity.identifier, "document sender_vat_cvr");
  if (!viesCheck.ok) return { ok: false, appliedRules: [...new Set([REVERSE_CHARGE_RULE_ID, ...viesCheck.appliedRules])], errors: viesCheck.errors };

  return postServiceReverseChargeLines(db, input, "EU_SERVICE_REVERSE_CHARGE", REVERSE_CHARGE_RULE_ID, "EU");
}

/** Explicit treatment for services bought from a supplier outside the EU.
 * Country + non_eu classification is enough to ingest and retain a voucher,
 * while automatic input-VAT deduction additionally requires invoice evidence
 * checked below. */
function postNonEuServiceReverseChargePurchaseInternal(db: Database, input: ReverseChargePurchaseInput, inCurrentTransaction: boolean): JournalPostResult {
  const errors = reverseChargeInputErrors(input);
  if (errors.length > 0) return { ok: false, appliedRules: [NON_EU_REVERSE_CHARGE_RULE_ID], errors };
  const splitErrors = unsupportedStructuredPurchaseLinesErrors(db, input.documentId, "reverse_charge");
  if (splitErrors.length > 0) return { ok: false, appliedRules: [NON_EU_REVERSE_CHARGE_RULE_ID], errors: splitErrors };
  if (!companyIsVatRegistered(db)) {
    return { ok: false, appliedRules: [REGISTRATION_RULE_ID, NON_EU_REVERSE_CHARGE_RULE_ID], errors: [NON_REGISTERED_NON_EU_SERVICE_MSG] };
  }
  const identity = resolveDocumentSupplierIdentity(db, input.documentId);
  if (!identity) return { ok: false, appliedRules: [NON_EU_REVERSE_CHARGE_RULE_ID], errors: [`documentId ${input.documentId} does not exist`] };
  if (!identity.ok || identity.identifierKind !== "non_eu") {
    return { ok: false, appliedRules: [NON_EU_REVERSE_CHARGE_RULE_ID], errors: ["document supplier identity must be resolved explicitly as non-EU before non-EU service reverse-charge booking"] };
  }
  const evidenceErrors = nonEuReverseChargeEvidenceErrors(db, input.documentId);
  if (evidenceErrors.length > 0) {
    return {
      ok: false,
      appliedRules: [NON_EU_REVERSE_CHARGE_RULE_ID],
      errors: ["document requires human resolution before non-EU reverse-charge input-VAT deduction", ...evidenceErrors],
    };
  }
  return postServiceReverseChargeLines(db, input, "NON_EU_SERVICE_REVERSE_CHARGE", NON_EU_REVERSE_CHARGE_RULE_ID, "Non-EU", inCurrentTransaction);
}

export function postNonEuServiceReverseChargePurchaseInCurrentTransaction(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return postNonEuServiceReverseChargePurchaseInternal(db, input, true);
}

export function postNonEuServiceReverseChargePurchase(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return db.transaction(() => postNonEuServiceReverseChargePurchaseInternal(db, input, true)).immediate();
}

/**
 * Expense-booking entry point for a foreign service. The user chooses one
 * reverse-charge action; the immutable supplier identity on the document
 * decides whether EU/VIES or non-EU provenance and VAT codes apply.
 */
function postForeignServiceReverseChargePurchaseInternal(db: Database, input: ReverseChargePurchaseInput, inCurrentTransaction: boolean): JournalPostResult {
  const errors = reverseChargeInputErrors(input);
  if (errors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors };
  const splitErrors = unsupportedStructuredPurchaseLinesErrors(db, input.documentId, "reverse_charge");
  if (splitErrors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: splitErrors };
  const identity = resolveDocumentSupplierIdentity(db, input.documentId);
  if (!identity) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [`documentId ${input.documentId} does not exist`] };
  if (!identity.ok) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: ["document supplier identity requires human resolution before reverse-charge booking"] };
  if (identity.identifierKind === "dk_cvr") {
    return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: ["document supplier identity is Danish — foreign service reverse charge is not applicable; use standard domestic purchase VAT"] };
  }
  if (!companyIsVatRegistered(db)) {
    const ruleId = identity.identifierKind === "non_eu" ? NON_EU_REVERSE_CHARGE_RULE_ID : REVERSE_CHARGE_RULE_ID;
    const message = identity.identifierKind === "non_eu" ? NON_REGISTERED_NON_EU_SERVICE_MSG : NON_REGISTERED_EU_SERVICE_MSG;
    return { ok: false, appliedRules: [REGISTRATION_RULE_ID, ruleId], errors: [message] };
  }
  if (identity.identifierKind === "non_eu") {
    return postNonEuServiceReverseChargePurchaseInternal(db, input, inCurrentTransaction);
  }
  const viesCheck = requireCachedViesValidation(db, identity.identifier, "document sender_vat_cvr");
  if (!viesCheck.ok) return { ok: false, appliedRules: [...new Set([REVERSE_CHARGE_RULE_ID, ...viesCheck.appliedRules])], errors: viesCheck.errors };
  return postServiceReverseChargeLines(db, input, "EU_SERVICE_REVERSE_CHARGE", REVERSE_CHARGE_RULE_ID, "EU", inCurrentTransaction);
}

export function postForeignServiceReverseChargePurchaseInCurrentTransaction(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return postForeignServiceReverseChargePurchaseInternal(db, input, true);
}

export function postForeignServiceReverseChargePurchase(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  return db.transaction(() => postForeignServiceReverseChargePurchaseInternal(db, input, true)).immediate();
}

function postRepresentationPurchaseInternal(db: Database, input: RepresentationPurchaseInput, inCurrentTransaction: boolean): JournalPostResult {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (typeof input.text !== "string" || input.text.trim().length === 0) errors.push("text is required");
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isFinite(input.netAmount) || input.netAmount <= 0) errors.push("netAmount must be a positive number");
  if (errors.length > 0) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors };

  // The § 42 partial representation deduction is a registered-business relief;
  // a non-registered company gets no deduction (§ 37), so the full VAT is a
  // cost. Refuse and direct to gross non_deductible absorption.
  if (!companyIsVatRegistered(db)) {
    return { ok: false, appliedRules: [REGISTRATION_RULE_ID, REPRESENTATION_RULE_ID], errors: [NON_REGISTERED_REPRESENTATION_MSG] };
  }
  const splitErrors = unsupportedStructuredPurchaseLinesErrors(db, input.documentId, "representation");
  if (splitErrors.length > 0) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors: splitErrors };
  const supplierErrors = documentDanishInputVatSupplierErrors(db, input.documentId);
  if (supplierErrors.length > 0) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors: supplierErrors };

  const fullVatAmount = percentOfDkk(input.netAmount, 25);
  const deductibleVatAmount = percentOfDkk(fullVatAmount, 25);
  const nonDeductibleVatAmount = subtractDkk(fullVatAmount, deductibleVatAmount);
  const grossAmount = addDkk(input.netAmount, fullVatAmount);
  const inputVat = resolveAccountRole(db, "input_vat");
  const payment = input.paymentAccountNo ? { ok: true as const, accountNo: input.paymentAccountNo } : resolveAccountRole(db, "bank");
  if (!inputVat.ok) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors: [inputVat.error] };
  if (!payment.ok) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors: [payment.error] };

  const post = inCurrentTransaction ? postJournalEntryInCurrentTransaction : postJournalEntry;
  const result = post(db, {
    transactionDate: input.transactionDate,
    text: input.text.trim(),
    documentId: input.documentId,
    sourceBankTransactionId: input.sourceBankTransactionId,
    currency: input.currency,
    amountForeign: input.amountForeign,
    amountDkk: input.amountDkk,
    fxRateToDkk: input.fxRateToDkk,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
    lines: [
      {
        accountNo: input.expenseAccountNo ?? "3070",
        debitAmount: roundDkk(input.netAmount),
        vatCode: "REPRESENTATION_SPECIAL",
        text: "Representation purchase base"
      },
      {
        accountNo: input.expenseAccountNo ?? "3070",
        debitAmount: nonDeductibleVatAmount,
        // This amount is a real cost, not a VAT base. It nevertheless needs
        // an explicit classification so the report can distinguish it from a
        // forgotten manual VAT code without inventing a deductible base.
        vatCode: "REPRESENTATION_NON_DEDUCTIBLE_VAT",
        text: "Non-deductible representation VAT (75%)"
      },
      { accountNo: inputVat.accountNo, debitAmount: deductibleVatAmount, text: "Deductible representation VAT (25%)" },
      { accountNo: payment.accountNo, creditAmount: grossAmount, text: "Payment / liability" },
    ],
  });

  return {
    ...result,
    appliedRules: result.ok ? [...new Set([...(result.appliedRules ?? []), REPRESENTATION_RULE_ID])] : [...new Set([REPRESENTATION_RULE_ID, ...(result.appliedRules ?? [])])],
  };
}

export function postRepresentationPurchaseInCurrentTransaction(db: Database, input: RepresentationPurchaseInput): JournalPostResult {
  return postRepresentationPurchaseInternal(db, input, true);
}

export function postRepresentationPurchase(db: Database, input: RepresentationPurchaseInput): JournalPostResult {
  return db.transaction(() => postRepresentationPurchaseInternal(db, input, true)).immediate();
}

export function buildVatReport(db: Database, periodStart: string, periodEnd: string): VatPeriodReport {
  const errors: string[] = [];
  const vatPeriodType = getCompanySettings(db).vatPeriodType;
  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (errors.length === 0 && periodStart > periodEnd) errors.push("periodStart must be before or equal to periodEnd");
  const canonicalWindow =
    errors.length === 0 && vatPeriodType !== null
      ? vatPeriodWindowFor(periodStart, vatPeriodType)
      : null;
  const filingDeadline =
    canonicalWindow?.start === periodStart && canonicalWindow.end === periodEnd
      ? canonicalWindow.filingDeadline
      : null;
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      periodStart,
      periodEnd,
      vatPeriodType,
      filingDeadline,
      outputVat: 0,
      inputVat: 0,
      netVatPayable: 0,
      purchaseBase25: 0,
      salesBase25: 0,
      reverseChargeSalesBase: 0,
      foreignReverseChargeSalesBase: 0,
      domesticReverseChargeSalesBase: 0,
      reverseChargePurchaseBase: 0,
      euGoodsAcquisitionPurchaseBase: 0,
      euGoodsAcquisitionOutputVat: 0,
      nonEuServiceReverseChargePurchaseBase: 0,
      reverseChargePurchaseOutputVat: 0,
      representationPurchaseBase: 0,
      badDebtReliefBase25: 0,
      exemptSalesBase: 0,
      ossConsumerSalesBase: 0,
      ossConsumerSalesEntryCount: 0,
      journalEntryCount: 0,
      reversedJournalEntryCount: 0,
      reversalJournalEntryCount: 0,
      totalJournalEntryCount: 0,
      linesConsidered: 0,
      reversedLinesConsidered: 0,
      reversalLinesConsidered: 0,
      totalLinesConsidered: 0,
      rubrikker: emptyVatRubric(),
      warnings: [],
      errors,
    };
  }

  const rows = db.query(
    `SELECT je.id as entry_id, jl.id as line_id, je.status, je.reversal_of_entry_id,
            je.currency as entry_currency, je.created_by_program,
            a.account_no, a.type as account_type, a.normal_balance,
            a.default_vat_code, jl.debit_amount, jl.credit_amount, jl.vat_code
     FROM journal_entries je
     JOIN journal_lines jl ON jl.journal_entry_id = je.id
     JOIN accounts a ON a.id = jl.account_id
     WHERE je.transaction_date >= ? AND je.transaction_date <= ?
     ORDER BY je.id ASC, jl.id ASC`
  ).all(periodStart, periodEnd) as Array<{
    entry_id: number;
    line_id: number;
    status: string;
    reversal_of_entry_id: number | null;
    entry_currency: string;
    created_by_program: string;
    account_no: string;
    account_type: string;
    normal_balance: "debit" | "credit";
    default_vat_code: string | null;
    debit_amount: number;
    credit_amount: number;
    vat_code: string | null;
  }>;
  const vatAccountSemantics = loadVatAccountSemantics(db);
  const vatAmountSideByAccountNo = new Map(
    vatAccountSemantics.amountSideByAccountNo,
  );
  const reverseChargeOutputAccountNos = new Set(
    vatAccountSemantics.reverseChargeOutputAccountNos,
  );
  const vatSettlementAccountNos = vatAccountSemantics.settlementAccountNos;

  // Account numbers are chart-local identifiers, not accounting semantics.
  // Current charts express VAT meaning through type=vat or confirmed roles.
  // Immutable pre-normalisation imports cannot be rewritten, so recover their
  // roles from exact, source-proven voucher evidence across the whole ledger.
  // Loading all trusted import rows makes later reporting periods independent
  // of whether the original role-establishing voucher falls inside the period.
  const historicalEvidenceRows = db.query(
    `SELECT je.id as entry_id, jl.id as line_id, je.status,
            a.account_no, a.type as account_type, a.default_vat_code,
            jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_entries je
       JOIN journal_lines jl ON jl.journal_entry_id = je.id
       JOIN accounts a ON a.id = jl.account_id
      WHERE je.created_by_program = ?
      ORDER BY je.id ASC, jl.id ASC`,
  ).all(HISTORICAL_IMPORT_PROGRAM) as LegacyVatEvidenceRow[];
  const legacyEvidence = inferLegacyVatEvidence(
    historicalEvidenceRows,
    vatAmountSideByAccountNo,
  );
  errors.push(...legacyEvidence.errors);
  for (const [accountNo, side] of legacyEvidence.amountSideByAccountNo) {
    vatAmountSideByAccountNo.set(accountNo, side);
  }
  for (const accountNo of legacyEvidence.reverseChargeOutputAccountNos) {
    reverseChargeOutputAccountNos.add(accountNo);
  }

  // Older Dinero ledgers can contain the source-native 64040/64060
  // reverse-charge control pair while Momstype was left blank on the expense
  // base. New imports persist the inferred code, but existing immutable rows
  // need the same conservative read-time interpretation. Infer only one exact
  // 25%-matching, otherwise unclassified expense line. Explicit/default codes
  // remain authoritative; ambiguous or inconsistent source patterns block the
  // report instead of becoming amount-only corrections with missing rubrik A.
  const inferredVatCodeByLineId = new Map(
    legacyEvidence.inferredVatCodeByLineId,
  );
  const trustedDineroInputLineIds = new Set<number>();
  const legacyRepresentationBaseByLineId = new Map<number, number>();
  const legacyReverseChargeToleranceByEntry = new Map(
    legacyEvidence.reverseChargeToleranceByEntry,
  );
  const historicalRowsByEntry = new Map<number, typeof rows>();
  for (const row of rows) {
    if (!isPersistedHistoricalImportProgram(row.created_by_program)) continue;
    const entryRows = historicalRowsByEntry.get(row.entry_id) ?? [];
    entryRows.push(row);
    historicalRowsByEntry.set(row.entry_id, entryRows);
  }
  for (const [entryId, entryRows] of historicalRowsByEntry) {
    const outputControls = entryRows.filter(
      (row) => row.account_no === "64040",
    );
    const inputControls = entryRows.filter(
      (row) => row.account_no === "64060",
    );
    if (outputControls.length === 0 || inputControls.length === 0) continue;
    const canonicalPair =
      outputControls.every(
        (row) => row.account_type === "vat" && row.normal_balance === "credit",
      ) &&
      inputControls.every(
        (row) => row.account_type === "vat" && row.normal_balance === "debit",
      );
    const legacyPair =
      outputControls.every(
        (row) => row.account_type === "liability" && row.normal_balance === "credit",
      ) &&
      inputControls.every(
        (row) =>
          row.account_type === "liability" &&
          (row.normal_balance === "debit" || row.normal_balance === "credit"),
      );
    if (!canonicalPair && !legacyPair) continue;

    const outputControl = roundDkk(
      outputControls.reduce(
        (sum, row) =>
          sum + Number(row.credit_amount ?? 0) - Number(row.debit_amount ?? 0),
        0,
      ),
    );
    const inputControl = roundDkk(
      inputControls.reduce(
        (sum, row) =>
          sum + Number(row.debit_amount ?? 0) - Number(row.credit_amount ?? 0),
        0,
      ),
    );
    if (
      outputControl === 0 ||
      inputControl === 0 ||
      toOre(outputControl) !== toOre(inputControl)
    ) {
      errors.push(
        `journal entry ${entryId} has an inconsistent Dinero reverse-charge control pattern on 64040/64060; human resolution is required`,
      );
      continue;
    }

    const expenseRows = entryRows.filter((row) => row.account_type === "expense");
    const explicitCode = (row: (typeof rows)[number]): string => row.vat_code?.trim() || "";
    const reverseChargeCodes = new Set([
      "EU_SERVICE_REVERSE_CHARGE",
      "NON_EU_SERVICE_REVERSE_CHARGE",
    ]);
    let baseRows = expenseRows.filter(
      (row) => reverseChargeCodes.has(explicitCode(row)),
    );
    const explicitReverseChargeCodes = new Set(
      baseRows.map((row) => explicitCode(row)),
    );
    if (explicitReverseChargeCodes.size > 1) {
      errors.push(
        `journal entry ${entryId} mixes EU and non-EU reverse-charge bases; human resolution is required`,
      );
      continue;
    }
    if (baseRows.length === 0) {
      // An explicit historical line code wins. Account defaults are not
      // source-line evidence and may be overridden only by this exact,
      // verified Dinero control pair.
      if (expenseRows.some((row) => explicitCode(row).length > 0)) {
        errors.push(
          `journal entry ${entryId} has Dinero reverse-charge controls but its expense base carries a conflicting VAT code; human resolution is required`,
        );
        continue;
      }
      baseRows = expenseRows.filter((row) => {
        const netDebit = roundDkk(
          Number(row.debit_amount ?? 0) - Number(row.credit_amount ?? 0),
        );
        return (
          netDebit !== 0 &&
          Math.sign(netDebit) === Math.sign(outputControl) &&
          oreDifference(percentOfDkk(netDebit, 25), outputControl) <= 1
        );
      });
      if (baseRows.length !== 1) {
        errors.push(
          `journal entry ${entryId} has Dinero reverse-charge controls but no single unclassified expense base matching 25%; human resolution is required`,
        );
        continue;
      }
      inferredVatCodeByLineId.set(
        baseRows[0]!.line_id,
        "EU_SERVICE_REVERSE_CHARGE",
      );
    }

    const classifiedBase = roundDkk(
      baseRows.reduce(
        (sum, row) =>
          sum + Number(row.debit_amount ?? 0) - Number(row.credit_amount ?? 0),
        0,
      ),
    );
    if (oreDifference(percentOfDkk(classifiedBase, 25), outputControl) > 1) {
      errors.push(
        `journal entry ${entryId} has Dinero reverse-charge controls that do not reconcile to its classified expense base; human resolution is required`,
      );
      continue;
    }

    // 64060 may occur as a liability/credit account in pre-normalisation
    // Dinero charts. Count it as input VAT only after the trusted provenance,
    // exact equal 64040/64060 controls and single base have all been proven.
    for (const row of inputControls) {
      trustedDineroInputLineIds.add(row.line_id);
      vatAmountSideByAccountNo.set(row.account_no, "input");
    }
    for (const row of outputControls) {
      vatAmountSideByAccountNo.set(row.account_no, "output");
      reverseChargeOutputAccountNos.add(row.account_no);
    }
    legacyReverseChargeToleranceByEntry.set(
      entryId,
      oreDifference(percentOfDkk(classifiedBase, 25), outputControl),
    );
  }

  // Dinero's historical representation vouchers can collapse the net expense
  // and the 75% non-deductible VAT into one REPRESENTATION_SPECIAL expense
  // line. Rentemester's native shape keeps those amounts on two lines. Recover
  // the net base only for the exact trusted import shape: one representation
  // expense, one input-VAT control, no other P/L line, and the statutory
  // 25%-of-VAT deduction reconciling within one øre. Anything else continues
  // through the ordinary mismatch gate instead of being guessed.
  for (const [entryId, entryRows] of historicalRowsByEntry) {
    const representationRows = entryRows.filter(
      (row) =>
        row.account_type === "expense" &&
        row.vat_code?.trim() === "REPRESENTATION_SPECIAL",
    );
    const hasSeparateNonDeductibleLine = entryRows.some(
      (row) => row.vat_code?.trim() === "REPRESENTATION_NON_DEDUCTIBLE_VAT",
    );
    if (representationRows.length === 0 || hasSeparateNonDeductibleLine) continue;
    const otherProfitLossRows = entryRows.filter(
      (row) =>
        (row.account_type === "expense" || row.account_type === "income") &&
        !representationRows.some((candidate) => candidate.line_id === row.line_id),
    );
    const inputRows = entryRows.filter(
      (row) => vatAmountSideByAccountNo.get(row.account_no) === "input",
    );
    if (
      representationRows.length !== 1 ||
      otherProfitLossRows.length > 0 ||
      inputRows.length !== 1
    ) continue;

    const expense = roundDkk(
      Number(representationRows[0]!.debit_amount ?? 0) -
        Number(representationRows[0]!.credit_amount ?? 0),
    );
    const bookedInput = roundDkk(
      Number(inputRows[0]!.debit_amount ?? 0) -
        Number(inputRows[0]!.credit_amount ?? 0),
    );
    if (expense <= 0 || bookedInput <= 0) continue;
    const recoveredNet = roundDkk(addDkk(expense, bookedInput) / 1.25);
    const fullVat = percentOfDkk(recoveredNet, 25);
    const expectedInput = percentOfDkk(fullVat, 25);
    const expectedExpense = addDkk(
      recoveredNet,
      subtractDkk(fullVat, expectedInput),
    );
    if (
      oreDifference(expectedInput, bookedInput) > 1 ||
      oreDifference(expectedExpense, expense) > 1
    ) {
      errors.push(
        `journal entry ${entryId} has a collapsed Dinero representation purchase that does not reconcile to the statutory partial VAT deduction; human resolution is required`,
      );
      continue;
    }
    legacyRepresentationBaseByLineId.set(
      representationRows[0]!.line_id,
      recoveredNet,
    );
  }

  // A pure transfer between VAT amount accounts and a confirmed settlement
  // account settles an earlier return; it is not fresh output/input VAT for
  // the transaction-date period. Identify the shape per journal before the
  // line scan so neither its amount nor its lack of a base code pollutes the
  // current report.
  const entryShapes = new Map<number, {
    touchesVatAmount: boolean;
    touchesVatSettlement: boolean;
    touchesOther: boolean;
  }>();
  for (const row of rows) {
    const shape = entryShapes.get(row.entry_id) ?? {
      touchesVatAmount: false,
      touchesVatSettlement: false,
      touchesOther: false,
    };
    if (vatAmountSideByAccountNo.has(row.account_no)) {
      shape.touchesVatAmount = true;
    } else if (vatSettlementAccountNos.has(row.account_no)) {
      shape.touchesVatSettlement = true;
    } else {
      shape.touchesOther = true;
    }
    entryShapes.set(row.entry_id, shape);
  }
  const settlementTransferEntryIds = new Set(
    [...entryShapes.entries()]
      .filter(([, shape]) =>
        shape.touchesVatAmount &&
        shape.touchesVatSettlement &&
        !shape.touchesOther)
      .map(([entryId]) => entryId),
  );

  let outputVat = 0;
  let inputVat = 0;
  let purchaseBase25 = 0;
  let salesBase25 = 0;
  let foreignReverseChargeSalesBase = 0;
  let domesticReverseChargeSalesBase = 0;
  let reverseChargePurchaseBase = 0;
  let euGoodsAcquisitionPurchaseBase = 0;
  let euGoodsAcquisitionOutputVat = 0;
  let nonEuServiceReverseChargePurchaseBase = 0;
  let representationPurchaseBase = 0;
  // Reverse-charge output VAT is booked per purchase on account 1200, øre-
  // rounded per transaction. To recover the *booked* total (which can differ
  // from 25% of the summed base by up to 1 øre per purchase), accumulate the
  // 1200 net (credit − debit) per journal entry and the set of entries that
  // carry an EU_SERVICE_REVERSE_CHARGE base line, then sum 1200 over exactly
  // those entries after the scan.
  const outputVatNetByEntry = new Map<number, number>();
  const dedicatedReverseChargeOutputNetByEntry = new Map<number, number>();
  const reverseChargeExpectedVatByEntry = new Map<number, number>();
  const inputVatNetByEntry = new Map<number, number>();
  const reverseChargeEntryIds = new Set<number>();
  let badDebtReliefBase25 = 0;
  let exemptSalesBase = 0;
  let ossConsumerSalesBase = 0;
  // OSS sales are counted per *entry*, not per line, so a multi-line OSS
  // invoice still counts as one entry.
  const ossConsumerSalesEntryIds = new Set<number>();
  // Count VAT-bearing base lines per category. 25% of a period-summed base is
  // not equal to the sum of per-line 25%-rounded VAT when amounts have odd
  // øre, so each base line can drift the aggregate by up to 1 øre. We allow a
  // (lineCount - 1)-øre tolerance on the reconciliation cross-check below.
  let outputVatBaseLines = 0;
  let inputVatBaseLines = 0;
  let foreignOutputVatBaseLines = 0;
  let foreignInputVatBaseLines = 0;
  let ordinaryOutputVatBaseLines = 0;
  let foreignOrdinaryOutputVatBaseLines = 0;
  let foreignReverseChargeOutputVatBaseLines = 0;
  const activeEntryIds = new Set<number>();
  const reversedEntryIds = new Set<number>();
  const reversalEntryIds = new Set<number>();
  const manualVatControlEntryIds = new Set<number>();
  const classifiedManualVatEntryIds = new Set<number>();
  const historicalVatControlEntryIds = new Set<number>();
  const classifiedHistoricalVatEntryIds = new Set<number>();
  const markClassified = (entryId: number, trustedHistoricalImport: boolean) => {
    (trustedHistoricalImport
      ? classifiedHistoricalVatEntryIds
      : classifiedManualVatEntryIds
    ).add(entryId);
  };
  let activeLinesConsidered = 0;
  let reversedLinesConsidered = 0;
  let reversalLinesConsidered = 0;
  const reversedByInPeriodReversal = new Set(rows.filter((row) => row.reversal_of_entry_id != null).map((row) => row.reversal_of_entry_id as number));

  for (const row of rows) {
    const isReversalEntry = row.reversal_of_entry_id != null;
    const isReversedEntry = !isReversalEntry && reversedByInPeriodReversal.has(row.entry_id);

    if (isReversalEntry) {
      reversalEntryIds.add(row.entry_id);
      reversalLinesConsidered += 1;
    } else if (isReversedEntry) {
      reversedEntryIds.add(row.entry_id);
      reversedLinesConsidered += 1;
    } else {
      activeEntryIds.add(row.entry_id);
      activeLinesConsidered += 1;
    }

    const debit = roundDkk(Number(row.debit_amount ?? 0));
    const credit = roundDkk(Number(row.credit_amount ?? 0));
    const amountSide =
      vatAmountSideByAccountNo.get(row.account_no) ??
      (trustedDineroInputLineIds.has(row.line_id) ? "input" : undefined);
    const trustedHistoricalImport = isPersistedHistoricalImportProgram(
      row.created_by_program,
    );
    // Explicit persisted line classification is authoritative. Only an exact
    // verified historical-import marker may fall back to the reviewed account
    // default; ordinary/manual entries never inherit one at report time.
    const effectiveVatCode =
      row.vat_code ??
      inferredVatCodeByLineId.get(row.line_id) ??
      (trustedHistoricalImport &&
      (row.account_type === "income" || row.account_type === "expense")
        ? row.default_vat_code
        : null);
    const isVatBaseLine =
      row.account_type === "income" || row.account_type === "expense";
    const isSettlementTransfer = settlementTransferEntryIds.has(row.entry_id);

    if (
      effectiveVatCode !== null &&
      !VAT_LINE_CODES.has(effectiveVatCode)
    ) {
      errors.push(
        `journal entry ${row.entry_id} line on account ${row.account_no} has unsupported vat_code '${effectiveVatCode}'`,
      );
    }

    if (!isSettlementTransfer && amountSide === "output") {
      outputVat += credit - debit;
      outputVatNetByEntry.set(
        row.entry_id,
        (outputVatNetByEntry.get(row.entry_id) ?? 0) + (credit - debit),
      );
      if (reverseChargeOutputAccountNos.has(row.account_no)) {
        dedicatedReverseChargeOutputNetByEntry.set(
          row.entry_id,
          (dedicatedReverseChargeOutputNetByEntry.get(row.entry_id) ?? 0) +
            (credit - debit),
        );
      }
    }
    if (!isSettlementTransfer && amountSide === "input") {
      inputVat += debit - credit;
      inputVatNetByEntry.set(
        row.entry_id,
        (inputVatNetByEntry.get(row.entry_id) ?? 0) + (debit - credit),
      );
    }
    if (!isSettlementTransfer && amountSide !== undefined && !trustedHistoricalImport) {
      manualVatControlEntryIds.add(row.entry_id);
    } else if (!isSettlementTransfer && amountSide !== undefined) {
      historicalVatControlEntryIds.add(row.entry_id);
    }

    const isForeignCurrencyEntry = row.entry_currency !== "DKK";
    if (isVatBaseLine && effectiveVatCode === "DK_PURCHASE_25") {
      markClassified(row.entry_id, trustedHistoricalImport);
      purchaseBase25 += debit - credit;
      inputVatBaseLines += 1;
      if (isForeignCurrencyEntry) foreignInputVatBaseLines += 1;
    }
    if (isVatBaseLine && effectiveVatCode === "DK_SALE_25") {
      markClassified(row.entry_id, trustedHistoricalImport);
      salesBase25 += credit - debit;
      outputVatBaseLines += 1;
      ordinaryOutputVatBaseLines += 1;
      if (isForeignCurrencyEntry) {
        foreignOutputVatBaseLines += 1;
        foreignOrdinaryOutputVatBaseLines += 1;
      }
    }
    // JUR-2/KODE-2: keep the two reverse-charge sales bases apart so the
    // momsangivelse can route foreign → rubrik B (VIES) and domestic → rubrik C.
    if (isVatBaseLine && effectiveVatCode === "REVERSE_CHARGE_EXEMPT") {
      markClassified(row.entry_id, trustedHistoricalImport);
      foreignReverseChargeSalesBase += credit - debit;
    }
    if (isVatBaseLine && effectiveVatCode === "DOMESTIC_REVERSE_CHARGE_EXEMPT") {
      markClassified(row.entry_id, trustedHistoricalImport);
      domesticReverseChargeSalesBase += credit - debit;
    }
    if (isVatBaseLine && effectiveVatCode === "EU_SERVICE_REVERSE_CHARGE") {
      markClassified(row.entry_id, trustedHistoricalImport);
      reverseChargePurchaseBase += debit - credit;
      reverseChargeEntryIds.add(row.entry_id);
      reverseChargeExpectedVatByEntry.set(
        row.entry_id,
        addDkk(
          reverseChargeExpectedVatByEntry.get(row.entry_id) ?? 0,
          percentOfDkk(debit - credit, 25),
        ),
      );
      // Reverse charge contributes to both output and input VAT.
      inputVatBaseLines += 1;
      outputVatBaseLines += 1;
      if (isForeignCurrencyEntry) {
        foreignInputVatBaseLines += 1;
        foreignOutputVatBaseLines += 1;
        foreignReverseChargeOutputVatBaseLines += 1;
      }
    }
    if (isVatBaseLine && effectiveVatCode === "EU_GOODS_ACQUISITION") {
      markClassified(row.entry_id, trustedHistoricalImport);
      euGoodsAcquisitionPurchaseBase += debit - credit;
      euGoodsAcquisitionOutputVat += percentOfDkk(debit - credit, 25);
      reverseChargeEntryIds.add(row.entry_id);
      reverseChargeExpectedVatByEntry.set(row.entry_id, addDkk(reverseChargeExpectedVatByEntry.get(row.entry_id) ?? 0, percentOfDkk(debit - credit, 25)));
      inputVatBaseLines += 1; outputVatBaseLines += 1;
      if (isForeignCurrencyEntry) { foreignInputVatBaseLines += 1; foreignOutputVatBaseLines += 1; foreignReverseChargeOutputVatBaseLines += 1; }
    }
    if (isVatBaseLine && effectiveVatCode === "NON_EU_SERVICE_REVERSE_CHARGE") {
      markClassified(row.entry_id, trustedHistoricalImport);
      nonEuServiceReverseChargePurchaseBase += debit - credit;
      reverseChargeEntryIds.add(row.entry_id);
      reverseChargeExpectedVatByEntry.set(
        row.entry_id,
        addDkk(
          reverseChargeExpectedVatByEntry.get(row.entry_id) ?? 0,
          percentOfDkk(debit - credit, 25),
        ),
      );
      inputVatBaseLines += 1;
      outputVatBaseLines += 1;
      if (isForeignCurrencyEntry) {
        foreignInputVatBaseLines += 1;
        foreignOutputVatBaseLines += 1;
        foreignReverseChargeOutputVatBaseLines += 1;
      }
    }
    if (isVatBaseLine && effectiveVatCode === "REPRESENTATION_SPECIAL") {
      markClassified(row.entry_id, trustedHistoricalImport);
      representationPurchaseBase +=
        legacyRepresentationBaseByLineId.get(row.line_id) ?? debit - credit;
      inputVatBaseLines += 1;
      if (isForeignCurrencyEntry) foreignInputVatBaseLines += 1;
    }
    if (isVatBaseLine && effectiveVatCode === "DK_BAD_DEBT_25") {
      markClassified(row.entry_id, trustedHistoricalImport);
      badDebtReliefBase25 += debit - credit;
      outputVatBaseLines += 1;
      ordinaryOutputVatBaseLines += 1;
      if (isForeignCurrencyEntry) {
        foreignOutputVatBaseLines += 1;
        foreignOrdinaryOutputVatBaseLines += 1;
      }
    }
    // VAT-exempt domestic sales (momsloven §13) and OSS consumer sales carry
    // NO Danish output VAT, so they are tracked in their own bases and are
    // deliberately NOT added to outputVatBaseLines (the output-VAT
    // reconciliation must not expect 25% of them).
    if (isVatBaseLine && effectiveVatCode === "DK_SALE_EXEMPT") {
      markClassified(row.entry_id, trustedHistoricalImport);
      exemptSalesBase += credit - debit;
    }
    if (isVatBaseLine && effectiveVatCode === "OSS_EU_CONSUMER") {
      markClassified(row.entry_id, trustedHistoricalImport);
      ossConsumerSalesBase += credit - debit;
      ossConsumerSalesEntryIds.add(row.entry_id);
    }
  }

  // #533 legacy guard: the write boundary now rejects this shape, but an old
  // ledger may already contain a manual amount-only VAT journal. Never present
  // its payable as filing-ready while the SKAT bases are empty.
  for (const entryId of manualVatControlEntryIds) {
    if (!classifiedManualVatEntryIds.has(entryId)) {
      errors.push(
        `journal entry ${entryId} changes a VAT amount account but has no explicit vat_code on a VAT base line; the VAT bases cannot be filed until the classification is corrected`,
      );
    }
  }

  outputVat = roundDkk(outputVat);
  inputVat = roundDkk(inputVat);
  purchaseBase25 = roundDkk(purchaseBase25);
  salesBase25 = roundDkk(salesBase25);
  foreignReverseChargeSalesBase = roundDkk(foreignReverseChargeSalesBase);
  domesticReverseChargeSalesBase = roundDkk(domesticReverseChargeSalesBase);
  const reverseChargeSalesBase = addDkk(foreignReverseChargeSalesBase, domesticReverseChargeSalesBase);
  reverseChargePurchaseBase = roundDkk(reverseChargePurchaseBase);
  euGoodsAcquisitionPurchaseBase = roundDkk(euGoodsAcquisitionPurchaseBase);
  euGoodsAcquisitionOutputVat = roundDkk(euGoodsAcquisitionOutputVat);
  nonEuServiceReverseChargePurchaseBase = roundDkk(nonEuServiceReverseChargePurchaseBase);
  const historicalAmountOnlyEntryIds = [...historicalVatControlEntryIds].filter(
    (entryId) => !classifiedHistoricalVatEntryIds.has(entryId),
  );
  const historicalOutputCorrection = roundDkk(
    historicalAmountOnlyEntryIds.reduce(
      (sum, entryId) => sum + (outputVatNetByEntry.get(entryId) ?? 0),
      0,
    ),
  );
  const historicalDedicatedReverseChargeCorrection = roundDkk(
    historicalAmountOnlyEntryIds.reduce(
      (sum, entryId) =>
        sum + (dedicatedReverseChargeOutputNetByEntry.get(entryId) ?? 0),
      0,
    ),
  );
  const historicalOrdinaryOutputCorrection = subtractDkk(
    historicalOutputCorrection,
    historicalDedicatedReverseChargeCorrection,
  );
  const historicalInputCorrection = roundDkk(
    historicalAmountOnlyEntryIds.reduce(
      (sum, entryId) => sum + (inputVatNetByEntry.get(entryId) ?? 0),
      0,
    ),
  );

  // A dedicated source-/role-proven control account (Dinero 64040) always
  // belongs to the foreign-service VAT category. For shared accounts (native
  // 1200), only the per-line expected reverse-charge VAT can be allocated; the
  // remaining booked output VAT stays in the ordinary sales category.
  let reverseChargePurchaseOutputVat = roundDkk(
    [...dedicatedReverseChargeOutputNetByEntry.values()].reduce(
      (sum, amount) => sum + amount,
      0,
    ),
  );
  for (const entryId of reverseChargeEntryIds) {
    if (!dedicatedReverseChargeOutputNetByEntry.has(entryId)) {
      reverseChargePurchaseOutputVat = addDkk(
        reverseChargePurchaseOutputVat,
        reverseChargeExpectedVatByEntry.get(entryId) ?? 0,
      );
    }
  }
  representationPurchaseBase = roundDkk(representationPurchaseBase);
  badDebtReliefBase25 = roundDkk(badDebtReliefBase25);
  exemptSalesBase = roundDkk(exemptSalesBase);
  ossConsumerSalesBase = roundDkk(ossConsumerSalesBase);

  const foreignServiceReverseChargeBase = addDkk(reverseChargePurchaseBase, nonEuServiceReverseChargePurchaseBase);
  const foreignPurchaseReverseChargeBase = addDkk(foreignServiceReverseChargeBase, euGoodsAcquisitionPurchaseBase);
  const expectedOutputVat = subtractDkk(addDkk(percentOfDkk(salesBase25, 25), percentOfDkk(foreignPurchaseReverseChargeBase, 25)), percentOfDkk(badDebtReliefBase25, 25));
  const expectedInputVat = addDkk(addDkk(percentOfDkk(purchaseBase25, 25), percentOfDkk(foreignPurchaseReverseChargeBase, 25)), percentOfDkk(percentOfDkk(representationPurchaseBase, 25), 25));
  const warnings: string[] = [];
  // Each VAT-bearing base line is rounded to øre independently when booked,
  // so the booked aggregate can differ from "25% of the summed base" by up to
  // 1 øre per line. Only the *first* line establishes the aggregate; the
  // remaining (n-1) lines can each drift it, so the tolerance is (n-1) øre.
  // A genuine mis-booking exceeds this small bound and blocks filing.
  // Foreign-currency entries round the stated VAT and its net base into DKK
  // independently before balancing. That conversion can add one further øre
  // of legitimate drift per foreign base line.
  const periodEntryIds = new Set(rows.map((row) => row.entry_id));
  const legacyReverseChargeTolerance = [...legacyReverseChargeToleranceByEntry]
    .filter(([entryId]) => periodEntryIds.has(entryId))
    .reduce((sum, [, tolerance]) => sum + tolerance, 0);
  const legacyOrdinaryInputTolerance = [...legacyEvidence.ordinaryInputToleranceByEntry]
    .filter(([entryId]) => periodEntryIds.has(entryId))
    .reduce((sum, [, tolerance]) => sum + tolerance, 0);
  const legacyOrdinaryOutputTolerance = [...legacyEvidence.ordinaryOutputToleranceByEntry]
    .filter(([entryId]) => periodEntryIds.has(entryId))
    .reduce((sum, [, tolerance]) => sum + tolerance, 0);
  const outputVatTolerance = Math.max(0, outputVatBaseLines - 1) + foreignOutputVatBaseLines + legacyReverseChargeTolerance + legacyOrdinaryOutputTolerance;
  const inputVatTolerance = Math.max(0, inputVatBaseLines - 1) + foreignInputVatBaseLines + legacyReverseChargeTolerance + legacyOrdinaryInputTolerance;
  const explainedOutputVat = addDkk(expectedOutputVat, historicalOutputCorrection);
  const explainedInputVat = addDkk(expectedInputVat, historicalInputCorrection);
  if (historicalOutputCorrection !== 0 || historicalInputCorrection !== 0) {
    warnings.push(
      `verified historical amount-only VAT corrections included: output ${historicalOutputCorrection}, input ${historicalInputCorrection}`,
    );
  }
  if (oreDifference(outputVat, explainedOutputVat) > outputVatTolerance) {
    errors.push(
      `output VAT mismatch: booked ${outputVat}, expected from base × rate plus verified historical corrections ${explainedOutputVat}`,
    );
  }
  if (oreDifference(inputVat, explainedInputVat) > inputVatTolerance) {
    errors.push(
      `input VAT mismatch: booked ${inputVat}, expected from base × rate plus verified historical corrections ${explainedInputVat}`,
    );
  }

  // Aggregate reconciliation is not sufficient: an overstated ordinary
  // output amount and an understated dedicated reverse-charge amount can
  // cancel each other while producing wrong SKAT rubrics. Reconcile both
  // categories independently. Shared-account reverse charge is allocated from
  // its base above, so any unexplained remainder lands in (and is checked
  // against) ordinary sales VAT.
  const expectedReverseChargeOutputVat = addDkk(
    [...reverseChargeExpectedVatByEntry.values()].reduce(
      (sum, amount) => sum + amount,
      0,
    ),
    historicalDedicatedReverseChargeCorrection,
  );
  const expectedOrdinaryOutputVat = addDkk(
    subtractDkk(
      percentOfDkk(salesBase25, 25),
      percentOfDkk(badDebtReliefBase25, 25),
    ),
    historicalOrdinaryOutputCorrection,
  );
  const bookedOrdinaryOutputVat = subtractDkk(
    outputVat,
    reverseChargePurchaseOutputVat,
  );
  const reverseChargeOutputVatTolerance =
    foreignReverseChargeOutputVatBaseLines + legacyReverseChargeTolerance;
  const ordinaryOutputVatTolerance =
    Math.max(0, ordinaryOutputVatBaseLines - 1) +
    foreignOrdinaryOutputVatBaseLines +
    legacyOrdinaryOutputTolerance;
  if (
    oreDifference(
      reverseChargePurchaseOutputVat,
      expectedReverseChargeOutputVat,
    ) > reverseChargeOutputVatTolerance
  ) {
    errors.push(
      `reverse-charge output VAT mismatch: booked ${reverseChargePurchaseOutputVat}, expected from foreign-service bases plus verified historical corrections ${expectedReverseChargeOutputVat}`,
    );
  }
  if (
    oreDifference(bookedOrdinaryOutputVat, expectedOrdinaryOutputVat) >
    ordinaryOutputVatTolerance
  ) {
    errors.push(
      `ordinary output VAT mismatch: booked ${bookedOrdinaryOutputVat}, expected from sales and bad-debt bases plus verified historical corrections ${expectedOrdinaryOutputVat}`,
    );
  }

  // Partial deduction (delvis fradragsret, momsloven §§37-38) is NOT modelled:
  // every purchase code assumes 100% deductible input VAT. When a period has
  // BOTH VAT-exempt turnover (DK_SALE_EXEMPT, §13) AND deducted input VAT, some
  // of that input VAT may be only partly deductible (a pro-rata split tied to
  // the exempt vs. taxable revenue mix). Rentemester does not compute that
  // split — warn so the user/advisor checks it. Warning only: no amount changes
  // and the netVatPayable invariant is untouched.
  if (exemptSalesBase > 0 && inputVat > 0) {
    warnings.push(
      "Delvis fradragsret (momsloven §§37-38) håndteres ikke: perioden har både " +
        "momsfri omsætning (DK_SALE_EXEMPT) og fuldt fradraget købsmoms. Noget af " +
        "købsmomsen kan være delvis fradragsberettiget (pro rata efter momspligtig " +
        "vs. momsfri omsætning). Få revisor til at vurdere fradragsprocenten — " +
        "Rentemester beregner ikke fordelingen.",
    );
  }

  const report: Omit<VatPeriodReport, "rubrikker"> = {
    ok: errors.length === 0,
    appliedRules: [RULE_ID],
    periodStart,
    periodEnd,
    vatPeriodType,
    filingDeadline,
    outputVat,
    inputVat,
    netVatPayable: subtractDkk(outputVat, inputVat),
    purchaseBase25,
    salesBase25,
    reverseChargeSalesBase,
    foreignReverseChargeSalesBase,
    domesticReverseChargeSalesBase,
    reverseChargePurchaseBase,
    euGoodsAcquisitionPurchaseBase,
    euGoodsAcquisitionOutputVat,
    nonEuServiceReverseChargePurchaseBase,
    reverseChargePurchaseOutputVat,
    representationPurchaseBase,
    badDebtReliefBase25,
    exemptSalesBase,
    ossConsumerSalesBase,
    ossConsumerSalesEntryCount: ossConsumerSalesEntryIds.size,
    journalEntryCount: activeEntryIds.size,
    reversedJournalEntryCount: reversedEntryIds.size,
    reversalJournalEntryCount: reversalEntryIds.size,
    totalJournalEntryCount: activeEntryIds.size + reversedEntryIds.size + reversalEntryIds.size,
    linesConsidered: activeLinesConsidered,
    reversedLinesConsidered,
    reversalLinesConsidered,
    totalLinesConsidered: rows.length,
    warnings,
    errors,
  };
  return { ...report, rubrikker: projectVatRubric(report as VatPeriodReport) };
}
