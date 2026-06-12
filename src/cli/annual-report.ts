// Annual report (#177): the `report annual` CLI command.
//
// `report annual --company <c> --from <fy-start> --to <fy-end>` assembles a
// regnskabsklasse-B arsrapport (resultatopgorelse + balance + notes skeleton +
// ledelsespategning) for a locked fiscal year and emits it as JSON. With
// `--ixbrl-out <path>` it also writes a deterministic iXBRL XHTML file (and
// surfaces its path + sha256 + taxonomy version in the JSON). With
// `--ixbrl-taxonomy` it instead prints the bounded, versioned iXBRL taxonomy
// subset (no ledger access) so callers can inspect exactly which class-B
// elements Rentemester maps.
//
// Read-only on the ledger: Rentemester prepares the arsrapport; the owner or
// advisor reviews it and is responsible for filing with Erhvervsstyrelsen.

import { writeFileSync } from "node:fs";
import { migrate } from "../core/db";
import { buildAnnualReport } from "../core/annual-report";
import { generateIxbrl, IXBRL_TAXONOMY_SUBSET } from "../core/ixbrl";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";
import { emitHumanReport } from "../cli-format";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("report", "annual", (ctx) => {
    // `--ixbrl-taxonomy` is a pure-introspection mode: it neither touches the
    // ledger nor needs --from/--to. It prints the bounded, versioned subset so
    // an owner/advisor can see exactly which class-B elements are covered.
    if (ctx.hasFlag("--ixbrl-taxonomy")) {
      emitHumanReport(
        "report-annual-ixbrl-taxonomy",
        {
          ok: true,
          name: IXBRL_TAXONOMY_SUBSET.name,
          version: IXBRL_TAXONOMY_SUBSET.version,
          scope: IXBRL_TAXONOMY_SUBSET.scope,
          prefix: IXBRL_TAXONOMY_SUBSET.prefix,
          namespace: IXBRL_TAXONOMY_SUBSET.namespace,
          elementCount: IXBRL_TAXONOMY_SUBSET.elements.length,
          elements: IXBRL_TAXONOMY_SUBSET.elements,
        },
        ctx.outputFormat,
      );
      return;
    }

    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const ixbrlOut = ctx.trimToNull(ctx.arg("--ixbrl-out"));

    const db = openCommandDb(ctx);
    migrate(db);
    const report = buildAnnualReport(db, from, to);

    // The JSON result is the assembled arsrapport plus, when requested and the
    // report passed its prerequisites, the iXBRL file metadata.
    const result: Record<string, unknown> = { ...report };
    if (ixbrlOut) {
      const ixbrl = generateIxbrl(report);
      if (ixbrl.ok) {
        writeFileSync(ixbrlOut, ixbrl.xhtml);
        result.ixbrl = {
          ok: true,
          path: ixbrlOut,
          sha256: ixbrl.sha256,
          taxonomy: ixbrl.taxonomy,
          appliedRules: ixbrl.appliedRules,
        };
      } else {
        // Surface the iXBRL failure without overriding a passing report:
        // the arsrapport JSON is still useful even if no file was written.
        result.ixbrl = { ok: false, path: null, sha256: "", errors: ixbrl.errors };
      }
    }

    emitHumanReport("report-annual", result, ctx.outputFormat);
    db.close();
  });
}
