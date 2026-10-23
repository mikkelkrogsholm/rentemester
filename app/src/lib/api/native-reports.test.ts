import { withProtocol } from "../../test/protocol";
import {expect,test} from "bun:test";
import {api} from "../api";
import {stubGlobal} from "../../test/globals";
test("native report clients preserve exact selectors and canonical unknown values",async()=>{
 const calls:Array<{path:string;init?:RequestInit}>=[];
 const portfolio={asOf:"2025-07-01",rollup:{liquidity:null},companies:[]};
 const cashflow={selectedYear:"2025/26",status:"partial",totalIn:null,closingBalance:0,months:[{label:"2025-07",indbetalinger:null}]};
 stubFetch(async(input:RequestInfo|URL,init?:RequestInit)=>{const path=String(input);calls.push({path,init});return new Response(JSON.stringify({ok:true,...(path.startsWith("/api/portfolio")?{portfolio}:{cashflow})}),{headers:{"content-type":"application/json"}});});
 expect<unknown>(await api.nativePortfolio("2025-07-01")).toEqual(portfolio);
 expect<unknown>(await api.nativeCashflow("synthetic","2025/26")).toEqual(cashflow);
 expect(calls.map(call=>call.path)).toEqual(["/api/portfolio?asOf=2025-07-01","/api/companies/synthetic/cashflow?year=2025%2F26"]);
 expect(calls.every(call=>!call.init?.body&&(!call.init?.method||call.init.method==="GET"))).toBe(true);
});

const stubFetch = (handler: Parameters<typeof withProtocol>[0]) => stubGlobal("fetch", withProtocol(handler, [["GET", "/api/portfolio"], ["GET", "/api/companies/{slug}/cashflow"]]));
