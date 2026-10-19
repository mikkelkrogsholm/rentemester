import { expect, test, vi } from "bun:test";
import { screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderAt } from "../test/render";
import { mockFetch, journal, trialBalance, incomeStatement, balance, overview } from "../test/fixtures";
import { IncomeStatementView } from "./IncomeStatementView";
import { BalanceView } from "./BalanceView";
import { TrialBalanceView } from "./TrialBalanceView";
import { JournalView } from "./JournalView";
import { DashboardView } from "./DashboardView";
import { formatKroner, compareMoney, moneyZero } from "../lib/format";
vi.mock("../components/PnlChart",()=>({PnlChart:()=> <div>Numeric live chart</div>}));
const hash="a".repeat(64),large="9007199254740991.01";
const meta={selectedYear:"2024",selectedSource:"archive" as const,source:"archive" as const,archived:true,archivedSource:"dinero",declaredCurrency:"DKK",fiscalYears:[{label:"2024",start:"2024-01-01",end:"2024-12-31",source:"live" as const},{label:"2024",start:null,end:null,source:"archive" as const,selectable:true}],periodStart:null,periodEnd:null,evidence:{importId:41,sourceHash:hash,parserContract:"rentemester-native-closed-history/1",declaredCurrency:"DKK",sourceSystem:"dinero",classificationSource:"current_chart",periodBasis:"archived_closing_snapshot; date not supplied",observedPostingStart:"2024-02-01",observedPostingEnd:"2024-03-01"},completeness:{status:"ready" as const,reasons:[],hasBalanceFile:true,closingBalanceCount:2,unclassifiedAccounts:[]},comparison:{status:"unavailable" as const,reason:"archive_period_bounds_unknown"}};
function mount(node:React.ReactElement,segment:string,extra=""){return renderAt(node,{route:`/companies/acme-aps/${segment}?year=2024&source=archive${extra}`,path:`/companies/:slug/${segment}`});}
function requestUrl(){return String((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.at(-1)![0]);}

test("P&L keeps exact large signed money, immutable currency, unavailable prior and source exports/drilldown",async()=>{
 mockFetch({"GET /api/companies/acme-aps/income-statement":{incomeStatement:incomeStatement({...meta,company:{...incomeStatement().company,name:"Current",cvr:null,country:"DK",currency:"EUR"},income:[{accountNo:"8000",name:"Original income",amount:large,priorAmount:null}],expense:[],totalIncome:large,totalExpense:"0.00",result:large,priorResult:null,priorTotalIncome:null,priorTotalExpense:null,priorYearLabel:null})}});
 mount(<IncomeStatementView/>,"resultatopgorelse");await screen.findByRole("heading",{name:"Resultatopgørelse",level:1});expect(requestUrl()).toContain("year=2024&source=archive");expect(screen.getAllByText("9.007.199.254.740.991,01 kr.").length).toBeGreaterThan(0);expect(screen.getByRole("columnheader",{name:"Sammenligning ikke tilgængelig"})).toBeInTheDocument();expect(screen.queryByRole("columnheader",{name:"2023"})).toBeNull();expect(screen.getByRole("link",{name:"8000"})).toHaveAttribute("href","/companies/acme-aps/posteringer?year=2024&source=archive&account=8000");expect(screen.getByRole("link",{name:"Hent CSV"})).toHaveAttribute("href","/api/companies/acme-aps/income-statement/export?format=csv&year=2024&source=archive");expect(screen.getByRole("link",{name:"Hent PDF"})).toHaveAttribute("href","/api/companies/acme-aps/income-statement/export?format=pdf&year=2024&source=archive");expect(screen.getByRole("link",{name:"Hent original arkivkilde"})).toHaveAttribute("href","/api/companies/acme-aps/archive-import/41/original");
});

test("unknown current classification keeps visible sources and null balance totals without fake dates or exports",async()=>{
 const unknown={accountNo:"9999",name:"Original unclassified",type:null,normalBalance:null,amount:"12.34",sourceRows:[{lineNo:7,sourceAmount:"12.340000",amount:"12.34"}]};
 mockFetch({"GET /api/companies/acme-aps/balance":{balance:balance({...meta,asOfDate:null,priorAsOfDate:null,assets:{lines:[],total:null,priorTotal:null},liabilities:{lines:[],total:null,priorTotal:null},equity:{lines:[],total:null,priorTotal:null},periodResult:null,totalAssets:null,totalLiabilitiesAndEquity:null,priorTotalLiabilitiesAndEquity:null,balanced:null,completeness:{...meta.completeness,status:"incomplete",reasons:["unknown_account_classification"],unclassifiedAccounts:[unknown]}})}});
 mount(<BalanceView/>,"balance");await screen.findByRole("heading",{name:"Balance",level:1});expect(screen.getByText(/Uklassificeret konto 9999/)).toHaveTextContent("12,34 kr.");expect(screen.getAllByText("Afslutningsdato ikke oplyst").length).toBeGreaterThan(0);expect(screen.getByText("Balancens kontrol kan ikke opgøres med det tilgængelige grundlag.")).toBeInTheDocument();expect(screen.queryByText(/2024-12-31/)).toBeNull();expect(screen.queryByText("0,00 kr.")).toBeNull();expect(screen.queryByRole("link",{name:"Hent CSV"})).toBeNull();
});

test("closing TB shows original six-place sources and missing closing file is unavailable rather than zero",async()=>{
 mockFetch({"GET /api/companies/acme-aps/trial-balance":{trialBalance:trialBalance({...meta,rows:[{accountNo:"9999",name:"Original",type:null,debit:"0.00",credit:"1.24",balance:"-1.24",sourceRows:[{lineNo:0,sourceAmount:"-1.235000",amount:"-1.24"}]}],totalDebit:"0.00",totalCredit:"1.24",balanced:false})}});
 const first=mount(<TrialBalanceView/>,"saldobalance");await screen.findByRole("heading",{name:"Saldobalance",level:1});expect(screen.getByText(/Afslutningssaldi, ikke årets/)).toBeInTheDocument();expect(screen.getByText(/-1.235000/)).toBeInTheDocument();expect(screen.getByRole("link",{name:"9999"})).toHaveAttribute("href","/companies/acme-aps/posteringer?year=2024&source=archive&account=9999");first.unmount();
 mockFetch({"GET /api/companies/acme-aps/trial-balance":{trialBalance:trialBalance({...meta,rows:[],totalDebit:null,totalCredit:null,balanced:null,completeness:{...meta.completeness,status:"unavailable",reasons:["missing_closing_balances"],hasBalanceFile:false,closingBalanceCount:0}})}});mount(<TrialBalanceView/>,"saldobalance");await screen.findByText("SaldoBalance-filen mangler; afslutningssaldi kan ikke opgøres.");expect(screen.queryByText("0,00 kr.")).toBeNull();expect(screen.getByText("Saldobalancens kontrol kan ikke opgøres med det tilgængelige grundlag.")).toBeInTheDocument();
});

test("archived journal groups use source IDs and expose raw amounts without fake posting/document actions",async()=>{
 const line={journalLineId:null,sourceId:"archive:source:posting:0",lineNo:0,accountNo:"9999",accountName:"Original",date:"2024-02-01",voucher:"42",voucherType:"Finans",vatType:"",debit:"0.00",credit:"1.24",balance:"-1.24",sourceAmount:"-1.235000",runningBalance:null,sourceRunningBalance:null,text:"Original line"};
 const entry={id:null,sourceId:"archive:source:voucher:42",entryNo:"42",date:"2024-02-01",text:"Original group",total:"1.24",lines:[line],documentId:null,documentNo:null};
 mockFetch({"GET /api/companies/acme-aps/journal":{journal:journal({...meta,entries:[entry,{...entry,entryNo:"43",sourceId:"archive:source:voucher:43",text:"Second group"}]})}});mount(<JournalView/>,"posteringer","&account=9999");await userEvent.click(await screen.findByRole("button",{name:/42.*Original group/}));expect(screen.getByText(/Originalbeløb: -1.235000/)).toBeInTheDocument();expect(screen.queryByRole("button",{name:"Ny postering"})).toBeNull();expect(screen.queryByRole("button",{name:"Tilbagefør postering"})).toBeNull();expect(screen.queryByRole("link",{name:/Åbn bilag/})).toBeNull();expect(screen.getByRole("link",{name:"Hent PDF"})).toHaveAttribute("href","/api/companies/acme-aps/journal/export?format=pdf&year=2024&account=9999&source=archive");
});

test("archived overview retains actual month labels and unavailable operational figures without zeros",async()=>{
 mockFetch({"GET /api/companies/acme-aps/overview":{overview:overview({...meta,profitAndLoss:{omsaetning:null,udgifter:null,resultat:null,months:[{month:"2024-02",label:"2024-02",income:"12.34",expense:null,status:"incomplete"}]},recentEntries:[{id:null,sourceId:"archive:row:0",entryNo:"42",date:"2024-02-01",text:"Observed",amount:"-1.24",sourceAmount:"-1.235000"}],lastPostedDate:"2024-02-01",bank:{balance:null,actualBalance:null,difference:null},receivables:{openCount:null,openTotal:null,status:"unavailable"},vat:null,exceptions:{count:null,rows:[],groups:[],status:"unavailable"},keyFigures:{bruttomargin:null,egenkapitalandel:null}})}});mount(<DashboardView/>,"");await screen.findByRole("heading",{name:"Overblik",level:1});const table=screen.getByRole("table",{name:"Observeret månedsaktivitet"});expect(within(table).getByText("2024-02")).toBeInTheDocument();expect(within(table).getByText("12,34 kr.")).toBeInTheDocument();expect(screen.getByText(/Bankafstemning, momsopgørelse/)).toBeInTheDocument();expect(screen.queryByText("0,00 kr.")).toBeNull();expect(screen.queryByText("Numeric live chart")).toBeNull();expect(screen.getByRole("link",{name:/Omsætning/})).toHaveAttribute("href","/companies/acme-aps/resultatopgorelse?year=2024&source=archive");
});

test("colliding source choices preserve explicit archive/live through requests and navigation",async()=>{
 mockFetch({"GET /api/companies/acme-aps/income-statement":{incomeStatement:incomeStatement({...meta,priorYearLabel:null})}});mount(<IncomeStatementView/>,"resultatopgorelse");const select=await screen.findByLabelText("Vælg regnskabsår");expect(within(select).getByRole("option",{name:"2024 (arkiv)"})).toHaveValue("archive:2024");expect(within(select).getByRole("option",{name:"2024 (aktiv)"})).toHaveValue("live:2024");await userEvent.selectOptions(select,"live:2024");await waitFor(()=>expect(requestUrl()).toContain("source=live"));
});

test("native missing prior comparison is unavailable and exact formatting never rounds large cents or null to zero",async()=>{
 expect(formatKroner(large,"DKK")).toBe("9.007.199.254.740.991,01 kr.");expect(formatKroner("-9007199254740991.01","EUR")).toBe("-9.007.199.254.740.991,01 EUR");expect(formatKroner(null)).toBe("—");expect(formatKroner("invalid")).toBe("—");expect(compareMoney(large,"9007199254740991.02")).toBe(-1);expect(moneyZero("0.00")).toBe(true);
 mockFetch({"GET /api/companies/acme-aps/income-statement":{incomeStatement:incomeStatement({priorYearLabel:null,priorResult:null,priorTotalIncome:null,priorTotalExpense:null,comparison:{status:"unavailable",reason:"prior_native_year_not_present"}})}});renderAt(<IncomeStatementView/>,{route:"/companies/acme-aps/resultatopgorelse",path:"/companies/:slug/resultatopgorelse"});expect(await screen.findByRole("columnheader",{name:"Sammenligning ikke tilgængelig"})).toBeInTheDocument();expect(screen.getByText(/Der findes ikke et foregående bogført år/)).toBeInTheDocument();
});


test("explicit archive requests never fall back to an alternative live report",async()=>{
 mockFetch({"GET /api/companies/acme-aps/journal":{journal:journal({selectedYear:"2024",selectedSource:"live",nativePosting:{ruleVersion:"v1",accounts:[]}})}});mount(<JournalView/>,"posteringer");await screen.findByText("Rapportens kilde svarer ikke til det valgte år og kilde. Der vises ingen alternativ rapport.");expect(screen.queryByRole("button",{name:"Ny postering"})).toBeNull();expect(screen.queryByText("Salg af ydelse")).toBeNull();
});
