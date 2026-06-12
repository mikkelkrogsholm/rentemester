/**
 * MCP-tools for fixed assets (#124 depreciation, #125 immediate write-off).
 *
 * 1:1-mapping af CLI-kommandoerne `asset register`, `asset depreciate`,
 * `asset write-off` (write-irreversible) og `asset register-report` (read).
 * Rentemester assisterer workflowet; bruger/revisor ejer den skattemæssige
 * vurdering af kapitalisering, afskrivningsplan og straksafskrivning.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerAsset,
  postDepreciationPeriod,
  postImmediateWriteOff,
  buildAssetRegisterReport,
} from "../../core/assets";
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

export function registerAssetTools(server: McpServer): void {
  server.registerTool(
    "asset_register",
    {
      title: "Register a depreciable fixed asset",
      description:
        "Registrerer et aktiv med en lineær afskrivningsplan. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        name: z.string().min(1).describe("Human-readable name of the asset, e.g. 'MacBook Pro 14\"'."),
        category: z.string().min(1).describe("Asset category (free text), e.g. 'IT-udstyr' or 'Inventar'."),
        acquisitionDate: z.string().min(1).describe("Date the asset was acquired, in YYYY-MM-DD format."),
        cost: z
          .number()
          .positive()
          .describe("Acquisition cost, in kroner (decimal DKK, 2 decimals — NOT øre). Must be positive."),
        usefulLifeMonths: z
          .number()
          .int()
          .positive()
          .describe("Useful life over which the asset is depreciated linearly, in whole months (a positive integer)."),
        documentId: z
          .number()
          .int()
          .positive()
          .describe("ID of the purchase document (bilag) for this asset. See documents_list."),
        assetAccount: z
          .string()
          .optional()
          .describe("Optional balance-sheet asset account number (default '5800' Driftsmidler og inventar)."),
        depreciationAccount: z
          .string()
          .optional()
          .describe("Optional depreciation-expense account number (default '5820' Afskrivninger)."),
        accumulatedAccount: z
          .string()
          .optional()
          .describe("Optional accumulated-depreciation contra-asset account number (default '5810')."),
        note: z.string().optional().describe("Optional free-text note stored on the asset."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      name: string;
      category: string;
      acquisitionDate: string;
      cost: number;
      usefulLifeMonths: number;
      documentId: number;
      assetAccount?: string;
      depreciationAccount?: string;
      accumulatedAccount?: string;
      note?: string;
      confirm?: boolean;
    }>(server, "asset_register", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the asset-acquisition posting to
      // the booking agent in the hash chain + audit_log, not the OS user.
      const result = registerAsset(
        db,
        withActor(
          {
            name: args.name,
            category: args.category,
            acquisitionDate: args.acquisitionDate,
            cost: args.cost,
            usefulLifeMonths: args.usefulLifeMonths,
            purchaseDocumentId: args.documentId,
            assetAccountNo: args.assetAccount,
            depreciationExpenseAccountNo: args.depreciationAccount,
            accumulatedDepreciationAccountNo: args.accumulatedAccount,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "asset_depreciate",
    {
      title: "Post a depreciation period for an asset",
      description:
        "Bogfører en periodes afskrivning (debet afskrivninger, kredit akkumulerede afskrivninger). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        assetId: z
          .number()
          .int()
          .positive()
          .describe("ID of the registered asset to depreciate. See asset_register_report."),
        period: z
          .number()
          .int()
          .positive()
          .describe("1-based index of the depreciation period to post (1 = the first month of the schedule)."),
        date: z.string().min(1).describe("Posting date for the depreciation entry, in YYYY-MM-DD format."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      assetId: number;
      period: number;
      date: string;
      confirm?: boolean;
    }>(server, "asset_depreciate", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the depreciation posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postDepreciationPeriod(
        db,
        withActor(
          {
            assetId: args.assetId,
            periodIndex: args.period,
            transactionDate: args.date,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "asset_write_off",
    {
      title: "Immediately write off a small asset (straksafskrivning)",
      description:
        "Bogfører straksafskrivning af et mindre aktiv. Kræver confirm og kildehenvisning til reglen; bruger/revisor ejer den skattemæssige vurdering. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        name: z.string().min(1).describe("Human-readable name of the asset being written off."),
        category: z.string().min(1).describe("Asset category (free text), e.g. 'IT-udstyr'."),
        acquisitionDate: z.string().min(1).describe("Date the asset was acquired, in YYYY-MM-DD format."),
        cost: z
          .number()
          .positive()
          .describe("Acquisition cost, in kroner (decimal DKK, 2 decimals — NOT øre). Must be positive."),
        documentId: z
          .number()
          .int()
          .positive()
          .describe("ID of the purchase document (bilag) for this asset. See documents_list."),
        expenseAccount: z
          .string()
          .min(1)
          .describe("Expense account number from the chart of accounts to write the cost off against, e.g. '3120'."),
        date: z.string().min(1).describe("Posting date for the write-off entry, in YYYY-MM-DD format."),
        thresholdRuleSource: z
          .string()
          .min(1)
          .describe(
            "Source-backed citation for the straksafskrivning threshold rule (e.g. the relevant " +
              "Afskrivningsloven §/circular). Required — the user/advisor owns the tax assessment.",
          ),
        confirmImmediateWriteOff: z
          .boolean()
          .describe(
            "Must be true to acknowledge that immediate write-off (straksafskrivning) — rather than " +
              "depreciation — is the chosen and appropriate treatment for this asset.",
          ),
        paymentAccount: z
          .string()
          .optional()
          .describe("Optional account number for the credit (payment) side, e.g. a bank account '2000'."),
        note: z.string().optional().describe("Optional free-text note stored with the write-off."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      name: string;
      category: string;
      acquisitionDate: string;
      cost: number;
      documentId: number;
      expenseAccount: string;
      date: string;
      thresholdRuleSource: string;
      confirmImmediateWriteOff: boolean;
      paymentAccount?: string;
      note?: string;
      confirm?: boolean;
    }>(server, "asset_write_off", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the straksafskrivning posting to
      // the booking agent in the hash chain + audit_log, not the OS user.
      const result = postImmediateWriteOff(
        db,
        withActor(
          {
            name: args.name,
            category: args.category,
            acquisitionDate: args.acquisitionDate,
            cost: args.cost,
            purchaseDocumentId: args.documentId,
            expenseAccountNo: args.expenseAccount,
            transactionDate: args.date,
            confirmImmediateWriteOff: args.confirmImmediateWriteOff,
            thresholdRuleSource: args.thresholdRuleSource,
            paymentAccountNo: args.paymentAccount,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "asset_register_report",
    {
      title: "Asset register and accumulated depreciation report",
      description:
        "Viser aktivregister med akkumulerede afskrivninger og bogført værdi. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const result = buildAssetRegisterReport(db);
      return wrapCoreResult(result);
    }),
  );
}
