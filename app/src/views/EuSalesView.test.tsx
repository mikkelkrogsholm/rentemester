import {test,expect} from "bun:test";
import {render,screen,fireEvent} from "@testing-library/react";
import {MemoryRouter,Routes,Route} from "react-router-dom";
import {EuSalesView} from "./EuSalesView";
import {stubGlobal} from "../test/globals";
test("EU sales preserves inclusive dates, signed DKK values and visible source facts",async()=>{
 const paths:string[]=[];stubGlobal("fetch",async(input:RequestInfo|URL)=>{paths.push(String(input));const q=new URL(String(input),"http://synthetic").searchParams;return new Response(JSON.stringify({ok:true,periodStart:q.get("from"),periodEnd:q.get("to"),customers:[{vatNumber:"DE123456789",countryCode:"DE",customerName:"Synthetic EU",invoiceCount:1,creditNoteCount:1,totalValue:745,invoiceNumbers:["S-1"],creditNoteNumbers:["CN-1"]}],totalValue:745,invoiceCount:1,creditNoteCount:1,sources:[{documentId:1,documentType:"credit_note",documentNumber:"CN-1",issueDate:"2025-10-12",vatNumber:"DE123456789",currency:"EUR",netAmountDkk:-745}],warnings:[],errors:[]}));});
 render(<MemoryRouter initialEntries={["/companies/synthetic/eu-salg"]}><Routes><Route path="/companies/:slug/eu-salg" element={<EuSalesView/>}/></Routes></MemoryRouter>);
 expect(await screen.findByText("Synthetic EU")).toBeInTheDocument();fireEvent.change(screen.getByLabelText("Fra"),{target:{value:"2025-09-01"}});fireEvent.change(screen.getByLabelText("Til"),{target:{value:"2025-10-12"}});fireEvent.click(screen.getByRole("button",{name:"Vis periode"}));
 expect(await screen.findByText(/2025-09-01 – 2025-10-12/)).toBeInTheDocument();expect(paths.at(-1)).toContain("from=2025-09-01&to=2025-10-12");expect(screen.getByText("EUR")).toBeInTheDocument();expect(screen.getByText(/^-745,00/)).toBeInTheDocument();
});
