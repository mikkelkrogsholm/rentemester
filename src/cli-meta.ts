import { registerCommandSpecs } from "./cli-meta/helpers";
import type { CommandSpec } from "./cli-meta/_shared";
import { initSpec, serveSpec, systemSpecs } from "./cli-meta/system-specs";
import { companySpecs } from "./cli-meta/company-specs";
import { masterDataSpecs } from "./cli-meta/master-data-specs";
import { invoiceSpecs, invoiceSendSpec } from "./cli-meta/invoice-specs";
import { documentsSpecs } from "./cli-meta/documents-specs";
import { bankSpecs } from "./cli-meta/bank-specs";
import { expenseSpecs } from "./cli-meta/expense-specs";
import { vatSpecs, vatFilingSpecs } from "./cli-meta/vat-specs";
import { periodSpecs } from "./cli-meta/period-specs";
import { journalSpecs } from "./cli-meta/journal-specs";
import { dashboardSpecs } from "./cli-meta/dashboard-specs";
import { recurringInvoiceSpecs } from "./cli-meta/recurring-invoice-specs";
import { mailIntakeSpecs } from "./cli-meta/mail-intake-specs";
import { mileageSpecs } from "./cli-meta/mileage-specs";
import { assetSpecs } from "./cli-meta/assets-specs";
import { reportSpecs, reportAnnualAndTaxSpecs } from "./cli-meta/reports-specs";
import { openingBalanceSpecs } from "./cli-meta/opening-balance-specs";
import { gdprSpecs } from "./cli-meta/gdpr-specs";
import { importSpecs } from "./cli-meta/import-specs";
import { agentSpecs } from "./cli-meta/agent-specs";
import { complianceSpecs } from "./cli-meta/compliance-specs";
import { accrualSpecs } from "./cli-meta/accrual-specs";
import { budgetSpecs } from "./cli-meta/budget-specs";
import { payableSpecs } from "./cli-meta/payables-specs";

export type { CommandSpec } from "./cli-meta/_shared";
export {
  GLOBAL_FLAGS,
  UNIVERSAL_GLOBAL_FLAGS,
  SIDE_EFFECTING_COMMANDS,
  flagsForSpec,
} from "./cli-meta/_shared";
export {
  getCommandKey,
  getCommandSpec,
  renderGlobalUsage,
  renderCommandHelp,
  validateCommandFlags,
} from "./cli-meta/helpers";

/**
 * The full list of CLI command specs, in canonical order. The order matters:
 * `renderGlobalUsage` walks this array three times (read-only, side-effecting,
 * mutating) to build the global `--help` listing, and tests assert the listing
 * order matches this canonical order.
 *
 * Each sub-array lives in `src/cli-meta/<domain>-specs.ts` — see those files
 * for the actual spec bodies. The barrel keeps the assembly + ordering in one
 * obvious place.
 */
export const COMMAND_SPECS: CommandSpec[] = [
  ...initSpec,
  ...companySpecs,
  ...serveSpec,
  ...systemSpecs,
  ...masterDataSpecs,
  ...invoiceSpecs,
  ...documentsSpecs,
  ...bankSpecs,
  ...expenseSpecs,
  ...vatSpecs,
  ...periodSpecs,
  ...journalSpecs,
  ...dashboardSpecs,
  ...recurringInvoiceSpecs,
  ...mailIntakeSpecs,
  ...mileageSpecs,
  ...assetSpecs,
  ...reportSpecs,
  ...vatFilingSpecs,
  ...openingBalanceSpecs,
  ...invoiceSendSpec,
  ...gdprSpecs,
  ...reportAnnualAndTaxSpecs,
  ...importSpecs,
  ...agentSpecs,
  ...complianceSpecs,
  ...accrualSpecs,
  ...budgetSpecs,
  ...payableSpecs,
];

registerCommandSpecs(COMMAND_SPECS);
