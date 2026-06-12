import type { CommandSpec } from "./_shared";

// ===== REGULATORY COVERAGE =====
export const complianceSpecs: CommandSpec[] = [
  {
    key: "reg coverage",
    usage: "reg coverage [--out <file.md>] [--format json|human]",
    description:
      "Måler regulatorisk dækning: hvor stor en del af de in-scope danske lovbestemmelser der er sporbart implementeret i regler/kode. Repo-statisk — kræver ingen --company. Fejler hvis en regel citerer en bestemmelse der ikke kan slås op (closure), hvis lovteksten har ændret sig siden citatet (drift), eller hvis scope-manifestet (sources/scope.yaml) er ufuldstændigt (scope).",
    allowedFlags: ["--out"],
    inputNotes: [
      "Læser rules/dk/*.yaml, sources/scope.yaml og kildekorpuset i sources/downloaded/ — ingen virksomhedsdata",
      "--out skriver den deterministiske Markdown-rapport (REGULATORY_COVERAGE.md-format)",
      "Dækning = in-scope operative bestemmelser citeret af mindst én regel / alle in-scope operative bestemmelser",
      "Scope afgrænses i sources/scope.yaml — en reviewbar manifest-fil",
    ],
  },
  {
    key: "reg citations",
    usage: "reg citations [--out <file.md>] [--format json|human]",
    description:
      "Skriver et deterministisk Markdown-review: for hver regel med citater vises reglens navn, forklaring og den ordrette lovtekst for hver citeret bestemmelse — så mapping regel→paragraf kan kontrolleres med øjnene. Repo-statisk.",
    allowedFlags: ["--out"],
    inputNotes: [
      "Læser rules/dk/*.yaml og kildekorpuset i sources/downloaded/ — ingen virksomhedsdata",
      "--out skriver review-filen; uden --out skrives den til stdout",
    ],
  },
  {
    key: "compliance report",
    usage: "compliance report --company <path> --out <file.html> [--as-of YYYY-MM-DD]",
    description:
      "Skriver en deterministisk HTML-rapport en virksomhedsejer kan udlevere til revisor eller myndighed. Rapporten samler audit-kæde-status, backup-overholdelse, opbevaringsfrister, GDPR-posture, regulatorisk dækning og hele regel→paragraf-mapping i ét printbart dokument. Rapport-fingerprint (sha256) viser at en udleveret kopi ikke er ændret.",
    allowedFlags: ["--company", "--out", "--as-of", "--as-of-instant"],
    inputNotes: [
      "--out: stien til HTML-filen der genereres. Forældermappen oprettes ved behov.",
      "--as-of (YYYY-MM-DD, standard: dags dato): regnedatoen retention og backup-status evalueres mod.",
      "--as-of-instant (ISO-8601, standard: nu): tidsstemplet i rapporthovedet. Bruges typisk til byte-deterministisk gendannelse i tests og smoke; spring over i daglig brug.",
      "Rapporten er byte-for-byte deterministisk når samme --as-of og --as-of-instant gives — fingerprint kan bruges til at bevise integritet.",
      "Genererer ikke nye bogføringsposter eller audit-events — rent læse-kald.",
    ],
  },
  // ===== END REGULATORY COVERAGE =====
];
