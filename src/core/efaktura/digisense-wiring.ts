// Digisense produktions-wiring (#efaktura) — binder secret-laget + state-laget +
// klienten + transmitteren sammen til ÉN konkret PeppolTransmitter, klar til
// transmitPublicEInvoicePeppol.
//
// Dette lag rører rigtig konfiguration (license-key fra config/digisense.json)
// og rigtig state (companyKey fra digisense_companies), men IKKE netværket:
// selve HTTP-kaldene sker først når transmitteren køres. Derfor er det her
// CLI/MCP henter sin transmitter, mens unit-tests injicerer en fake klient
// direkte i createDigisenseTransmitter og aldrig rører denne fil.
//
// Trust-boundary: license-key læses fra secret-filen og gives KUN til klienten;
// den lægges aldrig i ledgeren eller i en envelope. companyKey er ikke-secret
// state og må gerne returneres.

import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { createDigisenseClient, type DigisenseClient, type KsefEnvironment } from "./digisense-client";
import { createDigisenseTransmitter } from "./digisense-transmitter";
import { defaultDocumentDownloader, type DigisenseDocumentDownloader } from "./digisense-receive";
import { loadDigisenseSecretConfig } from "./digisense-config";
import { resolveBoundDigisenseCompanyKey } from "./digisense-identity";
import type { PeppolTransmitter, PeppolAccessPointConfig } from "../public-einvoice";

export type ResolveDigisenseTransmitter =
  | { ok: true; transmitter: PeppolTransmitter; companyKey: string; providerBinding: string }
  | { ok: false; errors: string[] };

export type ResolveDigisenseStatusChecker =
  | { ok: true; client: DigisenseClient; companyKey: string; environment: "test" | "production" }
  | { ok: false; errors: string[] };

/** Bind a preflight to the resolved account and environment, never to its secret. */
export function digisenseProviderBinding(environment: "test" | "production", companyKey: string) {
  return createHash("sha256").update(JSON.stringify(["digisense", environment, companyKey])).digest("hex");
}

/** Resolves the status-only client for a previously queued Digisense document. */
export function resolveDigisenseStatusChecker(
  db: Database,
  companyRoot: string,
  options: { companyKey?: string } = {},
): ResolveDigisenseStatusChecker {
  const secret = loadDigisenseSecretConfig(companyRoot);
  if (!secret) return { ok: false, errors: [digisenseNotConfiguredError("kontrolleres leveringsstatus")] };
  const companyKey = resolveBoundDigisenseCompanyKey(db, options.companyKey);
  if (!companyKey.ok) return { ok: false, errors: companyKey.errors };
  return { ok: true, companyKey: companyKey.value, environment: secret.environment, client: createDigisenseClient({ apiLicenseKey: secret.apiLicenseKey, environment: secret.environment }) };
}

/**
 * For Digisense ER access point'et Digisense selv: createDigisenseTransmitter
 * ignorerer accessPoint/receiverEndpointId helt og router på companyKey +
 * license-key. transmitPublicEInvoicePeppol kræver dog en ikke-tom access-point-
 * config (validateAccessPointConfig) OG udleder idempotency-nøglen af den.
 *
 * Hvis brugeren selv skulle opfinde disse felter, ville to afsendelser af SAMME
 * faktura med forskelligt-udfyldte dummy-felter give FORSKELLIGE idempotency-
 * nøgler — så `acknowledged`-fast-path'en ikke matcher og fakturaen leveres to
 * gange. Derfor syntetiserer vi en FAST, deterministisk identitet keyed på
 * companyKey: samme faktura + samme companyKey ⇒ samme idempotency-nøgle ⇒
 * gentaget transmit kollapser på den eksisterende acknowledged-række.
 */
export function digisenseAccessPointIdentity(companyKey: string): PeppolAccessPointConfig {
  const key = companyKey.trim();
  return {
    accessPointId: "digisense",
    endpointUrl: "digisense:access-point",
    senderEndpointId: key,
  };
}

/**
 * Bygger den rigtige Digisense-`PeppolTransmitter` for en virksomhed.
 *
 * companyKey vælges sådan:
 *   - `companyKey` angivet eksplicit ⇒ brug den (entydigt).
 *   - ellers: præcis ÉN registreret virksomhed i digisense_companies ⇒ brug
 *     dens companyKey.
 *   - ellers (nul, eller flere uden valg) ⇒ en tydelig fejl, så afsenderen
 *     aldrig leverer til en tilfældig companyKey.
 *
 * Returnerer et Result (ingen throw) så CLI/MCP kan vise en pæn fejl-envelope.
 */
