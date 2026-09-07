import type { CompanyRouteId } from "./company-route-registry";

export type PageInventoryState = "investigated" | "unavailable" | "mutation-required";

export type CockpitPageFamily = {
  template: string;
  state: PageInventoryState;
  reason: string;
  routeIds: readonly CompanyRouteId[];
};

/**
 * The human-facing inventory is intentionally by reusable page family, not URL.
 * `company-page-inventory.test.ts` makes this list exhaustive against the route
 * registry, so a new route must receive an explicit synthetic-test classification.
 */
export const COCKPIT_PAGE_FAMILIES: readonly CockpitPageFamily[] = [
  { template: "Overblik og arbejdsstatus", state: "mutation-required", reason: "Kræver syntetisk arbejds- og opmærksomhedsdata.", routeIds: ["dashboard", "suggestions", "exceptions", "workspace-inbox"] },
  { template: "Bankregister og afstemning", state: "investigated", reason: "BankView er baseline med syntetiske læse- og afstemningsfixtures.", routeIds: ["bank"] },
  { template: "Filtreret register", state: "mutation-required", reason: "Kræver syntetiske records og en sikker skriveopgave for fuld gennemgang.", routeIds: ["journal", "drafts", "documents", "payables", "purchase-overview", "mileage", "assets", "invoices", "invoice-templates", "contacts", "accounts", "dimensions", "bank-accounts", "receipt-email"] },
  { template: "Bogføringsarbejdsgang", state: "mutation-required", reason: "Kræver en syntetisk, bekræftet arbejdsgang uden produktionsdata.", routeIds: ["approval-policy", "posting-rules", "batch-bookkeeping", "period-lock", "accruals"] },
  { template: "Finansiel rapport", state: "unavailable", reason: "Mangler endnu et fælles syntetisk rapportkorpus til sidefamilie-gennemgang.", routeIds: ["income-statement", "balance", "trial-balance", "obligations", "liquidity", "budget", "multi-year", "annual-report", "vat"] },
  { template: "Virksomhedsadministration", state: "mutation-required", reason: "Kræver syntetisk virksomhedsprofil og eksplicitte, reversible mutationer.", routeIds: ["workspace-register", "archive", "manage", "retention", "integrity", "gdpr"] },
];

export const INVENTORIED_COMPANY_ROUTE_IDS = COCKPIT_PAGE_FAMILIES.flatMap((family) => family.routeIds);
