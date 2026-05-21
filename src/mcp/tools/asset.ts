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
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../tool-runtime";

export function registerAssetTools(server: McpServer): void {
  server.registerTool(
    "asset_register",
    {
      title: "Register a depreciable fixed asset",
      description:
        "Registrerer et aktiv med en lineær afskrivningsplan. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        name: z.string().min(1),
        category: z.string().min(1),
        acquisitionDate: z.string().min(1),
        cost: z.number().positive(),
        usefulLifeMonths: z.number().int().positive(),
        documentId: z.number().int().positive(),
        assetAccount: z.string().optional(),
        depreciationAccount: z.string().optional(),
        accumulatedAccount: z.string().optional(),
        note: z.string().optional(),
        confirm: z.boolean().optional(),
      },
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
      confirm: boolean;
    }>(server, "asset_register", ({ db, args }) => {
      const result = registerAsset(db, {
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
      });
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
        company: z.string().min(1),
        assetId: z.number().int().positive(),
        period: z.number().int().positive(),
        date: z.string().min(1),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      assetId: number;
      period: number;
      date: string;
      confirm: boolean;
    }>(server, "asset_depreciate", ({ db, args }) => {
      const result = postDepreciationPeriod(db, {
        assetId: args.assetId,
        periodIndex: args.period,
        transactionDate: args.date,
      });
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
        company: z.string().min(1),
        name: z.string().min(1),
        category: z.string().min(1),
        acquisitionDate: z.string().min(1),
        cost: z.number().positive(),
        documentId: z.number().int().positive(),
        expenseAccount: z.string().min(1),
        date: z.string().min(1),
        thresholdRuleSource: z.string().min(1),
        confirmImmediateWriteOff: z.boolean(),
        paymentAccount: z.string().optional(),
        note: z.string().optional(),
        confirm: z.boolean().optional(),
      },
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
      confirm: boolean;
    }>(server, "asset_write_off", ({ db, args }) => {
      const result = postImmediateWriteOff(db, {
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
      });
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
        company: z.string().min(1),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const result = buildAssetRegisterReport(db);
      return wrapCoreResult(result);
    }),
  );
}
