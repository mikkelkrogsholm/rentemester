/**
 * Central tools-registrering for Rentemester-MCP-serveren.
 *
 * `registerAllTools` registrerer hele tool-surface'en — 95 tools fordelt
 * på de domæne-funktioner der kaldes herunder. Den autoritative liste
 * (klassifikation, inputs, CLI-mapping) står i docs/mcp-tool-surface.md;
 * driv en kørende server med `tools/list` for den faktiske, aktuelle liste.
 * Tæl-konsistens er bevogtet af tests/unit/mcp-tool-count-docs.test.ts (#367).
 *
 * Sikkerhedsklasser:
 *  - read              — bivirkningsfrie; må kaldes frit og parallelt.
 *  - write-reversible  — opretter state der kan tilbageføres; kræver
 *                        `confirm: true`.
 *  - write-irreversible — bogfører i den append-only hash-kæde; kræver
 *                         `confirm: true`. Rettes kun via en modpostering.
 *  - destructive       — `system_restore_backup`; kræver derudover
 *                        `confirmText`.
 *
 * Hver `register*Tools(server)`-funktion lever i `src/mcp/tools/<area>.ts`
 * og tilføjer kun sit eget domæne. Det holder hver fil overskuelig og
 * tool-surface'en tæt på 1:1 med `src/cli-meta.ts` (kendte afvigelser er
 * dokumenteret i docs/mcp-tool-surface.md).
 */

import { existsSync } from "node:fs";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { migrate, openDb } from "../core/db";
import { companyPaths } from "../core/paths";
import { evaluateBackupLock } from "../core/backup-governance";
import { resolveCompanyArg } from "./tool-runtime";
import { envelopeToCallResult, errorEnvelope } from "./envelope";
import { registerAccountsTools } from "./tools/accounts";
import { registerAuditTools } from "./tools/audit";
import { registerBankTools } from "./tools/bank";
import { registerCustomerTools } from "./tools/customer";
import { registerCvrTools } from "./tools/cvr";
import { registerDocumentTools } from "./tools/documents";
import { registerExceptionTools } from "./tools/exceptions";
import { registerExpenseTools } from "./tools/expense";
import { registerInvoiceTools } from "./tools/invoice";
import { registerJournalTools } from "./tools/journal";
// PEPPOL submission (#128)
import { registerPeppolTools } from "./tools/peppol";
import { registerPeriodTools } from "./tools/period";
import { registerRetentionTools } from "./tools/retention";
import { registerSystemTools } from "./tools/system";
import { registerVatTools } from "./tools/vat";
import { registerVendorTools } from "./tools/vendor";
// ===== RECURRING INVOICES (#118) =====
import { registerRecurringInvoiceTools } from "./tools/recurring-invoice";
// ===== END RECURRING INVOICES (#118) =====
// ===== MAIL INTAKE (#122) =====
import { registerMailIntakeTools } from "./tools/mail-intake";
// ===== IMAP INTAKE (#181) =====
import { registerImapIntakeTools } from "./tools/imap-intake";
// ===== END IMAP INTAKE (#181) =====
// ===== MILEAGE LOG (#123) =====
import { registerMileageTools } from "./tools/mileage";
// Fixed assets (#124, #125)
import { registerAssetTools } from "./tools/asset";
// Multi-company portfolio: company_add + portfolio_overview (#172)
import { registerPortfolioTools } from "./tools/portfolio";
// ===== EMAIL DELIVERY (#180) =====
import { registerEmailTools } from "./tools/email";
// ===== END EMAIL DELIVERY (#180) =====
// ===== IMPORT ARCHIVE (#197) =====
import { registerImportTools } from "./tools/import";
// ===== END IMPORT ARCHIVE (#197) =====
// ===== TAX RETURN PREPARATION =====
import { registerTaxTools } from "./tools/tax";
// ===== END TAX RETURN PREPARATION =====
// ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
import { registerAccrualTools } from "./tools/accrual";
// ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
// ===== BUDGET + LIQUIDITY FORECAST =====
import { registerBudgetTools } from "./tools/budget";
// ===== END BUDGET + LIQUIDITY FORECAST =====
// ===== PAYABLES / KREDITORSTYRING =====
import { registerPayableTools } from "./tools/payable";
// ===== END PAYABLES / KREDITORSTYRING =====

