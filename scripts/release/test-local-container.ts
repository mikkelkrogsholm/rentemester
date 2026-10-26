import { randomUUID } from "node:crypto";
import { SYNTHETIC_PDF_TEXT, syntheticNoTextPdf, syntheticTextPdf } from "../../tests/fixtures/pdf-parser/synthetic-text-pdf";
import { resolveContainerBuildIdentity } from "./container-build-identity";

type CommandResult = { stdout: string; stderr: string };

function run(command: string[], options: { allowFailure?: boolean } = {}): CommandResult {
  const result = Bun.spawnSync(command, { stdout: "pipe", stderr: "pipe" });
  const stdout = result.stdout.toString();
  const stderr = result.stderr.toString();
  if (result.exitCode !== 0 && !options.allowFailure) {
    throw new Error(`${command.join(" ")} failed (${result.exitCode})\n${stderr || stdout}`);
  }
  return { stdout, stderr };
}

async function api(container: string, path: string, body?: unknown): Promise<any> {
  const source = `const r=await fetch("http://127.0.0.1:4319${path}",{method:${JSON.stringify(body === undefined ? "GET" : "POST")},headers:{origin:"http://127.0.0.1:4319","content-type":"application/json"},${body === undefined ? "" : `body:${JSON.stringify(JSON.stringify(body))}`}});const value=await r.json();if(!r.ok)throw new Error(JSON.stringify(value));console.log(JSON.stringify(value));`;
  return JSON.parse(run(["docker", "exec", container, "bun", "-e", source]).stdout);
}

function setLegacyDocumentPath(container: string, slug: string, documentId: number): string {
  const dbPath = `/workspace/${slug}/data/ledger.sqlite`;
  const source = `import {Database} from "bun:sqlite";const db=new Database(${JSON.stringify(dbPath)});const row=db.query("SELECT stored_path FROM documents WHERE id=?").get(${documentId});const name=String(row.stored_path).replaceAll("\\\\","/").split("/").at(-1);const historical="/legacy-host/company/documents/originals/"+name;db.query("UPDATE documents SET stored_path=? WHERE id=?").run(historical,${documentId});db.close();console.log(historical);`;
  return run(["docker", "exec", container, "bun", "-e", source]).stdout.trim();
}

function registeredDocumentPath(container: string, slug: string, documentId: number): string {
  const dbPath = `/workspace/${slug}/data/ledger.sqlite`;
  const source = `import {openLedgerReadOnly} from "./src/core/ledger-inspection";const db=openLedgerReadOnly(${JSON.stringify(dbPath)});const row=db.query("SELECT stored_path FROM documents WHERE id=?").get(${documentId});db.close();console.log(row.stored_path);`;
  return run(["docker", "exec", container, "bun", "-e", source]).stdout.trim();
}

function ledgerPhysicalIdentity(container: string, slug: string): unknown {
  const dbPath = `/workspace/${slug}/data/ledger.sqlite`;
  const source = `import {createHash} from "node:crypto";import {existsSync,readFileSync} from "node:fs";import {openLedgerReadOnly} from "./src/core/ledger-inspection";const path=${JSON.stringify(dbPath)};const files=[path,path+"-wal",path+"-shm"].map(file=>existsSync(file)?{name:file.slice(path.length),sha256:createHash("sha256").update(readFileSync(file)).digest("hex")}:{name:file.slice(path.length),missing:true});const db=openLedgerReadOnly(path);const schemaVersion=Number(Object.values(db.query("PRAGMA schema_version").get())[0]);db.close();console.log(JSON.stringify({files,schemaVersion}));`;
  return JSON.parse(run(["docker", "exec", container, "bun", "-e", source]).stdout);
}

