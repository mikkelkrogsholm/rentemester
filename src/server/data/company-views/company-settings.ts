import { openDb, migrate } from "../../../core/db";
import {
  getCompanySettings,
  resolveCompanyPaymentDetails,
  syncCompanyFromCvr,
  type CompanyPaymentDetails,
  type CompanySettings,
  type SyncCompanyFromCvrResult,
} from "../../../core/company";
import { requireCompanyDbPath } from "../shared";

// --------------------------------------------------------------------------
// Company settings (read-only) + CVR sync
// --------------------------------------------------------------------------

/**
 * The full company settings row plus the company's own payment/bank details.
 *
 * `payment` is the primary `bank_accounts` row resolved via
 * `core/company.ts#resolveCompanyPaymentDetails` — the same source every issued
 * invoice's payment block reads from. It is null when no bank account is
 * configured yet, which is exactly when the Cockpit must let the owner add one
 * (#284): without it, an invoice goes out with no payment instructions.
 */
export type CompanySettingsView = CompanySettings & {
  payment: CompanyPaymentDetails | null;
};

/**
 * The full company settings row, including the CVR-register stamdata and the
 * payment/bank details. Read-only — backs `GET /api/companies/:slug/company` so
 * the cockpit can show the synced address/branche/status and the bank account.
 */
export function buildCompanySettings(
  workspaceRoot: string,
  slug: string,
): CompanySettingsView {
  const db = openDb(requireCompanyDbPath(workspaceRoot, slug));
  try {
    migrate(db);
    const settings = getCompanySettings(db);
    const payment = resolveCompanyPaymentDetails(db, settings.currency) ?? null;
    return { ...settings, payment };
  } finally {
    db.close();
  }
}

/**
 * Refresh a company's CVR-register stamdata. Backs
 * `POST /api/companies/:slug/sync-cvr`. The CVR lookup runs server-side so the
 * CVR credentials never reach the browser.
 */
export async function syncCompanyCvr(
  workspaceRoot: string,
  slug: string,
): Promise<SyncCompanyFromCvrResult> {
  const db = openDb(requireCompanyDbPath(workspaceRoot, slug));
  try {
    migrate(db);
    return await syncCompanyFromCvr(db);
  } finally {
    db.close();
  }
}
