/**
 * MCP portfolio tools — the single-entry surface of multi-company (#172).
 *
 * Principle: **mutations stay single-company; only reads fan out.** A bookkeeper
 * managing several legal entities gets exactly two new tools — no per-action
 * endpoint explosion. Cross-company *actions* (e.g. "back up all") are the agent
 * looping an existing single-company tool with a different `company` slug.
 *
 *  - `company_add` (write-irreversible) — workspace-level write tool wrapping
 *    the core `createCompany`: creates a company volume under
 *    `<workspace>/<slug>/` and registers it in the workspace manifest. Like
 *    every other write tool it is gated by `confirm: true` (#252) — an agent
 *    cannot create a company by accident. It is classified `write-irreversible`
 *    (consistently with `docs/mcp-tool-surface.md`): creating a company volume
 *    is not undoable by a counter-posting, the same reasoning that places the
 *    `system_backup_*` configuration tools in that class.
 *  - `portfolio_overview` (read) — one read tool that *juxtaposes* per-company
 *    status across the workspace. Each company is a separate legal entity, so
 *    figures are listed side by side, never consolidated/summed.
 *
 * Both tools take an explicit `workspace` root; when omitted they fall back to
 * the configured `RENTEMESTER_WORKSPACE`.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { existsSync } from "node:fs";
import { z } from "zod";
import { createCompany, getCompanySettings } from "../../core/company";
import { companyPaths } from "../../core/paths";
import { vatPeriodWindowFor } from "../../core/periods";
import { diffDaysSafe as daysBetween } from "../../core/dates";
import { openDb, migrate } from "../../core/db";
import { verifyAuditChain } from "../../core/ledger";
import { buildInvoiceList } from "../../core/invoice-list";
import { listExceptions } from "../../core/exceptions";
import { buildVatReport } from "../../core/vat";
import { getBackupComplianceStatus } from "../../core/system-backups";
import {
  companyRootForSlug,
  listWorkspaceCompanies,
  resolveConfiguredWorkspaceRoot,
  resolveWorkspaceRoot,
} from "../../core/workspace";
import { envelopeShape, envelopeToCallResult, errorEnvelope, successEnvelope } from "../envelope";
import { confirmField, redactPaths } from "../tool-runtime";

const ISO_DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

/**
 * Resolves the workspace root for a portfolio tool: an explicit `workspace`
 * argument wins; otherwise `RENTEMESTER_WORKSPACE`. Both are `..`-guarded.
 */
