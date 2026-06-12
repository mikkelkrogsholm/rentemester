import type { CommandSpec } from "./_shared";

// ===== FINANCIAL STATEMENTS (#176) =====
export const reportSpecs: CommandSpec[] = [
  {
    key: "report trial-balance",
    usage: "report trial-balance --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger en saldobalance med debet, kredit og saldo pr. konto for perioden.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå rapportens skema op dér før maskinel parsing.",
    ],
  },
  {
    key: "report profit-loss",
    usage: "report profit-loss --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description: "Bygger en resultatopgørelse (indtægter minus omkostninger) for perioden.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå rapportens skema op dér før maskinel parsing.",
    ],
  },
  {
    key: "report balance",
    usage: "report balance --company <path> --as-of <YYYY-MM-DD>",
    description: "Bygger en balance (aktiver, passiver, egenkapital) på en given dato.",
    allowedFlags: ["--company", "--as-of"],
    inputNotes: [
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå rapportens skema op dér før maskinel parsing.",
    ],
  },
  // ===== END FINANCIAL STATEMENTS (#176) =====
];

// ===== ANNUAL REPORT (#177) =====
// ===== TAX RETURN PREPARATION =====
export const reportAnnualAndTaxSpecs: CommandSpec[] = [
  {
    key: "report annual",
    usage: "report annual --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--ixbrl-out <file.xhtml>] | report annual --ixbrl-taxonomy",
    description: "Samler en årsrapport for regnskabsklasse B (resultatopgørelse, balance, noteskelet og ledelsespåtegning) for et lukket regnskabsår og kan skrive en deterministisk iXBRL-fil. Rentemester forbereder; ejer/revisor gennemgår og indberetter.",
    allowedFlags: ["--company", "--from", "--to", "--ixbrl-out", "--ixbrl-taxonomy"],
    inputNotes: [
      "--from / --to afgrænser regnskabsåret (skal være helt dækket af en lukket/indberettet periode)",
      "Kræver registreret CVR og balancerede bøger",
      "--ixbrl-out skriver en deterministisk iXBRL (inline-XBRL) XHTML-fil mod et afgrænset, versioneret micro/small-taksonomi-udsnit (resultatopgørelse, balance, ledelsespåtegning og anvendt regnskabspraksis)",
      "--ixbrl-taxonomy udskriver det afgrænsede iXBRL-taksonomi-udsnit (navn, version og elementliste) uden at læse ledgeren — kræver hverken --from/--to eller --company",
      "--json/--format json-outputtets feltliste (årsrapportens form) er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå skemaet op dér før maskinel parsing.",
    ],
  },
  // ===== END ANNUAL REPORT (#177) =====
  // ===== TAX RETURN PREPARATION =====
  {
    key: "report tax",
    usage: "report tax --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>",
    description:
      "Forbereder selskabets skattepligtige indkomst (oplysningsskema) for et lukket regnskabsår: årets resultat plus de skattemæssige reguleringer systemet kan se deterministisk, samt 22% selskabsskat for et ApS. Rentemester forbereder tallene; ejer/revisor gennemgår og indberetter selv via TastSelv Erhverv.",
    allowedFlags: ["--company", "--from", "--to"],
    inputNotes: [
      "FORUDSÆTNING: samme som 'report annual' — --from..--to skal være helt dækket af en lukket/indberettet periode, virksomheden skal have et registreret CVR, og bøgerne skal balancere.",
      "Dette er et bevidst smalt FØRSTE SLICE: kun den deterministiske regulering for ikke-fradragsberettiget repræsentationsmoms beregnes. Skattemæssige afskrivninger (saldoafskrivning), fremført underskud og selskabsformer ud over ApS markeres som needs-review — de beregnes IKKE.",
      "Ikke en TastSelv-integration: outputtet er tallene du selv overfører til oplysningsskemaet.",
      "--json/--format json-outputtets feltliste er en CLI-only rapport dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå skemaet op dér før maskinel parsing.",
    ],
  },
  // ===== END TAX RETURN PREPARATION =====
];
