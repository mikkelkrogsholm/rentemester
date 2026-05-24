import { existsSync } from "node:fs";
import { companyPaths } from "../core/paths";
import {
  initialiseCompanyVolume,
  summariseCompanyVolume,
  type CompanyOnboardingSummary,
} from "../core/company";
import { vatPeriodTypeLabelDa } from "../core/periods";
import { MONTH_NAMES_DA, decapitalize } from "../core/dates";
import {
  registerCompanyDirIntoWorkspace,
  resolveConfiguredWorkspaceRoot,
  resolveWorkspaceRoot,
  type WorkspaceAutoRegisterResult,
} from "../core/workspace";
import { inferredMutationActor } from "../cli-actor";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";

const FISCAL_LABEL_DA: Record<string, string> = {
  "end-year": "navngives efter slutåret",
  "start-year": "navngives efter startåret",
  span: "navngives som spænd (fx 2025/2026)",
};

/**
 * Resolves the workspace root that an `init` invocation should register into.
 *
 * `init` is the legacy single-company path, so a workspace is *optional*:
 * `--workspace <dir>` wins, otherwise `RENTEMESTER_WORKSPACE`. Returns null
 * when neither is set — `init` then behaves exactly as before. An unsafe
 * value is fatal (parse error) so a traversal payload cannot relocate things.
 */
function resolveInitWorkspaceRoot(ctx: CommandContext): string | null {
  const fromFlag = ctx.trimToNull(ctx.arg("--workspace"));
  try {
    return fromFlag ? resolveWorkspaceRoot(fromFlag) : resolveConfiguredWorkspaceRoot();
  } catch (error) {
    ctx.fatal(error instanceof Error ? error.message : String(error));
  }
}

/**
 * #241: the missing-payment-details warning. Returned as a separate block so
 * the caller can route it to stderr — the rest of the onboarding output goes
 * to stdout, but a warning that signals "your invoices will be unpayable"
 * belongs on stderr so it survives stdout redirection (e.g. `init > log.txt`)
 * and so machine-consumers that parse stdout never see prose mixed in.
 */
export function buildPaymentDetailsWarningLines(): string[] {
  return [
    "ADVARSEL — ingen betalingsoplysninger:",
    "  Du har ikke angivet bank/IBAN. Fakturaer udstedt nu får INGEN",
    "  betalingsanvisning (BETALING-blok) på PDF'en — kunden kan ikke se,",
    "  hvor pengene skal hen. Sæt dem med 'rentemester company set-profile'",
    "  (--bank-name --bank-reg --bank-account --iban), før du sender fakturaer.",
  ];
}

