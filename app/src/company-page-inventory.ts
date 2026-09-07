import type { CompanyRouteId } from "./company-route-registry";

export type PageInventoryState = "investigated" | "unavailable" | "mutation-required";

export type CockpitPageFamily = {
  template: string;
  state: PageInventoryState;
  reason: string;
  routeIds: readonly CompanyRouteId[];
};

/**
 * Route-level, executable acceptance inventory for #655.  `synthetic` means
 * the component has an existing mock-HTTP test; it deliberately does not
 * claim visual proof at 390px or 200% zoom.  That remains the immutable
 * candidate-browser runner's job.
 */
export type CockpitRouteAcceptance = {
  routeId: CompanyRouteId;
  evidence: "synthetic" | "mutation-required";
  contracts: readonly ("states" | "filters" | "table" | "keyboard" | "viewport")[];
};

/**
 * Detail routes are audited separately from the route registry. They inherit
 * the family evidence but are not registry entries themselves, so this list
 * deliberately must not be used for registry-exhaustiveness checks.
 */
export const COCKPIT_DETAIL_ROUTE_EVIDENCE = [
  {
    id: "party-profile",
    path: "/companies/:slug/parter/:partyId",
    family: "Filtreret register",
    state: "mutation-required" as const,
    reason: "Kræver en syntetisk canonical part med dokumenterede koblinger; det er en detailrute, ikke et register.",
  },
] as const;

/**
 * The human-facing inventory is intentionally by reusable page family, not URL.
 * `company-page-inventory.test.ts` makes this list exhaustive against the route
 * registry, so a new route must receive an explicit synthetic-test classification.
 */
export const COCKPIT_PAGE_FAMILIES: readonly CockpitPageFamily[] = [
  { template: "Overblik og arbejdsstatus", state: "investigated", reason: "Syntetiske komponentfixtures dækker læsning, tomme-/fejltilstande og sikre næste handlinger; kandidatbrowseren er fortsat visuel autoritet.", routeIds: ["dashboard", "attention", "suggestions", "exceptions", "workspace-inbox"] },
  { template: "Bankregister og afstemning", state: "investigated", reason: "BankView er baseline med syntetiske læse- og afstemningsfixtures.", routeIds: ["bank"] },
  { template: "Filtreret register", state: "mutation-required", reason: "Kræver syntetiske records og en sikker skriveopgave for fuld gennemgang.", routeIds: ["journal", "drafts", "documents", "payables", "purchase-overview", "mileage", "assets", "invoices", "invoice-templates", "contacts", "party-hub"] },
  { template: "Bogføringsarbejdsgang", state: "investigated", reason: "Syntetiske komponentfixtures dækker visning, læsning og de eksisterende bekræftelsesgates; ingen ny mutation er indført.", routeIds: ["approval-policy", "posting-rules", "batch-bookkeeping", "period-lock", "accruals"] },
  { template: "Finansiel rapport", state: "investigated", reason: "Syntetiske rapportfixtures dækker læsning, sammenligning, tomme-/fejltilstande og readiness; kandidatbrowseren verificerer fortsat visuel responsivitet.", routeIds: ["income-statement", "balance", "trial-balance", "obligations", "liquidity", "budget", "multi-year", "annual-report", "vat"] },
  { template: "Administration: profil, daglig opsætning og sikkerhed", state: "investigated", reason: "Syntetiske komponentfixtures dækker profil, register, opbevaring, integritet, GDPR og sikre eksisterende mutationsgates.", routeIds: ["workspace-register", "archive", "manage", "retention", "integrity", "gdpr", "accounts", "dimensions", "bank-accounts", "receipt-email"] },
];

export const INVENTORIED_COMPANY_ROUTE_IDS = COCKPIT_PAGE_FAMILIES.flatMap((family) => family.routeIds);

const WORKFLOW_MUTATIONS = new Set<CompanyRouteId>([
  "approval-policy", "posting-rules", "batch-bookkeeping", "period-lock", "accruals",
  "workspace-register", "manage", "gdpr", "dimensions", "bank-accounts", "receipt-email",
]);

/** Every registry route has an explicit #655 evidence classification. */
export const COCKPIT_ROUTE_ACCEPTANCE_MATRIX: readonly CockpitRouteAcceptance[] = INVENTORIED_COMPANY_ROUTE_IDS.map((routeId) => ({
  routeId,
  evidence: WORKFLOW_MUTATIONS.has(routeId) ? "mutation-required" : "synthetic",
  contracts: WORKFLOW_MUTATIONS.has(routeId)
    ? ["states", "keyboard", "viewport"]
    : ["states", "filters", "table", "keyboard", "viewport"],
}));