// Wraps a write tool's callback with the opt-in backup lock. The MCP tool
// files are not uniform — some use the withCompanyDbConfirmed helper, some
// have inline handlers — so the lock cannot live in one helper. Intercepting
// registerTool catches every tool regardless of how its handler is written.
function lockGuardedCallback(
  toolName: string,
  callback: (...args: unknown[]) => unknown,
): (...args: unknown[]) => unknown {
  return (...invocation: unknown[]) => {
    const args = invocation[0] as { company?: unknown } | undefined;
    const company = args?.company;
    if (typeof company === "string" && company.length > 0) {
      const resolution = resolveCompanyArg(company);
      if (resolution.ok && existsSync(companyPaths(resolution.companyRoot).db)) {
        const db = openDb(companyPaths(resolution.companyRoot).db);
        try {
          migrate(db);
          const lock = evaluateBackupLock(db, resolution.companyRoot);
          if (lock.locked) {
            return envelopeToCallResult(
              errorEnvelope(
                `Bogføring er låst (${toolName}): ${lock.reason}. ` +
                  "Kør system_backup med archive:true for at låse op; placér derefter " +
                  "kopien på en EU/EØS-destination med system_backup_place.",
              ),
            );
          }
        } finally {
          db.close();
        }
      }
    }
    return callback(...invocation);
  };
}

// A registerTool-intercepting proxy: every write tool (not read-only, not a
// `system_*` tool) gets its callback wrapped with the backup lock. Read tools
// and system/backup tools pass through untouched — backing up must always
// stay possible, since that is the only way out of the lock.
function lockGuardServer(server: McpServer): McpServer {
  return new Proxy(server, {
    get(target, prop, receiver) {
      if (prop === "registerTool") {
        return (name: string, config: { annotations?: { readOnlyHint?: boolean } }, callback: (...a: unknown[]) => unknown) => {
          const readOnly = config?.annotations?.readOnlyHint === true;
          const guarded =
            readOnly || name.startsWith("system_") ? callback : lockGuardedCallback(name, callback);
          return (target.registerTool as (...a: unknown[]) => unknown)(name, config, guarded);
        };
      }
      const value = Reflect.get(target, prop, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

export function registerAllTools(server: McpServer): void {
  server = lockGuardServer(server);
  registerAccountsTools(server);
  registerAuditTools(server);
  registerBankTools(server);
  registerCustomerTools(server);
  registerCvrTools(server);
  registerDocumentTools(server);
  registerExceptionTools(server);
  registerExpenseTools(server);
  registerInvoiceTools(server);
  registerJournalTools(server);
  // PEPPOL submission (#128)
  registerPeppolTools(server);
  registerPeriodTools(server);
  registerRetentionTools(server);
  registerSystemTools(server);
  registerVatTools(server);
  registerVendorTools(server);
  // ===== RECURRING INVOICES (#118) =====
  registerRecurringInvoiceTools(server);
  // ===== END RECURRING INVOICES (#118) =====
  // ===== MAIL INTAKE (#122) =====
  registerMailIntakeTools(server);
  // ===== IMAP INTAKE (#181) =====
  registerImapIntakeTools(server);
  // ===== END IMAP INTAKE (#181) =====
  // ===== MILEAGE LOG (#123) =====
  registerMileageTools(server);
  // Fixed assets (#124, #125)
  registerAssetTools(server);
  // Multi-company portfolio: company_add + portfolio_overview (#172)
  registerPortfolioTools(server);
  // ===== EMAIL DELIVERY (#180) =====
  registerEmailTools(server);
  // ===== END EMAIL DELIVERY (#180) =====
  // ===== IMPORT ARCHIVE (#197) =====
  registerImportTools(server);
  // ===== END IMPORT ARCHIVE (#197) =====
  // ===== TAX RETURN PREPARATION =====
  registerTaxTools(server);
  // ===== END TAX RETURN PREPARATION =====
  // ===== ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  registerAccrualTools(server);
  // ===== END ACCRUALS / PERIODEAFGRÆNSNINGSPOSTER =====
  // ===== BUDGET + LIQUIDITY FORECAST =====
  registerBudgetTools(server);
  // ===== END BUDGET + LIQUIDITY FORECAST =====
  // ===== PAYABLES / KREDITORSTYRING =====
  registerPayableTools(server);
  // ===== END PAYABLES / KREDITORSTYRING =====
}
