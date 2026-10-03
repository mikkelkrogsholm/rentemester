import { matchPath } from "react-router-dom";

/**
 * The single catalogue of company destinations.  A company route belongs to
 * exactly one task area; both the SPA route table and CompanyNav consume this
 * data so a newly registered page cannot silently be omitted from navigation.
 */
export const COMPANY_TASK_AREAS = [
  { id: "overview", label: "Overblik" },
  { id: "bookkeeping", label: "Bogføring" },
  { id: "sales", label: "Salg og debitorer" },
  { id: "vat-periods", label: "Moms og perioder" },
  { id: "reports", label: "Rapporter og planlægning" },
  { id: "administration", label: "Virksomhedsadministration" },
] as const;

export type CompanyTaskAreaId = (typeof COMPANY_TASK_AREAS)[number]["id"];

const COMPANY_PAGE_ROUTES = [
  { id: "dashboard", segment: "", label: "Overblik", area: "overview" },
  { id: "income-statement", segment: "resultatopgorelse", label: "Resultatopgørelse", area: "reports" },
  { id: "balance", segment: "balance", label: "Balance", area: "reports" },
  { id: "trial-balance", segment: "saldobalance", label: "Saldobalance", area: "reports" },
  { id: "obligations", segment: "forpligtelser", label: "Forpligtelser", area: "reports" },
  { id: "liquidity", segment: "likviditet", label: "Likviditet", area: "reports" },
  { id: "budget", segment: "budget", label: "Budget", area: "reports" },
  { id: "journal", segment: "posteringer", label: "Posteringer", area: "bookkeeping" },
  { id: "drafts", segment: "kladder", label: "Kladder", area: "bookkeeping" },
  { id: "posting-rules", segment: "posteringsregler", label: "Posteringsregler", area: "bookkeeping" },
  { id: "batch-bookkeeping", segment: "batchbogfoering", label: "Bogføringskø", area: "bookkeeping" },
  { id: "bank", segment: "bank", label: "Bank", area: "bookkeeping" },
  { id: "vat", segment: "moms", label: "Moms", area: "vat-periods" },
  { id: "documents", segment: "bilag", label: "Bilag", area: "bookkeeping" },
  { id: "payables", segment: "leverandoerfaktura", label: "Leverandørfaktura", area: "bookkeeping" },
  { id: "invoices", segment: "fakturaer", label: "Fakturaer", area: "sales" },
  { id: "invoice-templates", segment: "faktura-skabeloner", label: "Skabeloner", area: "sales" },
  { id: "contacts", segment: "kontakter", label: "Kontakter", area: "sales" },
  { id: "workspace-register", segment: "workspace-register", label: "Workspace-register", area: "administration" },
  { id: "workspace-inbox", segment: "workspace-inbox", label: "Fælles indbakke", area: "administration" },
  { id: "mileage", segment: "koersel", label: "Kørsel", area: "bookkeeping" },
  { id: "assets", segment: "anlaeg", label: "Anlæg", area: "bookkeeping" },
  { id: "suggestions", segment: "agent-forslag", label: "Agent-forslag", area: "bookkeeping" },
  { id: "archive", segment: "arkiv", label: "Arkiv", area: "administration" },
  { id: "multi-year", segment: "fleraar", label: "Flerår", area: "reports" },
  { id: "manage", segment: "manage", label: "Virksomhedsoplysninger", area: "administration" },
  { id: "retention", segment: "retention", label: "Retention", area: "administration" },
  { id: "integrity", segment: "integritet", label: "Integritet", area: "administration" },
  { id: "accounts", segment: "kontoplan", label: "Kontoplan", area: "administration" },
  { id: "dimensions", segment: "dimensioner", label: "Dimensioner", area: "administration" },
  { id: "exceptions", segment: "undtagelser", label: "Undtagelser", area: "bookkeeping" },
  { id: "period-lock", segment: "periodelas", label: "Periodelås", area: "vat-periods" },
  { id: "bank-accounts", segment: "bankkonti", label: "Bankkonti", area: "administration" },
  { id: "gdpr", segment: "gdpr", label: "GDPR", area: "administration" },
  { id: "accruals", segment: "periodisering", label: "Periodisering", area: "vat-periods" },
  { id: "annual-report", segment: "aarsrapport", label: "Årsrapport", area: "reports" },
  { id: "receipt-email", segment: "bilagsmail", label: "Bilagsmail", area: "administration" },
] as const satisfies readonly {
  id: string;
  segment: string;
  label: string;
  area: CompanyTaskAreaId;
}[];

