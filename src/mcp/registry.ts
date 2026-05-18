/**
 * Central tools-registrering for Rentemester-MCP-serveren.
 *
 * Tool-surface (jf. docs/mcp-tool-surface.md):
 *  - read (22):           audit, accounts, bank list/suggest/reconcile,
 *                         customer list/validate-vat, vendor list,
 *                         documents list, exceptions list,
 *                         invoice status/list/find/overdue/interest/compensation/validate,
 *                         journal list, period list, retention status,
 *                         system backup-status/healthcheck, vat report
 *  - write-reversible (5): bank import, customer create, vendor create,
 *                          documents ingest, exception resolve
 *  - write-irreversible (~21): invoice issue/post/render/credit-note/
 *                              settle-bank/settle-claim-bank/write-off-bad-debt/
 *                              apply-payment/refund-bank/remind/post-reminder/
 *                              claim-interest/post-interest/claim-compensation/
 *                              post-compensation; journal post/reverse;
 *                              expense book; vat post-eu-service/post-representation;
 *                              period close; system backup/export-authority
 *  - destructive (1):     system restore-backup
 *
 * Hver `register*Tools(server)`-funktion lever i `src/mcp/tools/<area>.ts`
 * og tilføjer kun sit eget domæne. Det gør tool-surface'en tæt på 1:1 med
 * `src/cli-meta.ts` og holder hver fil overskuelig.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerAccountsTools } from "./tools/accounts";
import { registerAuditTools } from "./tools/audit";
import { registerBankTools } from "./tools/bank";
import { registerCustomerTools } from "./tools/customer";
import { registerDocumentTools } from "./tools/documents";
import { registerExceptionTools } from "./tools/exceptions";
import { registerExpenseTools } from "./tools/expense";
import { registerInvoiceTools } from "./tools/invoice";
import { registerJournalTools } from "./tools/journal";
import { registerPeriodTools } from "./tools/period";
import { registerRetentionTools } from "./tools/retention";
import { registerSystemTools } from "./tools/system";
import { registerVatTools } from "./tools/vat";
import { registerVendorTools } from "./tools/vendor";

export function registerAllTools(server: McpServer): void {
  registerAccountsTools(server);
  registerAuditTools(server);
  registerBankTools(server);
  registerCustomerTools(server);
  registerDocumentTools(server);
  registerExceptionTools(server);
  registerExpenseTools(server);
  registerInvoiceTools(server);
  registerJournalTools(server);
  registerPeriodTools(server);
  registerRetentionTools(server);
  registerSystemTools(server);
  registerVatTools(server);
  registerVendorTools(server);
}
