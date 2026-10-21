import { describe, expect, test, vi } from "bun:test";
import { screen, within } from "@testing-library/react";
import { PortfolioView } from "./PortfolioView";
import { renderAt } from "../test/render";
import { mockFetch, summary } from "../test/fixtures";

function portfolioRoute(companies: ReturnType<typeof summary>[]) {
  const rollup = companies.reduce(
    (acc, c) => ({
      resultat: acc.resultat + c.resultat,
      liquidity: acc.liquidity + (c.actualBankBalance ?? 0),
      vatPayable: acc.vatPayable + (c.vat?.payable ?? 0),
      openTaskCount: acc.openTaskCount + c.openTaskCount,
    }),
    { resultat: 0, liquidity: 0, vatPayable: 0, openTaskCount: 0 },
  );
  return {
    "GET /api/portfolio": {
      portfolio: {
        workspace: "/ws",
        asOf: "2026-05-20",
        companyCount: companies.length,
        rollup,
        totals: {},
        companies,
      },
    },
  };
}

describe("PortfolioView", () => {
  test("an empty workspace renders the first-run onboarding", async () => {
    mockFetch(portfolioRoute([]));
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/Velkommen til Rentemester/i)).toBeInTheDocument();
    expect(
      screen.getByRole("form", { name: /Opret virksomhed/i }),
    ).toBeInTheDocument();
  });

  test("renders one card per company, attention first", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "calm-aps", name: "Calm ApS" }),
        summary({ slug: "broken-aps", name: "Broken ApS", auditChainOk: false }),
      ]),
    );
    renderAt(<PortfolioView />);
    const headings = await screen.findAllByRole("heading", { level: 3 });
    // "needs attention" sorts the broken company first.
    expect(headings[0]).toHaveTextContent("Broken ApS");
    expect(headings[1]).toHaveTextContent("Calm ApS");
  });

  test("shows the attention summary count", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "a", name: "A", openTaskCount: 1 }),
        summary({ slug: "b", name: "B" }),
      ]),
    );
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/1 kræver opmærksomhed/i)).toBeInTheDocument();
  });

  test("renders the cross-company roll-up strip", async () => {
    mockFetch(
      portfolioRoute([
        summary({ slug: "a", name: "A" }),
        summary({ slug: "b", name: "B" }),
      ]),
    );
    renderAt(<PortfolioView />);
    expect(await screen.findByText(/Samlet resultat/i)).toBeInTheDocument();
    expect(screen.getByText(/Samlet likviditet/i)).toBeInTheDocument();
  });

  test("surfaces an API error with a retry affordance", async () => {
    mockFetch({
      "GET /api/portfolio": {
        __error: { code: "not_found", message: "Portfolio unavailable" },
      },
    });
    renderAt(<PortfolioView />);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Prøv igen/i })).toBeInTheDocument();
  });
});

test("unknown bank or VAT data does not imply healthy operation", async () => {
  mockFetch(portfolioRoute([summary({ actualBankBalance: null, vat: null })]));
  renderAt(<PortfolioView />);
  expect(await screen.findByText("Overblik kræver flere oplysninger")).toBeInTheDocument();
  expect(screen.queryByText("Sund drift")).not.toBeInTheDocument();
  expect(screen.queryByText("Ingen kendte advarsler")).not.toBeInTheDocument();
});

test("negative VAT is described as receivable in the portfolio and company card", async () => {
  mockFetch(portfolioRoute([summary({ vat: { payable: -250, deadline: "2026-09-01", daysRemaining: 10 } })]));
  renderAt(<PortfolioView />);
  expect(await screen.findAllByText("Moms til gode")).toHaveLength(2);
  expect(screen.queryByText("Moms at betale")).not.toBeInTheDocument();
});

test("native portfolio displays canonical cards and unknown aggregate without client sums",async()=>{
 const auth=await import("../lib/auth-context");vi.spyOn(auth,"useOptionalAuth").mockReturnValue({backend:"rust",loading:false,hosted:false,session:null,currentSessionId:null,context:null,refresh:async()=>{},clear:()=>{}});
 const card={...summary(),name:"Synthetic Native Company",periodStart:"2025-07-01",periodEnd:"2025-10-01",currency:"DKK",resultat:10,omsaetning:null,actualBankBalance:0,vat:{payable:25,periodStart:"2025-10-01",periodEnd:"2025-12-31",cutoff:"2025-10-01"},availability:{financials:{status:"unavailable",reason:"Synthetic source unavailable"},bank:{status:"complete",reason:null}},audit:{status:"verified",scope:"journal_hash_chain",entriesChecked:1},limitations:[]};
 mockFetch({"GET /api/portfolio":{portfolio:{workspace:"synthetic",asOf:"2025-10-01",companyCount:1,companies:[card],rollup:{resultat:999,liquidity:null,vatPayable:null,openTaskCount:null,bankBalanceComplete:false,knownBankCompanyCount:0,unknownBankCompanyCount:1},totals:{}}}});
 renderAt(<PortfolioView/>);
 const company=await screen.findByRole("article",{name:"Synthetic Native Company"});
 expect(within(company).getByText("Synthetic source unavailable")).toBeInTheDocument();
 expect(within(company).getByText(/Momsperiode 2025-10-01 – 2025-12-31, opgjort til 2025-10-01/)).toBeInTheDocument();
 expect(within(company).getByText("0,00 kr.",{exact:true})).toBeInTheDocument();
 expect(screen.getByText(/999,00/)).toBeInTheDocument();
 expect(screen.getByText(/Samlet banksaldo er ikke tilgængelig/)).toBeInTheDocument();
 expect(screen.getByText(/ikke et konsolideret koncernregnskab/)).toBeInTheDocument();
});


test("native imported company explains the invoice subset without hiding native invoice count",async()=>{
 const auth=await import("../lib/auth-context");vi.spyOn(auth,"useOptionalAuth").mockReturnValue({backend:"rust",loading:false,hosted:false,session:null,currentSessionId:null,context:null,refresh:async()=>{},clear:()=>{}});
 const card={...summary(),name:"Synthetic Imported Company",currency:"DKK",openInvoiceCount:1,availability:{invoices:{status:"partial",reason:"native_invoices_only_imported_receivables_separate"}},audit:{status:"verified",scope:"journal_hash_chain",entriesChecked:1},limitations:[]};
 mockFetch({"GET /api/portfolio":{portfolio:{workspace:"synthetic",asOf:"2025-10-01",companyCount:1,companies:[card],rollup:{resultat:0,liquidity:0,vatPayable:0,openTaskCount:0,bankBalanceComplete:true,knownBankCompanyCount:1,unknownBankCompanyCount:0},totals:{}}}});
 renderAt(<PortfolioView/>);
 const company=await screen.findByRole("article",{name:"Synthetic Imported Company"});
 expect(within(company).getByText("Fakturatal omfatter kun fakturaer udstedt i Rentemester. Importerede tilgodehavender følges særskilt.")).toBeInTheDocument();
 const invoiceLabel=within(company).getByText("Åbne fakturaer",{exact:true});
 expect(invoiceLabel.nextElementSibling).toHaveTextContent("1");
 expect(within(company).queryByText("Oplysningerne kræver yderligere dokumentation.")).not.toBeInTheDocument();
});
