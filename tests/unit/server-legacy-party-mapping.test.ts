import { describe, expect, test } from "bun:test";
import { createVendor } from "../../src/core/master-data";
import { createParty, linkPartyRole } from "../../src/core/party-registry";
import { activateWorkspaceUser, grantCompanyMembership } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { createWorkspaceServicePrincipal } from "../../src/core/workspace-service-principals";
import { createBetterAuthRequestProvider, openWorkspaceBetterAuth, WORKSPACE_SERVICE_PRINCIPAL_HEADER } from "../../src/server/better-auth";
import { companyPaths, companyRootForSlug, config, get, makeWorkspace, migrate, openDb, rmSync } from "./server-api/_shared";

const SECRET="I0UjL6i0-ScgvjfIgzMKJxPQyDpPXwg2mMKdLW3Y3WQ", ORIGIN="http://127.0.0.1:4319";

describe("#638 HTTP legacy party mapping",()=>{
  test("provides read-plan/list and membership-gated confirmed apply parity",async()=>{
    const workspace=makeWorkspace("legacy-party-http",["Allowed ApS"]),root=companyRootForSlug(workspace,"allowed-aps");
    const ledger=openDb(companyPaths(root).db);migrate(ledger);
    const vendor=createVendor(ledger,{name:"Foreign Supplier Inc.",address:"1 Evidence Way",countryCode:"US",identifierKind:"non_eu",notes:"keep this note"});
    if(!vendor.ok)throw new Error(vendor.errors.join("; "));
    ledger.query(`INSERT INTO documents(document_no,sha256_hash,payload_json,upload_datetime,source,status,document_type,sender_name,sender_address,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)`).run("HTTP-638","7".repeat(64),"{}","2026-09-01T00:00:00.000Z","synthetic","inbox","purchase_sale","Foreign Supplier Inc.","1 Evidence Way",null,"US","non_eu","resolved");ledger.close();
    const runtime=openWorkspaceBetterAuth(workspace,{secret:SECRET,trustedOrigins:[ORIGIN,"http://localhost"],baseURL:ORIGIN}),control=openWorkspaceControlDb(workspace);
    try{
      createParty(control,{partyId:"party-http-638",kind:"organization",name:"Foreign Supplier Inc.",identifiers:[],source:"synthetic",observedAt:"2026-09-01T00:00:00.000Z",reviewAssertion:"reviewed",actor:"user:test"});
      linkPartyRole(control,{partyId:"party-http-638",companySlug:"allowed-aps",role:"vendor",actor:"user:test"});
      const service=await createWorkspaceServicePrincipal(control,runtime.auth,{displayName:"Legacy mapping HTTP",actor:"user:test"});
      activateWorkspaceUser(control,{userId:service.serviceAccountId,workspaceRole:"member",actor:"user:test"});
      grantCompanyMembership(control,workspace,{userId:service.serviceAccountId,companySlug:"allowed-aps",role:"reader",actor:"user:test"});
      const hostedAuth={secret:SECRET,secrets:[{version:1,value:SECRET}],baseURL:ORIGIN,trustedOrigins:[ORIGIN],authEmail:{provider:"http-json-v1" as const,url:"https://mailer.example.test/send",bearerToken:"synthetic",from:"auth@example.test"},rateLimitIpHeader:"x-real-ip" as const};
      const hosted=config({workspaceRoot:workspace,deploymentProfile:"hosted",hostedBetterAuth:hostedAuth,betterAuthProvider:createBetterAuthRequestProvider(runtime.auth)});
      const headers={[WORKSPACE_SERVICE_PRINCIPAL_HEADER]:service.secret,"content-type":"application/json",origin:ORIGIN};
      const input={legacyKind:"vendor",legacyId:String(vendor.vendorId),partyId:"party-http-638",role:"vendor",documentId:1,reviewedLegacyReference:"review:http:1"};
      const plan=await get(hosted,"/api/companies/allowed-aps/legacy-party-mappings/plan",{method:"POST",headers,body:JSON.stringify(input)});
      expect(plan).toMatchObject({status:200,body:{ok:true,plan:{partyId:"party-http-638"}}});
      const planHash=(plan.body as any).plan.planHash;
      const applyBody={...input,planHash,idempotencyKey:"http-638-1",confirm:true};
      expect((await get(hosted,"/api/companies/allowed-aps/legacy-party-mappings/apply",{method:"POST",headers,body:JSON.stringify(applyBody)})).status).toBe(401);
      grantCompanyMembership(control,workspace,{userId:service.serviceAccountId,companySlug:"allowed-aps",role:"owner",actor:"user:test"});
      const applied=await get(hosted,"/api/companies/allowed-aps/legacy-party-mappings/apply",{method:"POST",headers,body:JSON.stringify(applyBody)});
      expect(applied).toMatchObject({status:200,body:{ok:true,idempotent:false}});
      const listed=await get(hosted,"/api/companies/allowed-aps/legacy-party-mappings?legacyKind=vendor",{headers});
      expect(listed).toMatchObject({status:200,body:{ok:true,rows:[{partyId:"party-http-638",contactSnapshot:{notes:"keep this note"}}]}});
    }finally{control.close();runtime.close();rmSync(workspace,{recursive:true,force:true});}
  });
});
