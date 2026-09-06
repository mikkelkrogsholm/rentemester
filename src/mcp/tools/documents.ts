/**
 * MCP-tools for bilag.
 *
 *  - `documents_list` (read)
 *  - `documents_ingest` (write-reversible — indlæser et bilag fra disk)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { PDF_EVIDENCE_TAMPERED, PdfParseError, parseRegisteredPdfBatch, parseRegisteredPdfDocument, planCurrentPdfParses } from "../../core/document-pdf-parser";
import { type DocumentMetadata, enrichDocumentMetadata, ingestDocument, purchaseVatLinesFromPayload } from "../../core/documents";
import { setDocumentCompanyContext } from "../../core/document-company-context";
import { reviewIncompleteStandardPurchaseVatEvidence } from "../../core/document-purchase-vat-evidence-review";
import { reviewNonEuReverseChargeEvidence } from "../../core/document-non-eu-reverse-charge-review";
import { applyDocumentPartyLink, decideInternalNoExternalParty, DOCUMENT_PARTY_ROLES, inspectDocumentPartyLinks, listDocumentPartyLinks, planDocumentPartyLink, supersedeDocumentPartyLink, supersedeInternalNoExternalParty } from "../../core/document-party-links";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../../core/workspace-control";
import { companyRootForSlug, listWorkspaceCompanies, resolveConfiguredWorkspaceRoot } from "../../core/workspace";
import { realpathSync } from "node:fs";
import { recordException } from "../../core/exceptions";
import { resolveDocumentMasterData } from "../../core/master-data";
import { extractDocumentInvoice, invoiceExtractionSurface } from "../../server/invoice-extraction-surface";
import { resolveConfiguredInvoiceExtractor } from "../../server/invoice-extractor";
import { documentPdfParsedText, documentPdfParseStatus } from "../../server/router/documents";
import { envelopeShape, errorEnvelope, successEnvelope, wrapCoreResult } from "../envelope";
import { applyPagination, paginationDescriptionSuffix, paginationFields } from "../pagination";
import { confirmField, idempotencyKeyField, withCompanyDb, withCompanyDbConfirmed, withCompanyReadOnlyDb } from "../tool-runtime";
import { currentMcpAuthenticatedPrincipal } from "../security";

const documentPartyLinkFields = {
  documentId: z.number().int().positive(), role: z.enum(DOCUMENT_PARTY_ROLES), partyId: z.string().min(3).max(64).optional(), jurisdiction: z.string().length(2).optional(), identifierKind: z.enum(["dk_cvr","eu_vat","non_eu"]).optional(), identifier: z.string().min(1).max(160).optional(), legacyKind: z.enum(["customer","vendor"]).optional(), legacyId: z.string().min(1).max(160).optional(), reviewedLegacyReference: z.string().min(1).max(500).optional(),
  sourceReview: z.object({ observedName:z.string().min(1).max(320), observedAddress:z.string().min(1).max(500).optional(), jurisdiction:z.string().length(2), identifierKind:z.enum(["dk_cvr","eu_vat","non_eu"]), identifier:z.string().min(1).max(160).optional(), sourceReference:z.string().min(1).max(500), sourceLocation:z.string().min(1).max(300), rationale:z.string().min(1).max(1000), vendorId:z.number().int().positive().optional() }).optional().describe("Human-reviewed identity observed in the immutable source. Required fields are exact evidence, not inferred metadata; vendorId optionally supplements only missing legacy contact identity."),
} as const;
const partyLinkPrincipal=()=>{
  const principal = currentMcpAuthenticatedPrincipal();
  if (!principal) throw new Error("authenticated service principal required");
  return `${principal.kind}:${principal.subjectId}`;
};
const documentPartyWorkspace=()=>resolveConfiguredWorkspaceRoot()??(()=>{throw new Error("RENTEMESTER_WORKSPACE is required for canonical document-party links");})();
function documentPartyScope(companyRoot:string){
  const workspace=documentPartyWorkspace();
  const actual=realpathSync(companyRoot);
  const company=listWorkspaceCompanies(workspace).find((entry)=>{
    try{return realpathSync(companyRootForSlug(workspace,entry.slug))===actual;}catch{return false;}
  });
  if(!company)throw new Error("company is not registered in the configured workspace");
  return {workspace,companySlug:company.slug};
}

const parseSummary = (run: any, documentId?: number) => ({ documentId, status: run?.status, errorCode: run?.errorCode ?? null, cached: Boolean(run?.cached), pageCount: Array.isArray(run?.pages) ? run.pages.length : 0, itemCount: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.layout?.length ?? 0), 0) : 0, textLength: Array.isArray(run?.pages) ? run.pages.reduce((n: number, p: any) => n + (p.text?.length ?? 0), 0) : 0, resultHash: run?.resultHash });

const documentPartySchema = z.object({
  name: z.string().optional().describe("Party name."),
  address: z.string().optional().describe("Party postal address."),
  vatOrCvr: z.string().optional().describe("Party VAT or CVR number, e.g. 'DK12345678'."),
  countryCode: z.string().length(2).optional().describe("Supplier ISO 3166-1 alpha-2 country evidence, e.g. 'US'. Required with identifierKind."),
  identifierKind: z.enum(["dk_cvr", "eu_vat", "non_eu"]).optional().describe("Typed supplier identifier. non_eu permits no identifier when country evidence is non-EU."),
});

/**
 * The named `DocumentMetadata` fields shared by `documents_ingest` and the
 * bilagsmail intake tools (`imap_intake_poll`, `mail_intake_ingest`).
 *
 * Exported as a bare shape (not a `z.object`) so the intake tools — which do
 * NOT take `source` (the pipeline sets it) — can build their own object from
 * the SAME field definitions, guaranteeing the two schemas cannot drift
 * apart (#274).
 */
