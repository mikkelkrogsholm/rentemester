type CommandSpec = {
  key: string;
  usage: string;
  description: string;
  allowedFlags: string[];
  examplePath?: string;
  exampleHint?: string;
  inputNotes?: string[];
};

const GLOBAL_FLAGS = ["--help", "--example", "--format", "--json", "--actor", "--actor-via"];

export const COMMAND_SPECS: CommandSpec[] = [
  {
    key: "init",
    usage: "init --company <path> [--cvr <DK12345678>] [--fiscal-year-start-month <1-12>] [--fiscal-year-label-strategy end-year|start-year|span]",
    description: "Initialiserer en virksomhed og opretter standardkontoplan.",
    allowedFlags: ["--company", "--cvr", "--fiscal-year-start-month", "--fiscal-year-label-strategy"],
  },
  { key: "system healthcheck", usage: "system healthcheck --company <path>", description: "Tjekker at virksomhedsmappen og kernefiler findes.", allowedFlags: ["--company"] },
  { key: "system backup", usage: "system backup --company <path> [--at <ISO-8601>] [--sign-with-ed25519]", description: "Opretter en revisionsklar backup. Med --sign-with-ed25519 tilføjes en asymmetrisk signatur som 3.-part kan verificere uafhængigt.", allowedFlags: ["--company", "--at", "--sign-with-ed25519"] },
  { key: "system backup-status", usage: "system backup-status --company <path> [--as-of <ISO-8601>]", description: "Viser om backup-pligten er opfyldt.", allowedFlags: ["--company", "--as-of"] },
  { key: "system restore-backup", usage: "system restore-backup --backup-dir <dir> --target-company <path> [--verify-key <path>] [--public-key <path>]", description: "Gendanner en backup til en ny virksomhedssti.", allowedFlags: ["--backup-dir", "--target-company", "--verify-key", "--public-key"] },
  { key: "system export-public-key", usage: "system export-public-key --company <path> --out <file>", description: "Eksporterer virksomhedens ed25519 backup-public-key til en fil (deles med revisor/myndighed).", allowedFlags: ["--company", "--out"] },
  { key: "system verify-backup-signature", usage: "system verify-backup-signature --backup-dir <dir> [--public-key <path>] [--verify-key <path>]", description: "Verificerer signaturen på en backup uden at restore. Med --public-key kan en 3.-part verificere uden source-adgang.", allowedFlags: ["--backup-dir", "--public-key", "--verify-key"] },
  { key: "system export-authority", usage: "system export-authority --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --out <dir> [--requested-at <ISO-8601>] [--requester <name>]", description: "Eksporterer materiale til myndighedsudlevering.", allowedFlags: ["--company", "--from", "--to", "--out", "--requested-at", "--requester"] },
  { key: "audit verify", usage: "audit verify --company <path>", description: "Verificerer audit-kæde og bogføringsintegritet.", allowedFlags: ["--company"] },
  { key: "accounts list", usage: "accounts list --company <path>", description: "Lister kontoplanen.", allowedFlags: ["--company"] },
  { key: "customer create", usage: "customer create --company <path> --name <text> [--address <text>] [--cvr <DK...>] [--email <text>] [--ean <text>] [--payment-terms <days>] [--currency <ISO>] [--notes <text>]", description: "Opretter en append-only kundepost til genbrug på fakturaer.", allowedFlags: ["--company", "--name", "--address", "--cvr", "--email", "--ean", "--payment-terms", "--currency", "--notes"] },
  { key: "customer list", usage: "customer list --company <path> [--archived]", description: "Lister kendte kunder.", allowedFlags: ["--company", "--archived"] },
  { key: "customer validate-vat", usage: "customer validate-vat --company <path> --cvr <EU-VAT>", description: "Validerer et EU-VAT-nummer via VIES og cacher resultatet.", allowedFlags: ["--company", "--cvr"] },
  { key: "vendor create", usage: "vendor create --company <path> --name <text> [--address <text>] [--cvr <DK...>] [--expense-account <konto>] [--default-vat <text>] [--notes <text>]", description: "Opretter en append-only leverandørpost til bilagsindlæsning.", allowedFlags: ["--company", "--name", "--address", "--cvr", "--expense-account", "--default-vat", "--notes"] },
  { key: "vendor list", usage: "vendor list --company <path> [--archived]", description: "Lister kendte leverandører.", allowedFlags: ["--company", "--archived"] },
  { key: "exceptions list", usage: "exceptions list --company <path> [--status open|resolved|all]", description: "Lister exceptions-køen.", allowedFlags: ["--company", "--status"] },
  { key: "exceptions resolve", usage: "exceptions resolve --company <path> --id <n> [--note <text>]", description: "Markerer en exception som løst.", allowedFlags: ["--company", "--id", "--note"] },
  {
    key: "invoice validate",
    usage: "invoice validate --input <file.json>",
    description: "Validerer en faktura-payload uden at gemme den.",
    allowedFlags: ["--input"],
    examplePath: "examples/full-invoice.dk.json",
    exampleHint: "rentemester invoice validate --example > faktura.json",
    inputNotes: [
      'invoiceType: "full" | "simplified"',
      'vatTreatment: "standard" | "domestic_reverse_charge" | "foreign_reverse_charge"',
      "issueDate: YYYY-MM-DD",
      "seller, buyer, lines, totals, currency",
    ],
  },
  {
    key: "invoice issue",
    usage: "invoice issue --company <path> --input <file.json> [--customer-id <n>]",
    description: "Udsteder en kundefaktura og gemmer et immutabelt snapshot.",
    allowedFlags: ["--company", "--input", "--customer-id"],
    examplePath: "examples/full-invoice.dk.json",
    exampleHint: "rentemester invoice issue --example > faktura.json",
    inputNotes: [
      'invoiceType: "full" | "simplified"',
      'vatTreatment: "standard" | "domestic_reverse_charge" | "foreign_reverse_charge"',
      "issueDate: YYYY-MM-DD",
      "invoiceNumber: valgfri; udelad for automatisk nummerering",
      "seller: { name, address, vatOrCvr }",
      "buyer: { name, address, ... }",
      "lines: [{ description, quantity, unitPriceExVat, lineTotalExVat }]",
      "totals: { netAmount, vatRate, vatAmount, grossAmount }",
      'currency: "DKK"',
      "dueDate: YYYY-MM-DD",
    ],
  },
  { key: "invoice render", usage: "invoice render --company <path> (--document-id <n> | --invoice-number <no>)", description: "Renderer eller genskaber en deterministisk PDF for en udstedt faktura.", allowedFlags: ["--company", "--document-id", "--invoice-number"] },
  { key: "invoice credit-note", usage: "invoice credit-note --company <path> --input <file.json>", description: "Udsteder en kreditnota mod en eksisterende faktura.", allowedFlags: ["--company", "--input"], examplePath: "examples/credit-note.json" },
  { key: "invoice post", usage: "invoice post --company <path> (--document-id <n> | --invoice-number <no>)", description: "Bogfører en udstedt faktura i finansen.", allowedFlags: ["--company", "--document-id", "--invoice-number"] },
  { key: "invoice settle-bank", usage: "invoice settle-bank --company <path> --input <file.json>", description: "Matcher en bankbetaling mod en faktura.", allowedFlags: ["--company", "--input"], examplePath: "examples/invoice-settlement.json" },
  { key: "invoice settle-claim-bank", usage: "invoice settle-claim-bank --company <path> --input <file.json>", description: "Matcher en bankbetaling mod fakturakrav.", allowedFlags: ["--company", "--input"], examplePath: "examples/invoice-claim-settlement.json" },
  { key: "invoice write-off-bad-debt", usage: "invoice write-off-bad-debt --company <path> --input <file.json>", description: "Bogfører tab på debitor.", allowedFlags: ["--company", "--input"], examplePath: "examples/invoice-bad-debt-writeoff.json" },
  { key: "invoice refund-bank", usage: "invoice refund-bank --company <path> --input <file.json>", description: "Bogfører refundering til kunden fra banken.", allowedFlags: ["--company", "--input"], examplePath: "examples/invoice-refund.json" },
  { key: "invoice apply-payment", usage: "invoice apply-payment --company <path> --input <file.json>", description: "Registrerer en fakturabetaling direkte fra payload.", allowedFlags: ["--company", "--input"], examplePath: "examples/invoice-payment.json" },
  { key: "invoice remind", usage: "invoice remind --company <path> (--document-id <n> | --invoice-number <no>) --date <YYYY-MM-DD> [--fee <n>] [--note <text>]", description: "Registrerer en rykker på en forfalden faktura.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--date", "--fee", "--note"] },
  { key: "invoice post-reminder", usage: "invoice post-reminder --company <path> (--document-id <n> | --invoice-number <no>) [--reminder-id <n>] [--date <YYYY-MM-DD>]", description: "Bogfører en registreret rykker.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--reminder-id", "--date"] },
  { key: "invoice status", usage: "invoice status --company <path> (--document-id <n> | --invoice-number <no>) [--as-of <YYYY-MM-DD>]", description: "Viser åben saldo og status på en faktura.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--as-of"] },
  { key: "invoice list", usage: "invoice list --company <path> [--status open|paid|credited|refunded|overpaid|written_off|overdue|all] [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>] [--customer-cvr <DK...>] [--customer <text>] [--invoice-number <no>] [--min-amount <n>] [--max-amount <n>] [--as-of <YYYY-MM-DD>]", description: "Lister udstedte fakturaer med filtre for status, kunde og dato.", allowedFlags: ["--company", "--status", "--from", "--to", "--customer-cvr", "--customer", "--invoice-number", "--min-amount", "--max-amount", "--as-of"] },
  { key: "invoice find", usage: "invoice find --company <path> [<query>] [--customer <text>] [--amount <n>] [--invoice-number <no>] [--as-of <YYYY-MM-DD>]", description: "Finder udstedte fakturaer via nummer, kunde eller beløb.", allowedFlags: ["--company", "--customer", "--amount", "--invoice-number", "--as-of"] },
  { key: "invoice overdue", usage: "invoice overdue --company <path> [--as-of <YYYY-MM-DD>] [--min-days <n>]", description: "Lister forfaldne udstedte fakturaer som ikke er fuldt afregnet.", allowedFlags: ["--company", "--as-of", "--min-days"] },
  { key: "invoice interest", usage: "invoice interest --company <path> (--document-id <n> | --invoice-number <no>) --as-of <YYYY-MM-DD> --reference-rate <pct>", description: "Beregner morarente på en faktura.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--as-of", "--reference-rate"] },
  { key: "invoice claim-interest", usage: "invoice claim-interest --company <path> (--document-id <n> | --invoice-number <no>) --as-of <YYYY-MM-DD> --reference-rate <pct> [--note <text>]", description: "Registrerer et morarentekrav.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--as-of", "--reference-rate", "--note"] },
  { key: "invoice post-interest", usage: "invoice post-interest --company <path> (--document-id <n> | --invoice-number <no>) [--claim-id <n>] [--date <YYYY-MM-DD>]", description: "Bogfører et registreret morarentekrav.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--claim-id", "--date"] },
  { key: "invoice compensation", usage: "invoice compensation --company <path> (--document-id <n> | --invoice-number <no>) --as-of <YYYY-MM-DD> [--amount-dkk <n>]", description: "Beregner kompensationskrav for sen betaling.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--as-of", "--amount-dkk"] },
  { key: "invoice claim-compensation", usage: "invoice claim-compensation --company <path> (--document-id <n> | --invoice-number <no>) --as-of <YYYY-MM-DD> [--amount-dkk <n>] [--note <text>]", description: "Registrerer kompensationskrav.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--as-of", "--amount-dkk", "--note"] },
  { key: "invoice post-compensation", usage: "invoice post-compensation --company <path> (--document-id <n> | --invoice-number <no>) [--date <YYYY-MM-DD>]", description: "Bogfører registreret kompensation.", allowedFlags: ["--company", "--document-id", "--invoice-number", "--date"] },
  { key: "documents ingest", usage: "documents ingest --company <path> --file <path> --metadata <file.json> [--vendor-id <n>] [--force]", description: "Indlæser og validerer et bilag med metadata.", allowedFlags: ["--company", "--file", "--metadata", "--vendor-id", "--force"], examplePath: "examples/vendor-invoice.metadata.json" },
  { key: "documents list", usage: "documents list --company <path>", description: "Lister gemte bilag.", allowedFlags: ["--company"] },
  { key: "bank import", usage: "bank import --company <path> --file <transactions.csv>", description: "Importerer banktransaktioner fra CSV.", allowedFlags: ["--company", "--file"], examplePath: "examples/bank-transactions.csv" },
  { key: "bank list", usage: "bank list --company <path> [--status all|matched|unmatched] [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>] [--text-match <text>] [--amount <n>]", description: "Lister importerede banktransaktioner med filtre for afstemningsstatus.", allowedFlags: ["--company", "--status", "--from", "--to", "--text-match", "--amount"] },
  { key: "bank suggest-matches", usage: "bank suggest-matches --company <path> [--bank-transaction-id <n>] [--max <n>]", description: "Foreslår deterministiske match mellem uafstemte banktransaktioner og fakturaer/bilag.", allowedFlags: ["--company", "--bank-transaction-id", "--max"] },
  { key: "reconcile bank", usage: "reconcile bank --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--status all|matched|unmatched] [--text-match <text>] [--amount <n>]", description: "Viser afstemte og uafstemte banktransaktioner med filtre.", allowedFlags: ["--company", "--from", "--to", "--status", "--text-match", "--amount"] },
  { key: "expense book", usage: "expense book --company <path> --document-id <n> --bank-transaction-id <n> --expense-account <konto> [--vat-treatment standard|reverse_charge|representation|exempt] [--payment-account <konto>] [--date <YYYY-MM-DD>] [--text <tekst>]", description: "Bogfører en leverandørudgift direkte fra bilag + bankpost.", allowedFlags: ["--company", "--document-id", "--bank-transaction-id", "--expense-account", "--vat-treatment", "--payment-account", "--date", "--text"] },
  { key: "vat report", usage: "vat report --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>", description: "Bygger en momsrapport for perioden.", allowedFlags: ["--company", "--from", "--to"] },
  { key: "vat post-eu-service-purchase", usage: "vat post-eu-service-purchase --company <path> --input <file.json>", description: "Bogfører et EU-servicekøb med reverse charge.", allowedFlags: ["--company", "--input"], examplePath: "examples/eu-service-purchase.json" },
  { key: "vat post-representation-purchase", usage: "vat post-representation-purchase --company <path> --input <file.json>", description: "Bogfører repræsentationsudgift med delvis momsfradrag.", allowedFlags: ["--company", "--input"], examplePath: "examples/representation-purchase.json" },
  { key: "period close", usage: "period close --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> [--kind vat_quarter|fiscal_year|custom] [--status closed|reported] [--reference <text>]", description: "Lukker eller markerer en regnskabsperiode.", allowedFlags: ["--company", "--from", "--to", "--kind", "--status", "--reference"] },
  { key: "retention status", usage: "retention status --company <path> [--as-of <YYYY-MM-DD>]", description: "Viser opbevaringsfrister og udløbet materiale.", allowedFlags: ["--company", "--as-of"] },
  { key: "journal post", usage: "journal post --company <path> --input <file.json>", description: "Bogfører en manuel finanspostering.", allowedFlags: ["--company", "--input"], examplePath: "examples/journal-entry.expense.json" },
  { key: "journal reverse", usage: "journal reverse --company <path> (--entry-id <n> | --entry-no <no> | --match-text <text> [--match-date <YYYY-MM-DD>] [--match-document-id <n>]) --date <YYYY-MM-DD> --reason <text>", description: "Tilbagefører en bogført finanspostering.", allowedFlags: ["--company", "--entry-id", "--entry-no", "--match-text", "--match-date", "--match-document-id", "--date", "--reason"] },
  { key: "journal list", usage: "journal list --company <path>", description: "Lister finansposteringer.", allowedFlags: ["--company"] },
  {
    key: "dashboard",
    usage: "dashboard --company <path> --out <file.html> [--as-of <YYYY-MM-DD>] [--open]",
    description: "Genererer et statisk HTML-dashboard over virksomhedens nuværende bogføringsstatus.",
    allowedFlags: ["--company", "--out", "--as-of", "--open"],
  },
];

