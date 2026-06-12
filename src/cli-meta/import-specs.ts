import type { CommandSpec } from "./_shared";

// ===== IMPORT FRAMEWORK (#185) =====
export const importSpecs: CommandSpec[] = [
  {
    key: "import run",
    usage: "import run --company <path> --file <export-file> [--system <id>]",
    description: "Migrerer en virksomhed fra et andet bogføringssystem ind i Rentemester. Parser eksportfilen med den valgte per-system-parser og bogfører resultatet som virksomhedens primobalance (#179). Idempotent: præcis én import/primobalance pr. virksomhed.",
    allowedFlags: ["--company", "--file", "--system"],
    examplePath: "examples/import-synthetic.csv",
    exampleHint: "rentemester import run --example",
    inputNotes: [
      "--system vælger parseren; standard er 'synthetic-csv' (det indbyggede eksempel)",
      "Brug 'import systems' for at se tilgængelige parsere",
      "Eksportfilen skal balancere: sum debet == sum kredit (heltal øre)",
      "De rigtige e-conomic/Billy-parsere er en opfølgning — de kræver rigtige eksportfiler",
    ],
  },
  {
    key: "import systems",
    usage: "import systems [--format json|human]",
    description: "Lister de bogføringssystemer import-frameworket har en parser til. Read-only.",
    allowedFlags: [],
  },
  {
    key: "import archive",
    usage: "import archive --company <path> [--system <id>] [--year <YYYY>]",
    description: "Læser pre-cut-over arkivet: de tidligere regnskabsår fra et flerårigt eksport, der er gemt som read-only referencedata uden for hovedbogen (#197). Uden --year listes de arkiverede år; med --year vises årets fulde Posteringer/SaldoBalance.",
    allowedFlags: ["--company", "--system", "--year"],
    inputNotes: [
      "--system standard er 'dinero'",
      "Arkivet bogføres aldrig i den hash-kædede journal — kun cut-over året lander i hovedbogen",
    ],
  },
  {
    key: "import contacts",
    usage: "import contacts --company <path> --file <Kontakter.csv> [--enrich-cvr] [--default-role customer|vendor]",
    description: "Importerer en Dinero kontakt-eksport (Kontakter.csv) til kunde- og leverandørkartoteket. Hver kontakt klassificeres ud fra salgs-/købshistorik. Idempotent: allerede kendte kontakter springes over.",
    allowedFlags: ["--company", "--file", "--enrich-cvr", "--default-role"],
    inputNotes: [
      "--enrich-cvr beriger danske kontakter med stamdata fra CVR-registret (kræver CVR_USERNAME/CVR_PASSWORD)",
      "CSV-værdier vinder altid over CVR-data — berigelse udfylder kun tomme felter",
      "--default-role styrer kontakter uden salgs-/købshistorik (standard: vendor)",
      "Selve CSV-importen er deterministisk og offline; berigelse er det valgfri netværkslag",
    ],
  },
  // ===== END IMPORT FRAMEWORK (#185) =====
];
