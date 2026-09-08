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