export const documentMetadataFields = {
  documentType: z
      .enum(["purchase_sale", "cash_register_receipt", "internal_voucher", "external_accounting_evidence"])
      .optional()
      .describe("Document type (default 'purchase_sale')."),
    issueDate: z.string().optional().describe("Document/invoice date in YYYY-MM-DD format."),
    invoiceNo: z.string().optional().describe("Invoice or receipt number printed on the document."),
    deliveryDescription: z
      .string()
      .optional()
      .describe("Free-text description of the goods or services."),
    amountIncVat: z
      .number()
      .optional()
      .describe("Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK')."),
    sender: documentPartySchema.optional().describe("Sender/supplier details."),
    recipient: documentPartySchema.optional().describe("Recipient/buyer details."),
    vatAmount: z
      .number()
      .optional()
      .describe("VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    purchaseVatLines: z.array(z.object({
      classification: z.enum(["dk_purchase_25", "exempt"]),
      netAmount: z.number().nonnegative().describe("Tax base in kroner."),
      vatAmount: z.number().nonnegative().optional().describe("VAT amount in kroner; 25% for dk_purchase_25, otherwise zero."),
    })).min(1).optional().describe("Optional durable purchase VAT split. Its net and VAT totals must reconcile exactly with the document totals."),
    reverseChargeWordingConfirmed: z
      .boolean()
      .optional()
      .describe("True only when a human has confirmed that the supplier invoice contains reverse-charge wording; required with the other invoice evidence before non-EU input-VAT deduction."),
    reverseChargeWordingEvidence: z.object({ excerpt: z.string().min(1).max(2000), location: z.string().min(1).max(300) }).optional().describe("Verbatim reverse-charge statement and its source location, e.g. page 1. It is hash-bound to the immutable document metadata."),
    danishSimplifiedPurchaseInvoice: z.boolean().optional().describe("Explicit source fact: this is a Danish simplified purchase invoice. This never changes recipient invoice fields."),
    incompleteStandardPurchaseInvoice: z.boolean().optional().describe("Truthful intake marker for a standard invoice with absent buyer fields. It never grants VAT eligibility; record company context separately."),
    paymentDetails: z
      .string()
      .optional()
      .describe("Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'."),
    exemptionCode: z
      .literal("FOREIGN_PHYSICAL_ONLY")
      .nullable()
      .optional()
      .describe("Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit."),
    sourceBankTransactionId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Required for bank_evidenced internal_voucher; forbidden for non_cash_balance_correction and legacy_opening_creditor_reclassification."),
    internalVoucherKind: z
      .enum(["bank_evidenced", "non_cash_balance_correction", "legacy_opening_creditor_reclassification"])
      .optional()
      .describe("Explicit internal-voucher evidence contract. Omitted legacy vouchers remain bank_evidenced."),
    legacyOpeningJournalEntryId: z.number().int().positive().optional().describe("Required only for legacy_opening_creditor_reclassification: immutable primobalance journal entry ID."),
    legacyOpeningJournalLineId: z.number().int().positive().optional().describe("Required only for legacy_opening_creditor_reclassification: exact creditor line ID in that primobalance."),
    accountingRationale: z
      .string()
      .min(1)
      .max(2000)
      .optional()
      .describe("Required for internal_voucher: the accounting reason for the posting."),
    externalAccountingEvidence: z.object({ category: z.literal("payroll"), accountingPeriod: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/), externalReference: z.string().min(1).max(300), totals: z.object({ debitAmount: z.number().positive(), creditAmount: z.number().positive() }) }).optional().describe("Source facts for an externally issued non-invoice payroll report. It supports ordinary source-linked journals and is never a purchase invoice or VAT evidence."),
} as const;