const SPEC_MAP = new Map(COMMAND_SPECS.map((spec) => [spec.key, spec]));

export function getCommandKey(cmd?: string, sub?: string) {
  if (!cmd) return "";
  return sub ? `${cmd} ${sub}` : cmd;
}

export function getCommandSpec(cmd?: string, sub?: string) {
  return SPEC_MAP.get(getCommandKey(cmd, sub));
}

export function renderGlobalUsage() {
  const lines = ["Rentemester v0.0.1", "", "Commands:"];
  for (const spec of COMMAND_SPECS) lines.push(`  ${spec.usage}`);
  lines.push("", "Global flags:", "  --help", "  --example", "  --format json|human", "  --json");
  return lines.join("\n");
}

export function renderCommandHelp(spec: CommandSpec) {
  const lines = [spec.description, "", "Brug:", `  rentemester ${spec.usage}`];
  if (spec.inputNotes?.length) {
    lines.push("", "Inputnoter:");
    for (const note of spec.inputNotes) lines.push(`  - ${note}`);
  }
  if (spec.examplePath) {
    lines.push("", "Eksempel:", `  ${spec.exampleHint ?? `rentemester ${spec.key} --example`}`, `  # Kilde: ${spec.examplePath}`);
  }
  lines.push("", "Tilladte flags:");
  for (const flag of [...spec.allowedFlags, ...GLOBAL_FLAGS]) lines.push(`  ${flag}`);
  return lines.join("\n");
}