function resolvePortfolioWorkspace(raw?: string): { ok: true; root: string } | { ok: false; error: string } {
  const trimmed = typeof raw === "string" ? raw.trim() : "";
  try {
    if (trimmed.length > 0) {
      return { ok: true, root: resolveWorkspaceRoot(trimmed) };
    }
    const configured = resolveConfiguredWorkspaceRoot();
    if (!configured) {
      return {
        ok: false,
        error: "no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE",
      };
    }
    return { ok: true, root: configured };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

/** Pure UTC wall-clock date — read tools may sample the clock; the cores stay pure. */
function todayIsoDate(): string {
  const now = new Date();
  const y = now.getUTCFullYear();
  const m = String(now.getUTCMonth() + 1).padStart(2, "0");
  const d = String(now.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/**
 * Builds the juxtaposed status row for a single company. Opens, migrates and
 * closes the company's ledger; failures are captured per-company so one broken
 * volume never sinks the whole overview.
 */
function companyStatusRow(
  slug: string,
  registeredName: string,
  companyRoot: string,
  asOfDate: string,
): Record<string, unknown> {
  const base = { slug, name: registeredName };
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    return { ...base, ok: false, error: "company ledger not found on disk" };
  }
  const db = openDb(dbPath);
  try {
    migrate(db);
    const settings = getCompanySettings(db);
    // The VAT period window follows the company's real SKAT cadence
    // (`vatPeriodType`) — a monthly filer gets a one-month window, a
    // half-yearly filer a six-month one. `core/periods.ts` owns the math.
    const period = vatPeriodWindowFor(asOfDate, settings.vatPeriodType);
    const vat = buildVatReport(db, period.start, period.end);
    const open = buildInvoiceList(db, { status: "open", asOfDate });
    const overdue = buildInvoiceList(db, { status: "overdue", asOfDate });
    const exceptions = listExceptions(db, { status: "open" });
    const backup = getBackupComplianceStatus(db, companyRoot, asOfDate);
    const audit = verifyAuditChain(db);
    const openReceivables = open.rows.reduce((acc, r) => acc + r.openBalance, 0);
    return {
      ...base,
      name: settings.name || registeredName,
      cvr: settings.cvr,
      ok: true,
      vat: {
        periodStart: period.start,
        periodEnd: period.end,
        daysUntilDue: daysBetween(asOfDate, period.end),
        netVatPayable: vat.ok ? vat.netVatPayable : null,
        canCompute: vat.ok,
      },
      openReceivables: {
        count: open.count,
        totalOpenBalance: openReceivables,
        overdueCount: overdue.count,
      },
      backup: {
        backupDue: backup.backupDue,
        daysSinceLatestBackup: backup.daysSinceLatestBackup,
        hasActivitySinceBackup: backup.hasActivitySinceBackup,
        backupsFound: backup.backupsFound,
      },
      audit: { ok: audit.ok, entries: audit.entries, firstError: audit.errors[0] ?? null },
      openExceptions: {
        count: exceptions.count,
        highSeverityCount: exceptions.rows.filter((r: { severity: string }) => r.severity === "high").length,
      },
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { ...base, ok: false, error: redactPaths(message) };
  } finally {
    db.close();
  }
}

export function registerPortfolioTools(server: McpServer): void {
  server.registerTool(
    "company_add",
    {
      title: "Add a company to the workspace",
      description:
        "Opretter en ny virksomhed i workspace'et: udleder/validerer et slug, bygger virksomheds-volumen " +
        "under <workspace>/<slug>/, initialiserer ledgeren og registrerer virksomheden i workspace-manifestet. " +
        "Workspace-niveau write-tool — kalder kerne-funktionen createCompany. " +
        "Mutationer er altid enkelt-virksomhed; dette tool tilføjer netop én. " +
        "write-irreversible.",
      inputSchema: {
        workspace: z
          .string()
          .optional()
          .describe(
            "Absolute path to the workspace root. The new company is created at " +
              "<workspace>/<slug>/. When omitted, the workspace falls back to the " +
              "RENTEMESTER_WORKSPACE environment variable on the MCP server's host; " +
              "if that is also unset the call is rejected with " +
              "\"no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE\".",
          ),
        name: z
          .string()
          .min(1)
          .describe("Registered name of the company, e.g. 'Acme ApS'."),
        slug: z
          .string()
          .optional()
          .describe(
            "Optional URL-safe slug used as the company's directory name under the workspace. " +
              "When omitted, a slug is derived from `name`.",
          ),
        cvr: z.string().optional().describe("Optional Danish CVR number for the company, e.g. '12345678'."),
        fiscalYearStartMonth: z
          .number()
          .int()
          .min(1)
          .max(12)
          .optional()
          .describe("Month the fiscal year starts, 1 = January … 12 = December (default 1)."),
        fiscalYearLabelStrategy: z
          .enum(["end-year", "start-year", "span"])
          .optional()
          .describe(
            "How a fiscal year spanning two calendar years is labelled: " +
              "'end-year' = the year it ends in; 'start-year' = the year it starts in; " +
              "'span' = both years (e.g. '2025/2026').",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    async (args: {
      workspace?: string;
      name: string;
      slug?: string;
      cvr?: string;
      fiscalYearStartMonth?: number;
      fiscalYearLabelStrategy?: "end-year" | "start-year" | "span";
      confirm?: boolean;
    }) => {
      // `company_add` is a write tool — it creates a company directory and
      // initialises a ledger. Like every other write tool it refuses to run
      // unless `confirm: true` is set, so an agent cannot create a company by
      // accident. The check happens before the workspace is even resolved.
      // (#252)
      if (args.confirm !== true) {
        return envelopeToCallResult(
          errorEnvelope("confirm: true required for write tool company_add"),
        );
      }
      const ws = resolvePortfolioWorkspace(args.workspace);
      if (!ws.ok) {
        return envelopeToCallResult(errorEnvelope(redactPaths(ws.error)));
      }
      try {
        const result = createCompany(ws.root, {
          name: args.name,
          slug: args.slug,
          cvr: args.cvr,
          fiscalYearStartMonth: args.fiscalYearStartMonth,
          fiscalYearLabelStrategy: args.fiscalYearLabelStrategy,
        });
        return envelopeToCallResult(
          successEnvelope({ slug: result.slug, name: result.name }),
        );
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        console.error(`[mcp:company_add] ${message}`);
        return envelopeToCallResult(errorEnvelope(redactPaths(message)));
      }
    },
  );

  server.registerTool(
    "portfolio_overview",
    {
      title: "Cross-company portfolio overview",
      description:
        "Read-only. Returnerer status side om side for hver virksomhed i workspace'et: momsdeadline, " +
        "åbne tilgodehavender, backup-status, audit-status og åbne exceptions. " +
        "Tallene er JUXTAPOSEREDE — hver virksomhed er en selvstændig juridisk enhed, så intet " +
        "konsolideres eller summeres på tværs af enhederne. Cross-company handlinger udføres ved at " +
        "loope de eksisterende enkelt-virksomheds-tools.",
      inputSchema: {
        workspace: z
          .string()
          .optional()
          .describe(
            "Absolute path to the workspace root whose companies are juxtaposed. " +
              "When omitted, the workspace falls back to the RENTEMESTER_WORKSPACE " +
              "environment variable on the MCP server's host; if that is also unset " +
              "the call is rejected with " +
              "\"no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE\".",
          ),
        asOf: z
          .string()
          .regex(ISO_DATE_RE, "asOf must be YYYY-MM-DD")
          .optional()
          .describe(
            "Optional as-of date (YYYY-MM-DD) for the status snapshot — VAT " +
              "deadlines, open receivables and backup compliance are evaluated " +
              "against it. When omitted, the server's current UTC date is used.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    async (args: { workspace?: string; asOf?: string }) => {
      const ws = resolvePortfolioWorkspace(args.workspace);
      if (!ws.ok) {
        return envelopeToCallResult(errorEnvelope(redactPaths(ws.error)));
      }
      const asOfDate = args.asOf ?? todayIsoDate();
      let registered;
      try {
        registered = listWorkspaceCompanies(ws.root);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        return envelopeToCallResult(errorEnvelope(redactPaths(message)));
      }
      const active = registered.filter((c) => !c.archived);
      const companies = active
        .slice()
        .sort((a, b) => a.slug.localeCompare(b.slug))
        .map((entry) =>
          companyStatusRow(
            entry.slug,
            entry.name,
            companyRootForSlug(ws.root, entry.slug),
            asOfDate,
          ),
        );
      return envelopeToCallResult(
        successEnvelope({ asOf: asOfDate, companyCount: companies.length, companies }),
      );
    },
  );
}