/**
 * The `documents_ingest` metadata schema: the shared `DocumentMetadata`
 * fields PLUS the required `source` field (how the document arrived).
 */
const documentMetadataSchema = z
  .object({
    source: z
      .string()
      .describe("How the document arrived, e.g. 'email', 'photo-upload', 'mobile-scan'. Required."),
    ...documentMetadataFields,
  })
  .describe(
    "Document (bilag) metadata. amountIncVat and vatAmount are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre).",
  );

export function registerDocumentTools(server: McpServer): void {
  server.registerTool("documents_party_link_plan", { title:"Plan canonical document party link", description:"Read-only deterministic plan that binds immutable document bytes to one visible canonical party. Existing typed identity is used directly; sourceReview supports explicit human review of missing legacy metadata without changing the document, journal or VAT.", inputSchema:{company:z.string().min(1),...documentPartyLinkFields}, outputSchema:envelopeShape, annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyReadOnlyDb<any>(({db,args})=>{const scope=documentPartyScope(args.company);const registry=openWorkspaceControlReadOnlyDb(scope.workspace);try{return successEnvelope(planDocumentPartyLink(db,registry,{...args,companySlug:scope.companySlug},args.company));}finally{registry.close();}}));
  server.registerTool("documents_party_link_list", { title:"List document party resolutions", description:"Read-only deterministic state: resolved, internal_no_external_party, or unresolved.", inputSchema:{company:z.string().min(1),status:z.enum(["linked","unlinked","resolved","internal_no_external_party","unresolved"]).optional()}, outputSchema:envelopeShape, annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyReadOnlyDb<any>(({db,args})=>successEnvelope({links:listDocumentPartyLinks(db,args)})));
  server.registerTool("documents_party_link_inspect", { title:"Inspect document party link history", description:"Read-only append-only link and supersession provenance.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive()}, outputSchema:envelopeShape, annotations:{readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyReadOnlyDb<any>(({db,args})=>successEnvelope({links:inspectDocumentPartyLinks(db,args.documentId)})));
  server.registerTool("documents_party_link_apply", { title:"Apply canonical document party link", description:"Records the exact reviewed plan append-only. sourceReview may supplement only missing legacy vendor identity with hash-bound before/after evidence; document, journal and VAT facts remain unchanged. Requires confirm:true and idempotencyKey.", inputSchema:{company:z.string().min(1),...documentPartyLinkFields,planHash:z.string().length(64),idempotencyKey:idempotencyKeyField,confirm:confirmField}, outputSchema:envelopeShape, annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyDbConfirmed<any>(server,"documents_party_link_apply",({db,actor,args})=>{const scope=documentPartyScope(args.company);const registry=openWorkspaceControlDb(scope.workspace);try{return wrapCoreResult(applyDocumentPartyLink(db,registry,{...args,companySlug:scope.companySlug,confirm:args.confirm===true,actor:actor.createdBy,principal:partyLinkPrincipal(),idempotencyKey:args.idempotencyKey},args.company));}finally{registry.close();}}));
  server.registerTool("documents_party_link_supersede", { title:"Supersede document party link", description:"Appends a documented correction; never deletes historical party-link evidence. Requires confirm:true.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive(),role:z.enum(DOCUMENT_PARTY_ROLES),planHash:z.string().length(64),reason:z.string().min(1).max(1000),confirm:confirmField}, outputSchema:envelopeShape, annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyDbConfirmed<any>(server,"documents_party_link_supersede",({db,actor,args})=>wrapCoreResult(supersedeDocumentPartyLink(db,{...args,confirm:args.confirm===true,actor:actor.createdBy,principal:partyLinkPrincipal()}))));
  server.registerTool("documents_internal_no_external_party", { title:"Confirm internal voucher has no external party", description:"Append a confirmed, actor-audited hash-bound decision for an internal voucher. It cannot alter evidence, VAT, or journals.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive(),reason:z.string().min(1).max(1000),idempotencyKey:idempotencyKeyField,confirm:confirmField}, outputSchema:envelopeShape, annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyDbConfirmed<any>(server,"documents_internal_no_external_party",({db,actor,args})=>wrapCoreResult(decideInternalNoExternalParty(db,{...args,confirm:args.confirm===true,actor:actor.createdBy,principal:partyLinkPrincipal()}))));
  server.registerTool("documents_internal_no_external_party_supersede", { title:"Supersede no-external-party decision", description:"Append a correction to the exact current no-external-party decision.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive(),decisionHash:z.string().length(64),reason:z.string().min(1).max(1000),confirm:confirmField}, outputSchema:envelopeShape, annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}}, withCompanyDbConfirmed<any>(server,"documents_internal_no_external_party_supersede",({db,actor,args})=>wrapCoreResult(supersedeInternalNoExternalParty(db,{...args,confirm:args.confirm===true,actor:actor.createdBy,principal:partyLinkPrincipal()}))));
  server.registerTool(
    "documents_set_company_context",
    { title: "Record reviewed purchase-document company context", description: "Records append-only, actor-audited and hash-bound business attribution for one Danish simplified purchase invoice or truthfully incomplete standard purchase invoice. It never modifies issuer recipient fields and never grants VAT eligibility. Requires confirm:true. write-reversible.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive(), sourceReference: z.string().min(1).max(2000), businessUseReason: z.string().min(1).max(2000), confirm: confirmField }, outputSchema: envelopeShape, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false } },
    withCompanyDbConfirmed<{ company: string; documentId: number; sourceReference: string; businessUseReason: string; confirm?: boolean }>(server, "documents_set_company_context", ({ db, actor, args }) => wrapCoreResult(setDocumentCompanyContext(db, { documentId: args.documentId, sourceReference: args.sourceReference, businessUseReason: args.businessUseReason, confirm: args.confirm === true, createdBy: actor.createdBy, createdByProgram: actor.createdByProgram }))),
  );
  server.registerTool("documents_review_purchase_vat_evidence", { title:"Review formal purchase-invoice VAT evidence", description:"Records an append-only, hash-bound review for a truthfully incomplete Danish standard invoice. It is not an override: supplier identity, exact company payment and business-use evidence remain mandatory. Requires authenticated principal and confirm:true.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive(),bankTransactionId:z.number().int().positive(),businessEvidenceReference:z.string().min(1).max(2000),businessEvidenceSha256:z.string().regex(/^[a-fA-F0-9]{64}$/),rationale:z.string().min(1).max(2000),supersedesReviewSha256:z.string().regex(/^[a-fA-F0-9]{64}$/).optional(),confirm:confirmField}, outputSchema:envelopeShape, annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}},withCompanyDbConfirmed<any>(server,"documents_review_purchase_vat_evidence",({db,actor,args})=>{const p=currentMcpAuthenticatedPrincipal();return wrapCoreResult(reviewIncompleteStandardPurchaseVatEvidence(db,{...args,confirm:true,createdBy:actor.createdBy,createdByProgram:actor.createdByProgram,principal:p?`${p.kind}:${p.subjectId}`:undefined}));}));
  server.registerTool("documents_review_non_eu_reverse_charge_evidence", { title:"Review non-EU reverse-charge material evidence", description:"Records a hash-bound, append-only material review for formal non-EU supplier invoice defects. It never changes invoice facts, treats observed OSS/IE VAT as EU establishment, or accepts foreign charged VAT as Danish reverse charge.", inputSchema:{company:z.string().min(1),documentId:z.number().int().positive(),supplierCountryCode:z.string().length(2),actualBuyerVat:z.string().regex(/^DK\d{8}$/i),taxPeriod:z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),deductionPercent:z.number().gt(0).lte(100),supplierEvidenceReference:z.string().min(1).max(2000),supplierEvidenceSha256:z.string().regex(/^[a-fA-F0-9]{64}$/),buyerEvidenceReference:z.string().min(1).max(2000),buyerEvidenceSha256:z.string().regex(/^[a-fA-F0-9]{64}$/),serviceEvidenceReference:z.string().min(1).max(2000),serviceEvidenceSha256:z.string().regex(/^[a-fA-F0-9]{64}$/),formalDeficiencies:z.array(z.string().min(1).max(200)).min(1),rationale:z.string().min(1).max(2000),foreignVatCharged:z.literal(false),supersedesReviewSha256:z.string().regex(/^[a-fA-F0-9]{64}$/).optional(),confirm:confirmField},outputSchema:envelopeShape,annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:false}},withCompanyDbConfirmed<any>(server,"documents_review_non_eu_reverse_charge_evidence",({db,actor,args})=>{const p=currentMcpAuthenticatedPrincipal();return wrapCoreResult(reviewNonEuReverseChargeEvidence(db,{...args,confirm:true,createdBy:actor.createdBy,createdByProgram:actor.createdByProgram,principal:p?`${p.kind}:${p.subjectId}`:undefined}));}));
  server.registerTool(
    "documents_enrich",
    {
      title: "Enrich document metadata",
      description: "Completes missing metadata for one unlinked legacy document. Requires confirm:true; identical retries are idempotent. write-reversible.",
      inputSchema: { company: z.string().min(1), documentId: z.number().int().positive(), metadata: documentMetadataSchema, confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; documentId: number; metadata: DocumentMetadata; confirm?: boolean }>(server, "documents_enrich", ({ db, actor, args }) =>
      wrapCoreResult(enrichDocumentMetadata(db, args.documentId, args.metadata, { createdBy: actor.createdBy, createdByProgram: actor.createdByProgram })),
    ),
  );
  server.registerTool("documents_parse", { title: "Parse PDF document", description: "Offline, deterministic PDF text parse of an already stored document. Requires confirm:true; it has no bookkeeping authority. write-reversible.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive(), confirm: confirmField.optional() }, outputSchema: envelopeShape, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyDbConfirmed<{ company: string; documentId: number; confirm?: boolean }>(server, "documents_parse", async ({ db, actor, args }) => { try { return successEnvelope({ parse: parseSummary(await parseRegisteredPdfDocument(db, args.company, { documentId: args.documentId, createdBy: actor.createdBy, createdByProgram: actor.createdByProgram }), args.documentId) }); } catch { return errorEnvelope(["PDF_PARSE_FAILED"]); } }));
  server.registerTool("documents_parse_pending", { title: "Parse pending PDFs", description: "Parses up to 100 stored PDFs that have no parse result. Requires confirm:true; no ingest or bookkeeping is performed. write-reversible.", inputSchema: { company: z.string().min(1), limit: z.number().int().min(1).max(100).optional(), cursor: z.number().int().min(0).optional(), confirm: confirmField.optional() }, outputSchema: envelopeShape, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyDbConfirmed<{ company: string; limit?: number; cursor?: number; confirm?: boolean }>(server, "documents_parse_pending", async ({ db, actor, args }) => { const plan=planCurrentPdfParses(db,{limit:args.limit,cursor:args.cursor}); const parses = await parseRegisteredPdfBatch(db, args.company, plan.documentIds, { createdBy: actor.createdBy, createdByProgram: actor.createdByProgram }); const failed = parses.filter((p: any) => !p.ok); return successEnvelope({ batch: { requested: plan.documentIds.length, parsed: parses.length - failed.length, failed: failed.length, cursor:plan.cursor, nextCursor:plan.nextCursor, resume: failed.length ? { documentIds: failed.map((p: any) => p.documentId) } : null } }); }));
  server.registerTool("documents_parse_status", { title: "Read PDF parse status", description: "Read-only latest parser status and metrics; never exposes paths, raw child stderr, or secrets.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive() }, outputSchema: envelopeShape, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyReadOnlyDb<{ company: string; documentId: number }>(({ db, args }) => { try { return successEnvelope({ parse: documentPdfParseStatus(db, args.company, args.documentId) }); } catch (error) { return errorEnvelope([error instanceof PdfParseError && error.code === "tampered_result" ? PDF_EVIDENCE_TAMPERED : "PDF_PARSE_FAILED"], { code: error instanceof PdfParseError && error.code === "tampered_result" ? PDF_EVIDENCE_TAMPERED : undefined }); } }));
  server.registerTool("documents_parsed_text", { title: "Read parsed PDF text", description: "Read-only parsed text pages. At most 10 pages per call; never exposes paths, raw child stderr, or secrets.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive(), offset: z.number().int().min(0).optional(), limit: z.number().int().min(1).max(10).optional() }, outputSchema: envelopeShape, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyReadOnlyDb<{ company: string; documentId: number; offset?: number; limit?: number }>(({ db, args }) => { try { return successEnvelope(documentPdfParsedText(db, args.company, args.documentId, args.offset ?? 0, args.limit ?? 10)); } catch (error) { return errorEnvelope([error instanceof PdfParseError && error.code === "tampered_result" ? PDF_EVIDENCE_TAMPERED : "PDF_PARSE_FAILED"], { code: error instanceof PdfParseError && error.code === "tampered_result" ? PDF_EVIDENCE_TAMPERED : undefined }); } }));
  server.registerTool("documents_invoice_extraction", { title: "Read invoice extraction", description: "Returns cited invoice values, confidence, provenance, conflicts, hash, resolutions and exception state; no paths or secrets.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive() }, outputSchema: envelopeShape, annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyDb<{ company: string; documentId: number }>(server, ({ db, args }) => successEnvelope({ extraction: invoiceExtractionSurface(db, args.documentId) })));
  server.registerTool("documents_extract_invoice", { title: "Extract invoice", description: "Extracts cited evidence from a stored PDF. Requires confirm:true and a configured production extraction provider. write-reversible.", inputSchema: { company: z.string().min(1), documentId: z.number().int().positive(), confirm: confirmField.optional() }, outputSchema: envelopeShape, annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false } }, withCompanyDbConfirmed<{ company: string; documentId: number; confirm?: boolean }>(server, "documents_extract_invoice", async ({ db, actor, args }) => { const extractor = resolveConfiguredInvoiceExtractor(); if (!extractor) return errorEnvelope(["EXTRACTION_PROVIDER_UNAVAILABLE"]); try { await extractDocumentInvoice(db, args.company, args.documentId, extractor, actor.createdBy); return successEnvelope({ extraction: invoiceExtractionSurface(db, args.documentId) }); } catch (error) { return errorEnvelope([error instanceof Error && /^EXTRACTION_[A-Z_]+$/.test(error.message) ? error.message : "EXTRACTION_FAILED"]); } }));
  server.registerTool(
    "documents_list",
    {
      title: "List documents",
      description:
        "Lister gemte bilag i virksomhedsmappen. Read-only. " +
        "Rækkefølge: id DESC (nyeste først, deterministisk)." +
        paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; limit?: number; offset?: number }>(server, ({ db, args }) => {
      const rows = db
        .query(
          `SELECT d.id, d.document_no, d.source, d.original_filename,
                  d.document_type, d.invoice_date, d.amount_inc_vat,
                  d.currency, d.status, d.stored_path, d.payload_json,
                  d.sender_vat_cvr, d.supplier_country_code,
                  d.supplier_identifier_kind, d.supplier_identity_status,
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
        .all() as Array<{
          id: number;
          document_no: string | null;
          source: string;
          original_filename: string;
          document_type: string;
          invoice_date: string | null;
          amount_inc_vat: number | null;
          currency: string | null;
          status: string;
          stored_path: string | null;
          payload_json: string | null;
          sender_vat_cvr: string | null; supplier_country_code: string | null; supplier_identifier_kind: string | null; supplier_identity_status: string | null;
          source_bank_transaction_id: number | null;
          internal_voucher_kind: "bank_evidenced" | "non_cash_balance_correction" | "legacy_opening_creditor_reclassification" | null;
          legacy_opening_journal_entry_id: number | null; legacy_opening_journal_line_id: number | null;
          accounting_rationale: string | null;
          prepared_by: string | null;
          prepared_by_program: string | null;
          prepared_at: string | null;
        }>;
      const mapped = rows.map((row) => ({
        id: row.id,
        documentNo: row.document_no,
        source: row.source,
        originalFilename: row.original_filename,
        documentType: row.document_type,
        invoiceDate: row.invoice_date,
        amountIncVat: row.amount_inc_vat,
        currency: row.currency,
        status: row.status,
        storedPath: row.stored_path,
        purchaseVatLines: purchaseVatLinesFromPayload(row.payload_json),
        senderVatOrCvr: row.sender_vat_cvr,
        supplierCountryCode: row.supplier_country_code,
        supplierIdentifierKind: row.supplier_identifier_kind,
        supplierIdentityStatus: row.supplier_identity_status,
        sourceBankTransactionId: row.source_bank_transaction_id,
          internalVoucherKind: row.internal_voucher_kind,
          legacyOpeningJournalEntryId: row.legacy_opening_journal_entry_id, legacyOpeningJournalLineId: row.legacy_opening_journal_line_id,
        accountingRationale: row.accounting_rationale,
        preparedBy: row.prepared_by,
        preparedByProgram: row.prepared_by_program,
        preparedAt: row.prepared_at,
      }));
      const { pageRows, meta } = applyPagination(mapped, { limit: args.limit, offset: args.offset });
      return successEnvelope({ documents: pageRows, ...meta });
    }),
  );

  server.registerTool(
    "documents_ingest",
    {
      title: "Ingest document",
      description:
        "Indlæser og hash-lagrer et bilag med metadata. Kræver confirm:true. " +
        "BIVIRKNING ved fejl: hver gang ingest blokeres (fx duplicate, manglende " +
        "fil, valideringsfejl) skrives en `DOCUMENT_INGEST_BLOCKED` exception-række. " +
        "Skrivningen er idempotent på (type, filePath, requiredAction): gentagne " +
        "retries af præcis samme fejlende input opretter IKKE duplikat-exceptions " +
        "— de matcher den eksisterende åbne række og no-op'er. Brug `exceptions_list` " +
        "for at se de afledte exceptions agenten har efterladt. " +
        "VIGTIGT: filePath er en sti på MCP-serverens eget filsystem — bilaget skal allerede " +
        "ligge på serveren. Klienten/agenten kan IKKE uploade en fil her, og der findes (i " +
        "modsætning til bank_import's csvContent) ingen inline-content-variant: filen kan kun " +
        "angives via sti. Alle beløb i metadata er i kroner (decimal DKK, ikke øre). " +
        "write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        filePath: z
          .string()
          .min(1)
          .describe(
            "Absolute path to the document file ON THE MCP SERVER'S FILESYSTEM. The file " +
              "must already exist on the server — this tool does not accept uploaded or " +
              "inline file content (no csvContent-style alternative exists, unlike bank_import).",
          ),
        metadata: documentMetadataSchema,
        vendorId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Optional ID of an existing vendor to associate with the document. See vendor_list."),
        force: z
          .boolean()
          .optional()
          .describe(
            "Set true to bypass duplicate detection and force ingest even when a " +
              "document with the same logical identity already exists. When omitted " +
              "(or false), a duplicate is blocked and an exception is recorded.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      filePath: string;
      metadata: DocumentMetadata;
      vendorId?: number;
      force?: boolean;
      confirm?: boolean;
    }>(server, "documents_ingest", ({ db, actor, args }) => {
      const resolved = resolveDocumentMasterData(db, args.metadata, { vendorId: args.vendorId });
      if (!resolved.ok) return errorEnvelope(resolved.errors ?? ["resolveDocumentMasterData failed"]);
      const result = ingestDocument(db, args.company, args.filePath, resolved.metadata, {
        forceDuplicateLogicalIdentity: args.force === true,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      if (!result.ok) {
        recordException(db, {
          type: "DOCUMENT_INGEST_BLOCKED",
          severity: "medium",
          message: `Document ingest blocked for ${args.filePath}`,
          requiredAction: "Fix document metadata or duplicate handling, then retry ingest.",
          sourceEvidence: { file: args.filePath, errors: result.errors ?? [] },
          postingPreview: { retryCommand: "documents_ingest" },
        });
      }
      return wrapCoreResult(result);
    }),
  );
}
