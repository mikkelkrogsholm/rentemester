import type { CommandSpec } from "./_shared";

// ===== MILEAGE LOG (#123) =====
export const mileageSpecs: CommandSpec[] = [
  {
    key: "mileage log",
    usage:
      "mileage log --company <path> --date <YYYY-MM-DD> --purpose <text> --from <text> --to <text> --km <n> --vehicle <text> --driver <text> --rate-per-km <n> --rate-basis <text> [--rate-source <text>] [--notes <text>]",
    description:
      "Registrerer en append-only kørselspost i kørselsregnskabet. Satsen er bruger-oplyst og kilde-bakket; intet bogføres i finansen.",
    allowedFlags: [
      "--company", "--date", "--purpose", "--from", "--to", "--km", "--vehicle",
      "--driver", "--rate-per-km", "--rate-basis", "--rate-source", "--notes",
    ],
    inputNotes: [
      "rate-per-km og rate-basis skal være bruger-oplyst / kilde-bakket",
      "Rentemester fører kun loggen — skattemæssig behandling er brugerens/rådgiverens ansvar",
    ],
  },
  { key: "mileage list", usage: "mileage list --company <path>", description: "Lister registrerede kørselsposter.", allowedFlags: ["--company"] },
  { key: "mileage report", usage: "mileage report --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD>", description: "Deterministisk periode-rapport over kilometer og beløbsgrundlag.", allowedFlags: ["--company", "--from", "--to"] },
  { key: "mileage export", usage: "mileage export --company <path> --from <YYYY-MM-DD> --to <YYYY-MM-DD> --out <dir>", description: "Skriver et deterministisk eksport-artifact (JSON + CSV) over kørselsregnskabet.", allowedFlags: ["--company", "--from", "--to", "--out"] },
];
