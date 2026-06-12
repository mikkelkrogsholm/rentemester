// Bank import + bank-account write handlers (#213 slice 2, #345).

import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { syncUnmatchedBankTransactionExceptions } from "../../core/exceptions";
import { addBankAccount, importBankCsv } from "../../core/bank";
import type { ServerConfig } from "../config";
import { withCompanyMutation } from "../mutations";
import {
  MAX_UPLOAD_BODY_BYTES,
  okResponse,
  optionalBodyString,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/bank/import — imports a bank-statement CSV.
 *
 * Body: `{ csvContent: string, account?: string, profile?: string,
 * confirm: true }`. The frontend reads the chosen CSV file in the browser and
 * POSTs its text as `csvContent`; the handler writes it to a `mkdtemp` file
 * and calls the SAME `importBankCsv` core function the CLI/MCP use, then runs
 * `syncUnmatchedBankTransactionExceptions` exactly as `bank import` does.
 *
 * Destructive (it appends ledger rows) so `requireConfirm` is set — the body
 * must carry `confirm: true`. A `maxBodyBytes` cap hardens the upload route.
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply.
 */
export async function handleBankImport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const csvContent = requireBodyString(body, "csvContent");
      const account = optionalBodyString(body, "account");
      const profile = optionalBodyString(body, "profile");

      // Mirror the MCP `csvContent` pattern: persist the inline CSV to a
      // private temp file, then hand core a path — core reads from disk and
      // copies the file into the company dir, so the temp dir is a transient
      // staging area that must be removed on EVERY exit path (success, import
      // rejection or throw). Without the finally each cockpit import — and each
      // retried/failing one — leaked a temp dir forever (matches the MCP bank
      // tool's #383 cleanup).
      const tmpDir = mkdtempSync(join(tmpdir(), "rentemester-cockpit-bank-"));
      try {
        const csvPath = join(tmpDir, "bank-import.csv");
        writeFileSync(csvPath, csvContent, "utf8");

        const imported = importBankCsv(ctx.db, ctx.companyRoot, csvPath, {
          account,
          profile,
        });
        // The CLI/MCP both sync unmatched-transaction exceptions after a
        // successful import — replicate that so the Cockpit behaves identically.
        const sync = imported.ok
          ? syncUnmatchedBankTransactionExceptions(ctx.db)
          : { ok: true, created: 0, errors: [] };
        return {
          ...(imported as Record<string, unknown>),
          ok: imported.ok,
          errors: imported.errors,
          exceptionsCreated: sync.created,
        };
      } finally {
        rmSync(tmpDir, { recursive: true, force: true });
      }
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  // The core `BankImportResult` shape is echoed back so the UI can report the
  // batch id, the imported/skipped counts and any balance warnings.
  return okResponse({
    import: {
      importBatchId: result.importBatchId,
      imported: result.imported ?? 0,
      skippedDuplicates: result.skippedDuplicates ?? 0,
      skippedDuplicateRows: result.skippedDuplicateRows ?? [],
      bankAccountSlug: result.bankAccountSlug,
      profile: result.profile,
      balanceWarnings: result.balanceWarnings ?? [],
      exceptionsCreated: result.exceptionsCreated ?? 0,
    },
  });
}

/**
 * POST /api/companies/:slug/bank-accounts — opretter en bankkonto (#345).
 *
 * Body: `{ name, slug?, bankName?, registrationNo?, accountNo?, iban?,
 * currency?, ledgerAccountNo? }`. Wrapper omkring `addBankAccount` fra
 * kernen. Backup-lock + actor-attribution sker via `withCompanyMutation`.
 */
export async function handleCreateBankAccount(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const name = requireBodyString(body, "name");
      const accSlug = optionalBodyString(body, "slug");
      const bankName = optionalBodyString(body, "bankName");
      const registrationNo = optionalBodyString(body, "registrationNo");
      const accountNo = optionalBodyString(body, "accountNo");
      const iban = optionalBodyString(body, "iban");
      const currency = optionalBodyString(body, "currency");
      const ledgerAccountNo = optionalBodyString(body, "ledgerAccountNo");
      const created = addBankAccount(ctx.db, {
        name,
        ...(accSlug ? { slug: accSlug } : {}),
        ...(bankName ? { bankName } : {}),
        ...(registrationNo ? { registrationNo } : {}),
        ...(accountNo ? { accountNo } : {}),
        ...(iban ? { iban } : {}),
        ...(currency ? { currency } : {}),
        ...(ledgerAccountNo ? { ledgerAccountNo } : {}),
      });
      // Marker actor på audit-log'en (write går gennem withCompanyMutation,
      // som sørger for at append-only audit-log fanger den).
      void ctx.actor;
      if (!created.ok) {
        return { ok: false, account: null, errors: created.errors };
      }
      return { ok: true, account: created.account, errors: [] as string[] };
    },
  );
  return okResponse({ bankAccount: result.account });
}
