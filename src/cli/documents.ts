import { readFileSync } from "node:fs";
import type { CommandDispatch } from "../cli-dispatch";
import { openCommandDb } from "../cli-dispatch";
import { formatKroner } from "../cli-format";
import { migrate, openDb } from "../core/db";
import { PDF_EVIDENCE_TAMPERED, PdfParseError, parseRegisteredPdfBatch, parseRegisteredPdfDocument, planCurrentPdfParses } from "../core/document-pdf-parser";
import { enrichDocumentMetadata, ingestDocument, purchaseVatLinesFromPayload } from "../core/documents";
import { setDocumentCompanyContext } from "../core/document-company-context";
import { reviewIncompleteStandardPurchaseVatEvidence } from "../core/document-purchase-vat-evidence-review";
import { reviewNonEuReverseChargeEvidence } from "../core/document-non-eu-reverse-charge-review";
import { authorizeMcpTool, createMcpSecurityContextFromEnv } from "../mcp/security";
import { recordException } from "../core/exceptions";
import { inspectOpenLedger, openLedgerReadOnly } from "../core/ledger-inspection";
import { resolveDocumentMasterData } from "../core/master-data";
import { companyPaths } from "../core/paths";
import { extractDocumentInvoice, invoiceExtractionSurface } from "../server/invoice-extraction-surface";
import { resolveConfiguredInvoiceExtractor } from "../server/invoice-extractor";
import { documentPdfParsedText, documentPdfParseStatus } from "../server/router/documents";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../core/workspace-control";
import { resolveWorkspaceRoot } from "../core/workspace";
import { applyDocumentPartyLink, decideInternalNoExternalParty, inspectDocumentPartyLinks, listDocumentPartyLinks, planDocumentPartyLink, supersedeDocumentPartyLink, supersedeInternalNoExternalParty } from "../core/document-party-links";

const parseSummary = (run: any) => ({ documentId: run?.documentId, status: run?.status, errorCode: run?.errorCode ?? null, cached: Boolean(run?.cached), pageCount: Array.isArray(run?.pages) ? run.pages.length : 0, itemCount: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.layout?.length ?? 0), 0) : 0, textLength: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.text?.length ?? 0), 0) : 0, resultHash: run?.resultHash });

