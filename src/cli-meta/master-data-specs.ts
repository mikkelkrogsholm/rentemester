import type { CommandSpec } from "./_shared";

export const masterDataSpecs: CommandSpec[] = [
  {
    key: "customer create",
    usage:
      "customer create --company <path> --name <text> [--address <text>] [--cvr <DK...>] [--email <text>] [--ean <text>] [--payment-terms <days>] [--currency <ISO>] [--notes <text>] [--from-cvr <DK...>]",
    description: "Opretter en append-only kundepost til genbrug på fakturaer. Med --from-cvr udfyldes navn/adresse/CVR/email automatisk fra CVR-registret.",
    allowedFlags: ["--company", "--name", "--address", "--cvr", "--email", "--ean", "--payment-terms", "--currency", "--notes", "--from-cvr"],
    inputNotes: [
      "--from-cvr slår CVR-nummeret op i CVR-registret og udfylder felter brugeren ikke selv har sat",
      "Eksplicitte flag (--name, --address, ...) vinder altid over CVR-data",
      "--from-cvr kræver CVR_USERNAME/CVR_PASSWORD som miljøvariabler",
    ],
  },
  { key: "customer list", usage: "customer list --company <path> [--archived]", description: "Lister kendte kunder.", allowedFlags: ["--company", "--archived"] },
  { key: "customer validate-vat", usage: "customer validate-vat --company <path> --cvr <EU-VAT>", description: "Validerer et EU-VAT-nummer via VIES og cacher resultatet.", allowedFlags: ["--company", "--cvr"] },
  {
    key: "customer cvr-lookup",
    usage: "customer cvr-lookup --company <path> --cvr <DK12345678>",
    description: "Slår en virksomhed op i CVR-registret og viser stamdata (navn, adresse, branche, form, status, ledelse). Read-only; cacher snapshottet. Kræver CVR_USERNAME/CVR_PASSWORD.",
    allowedFlags: ["--company", "--cvr"],
  },
  { key: "vendor create", usage: "vendor create --company <path> --name <text> [--address <text>] [--cvr <DK...>] [--expense-account <konto>] [--default-vat <text>] [--notes <text>] [--from-cvr <DK...>]", description: "Opretter en append-only leverandørpost til bilagsindlæsning. Med --from-cvr udfyldes navn/adresse/CVR automatisk fra CVR-registret.", allowedFlags: ["--company", "--name", "--address", "--cvr", "--expense-account", "--default-vat", "--notes", "--from-cvr"] },
  { key: "vendor list", usage: "vendor list --company <path> [--archived]", description: "Lister kendte leverandører.", allowedFlags: ["--company", "--archived"] },
  { key: "exceptions list", usage: "exceptions list --company <path> [--status open|resolved|all] [--include-archived]", description: "Lister exceptions-køen. Exceptions i arkiverede/lukkede perioder udelades som standard — vis dem med --include-archived.", allowedFlags: ["--company", "--status", "--include-archived"] },
  { key: "exceptions resolve", usage: "exceptions resolve --company <path> --id <n> [--note <text>]", description: "Markerer en exception som løst.", allowedFlags: ["--company", "--id", "--note"] },
];