/** Renders the human-readable onboarding block printed after a successful `init`. */
function buildOnboardingLines(
  root: string,
  summary: CompanyOnboardingSummary,
  registration: WorkspaceAutoRegisterResult | null,
): string[] {
  const capitalisedMonth = MONTH_NAMES_DA[summary.fiscalYearStartMonth - 1];
  const monthName = capitalisedMonth ? decapitalize(capitalisedMonth) : `måned ${summary.fiscalYearStartMonth}`;
  const fiscalLabel = FISCAL_LABEL_DA[summary.fiscalYearLabelStrategy] ?? summary.fiscalYearLabelStrategy;
  const lines: string[] = [];

  lines.push("");
  lines.push(`Virksomheden '${summary.name}' er klar.`);
  lines.push("");
  lines.push("Oprettet:");
  lines.push(`  - Virksomhedsmappe: ${root}`);
  lines.push(`  - Hovedbog (ledger): ${companyPaths(root).db}`);
  lines.push(`  - Standardkontoplan: ${summary.accountCount} konti (resultat, balance, moms)`);
  if (registration?.status === "registered") {
    lines.push(`  - Registreret i Cockpit-workspacet som '${registration.slug}'`);
  } else if (registration?.status === "already-registered") {
    lines.push(`  - Allerede registreret i Cockpit-workspacet som '${registration.slug}'`);
  }

  // #241: the missing-payment-details warning used to live here too, but it is
  // now emitted to stderr by the dispatch handler so machine-consumers that
  // capture stdout never see it (and so a redirected stdout does not swallow
  // the warning). See buildPaymentDetailsWarningLines above.

  const vatLabel = vatPeriodTypeLabelDa(summary.vatPeriod);
  lines.push("");
  lines.push("Tjek disse indstillinger — de er svære at ændre senere:");
  lines.push(`  - Regnskabsår: starter 1. ${monthName} (${fiscalLabel})`);
  lines.push(`  - Momsperiode: ${vatLabel}. Afregner du en anden momsperiode,`);
  lines.push(
    `    så kør 'init --vat-period month|quarter|half-year' (standard: quarter).`,
  );
  lines.push(`  - CVR: ${summary.cvr ?? "ikke sat — sæt det med 'init --cvr <DK########>'"}`);

  lines.push("");
  lines.push("Næste skridt:");
  if (!summary.cvr) {
    lines.push("  1. Sæt CVR-nummeret, så fakturaer og rapporter er korrekte.");
    lines.push("  2. Hent virksomhedens stamdata: 'rentemester company sync-cvr --company <sti>'.");
    lines.push("  3. Opret din første kunde: 'rentemester customer create --company <sti> ...'.");
    lines.push("  4. Bogfør dit første bilag, eller udsted din første faktura.");
  } else {
    lines.push("  1. Hent virksomhedens stamdata: 'rentemester company sync-cvr --company <sti>'.");
    lines.push("  2. Opret din første kunde: 'rentemester customer create --company <sti> ...'.");
    lines.push("  3. Bogfør dit første bilag, eller udsted din første faktura.");
  }
  lines.push("");
  lines.push("Se kontoplanen med: 'rentemester accounts list --company <sti>'.");

  return lines;
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("init", null, (ctx) => {
    const root = ctx.companyRoot();
    const workspaceRoot = resolveInitWorkspaceRoot(ctx);

    // #248: seed whoever runs onboarding into the actor_allowlist so the
    // derived OS actor and an explicit --actor with the same id behave
    // consistently. An explicit --actor wins; otherwise the derived actor.
    const onboardingActor =
      ctx.trimToNull(ctx.arg("--actor")) ?? inferredMutationActor();

    let summary: CompanyOnboardingSummary;
    try {
      initialiseCompanyVolume(root, {
        // #221: capture the company's own identity + payment details once, so
        // every issued invoice and its PDF inherit them without re-typing.
        name: ctx.trimToNull(ctx.arg("--name")) ?? undefined,
        cvr: ctx.arg("--cvr"),
        fiscalYearStartMonth: ctx.arg("--fiscal-year-start-month"),
        fiscalYearLabelStrategy: ctx.arg("--fiscal-year-label-strategy"),
        // #289: the company's VAT settlement cadence — month/quarter/half-year.
        vatPeriodType: ctx.arg("--vat-period"),
        address: ctx.trimToNull(ctx.arg("--address")) ?? undefined,
        postalCode: ctx.trimToNull(ctx.arg("--postal-code")) ?? undefined,
        city: ctx.trimToNull(ctx.arg("--city")) ?? undefined,
        paymentTermsDays: ctx.arg("--payment-terms"),
        payment: {
          bankName: ctx.trimToNull(ctx.arg("--bank-name")) ?? undefined,
          registrationNo: ctx.trimToNull(ctx.arg("--bank-reg")) ?? undefined,
          accountNo: ctx.trimToNull(ctx.arg("--bank-account")) ?? undefined,
          iban: ctx.trimToNull(ctx.arg("--iban")) ?? undefined,
        },
        onboardingActor,
      });
      summary = summariseCompanyVolume(root);
    } catch (error) {
      console.error(error instanceof Error ? error.message : String(error));
      process.exit(2);
    }

    // #216: when the company directory lives inside a configured workspace,
    // also register it in the workspace manifest so the Cockpit sees it.
    // registerCompanyDirIntoWorkspace never throws, so this can never break
    // `init` for a `--company` path that is not a workspace member.
    let registration: WorkspaceAutoRegisterResult | null = null;
    if (workspaceRoot) {
      registration = registerCompanyDirIntoWorkspace(workspaceRoot, root, {
        name: summary.name,
      });
    }

    ctx.emitResult({
      ok: true,
      message: `Initialized Rentemester company at ${root}`,
      companyRoot: root,
      ledger: companyPaths(root).db,
      accountCount: summary.accountCount,
      cvr: summary.cvr,
      fiscalYearStartMonth: summary.fiscalYearStartMonth,
      fiscalYearLabelStrategy: summary.fiscalYearLabelStrategy,
      vatPeriod: summary.vatPeriod,
      // #241: agents (and the Cockpit) can surface the same payment-details
      // warning the human onboarding block prints.
      hasPaymentDetails: summary.hasPaymentDetails,
      workspace: workspaceRoot,
      workspaceRegistered: registration?.status === "registered",
      workspaceSlug:
        registration && registration.status !== "outside-workspace"
          ? registration.slug
          : undefined,
    });

    // The structured JSON above stays machine-stable for agents; the rich
    // onboarding block (#214) is human-output only.
    if (ctx.outputFormat === "human") {
      for (const line of buildOnboardingLines(root, summary, registration)) {
        console.log(line);
      }
    }

    // #241: warn loudly on stderr (in *any* output format) when no bank/IBAN
    // is set — issued invoices will have no BETALING block and become
    // unpayable. stderr keeps stdout machine-clean and survives `init > log`
    // redirection. Init itself still succeeds (exit 0); this is advisory.
    if (!summary.hasPaymentDetails) {
      for (const line of buildPaymentDetailsWarningLines()) {
        console.error(line);
      }
    }
  });

  dispatch.on("system", "healthcheck", (ctx) => {
    const p = companyPaths(ctx.companyRoot());
    const checks: Array<[string, boolean]> = [
      ["company_root", existsSync(p.root)],
      ["data_dir", existsSync(p.data)],
      ["ledger", existsSync(p.db)],
      ["documents", existsSync(p.documentsInbox)],
      ["config", existsSync(p.config)],
    ];
    let ok = true;
    for (const [name, pass] of checks) {
      console.log(`${pass ? "OK" : "FAIL"} ${name}`);
      if (!pass) ok = false;
    }
    if (!ok) process.exit(1);
  });
}