export function register(dispatch: CommandDispatch): void {
  const partyInput = (ctx: any) => ({ documentId:Number(ctx.arg("--document-id")), companySlug:ctx.arg("--company-slug")!, role:ctx.arg("--role"), partyId:ctx.arg("--party-id"), jurisdiction:ctx.arg("--jurisdiction"), identifierKind:ctx.arg("--identifier-kind"), identifier:ctx.arg("--identifier"), legacyKind:ctx.arg("--legacy-kind"), legacyId:ctx.arg("--legacy-id"), reviewedLegacyReference:ctx.arg("--reviewed-legacy-reference") });
  const registry = (ctx:any, write=false) => (write ? openWorkspaceControlDb : openWorkspaceControlReadOnlyDb)(resolveWorkspaceRoot(ctx.arg("--workspace")!));
  dispatch.on("documents", "party-link-plan", (ctx) => { const ledger=openLedgerReadOnly(ctx.companyRoot()), control=registry(ctx); try { ctx.emitResult(planDocumentPartyLink(ledger,control,partyInput(ctx)) as any); } finally {control.close();ledger.close();} });
  dispatch.on("documents", "party-link-apply", (ctx) => { const ledger=openCommandDb(ctx), control=registry(ctx); migrate(ledger); try { ctx.emitResult(applyDocumentPartyLink(ledger,control,{...partyInput(ctx),planHash:ctx.arg("--plan-hash")!,confirm:ctx.arg("--confirm")==="yes",actor:ctx.cliActor??ctx.inferredMutationActor()??undefined,principal:ctx.arg("--principal"),idempotencyKey:ctx.arg("--idempotency-key")} ) as any); } finally {control.close();ledger.close();} });
  dispatch.on("documents", "party-link-supersede", (ctx) => { const ledger=openCommandDb(ctx); migrate(ledger); try { ctx.emitResult(supersedeDocumentPartyLink(ledger,{documentId:Number(ctx.arg("--document-id")),role:ctx.arg("--role") as any,planHash:ctx.arg("--plan-hash")!,reason:ctx.arg("--reason")!,confirm:ctx.arg("--confirm")==="yes",actor:ctx.cliActor??ctx.inferredMutationActor()??undefined,principal:ctx.arg("--principal")}) as any); } finally {ledger.close();} });
  dispatch.on("documents", "party-link-inspect", (ctx) => { const ledger=openLedgerReadOnly(ctx.companyRoot()); try { ctx.emitResult({ok:true,links:inspectDocumentPartyLinks(ledger,Number(ctx.arg("--document-id")))}); } finally {ledger.close();} });
  dispatch.on("documents", "party-link-list", (ctx) => { const ledger=openLedgerReadOnly(ctx.companyRoot()); try { ctx.emitResult({ok:true,links:listDocumentPartyLinks(ledger,{status:ctx.arg("--status") as any})}); } finally {ledger.close();} });
  dispatch.on("documents", "internal-no-external-party", (ctx) => { const ledger=openCommandDb(ctx); migrate(ledger); try { ctx.emitResult(decideInternalNoExternalParty(ledger,{documentId:Number(ctx.arg("--document-id")),reason:ctx.arg("--reason")!,confirm:ctx.arg("--confirm")==="yes",actor:ctx.cliActor??ctx.inferredMutationActor()??undefined,principal:ctx.arg("--principal"),idempotencyKey:ctx.arg("--idempotency-key")}) as any); } finally {ledger.close();} });
  dispatch.on("documents", "internal-no-external-party-supersede", (ctx) => { const ledger=openCommandDb(ctx); migrate(ledger); try { ctx.emitResult(supersedeInternalNoExternalParty(ledger,{documentId:Number(ctx.arg("--document-id")),decisionHash:ctx.arg("--decision-hash")!,reason:ctx.arg("--reason")!,confirm:ctx.arg("--confirm")==="yes",actor:ctx.cliActor??ctx.inferredMutationActor()??undefined,principal:ctx.arg("--principal")}) as any); } finally {ledger.close();} });
  dispatch.on("documents", "set-company-context", (ctx) => {
    const documentId = Number(ctx.arg("--document-id"));
    const sourceReference = ctx.arg("--source-reference");
    const businessUseReason = ctx.arg("--business-use-reason");
    if (!Number.isInteger(documentId) || documentId <= 0) ctx.fatal("Missing required --document-id <n>");
    if (!sourceReference) ctx.fatal("Missing required --source-reference <text>");
    if (!businessUseReason) ctx.fatal("Missing required --business-use-reason <text>");
    if (ctx.arg("--confirm") !== "yes") ctx.fatal("documents set-company-context requires the exact confirmation --confirm yes");
    const db = openCommandDb(ctx); migrate(db);
    try { ctx.emitResult(setDocumentCompanyContext(db, { documentId, sourceReference: sourceReference!, businessUseReason: businessUseReason!, confirm: true, createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined, createdByProgram: ctx.cliActorVia ?? "rentemester-cli" })); }
    finally { db.close(); }
  });
  dispatch.on("documents", "review-purchase-vat-evidence", async (ctx) => {
    const documentId=Number(ctx.arg("--document-id")), bankTransactionId=Number(ctx.arg("--bank-transaction-id"));
    const businessEvidenceReference=ctx.arg("--business-evidence-reference"), businessEvidenceSha256=ctx.arg("--business-evidence-sha256"), rationale=ctx.arg("--rationale");
    if(!Number.isInteger(documentId)||documentId<=0)ctx.fatal("Missing required --document-id <n>");
    if(!Number.isInteger(bankTransactionId)||bankTransactionId<=0)ctx.fatal("Missing required --bank-transaction-id <n>");
    if(!businessEvidenceReference||!businessEvidenceSha256||!rationale)ctx.fatal("Missing evidence reference/hash or rationale");
    if(ctx.arg("--confirm")!=="yes")ctx.fatal("documents review-purchase-vat-evidence requires the exact confirmation --confirm yes");
    const security=createMcpSecurityContextFromEnv();if(!security)ctx.fatal("documents review-purchase-vat-evidence requires RENTEMESTER_SERVICE_PRINCIPAL_TOKEN and RENTEMESTER_WORKSPACE");
    const authorized=await authorizeMcpTool(security!,"documents_review_purchase_vat_evidence",{company:ctx.companyRoot()});if(!authorized)ctx.fatal("documents review-purchase-vat-evidence requires an active authenticated service principal with company.master-data membership");
    const db=openCommandDb(ctx);migrate(db);try{ctx.emitResult(reviewIncompleteStandardPurchaseVatEvidence(db,{documentId,bankTransactionId,businessEvidenceReference:businessEvidenceReference!,businessEvidenceSha256:businessEvidenceSha256!,rationale:rationale!,supersedesReviewSha256:ctx.arg("--supersedes-review-sha256"),principal:`${authorized!.principal.kind}:${authorized!.principal.subjectId}`,confirm:true,createdBy:ctx.cliActor??ctx.inferredMutationActor()??undefined,createdByProgram:ctx.cliActorVia??"rentemester-cli"}));}finally{db.close();}
  });
  dispatch.on("documents", "review-non-eu-reverse-charge-evidence", async (ctx) => {
    const documentId=Number(ctx.arg("--document-id"));if(!Number.isInteger(documentId)||documentId<=0)ctx.fatal("Missing required --document-id <n>");
    if(ctx.arg("--confirm")!=="yes")ctx.fatal("documents review-non-eu-reverse-charge-evidence requires the exact confirmation --confirm yes");
    const security=createMcpSecurityContextFromEnv();if(!security)ctx.fatal("documents review-non-eu-reverse-charge-evidence requires RENTEMESTER_SERVICE_PRINCIPAL_TOKEN and RENTEMESTER_WORKSPACE");
    const authorized=await authorizeMcpTool(security!,"documents_review_non_eu_reverse_charge_evidence",{company:ctx.companyRoot()});if(!authorized)ctx.fatal("documents review-non-eu-reverse-charge-evidence requires an active authenticated service principal with company.master-data membership");
    const fields=["--supplier-country-code","--actual-buyer-vat","--tax-period","--deduction-percent","--supplier-evidence-reference","--supplier-evidence-sha256","--buyer-evidence-reference","--buyer-evidence-sha256","--service-evidence-reference","--service-evidence-sha256","--formal-deficiencies","--rationale"];if(fields.some(f=>!ctx.arg(f)))ctx.fatal("Missing non-EU review evidence fields");
    const db=openCommandDb(ctx);migrate(db);try{ctx.emitResult(reviewNonEuReverseChargeEvidence(db,{documentId,supplierCountryCode:ctx.arg("--supplier-country-code")!,actualBuyerVat:ctx.arg("--actual-buyer-vat")!,taxPeriod:ctx.arg("--tax-period")!,deductionPercent:Number(ctx.arg("--deduction-percent")),supplierEvidenceReference:ctx.arg("--supplier-evidence-reference")!,supplierEvidenceSha256:ctx.arg("--supplier-evidence-sha256")!,buyerEvidenceReference:ctx.arg("--buyer-evidence-reference")!,buyerEvidenceSha256:ctx.arg("--buyer-evidence-sha256")!,serviceEvidenceReference:ctx.arg("--service-evidence-reference")!,serviceEvidenceSha256:ctx.arg("--service-evidence-sha256")!,formalDeficiencies:ctx.arg("--formal-deficiencies")!.split(",").filter(Boolean),rationale:ctx.arg("--rationale")!,foreignVatCharged:false,supersedesReviewSha256:ctx.arg("--supersedes-review-sha256"),confirm:true,principal:`${authorized!.principal.kind}:${authorized!.principal.subjectId}`,createdBy:ctx.cliActor??ctx.inferredMutationActor()??undefined,createdByProgram:ctx.cliActorVia??"rentemester-cli"}));}finally{db.close();}
  });
  dispatch.on("documents", "enrich", (ctx) => {
    const id = Number(ctx.arg("--document-id"));
    const metadataFile = ctx.arg("--metadata");
    if (!Number.isInteger(id) || id <= 0) {
      ctx.fatal("Missing required --document-id <n>");
      return;
    }
    if (!metadataFile) {
      ctx.fatal("Missing required --metadata <file.json>");
      return;
    }
    if (ctx.arg("--confirm") !== "yes") {
      ctx.fatal("documents enrich requires the exact confirmation --confirm yes");
      return;
    }
    const db = openCommandDb(ctx);
    migrate(db);
    try {
      const metadata = JSON.parse(readFileSync(metadataFile, "utf8"));
      ctx.emitResult(enrichDocumentMetadata(db, id, metadata, {
        createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined,
        createdByProgram: ctx.cliActorVia ?? "rentemester-cli",
      }));
    } finally {
      db.close();
    }
  });

  dispatch.on("documents", "ingest", (ctx) => {
    const file = ctx.arg("--file");
    const metadataFile = ctx.arg("--metadata");
    if (!file || !metadataFile) {
      console.error("Missing required --file <path> or --metadata <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const metadata = JSON.parse(readFileSync(metadataFile, "utf8"));
    const vendorIdRaw = ctx.arg("--vendor-id");
    const vendorId = vendorIdRaw === undefined ? undefined : Number(vendorIdRaw);
    const resolved = resolveDocumentMasterData(db, metadata, {
      vendorId:
        typeof vendorId === "number" && Number.isInteger(vendorId) && vendorId > 0
          ? vendorId
          : undefined,
    });
    if (!resolved.ok) {
      ctx.emitResult(resolved as Record<string, unknown>);
      db.close();
      process.exit(1);
      return;
    }
    const result = ingestDocument(db, root, file, resolved.metadata, {
      forceDuplicateLogicalIdentity: ctx.hasFlag("--force"),
      createdBy:
        ctx.cliActor ??
        process.env.RENTEMESTER_ACTOR ??
        ctx.inferredMutationActor() ??
        undefined,
      createdByProgram:
        ctx.cliActorVia ??
        process.env.RENTEMESTER_ACTOR_VIA ??
        "rentemester-cli",
    });
    if (!result.ok) {
      recordException(db, {
        type: "DOCUMENT_INGEST_BLOCKED",
        severity: "medium",
        message: `Bilaget ${file} kunne ikke indlæses`,
        requiredAction: "Ret bilagets metadata eller dublethåndtering, og prøv at indlæse igen.",
        sourceEvidence: {
          file,
          metadataFile,
          errors: result.errors ?? [],
        },
        postingPreview: {
          retryCommand:
            "documents ingest --company <path> --file <file> --metadata <file.json>",
        },
      });
    }
    // EJER-17: a success confirmation, not the command description. Without a
    // `message` the human renderer falls back to printing the command's help
    // text ("✔ Indlæser og validerer et bilag") as the heading, which reads as
    // a description of what the command does — not what it just did.
    const confirmed = result.ok
      ? {
          ...(result as Record<string, unknown>),
          message: `Bilag ${result.documentNo ?? ""}`.trim() + " er indlæst.",
        }
      : (result as Record<string, unknown>);
    ctx.emitResult(confirmed);
    db.close();
  });

  dispatch.on("documents", "list", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const rows = db
      .query(
        `SELECT d.id, d.document_no, d.source, d.original_filename,
                d.document_type, d.invoice_date, d.amount_inc_vat, d.currency,
                d.status, d.stored_path, d.sender_vat_cvr,
                d.supplier_country_code, d.supplier_identifier_kind,
                d.supplier_identity_status, d.payload_json,
                ive.bank_transaction_id AS source_bank_transaction_id,
                CASE WHEN legacy.document_id IS NOT NULL THEN 'legacy_opening_creditor_reclassification' WHEN ncc.document_id IS NOT NULL THEN 'non_cash_balance_correction' WHEN ive.document_id IS NOT NULL THEN 'bank_evidenced' ELSE NULL END AS internal_voucher_kind,
                legacy.opening_journal_entry_id AS legacy_opening_journal_entry_id, legacy.opening_journal_line_id AS legacy_opening_journal_line_id,
                COALESCE(ive.accounting_rationale,ncc.accounting_rationale) AS accounting_rationale,
                COALESCE(ive.prepared_by,ncc.prepared_by) AS prepared_by,
                COALESCE(ive.prepared_by_program,ncc.prepared_by_program) AS prepared_by_program,
                COALESCE(ive.created_at,ncc.created_at) AS prepared_at
           FROM documents d
           LEFT JOIN internal_voucher_evidence ive ON ive.document_id = d.id
           LEFT JOIN non_cash_balance_correction_evidence ncc ON ncc.document_id = d.id
           LEFT JOIN legacy_opening_creditor_reclassification_evidence legacy ON legacy.document_id = d.id
          ORDER BY d.id DESC`,
      )
      .all() as Array<Record<string, unknown>>;
    if (ctx.outputFormat === "json") {
      console.log(JSON.stringify(rows.map((row) => ({
        ...row,
        purchase_vat_lines: purchaseVatLinesFromPayload(typeof row.payload_json === "string" ? row.payload_json : null),
        payload_json: undefined,
      })), null, 2));
      db.close();
      return;
    }
    console.log(`Bilag (${rows.length})`);
    if (rows.length === 0) {
      console.log("Ingen bilag gemt.");
    }
    for (const row of rows) {
      const currency = String(row.currency ?? "DKK").toUpperCase();
      console.log("");
      console.log(`#${row.document_no ?? row.id} — ${row.original_filename ?? "—"}`);
      console.log(`  Bilagsdato: ${row.invoice_date ?? "—"} | Kilde: ${row.source ?? "—"}`);
      if (row.document_type === "internal_voucher") {
        console.log(
          `  Internt bilag: ${row.internal_voucher_kind === "legacy_opening_creditor_reclassification" ? `legacy kreditor-primobalance #${row.legacy_opening_journal_entry_id ?? "—"}/linje #${row.legacy_opening_journal_line_id ?? "—"}` : row.internal_voucher_kind === "non_cash_balance_correction" ? "balancekorrektion — ingen bankbevægelse" : `bankpost #${row.source_bank_transaction_id ?? "—"}`} | Udarbejdet af: ${row.prepared_by ?? "—"} via ${row.prepared_by_program ?? "—"} · ${row.prepared_at ?? "—"}`,
        );
        console.log(`  Begrundelse: ${row.accounting_rationale ?? "—"}`);
      }
      if (row.supplier_country_code || row.supplier_identifier_kind || row.supplier_identity_status) {
        console.log(`  Leverandøridentitet: ${row.supplier_country_code ?? "—"} · ${row.supplier_identifier_kind ?? "—"} · ${row.supplier_identity_status ?? "—"}`);
      }
      let amountLine = `  Beløb (inkl. moms): ${formatKroner(row.amount_inc_vat)}`;
      if (currency !== "DKK") amountLine += ` ${currency}`;
      console.log(amountLine);
      console.log(`  Status: ${row.status ?? "—"}`);
    }
    db.close();
  });

  dispatch.on("documents", "extract-invoice", async (ctx) => {
    const id = Number(ctx.arg("--document-id"));
    if (!Number.isInteger(id) || id <= 0) ctx.fatal("Missing required --document-id <n>");
    const extractor = resolveConfiguredInvoiceExtractor();
    if (!extractor) { ctx.fatal("invoice extraction requires a configured production provider"); return; }
    const db = openCommandDb(ctx); migrate(db);
    try { await extractDocumentInvoice(db, ctx.companyRoot(), id, extractor, ctx.cliActor ?? ctx.inferredMutationActor() ?? "system:invoice-extraction"); ctx.emitResult({ ok: true, extraction: invoiceExtractionSurface(db, id) }); }
    catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error && /^EXTRACTION_[A-Z_]+$/.test(error.message) ? error.message : "EXTRACTION_FAILED"] }); }
    finally { db.close(); }
  });

  dispatch.on("documents", "invoice-extraction", (ctx) => {
    const id = Number(ctx.arg("--document-id"));
    if (!Number.isInteger(id) || id <= 0) ctx.fatal("Missing required --document-id <n>");
    const db = openCommandDb(ctx); migrate(db);
    try { ctx.emitResult({ ok: true, extraction: invoiceExtractionSurface(db, id) }); } finally { db.close(); }
  });

  dispatch.on("documents", "parse", async (ctx) => {
    const id = Number(ctx.arg("--document-id")); if (!Number.isInteger(id) || id <= 0) ctx.fatal("Missing required --document-id <n>");
    const db = openCommandDb(ctx); migrate(db); try { const result = await parseRegisteredPdfDocument(db, ctx.companyRoot(), { documentId: id, createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined, createdByProgram: "rentemester-cli" }); ctx.emitResult({ ok: true, parse: { ...parseSummary(result), documentId: id } }); } catch { ctx.emitResult({ ok: false, errors: ["PDF_PARSE_FAILED"] }); } finally { db.close(); }
  });
  dispatch.on("documents", "parse-pending", async (ctx) => {
    const limit = ctx.arg("--limit") === undefined ? 100 : Number(ctx.arg("--limit")); if (!Number.isInteger(limit) || limit < 1 || limit > 100) ctx.fatal("--limit must be an integer between 1 and 100");
    const db = openCommandDb(ctx); migrate(db); try { const plan = planCurrentPdfParses(db, { limit, cursor: Number(ctx.arg("--cursor") ?? 0) }); const parses = await parseRegisteredPdfBatch(db, ctx.companyRoot(), plan.documentIds, { createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined, createdByProgram: "rentemester-cli" }); const failed = parses.filter((p: any) => !p.ok); ctx.emitResult({ ok: true, batch: { requested: plan.documentIds.length, parsed: parses.length - failed.length, failed: failed.length, cursor: plan.cursor, nextCursor: plan.nextCursor, resume: failed.length ? { documentIds: failed.map((p: any) => p.documentId) } : null } }); } finally { db.close(); }
  });
  dispatch.on("documents", "parse-status", (ctx) => { const id = Number(ctx.arg("--document-id")); if (!Number.isInteger(id) || id <= 0) ctx.fatal("Missing required --document-id <n>"); const root=ctx.companyRoot(), db = openLedgerReadOnly(companyPaths(root).db); try { if (inspectOpenLedger(db).status !== "current") ctx.fatal("ledger is not ready for read-only inspection"); ctx.emitResult({ ok: true, parse: documentPdfParseStatus(db, root, id) }); } catch (error) { const tampered=error instanceof PdfParseError && error.code === "tampered_result"; ctx.emitResult({ ok:false, errors:[tampered ? PDF_EVIDENCE_TAMPERED : "PDF_PARSE_FAILED"], ...(tampered ? { code:PDF_EVIDENCE_TAMPERED } : {}) }); } finally { db.close(); } });
  dispatch.on("documents", "parsed-text", (ctx) => { const id = Number(ctx.arg("--document-id")); const offset = Number(ctx.arg("--offset") ?? 0); const limit = Number(ctx.arg("--limit") ?? 10); if (!Number.isInteger(id) || id <= 0) ctx.fatal("Missing required --document-id <n>"); if (!Number.isInteger(offset) || offset < 0 || !Number.isInteger(limit) || limit < 1 || limit > 10) ctx.fatal("--offset >= 0 and --limit 1..10 are required"); const root=ctx.companyRoot(), db = openLedgerReadOnly(companyPaths(root).db); try { if (inspectOpenLedger(db).status !== "current") ctx.fatal("ledger is not ready for read-only inspection"); ctx.emitResult({ ok: true, ...documentPdfParsedText(db, root, id, offset, limit) }); } catch (error) { const tampered=error instanceof PdfParseError && error.code === "tampered_result"; ctx.emitResult({ ok:false, errors:[tampered ? PDF_EVIDENCE_TAMPERED : "PDF_PARSE_FAILED"], ...(tampered ? { code:PDF_EVIDENCE_TAMPERED } : {}) }); } finally { db.close(); } });
}
