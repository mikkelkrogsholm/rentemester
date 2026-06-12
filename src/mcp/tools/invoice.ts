/**
 * MCP-tools for fakturaer (issued invoices).
 *
 * Read:
 *   - invoice_status, invoice_list, invoice_find, invoice_overdue
 *   - invoice_interest_calc, invoice_compensation_calc
 *   - invoice_validate
 *
 * Write-irreversible:
 *   - invoice_issue, invoice_post, invoice_render
 *   - invoice_credit_note, invoice_settle_bank, invoice_settle_claim_bank
 *   - invoice_write_off_bad_debt, invoice_apply_payment, invoice_refund_bank
 *   - invoice_remind, invoice_post_reminder
 *   - invoice_claim_interest, invoice_post_interest
 *   - invoice_claim_compensation, invoice_post_compensation
 *
 * Implementation note (Batch G): the 22 tool registrations used to live
 * inline in this file. They were split into per-sub-domain helpers in
 * `./invoice/`. Each helper is called below in the same order the tools were
 * originally registered — MCP clients may rely on `tools/list` returning the
 * tools in this exact sequence.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerInvoiceQueryTools } from "./invoice/query";
import { registerInvoiceIssuanceTools } from "./invoice/issuance";
import { registerInvoiceSettlementTools } from "./invoice/settlement";
import { registerInvoiceReminderTools } from "./invoice/reminder";
import { registerInvoiceInterestTools } from "./invoice/interest";
import { registerInvoiceCompensationTools } from "./invoice/compensation";

export function registerInvoiceTools(server: McpServer): void {
  // Order is load-bearing — see the file-level comment.
  registerInvoiceQueryTools(server);
  registerInvoiceIssuanceTools(server);
  registerInvoiceSettlementTools(server);
  registerInvoiceReminderTools(server);
  registerInvoiceInterestTools(server);
  registerInvoiceCompensationTools(server);
}
