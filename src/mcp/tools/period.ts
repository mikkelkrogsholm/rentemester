/**
 * MCP-tools for regnskabsperioder.
 *
 *  - `period_close` (write-irreversible) — lukker eller markerer en periode
 *  - `period_list` (read) — lister kendte regnskabsperioder. Bemærk: CLI har
 *    ikke en dedikeret `period list` endnu (jf. docs/mcp-tool-surface.md);
 *    vi eksponerer her tabellen direkte fra MCP for at give agenter et
 *    minimums-view uden at gå udenom kerne-laget.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  closeAccountingPeriod,
  type AccountingPeriodKind,
} from "../../core/periods";
import { computePeriodCloseReadiness, loadPeriodCloseReview, periodCloseReviewSchemaAvailable, projectHumanReadiness, reviewPeriodCloseReadiness } from "../../core/period-close-readiness";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, withCompanyReadOnlyDb, confirmField } from "../tool-runtime";
import { currentMcpAuthenticatedPrincipal, mcpHasLiveCompanyPermission } from "../security";

export function registerPeriodTools(server: McpServer): void {
  server.registerTool("period_close_readiness", { title: "Inspect close readiness", description: "Computes a deterministic, read-only close-readiness packet and non-durable human status. Review it explicitly before close.", inputSchema: { company: z.string().min(1), from: z.string().min(1), to: z.string().min(1) }, outputSchema: envelopeShape, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyReadOnlyDb<{company:string;from:string;to:string}>(({db,args}) => { const packet=computePeriodCloseReadiness(db, { periodStart: args.from, periodEnd: args.to, companyRoot: args.company }); return successEnvelope({ packet, readiness: projectHumanReadiness(packet) }); },{allowSchemaNotCurrent:true}));
  server.registerTool("period_close_status", { title: "Read durable close review", description: "Reads a persisted review packet without recomputing readiness.", inputSchema: { company:z.string().min(1), reviewId:z.number().int().positive() }, outputSchema:envelopeShape, annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false} }, withCompanyReadOnlyDb<{company:string;reviewId:number}>(({db,args})=>periodCloseReviewSchemaAvailable(db)?successEnvelope({review:loadPeriodCloseReview(db,args.reviewId)}):successEnvelope({review:null,status:"unavailable",code:"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE"}),{allowSchemaNotCurrent:true}));
  server.registerTool("period_close_review", { title:"Persist period-close review", description:"Persists the exact inspected readiness packet for later close. Requires actor attribution and confirm:true; retry creates a new immutable review and does not close the period. write-reversible.", inputSchema:{company:z.string().min(1),from:z.string().min(1),to:z.string().min(1),confirm:confirmField},outputSchema:envelopeShape,annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:false,openWorldHint:false}},withCompanyDbConfirmed<{company:string;from:string;to:string;confirm?:boolean}>(server,"period_close_review",({db,actor,args})=>{const packet=computePeriodCloseReadiness(db,{periodStart:args.from,periodEnd:args.to,companyRoot:args.company});const authenticated=currentMcpAuthenticatedPrincipal();return successEnvelope({review:reviewPeriodCloseReadiness(db,{packet,reviewerActor:actor.createdBy,reviewerPrincipal:authenticated?{kind:authenticated.kind,subjectId:authenticated.subjectId}:{kind:"local-trusted",subjectId:actor.createdBy}})});}));
  server.registerTool(
    "period_list",
    {
      title: "List accounting periods",
      description:
        "Lister regnskabsperioder (open/closed/reported). Read-only. " +
        "Rækkefølge: period_end DESC, id DESC (nyeste først, deterministisk).",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const rows = db
        .query(
          `SELECT id, period_start, period_end, kind, status, reference, created_at
           FROM accounting_periods
           ORDER BY period_start DESC, id DESC`,
        )
        .all() as Array<{
          id: number;
          period_start: string;
          period_end: string;
          kind: string;
          status: string;
          reference: string | null;
          created_at: string;
        }>;
      return successEnvelope({
        periods: rows.map((row) => ({
          id: row.id,
          periodStart: row.period_start,
          periodEnd: row.period_end,
          kind: row.kind,
          status: row.status,
          reference: row.reference,
          createdAt: row.created_at,
        })),
        count: rows.length,
      });
    }),
  );

  server.registerTool(
    "period_close",
    {
      title: "Close accounting period",
      description: "Lukker eller markerer regnskabsperiode (closed/reported). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Start of the period to close (inclusive), in YYYY-MM-DD format."),
        to: z
          .string()
          .min(1)
          .describe("End of the period to close (inclusive), in YYYY-MM-DD format. Must not be before `from`."),
        kind: z
          .enum(["vat_period", "vat_quarter", "fiscal_year", "custom"])
          .optional()
          .describe(
            "Type of accounting period (default 'vat_period'): 'vat_period' = a VAT period for the company's registered cadence; " +
              "'vat_quarter' is accepted only as a legacy alias; " +
              "'fiscal_year' = a full fiscal year; 'custom' = an arbitrary date range. " +
              "A new period must not overlap an existing period of the same kind.",
          ),
        status: z
          .enum(["closed", "reported"])
          .optional()
          .describe(
            "The status to mark the period with (default 'closed'). " +
              "'closed' = the period is locked: no further bookkeeping writes are accepted into it. " +
              "A 'closed' period can still be reopened via the CLI-only `rentemester period reopen`. " +
              "'reported' = the period is closed AND has been reported to the authority (e.g. the VAT " +
              "return filed); a reported timestamp is recorded in addition to the lock. " +
              "WARNING: selecting 'reported' is IRREVERSIBLE — a reported period can NEVER be " +
              "reopened, not even via `period reopen`. Only use 'reported' once the filing is final.",
          ),
        reference: z
          .string()
          .optional()
          .describe("Optional external reference for the closure, e.g. a VAT-return receipt number."),
        force: z
          .boolean()
          .optional()
          .describe(
            "When true, bypass the open-high/medium-exceptions safety check " +
              "(Batch D-7). The default (false) rejects the close with a clear " +
              "error listing the open exception IDs that fall inside the period, " +
              "so the owner cannot silently hide outstanding items by closing.",
          ),
        // Optional at schema level so a missing confirm is consistently
        // returned as the shared confirmation envelope before business-input
        // validation. The core still rejects missing values fail-closed.
        packetHash: z.string().length(64).optional().describe("Exact hash returned by period_close_readiness."),
        reviewId: z.number().int().positive().optional().describe("Exact persisted review id returned by period_close_review."),
        reason: z.string().min(1).optional().describe("Mandatory non-empty waiver reason when force is true."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      from: string;
      to: string;
      kind?: AccountingPeriodKind;
      status?: "closed" | "reported";
      reference?: string;
      force?: boolean;
      packetHash?: string;
      reviewId?: number;
      reason?: string;
      confirm?: boolean;
    }>(server, "period_close", ({ db, actor, args }) => {
      const authenticated = currentMcpAuthenticatedPrincipal();
      const forceAuthorization = args.force && authenticated && mcpHasLiveCompanyPermission(args.company, "company.period.force-close")
        ? { principal: { kind: authenticated.kind, subjectId: authenticated.subjectId }, permissions: ["company.period.force-close"] as const }
        : undefined;
      const result = closeAccountingPeriod(db, {
        periodStart: args.from,
        periodEnd: args.to,
        kind: args.kind,
        status: args.status,
        reference: args.reference,
        force: args.force,
        readinessPacketHash: args.packetHash,
        readinessReviewId: args.reviewId,
        forceReason: args.reason,
        forceAuthorization,
        forceConfirmed: args.confirm === true,
        companyRoot: args.company,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );
}
