/**
 * MCP-tools for accruals / periodeafgrænsningsposter.
 *
 * 1:1-mapping af CLI-kommandoerne `accrual register`, `accrual recognize`
 * (write-irreversible) og `accrual register-report` (read). En
 * periodeafgrænsningspost matcher en omkostning eller indtægt til den periode,
 * den hører til: registreringen parkerer beløbet på en balancekonto, og hver
 * periode indtægts-/omkostningsføres deterministisk. Rentemester assisterer
 * workflowet; bruger/revisor ejer valget af periodeopdeling og konti.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerAccrual,
  recognizeAccrualPeriod,
  buildAccrualRegisterReport,
  type AccrualType,
} from "../../core/accruals";
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

const accrualTypeSchema = z
  .enum(["prepaid_expense", "accrued_expense", "deferred_revenue"])
  .describe(
    "prepaid_expense (forudbetalt omkostning — parked on a balance-sheet asset), " +
      "accrued_expense (skyldig omkostning — a cost recognised now against a liability), " +
      "deferred_revenue (forudbetalt indtægt — cash received parked as a liability).",
  );

export function registerAccrualTools(server: McpServer): void {
  server.registerTool(
    "accrual_register",
    {
      title: "Register an accrual (periodeafgrænsningspost)",
      description:
        "Registrerer en periodeafgrænsningspost: bogfører den balancerede registreringspostering " +
        "der parkerer beløbet på en balancekonto, og gemmer hovedet så hver senere periode kan " +
        "periodiseres. Beløbet fordeles ligeligt over recognitionPeriods perioder; sidste periode " +
        "bærer øre-resten. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        accrualType: accrualTypeSchema,
        description: z.string().min(1).describe("Human-readable description of the accrual."),
        totalAmount: z
          .number()
          .positive()
          .describe("The whole amount to spread, in kroner (decimal DKK, 2 decimals — NOT øre). Must be positive."),
        recognitionPeriods: z
          .number()
          .int()
          .positive()
          .describe("Number of periods the amount is recognised over (a positive integer)."),
        firstRecognitionDate: z
          .string()
          .min(1)
          .describe("Posting date of the first recognition period, in YYYY-MM-DD format."),
        resultAccountNo: z
          .string()
          .min(1)
          .describe(
            "Income-statement account each period is recognised on — an expense account for " +
              "prepaid_expense/accrued_expense, an income account for deferred_revenue.",
          ),
        registrationDate: z
          .string()
          .optional()
          .describe("Optional posting date for the registration entry (default: firstRecognitionDate)."),
        periodStepMonths: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Optional whole-month step between recognition periods (default 1)."),
        balanceAccountNo: z
          .string()
          .optional()
          .describe("Optional balance-sheet parking account (defaults per type: 1300 / 7300 / 7310)."),
        settlementAccountNo: z
          .string()
          .optional()
          .describe("Optional settlement/payment account for the registration entry's other leg (default '2000')."),
        documentId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Optional purchase/sales document ID (bilag). Required whenever the registration entry " +
              "touches an expense or income account (accrued_expense and deferred_revenue always do).",
          ),
        note: z.string().optional().describe("Optional free-text note stored on the accrual."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      accrualType: AccrualType;
      description: string;
      totalAmount: number;
      recognitionPeriods: number;
      firstRecognitionDate: string;
      resultAccountNo: string;
      registrationDate?: string;
      periodStepMonths?: number;
      balanceAccountNo?: string;
      settlementAccountNo?: string;
      documentId?: number;
      note?: string;
      confirm?: boolean;
    }>(server, "accrual_register", ({ db, actor, args }) => {
      const result = registerAccrual(
        db,
        withActor(
          {
            accrualType: args.accrualType,
            description: args.description,
            totalAmount: args.totalAmount,
            recognitionPeriods: args.recognitionPeriods,
            firstRecognitionDate: args.firstRecognitionDate,
            resultAccountNo: args.resultAccountNo,
            registrationDate: args.registrationDate,
            periodStepMonths: args.periodStepMonths,
            balanceAccountNo: args.balanceAccountNo,
            settlementAccountNo: args.settlementAccountNo,
            documentId: args.documentId,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "accrual_recognize",
    {
      title: "Recognise one period of an accrual",
      description:
        "Indtægts-/omkostningsfører én periode af en periodeafgrænsningspost ved at bogføre den " +
        "balancerede postering der flytter periodens andel mellem balancekontoen og resultatkontoen. " +
        "Gentaget bogføring af samme periode blokeres. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        accrualId: z
          .number()
          .int()
          .positive()
          .describe("ID of the registered accrual. See accrual_register_report."),
        period: z
          .number()
          .int()
          .positive()
          .describe("1-based index of the recognition period to post (1 = the first period of the schedule)."),
        date: z
          .string()
          .optional()
          .describe("Optional posting date in YYYY-MM-DD format (default: the schedule's date for the period)."),
        settlementAccountNo: z
          .string()
          .optional()
          .describe("Optional settlement account, used only by accrued_expense recognition (default '2000')."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      accrualId: number;
      period: number;
      date?: string;
      settlementAccountNo?: string;
      confirm?: boolean;
    }>(server, "accrual_recognize", ({ db, actor, args }) => {
      const result = recognizeAccrualPeriod(db, {
        accrualId: args.accrualId,
        periodIndex: args.period,
        transactionDate: args.date,
        settlementAccountNo: args.settlementAccountNo,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "accrual_register_report",
    {
      title: "Accrual register and recognition report",
      description:
        "Viser registret af periodeafgrænsningsposter med bogførte perioder, periodiseret beløb " +
        "og resterende balanceeksponering. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      const result = buildAccrualRegisterReport(db);
      return wrapCoreResult(result);
    }),
  );
}