async function assertCompanyReadsArePhysicallyReadOnly(container: string, slug: string): Promise<unknown> {
  const before = ledgerPhysicalIdentity(container, slug);
  const repeatedBaseline = ledgerPhysicalIdentity(container, slug);
  if (JSON.stringify(repeatedBaseline) !== JSON.stringify(before)) {
    throw new Error(`read-only identity inspection changed SQLite state: ${JSON.stringify({ before, repeatedBaseline })}`);
  }
  const paths = [
    `/api/companies/${slug}/overview?year=2026&asOf=2026-01-01`,
    `/api/companies/${slug}/dashboard?asOf=2026-01-01`,
    `/api/companies/${slug}/income-statement?year=2026`,
  ];
  for (let pass = 0; pass < 2; pass += 1) {
    for (const path of paths) {
      await api(container, path);
      const after = ledgerPhysicalIdentity(container, slug);
      if (JSON.stringify(after) !== JSON.stringify(before)) {
        throw new Error(`read-only route ${path} changed SQLite state: ${JSON.stringify({ before, after })}`);
      }
    }
  }
  return before;
}

function syntheticMetadata(invoiceNo: string) {
  return { source: "email", documentType: "purchase_sale", issueDate: "2026-01-01", invoiceNo,
    deliveryDescription: "Synthetic parser smoke", amountIncVat: 1, vatAmount: 0, currency: "DKK",
    sender: { name: "Synthetic supplier", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK11223344" },
    recipient: { name: "Container Example ApS", address: "Testvej 2, 2100 København Ø", vatOrCvr: "DK12345678" } };
}

async function assertStylexPdfRendering(container: string, slug: string): Promise<void> {
  const issued = await api(container, `/api/companies/${slug}/invoices/issue`, {
    issueDate: "2026-05-16", dueDate: "2026-06-15", currency: "DKK",
    seller: { name: "Container Økonomi ApS", address: "Æblevej 1, København Ø", vatOrCvr: "DK10000001" },
    buyer: { name: "Żółć S.A.", address: "Łódź, Bærvej 2" },
    lines: Array.from({ length: 75 }, (_, index) => ({ description: `Rådgivning ${index + 1}`, quantity: 1, unitPriceExVat: 100 })),
  });
  if (!Number.isInteger(issued?.invoice?.documentId)) throw new Error("container invoice issuance failed");
  const source = `
    import {parsePdfBytes} from "./src/core/document-pdf-parser";
    import {buildIssuedInvoicePdf} from "./src/core/invoice-pdf";
    import {buildStatementPdf} from "./src/server/data/statement-pdf";
    import {renderDocumentPdf} from "./src/design/pdf-render";
    import {openLedgerReadOnly} from "./src/core/ledger-inspection";
    const db=openLedgerReadOnly(${JSON.stringify(`/workspace/${slug}/data/ledger.sqlite`)});
    const row=db.query("SELECT payload_json FROM documents WHERE id=?").get(${issued.invoice.documentId});
    db.close();
    const payload=JSON.parse(row.payload_json);
    const first=buildIssuedInvoicePdf(payload), second=buildIssuedInvoicePdf(payload);
    if(!first.equals(second))throw new Error("container PDF bytes are nondeterministic");
    const response=await fetch(${JSON.stringify(`http://127.0.0.1:4319/api/companies/${slug}/invoices/${issued.invoice.documentId}/pdf`)});
    if(!response.ok)throw new Error("container issued PDF download failed");
    const bytes=new Uint8Array(await response.arrayBuffer()), parsed=await parsePdfBytes(bytes);
    const text=parsed.pages.map(page=>page.text).join("\\n");
    if(parsed.status!=="ok"||parsed.pages.length<3||!text.includes("Żółć S.A.")||!text.includes("Rådgivning 75")||!text.includes("København Ø"))throw new Error("container PDF pagination/font/text failure");
    const report=buildStatementPdf({title:"Resultatopgørelse",company:{name:"Container Økonomi ApS",cvr:"10000001",currency:"DKK"},yearLabel:"2025/2026",generatedAtIsoDate:"2026-05-16",rows:Array.from({length:130},(_,i)=>({kind:"line",label:"Regnskabslinje "+(i+1),amount:"-100,00 DKK"}))});
    const reportParsed=await parsePdfBytes(report);
    if(reportParsed.pages.length<3||!reportParsed.pages.some(page=>page.text.includes("Regnskabslinje 130")))throw new Error("container statement PDF failed");
    let timedOut=false;
    try{renderDocumentPdf({html:"hello",title:"Synthetic",date:"2026-05-16",footer:""},{timeoutMs:1});}catch(error){timedOut=error.message.includes("timed out");}
    if(!timedOut)throw new Error("container PDF timeout failed");
    console.log("StyleX Takumi container PDF verified: fonts, selectable text, pagination, deterministic bytes and timeout");
  `;
  console.log(run(["docker", "exec", container, "bun", "-e", source]).stdout.trim());
}

async function waitForContainerReadiness(container: string): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const body = await api(container, "/api/ready");
      const health = await api(container, "/api/health");
      if (body?.ok === true && body?.checks?.workspaceControl === "ok" && body?.checks?.companyLedgers === "ok" && health?.deploymentProfile === "local-container") return;
    } catch { /* startup retry */ }
    await Bun.sleep(100);
  }
  const logs = run(["docker", "logs", container], { allowFailure: true });
  throw new Error(`networkless container did not become ready\n${logs.stderr}${logs.stdout}`);
}

const suffix = randomUUID().replaceAll("-", "").slice(0, 16);
const image = `rentemester-integration:${suffix}`;
const volume = `rentemester-integration-${suffix}`;
const first = `rentemester-first-${suffix}`;
const second = `rentemester-second-${suffix}`;
const identity = resolveContainerBuildIdentity();

try {
  run([
    "docker", "build", "--provenance=false", "--tag", image,
    "--build-arg", `RENTEMESTER_VERSION=${identity.version}`,
    "--build-arg", `RENTEMESTER_GIT_COMMIT=${identity.commit}`,
    "--build-arg", `RENTEMESTER_BUILT_AT=${identity.builtAt}`,
    "--build-arg", `RENTEMESTER_BUN_VERSION=${identity.bunVersion}`,
    "--build-arg", `RENTEMESTER_BASE_IMAGE_DIGEST=${identity.baseImageDigest}`,
    "--build-arg", `SOURCE_DATE_EPOCH=${identity.sourceDateEpoch}`,
    ".",
  ]);
  const documentExample = JSON.parse(
    run(["docker", "run", "--rm", "--read-only", image, "documents", "ingest", "--example"]).stdout,
  );
  if (
    documentExample?.source !== "email" ||
    documentExample?.currency !== "DKK" ||
    typeof documentExample?.amountIncVat !== "number"
  ) throw new Error("documents ingest --example must emit valid metadata JSON");
  run(["docker", "volume", "create", volume]);

  run([
    "docker", "run", "--detach", "--name", first, "--read-only", "--network", "none",
    "--memory", "512m", "--cpus", "1.0", "--pids-limit", "128",
    "--env", "RENTEMESTER_DEPLOYMENT_PROFILE=local-container",
    "--env", "RENTEMESTER_APP_AUTH=off",
    "--tmpfs", "/tmp:rw,noexec,nosuid,size=64m",
    "--tmpfs", "/import:rw,nosuid,size=64m",
    "--volume", `${volume}:/workspace`, image,
  ]);
  await waitForContainerReadiness(first);
  if (run(["docker", "exec", first, "id", "-u"]).stdout.trim() !== "1000") {
    throw new Error("container must run as uid 1000");
  }
  const emptyCompaniesBody = await api(first, "/api/companies");
  if (emptyCompaniesBody?.companies?.length !== 0) {
    throw new Error(`fresh volume must start with zero companies: ${JSON.stringify(emptyCompaniesBody)}`);
  }
  const createBody = await api(first, "/api/companies", { name: "Container Example ApS", cvr: "DK10000001" });
  if (createBody?.company?.slug !== "container-example-aps") {
    throw new Error(`first company creation failed: ${JSON.stringify(createBody)}`);
  }
  const slug = createBody.company.slug;
  run(["docker", "exec", first, "bun", "-e",
    `import {cpSync} from "node:fs";for(const copy of ["dry-run-copy","baseline-copy","restore-copy","retest-copy"])cpSync("/workspace/${slug}","/workspace/"+copy,{recursive:true});`,
  ]);
  const canonicalCompanies = await api(first, "/api/companies");
  if (canonicalCompanies?.companies?.map((company: { slug: string }) => company.slug).join(",") !== slug) {
    throw new Error(`mounted workspace must expose only its canonical live company: ${JSON.stringify(canonicalCompanies)}`);
  }
  const ingest = async (fileName: string, bytes: Uint8Array, invoiceNo: string) => api(first, `/api/companies/${slug}/documents/ingest`, { fileName, fileBase64: Buffer.from(bytes).toString("base64"), metadata: syntheticMetadata(invoiceNo), confirm: true });
  const textDocument = await ingest("synthetic-text.pdf", syntheticTextPdf(), "SYN-001");
  const textId = textDocument?.document?.id;
  if (!Number.isInteger(textId)) throw new Error(`synthetic text PDF ingest failed: ${JSON.stringify(textDocument)}`);
  const historicalTextPath = setLegacyDocumentPath(first, slug, textId);
  const firstParse = await api(first, `/api/companies/${slug}/documents/${textId}/parse`, { confirm: true });
  if (firstParse?.parse?.status !== "ok" || firstParse.parse.pageCount !== 1 || firstParse.parse.cached !== false) throw new Error(`text PDF parse failed: ${JSON.stringify(firstParse)}`);
  const parsedText = await api(first, `/api/companies/${slug}/documents/${textId}/parsed-text`);
  if (parsedText?.pages?.[0]?.text !== SYNTHETIC_PDF_TEXT || parsedText?.pages?.[0]?.itemCount < 2) throw new Error(`verified PDF text/layout missing: ${JSON.stringify(parsedText)}`);
  if (registeredDocumentPath(first, slug, textId) !== historicalTextPath) throw new Error("legacy stored_path was rewritten during container parsing");
  const cachedParse = await api(first, `/api/companies/${slug}/documents/${textId}/parse`, { confirm: true });
  if (cachedParse?.parse?.cached !== true || cachedParse?.parse?.resultHash !== firstParse.parse.resultHash) throw new Error(`PDF parse cache was not reused: ${JSON.stringify(cachedParse)}`);
  const noTextDocument = await ingest("synthetic-no-text.pdf", syntheticNoTextPdf(), "SYN-002");
  const noTextId = noTextDocument?.document?.id;
  const noTextParse = await api(first, `/api/companies/${slug}/documents/${noTextId}/parse`, { confirm: true });
  if (noTextParse?.parse?.status !== "no_text_layer") throw new Error(`no-text PDF outcome failed: ${JSON.stringify(noTextParse)}`);
  await assertStylexPdfRendering(first, slug);
  const firstReadIdentity = await assertCompanyReadsArePhysicallyReadOnly(first, slug);
  run(["docker", "rm", "--force", first]);

  run([
    "docker", "run", "--detach", "--name", second, "--read-only", "--network", "none",
    "--memory", "512m", "--cpus", "1.0", "--pids-limit", "128",
    "--env", "RENTEMESTER_DEPLOYMENT_PROFILE=local-container",
    "--env", "RENTEMESTER_APP_AUTH=off",
    "--tmpfs", "/tmp:rw,noexec,nosuid,size=64m",
    "--tmpfs", "/import:rw,nosuid,size=64m",
    "--volume", `${volume}:/workspace`, image,
  ]);
  await waitForContainerReadiness(second);
  const companiesBody = await api(second, "/api/companies");
  if (
    companiesBody?.companies?.length !== 1 ||
    companiesBody.companies[0]?.slug !== "container-example-aps"
  ) throw new Error(JSON.stringify(companiesBody));
  const restartedReadIdentity = await assertCompanyReadsArePhysicallyReadOnly(second, slug);
  if (JSON.stringify(restartedReadIdentity) !== JSON.stringify(firstReadIdentity)) {
    throw new Error(`container restart changed SQLite state: ${JSON.stringify({ firstReadIdentity, restartedReadIdentity })}`);
  }
  console.log("container integration passed: CLI example, canonical-only mounted workspace, constrained networkless non-root read-only runtime, legacy host-path PDF text/layout, cache, no-text outcome, physically read-only routes, persisted restart");
} finally {
  run(["docker", "rm", "--force", first], { allowFailure: true });
  run(["docker", "rm", "--force", second], { allowFailure: true });
  run(["docker", "volume", "rm", "--force", volume], { allowFailure: true });
  run(["docker", "image", "rm", "--force", image], { allowFailure: true });
}