export function resolveDigisenseTransmitter(
  db: Database,
  companyRoot: string,
  options: { companyKey?: string; ksefEnvironment?: KsefEnvironment } = {},
): ResolveDigisenseTransmitter {
  const secret = loadDigisenseSecretConfig(companyRoot);
  if (!secret) {
    return { ok: false, errors: [digisenseNotConfiguredError("sendes")] };
  }

  const companyKey = resolveBoundDigisenseCompanyKey(db, options.companyKey);
  if (!companyKey.ok) return { ok: false, errors: companyKey.errors };

  const client = createDigisenseClient({
    apiLicenseKey: secret.apiLicenseKey,
    environment: secret.environment,
  });

  // KSeF uses PROD (not "PRODUCTION") as its production environment value.
  // hvis ikke eksplicit sat — så test-credentials aldrig rammer prod-routing.
  const ksefEnvironment =
    options.ksefEnvironment ?? (secret.environment === "production" ? "PROD" : "TEST");

  const transmitter = createDigisenseTransmitter(client, {
    companyKey: companyKey.value,
    ksefEnvironment,
  });
  return { ok: true, transmitter, companyKey: companyKey.value, providerBinding: digisenseProviderBinding(secret.environment, companyKey.value) };
}

export type ResolveDigisenseReceiver =
  | { ok: true; client: DigisenseClient; downloader: DigisenseDocumentDownloader; companyKey: string }
  | { ok: false; errors: string[] };

/**
 * Bygger den rigtige Digisense-klient + XML-downloader til MODTAG-stien for en
 * virksomhed. Samme companyKey-valg som transmitteren: eksplicit angivet, eller
 * præcis ÉN registreret virksomhed, ellers en tydelig fejl.
 *
 * Rører rigtig config (license-key) + rigtig state (companyKey) men IKKE
 * netværket: HTTP-kald sker først når pollen kører. Unit-tests injicerer en fake
 * klient + downloader direkte i pollDigisenseReceived og rører aldrig denne fil.
 */
export function resolveDigisenseReceiver(
  db: Database,
  companyRoot: string,
  options: { companyKey?: string } = {},
): ResolveDigisenseReceiver {
  const secret = loadDigisenseSecretConfig(companyRoot);
  if (!secret) {
    return { ok: false, errors: [digisenseNotConfiguredError("modtages")] };
  }

  const companyKey = resolveBoundDigisenseCompanyKey(db, options.companyKey);
  if (!companyKey.ok) return { ok: false, errors: companyKey.errors };

  const client = createDigisenseClient({
    apiLicenseKey: secret.apiLicenseKey,
    environment: secret.environment,
  });
  return {
    ok: true,
    client,
    downloader: defaultDocumentDownloader(),
    companyKey: companyKey.value,
  };
}

export type ResolveDigisenseRegistrar =
  | { ok: true; client: DigisenseClient }
  | { ok: false; errors: string[] };

/**
 * Bygger den rigtige Digisense-klient til REGISTRÉR-stien for en virksomhed.
 *
 * Modsat send/modtag kræver registrering INGEN eksisterende companyKey — den
 * SKABER en — så her behøves kun secret-laget (license-key). companyKey'en
 * gemmes af registreringen selv (registerDigisenseCompany).
 *
 * Rører rigtig config (license-key) men IKKE netværket: HTTP-kald sker først når
 * registreringen kører. Unit-tests injicerer en fake klient direkte i
 * registerDigisenseCompany og rører aldrig denne fil.
 */
export function resolveDigisenseRegistrar(
  companyRoot: string,
): ResolveDigisenseRegistrar {
  const secret = loadDigisenseSecretConfig(companyRoot);
  if (!secret) {
    return { ok: false, errors: [digisenseNotConfiguredError("registreres en virksomhed til e-faktura")] };
  }
  const client = createDigisenseClient({
    apiLicenseKey: secret.apiLicenseKey,
    environment: secret.environment,
  });
  return { ok: true, client };
}

/**
 * Den fælles "ikke konfigureret"-fejl. Peger på den KONKRETE opskrift en agent
 * skal følge (kommandoen `efaktura konfigurer` / MCP-tool'et `efaktura_konfigurer`,
 * der skriver config/digisense.json med 0600), i stedet for den interne frase
 * "via secret-laget" — så konfigurations-precondition'en er opnåelig via
 * interfacet alene.
 */
function digisenseNotConfiguredError(action: string): string {
  return (
    `Digisense er ikke konfigureret: gem en API license-key først med ` +
    `\`rentemester efaktura konfigurer --company <path> --api-license-key <secret> ` +
    `--environment test|production\` (eller MCP-tool'et efaktura_konfigurer). Det skriver ` +
    `config/digisense.json (0600) før der kan ${action} e-fakturaer.`
  );
}