export function validateCommandFlags(cmd: string | undefined, sub: string | undefined, flags: Iterable<string>) {
  const spec = getCommandSpec(cmd, sub);
  if (!spec) return [] as string[];
  const allowed = new Set([...spec.allowedFlags, ...GLOBAL_FLAGS]);
  const errors: string[] = [];
  for (const flag of flags) {
    if (allowed.has(flag)) continue;
    const suggestion = suggestFlag(flag, [...allowed]);
    errors.push(`Unknown flag ${flag} for ${spec.key}.${suggestion ? ` Did you mean ${suggestion}?` : ""}`);
  }
  return errors;
}

function suggestFlag(input: string, candidates: string[]) {
  let best: { value: string; score: number } | null = null;
  for (const candidate of candidates) {
    const score = levenshtein(input, candidate);
    if (score > 3) continue;
    if (!best || score < best.score) best = { value: candidate, score };
  }
  return best?.value ?? null;
}

function levenshtein(a: string, b: string) {
  const dp = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = 0; i <= a.length; i += 1) dp[i]![0] = i;
  for (let j = 0; j <= b.length; j += 1) dp[0]![j] = j;
  for (let i = 1; i <= a.length; i += 1) {
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dp[i]![j] = Math.min(dp[i - 1]![j]! + 1, dp[i]![j - 1]! + 1, dp[i - 1]![j - 1]! + cost);
    }
  }
  return dp[a.length]![b.length]!;
}