export const COMPANY_ROUTE_DEFINITIONS = [
  ...COMPANY_PAGE_ROUTES.map((route) => ({ ...route, kind: "page" as const, parentId: undefined })),
  { id: "invoice-create", segment: "fakturaer/ny", label: "Udsted faktura", area: "sales", kind: "flow", parentId: "invoices" },
  { id: "invoice-detail", segment: "fakturaer/:documentId", label: "Fakturadetaljer", area: "sales", kind: "flow", parentId: "invoices" },
  { id: "document-detail", segment: "bilag/:documentId", label: "Bilagsdetaljer", area: "bookkeeping", kind: "flow", parentId: "documents" },
  { id: "document-booking", segment: "bilag/:documentId/bogfoer", label: "Bogfør bilag", area: "bookkeeping", kind: "flow", parentId: "documents" },
] as const;

export type CompanyYearScope = "year" | "company" | "multi-year" | "vat-period";
export function companyYearScope(id: CompanyRouteId): CompanyYearScope {
  if (["documents", "document-detail", "workspace-register", "workspace-inbox", "contacts", "invoice-templates", "manage", "accounts", "dimensions", "bank-accounts", "gdpr", "retention", "integrity", "receipt-email", "posting-rules", "drafts", "batch-bookkeeping", "suggestions", "exceptions"].includes(id)) return "company";
  if (id === "multi-year") return "multi-year";
  if (id === "vat") return "vat-period";
  return "year";
}

export type CompanyRouteId = (typeof COMPANY_ROUTE_DEFINITIONS)[number]["id"];
export type CompanyRouteDefinition = (typeof COMPANY_ROUTE_DEFINITIONS)[number];

export function companyRoutePattern(segment: string): string {
  return segment ? `/companies/:slug/${segment}` : "/companies/:slug";
}

/** Fails closed if route registration and task-area classification drift apart. */
export function assertCompanyRouteCoverage(registeredRouteIds: readonly string[]) {
  const expected = new Set<string>(COMPANY_ROUTE_DEFINITIONS.map((route) => route.id));
  const registered = new Set(registeredRouteIds);
  const duplicateDefinitions = COMPANY_ROUTE_DEFINITIONS.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.id === route.id) !== index,
  );
  const duplicateSegments = COMPANY_ROUTE_DEFINITIONS.filter(
    (route, index, routes) => routes.findIndex((candidate) => candidate.segment === route.segment) !== index,
  );
  const invalidAreas = COMPANY_ROUTE_DEFINITIONS.filter(
    (route) => !COMPANY_TASK_AREAS.some((area) => area.id === route.area),
  );
  const missing = [...expected].filter((id) => !registered.has(id));
  const unexpected = [...registered].filter((id) => !expected.has(id));
  const duplicateRegistered = registered.size !== registeredRouteIds.length;

  if (duplicateDefinitions.length || duplicateSegments.length || invalidAreas.length || missing.length || unexpected.length || duplicateRegistered) {
    throw new Error(
      `Company route coverage failed: missing=${missing.join(",") || "none"}; unexpected=${unexpected.join(",") || "none"}; duplicates=${duplicateDefinitions.length + duplicateSegments.length + (duplicateRegistered ? 1 : 0)}`,
    );
  }
}

export function companyRouteForPath(pathname: string): CompanyRouteDefinition | undefined {
  if (pathname === "/companies/new" || pathname.startsWith("/companies/new/")) return undefined;
  return [...COMPANY_ROUTE_DEFINITIONS].sort((a, b) => b.segment.split("/").length - a.segment.split("/").length).find((route) => matchPath({ path: companyRoutePattern(route.segment), end: true }, pathname));
}
