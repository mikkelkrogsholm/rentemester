import type { CommandSpec } from "./_shared";

// ===== GDPR (#184) =====
export const gdprSpecs: CommandSpec[] = [
  {
    key: "gdpr audit-log",
    usage: "gdpr audit-log --company <path> [--since <YYYY-MM-DD>] [--until <YYYY-MM-DD>] [--as-of <YYYY-MM-DD>] [--out <file>] [--sign-with-ed25519]",
    description: "Eksporterer alle GDPR-handlinger (export, discover, erasure) som signed JSON-pakke. Genbruger backup-systemets Ed25519-nøgle så pakken kan verificeres uden Rentemester installeret.",
    allowedFlags: ["--company", "--since", "--until", "--as-of", "--out", "--sign-with-ed25519"],
    inputNotes: [
      "Filtrerer audit_log-tabellen til event_type LIKE 'gdpr_%' (export, discover, erasure)",
      "fingerprint er sha256 af det deterministiske JSON-output uden signature-feltet selv",
      "--sign-with-ed25519 kræver at backup-nøgleparret er genereret én gang via 'system backup --sign-with-ed25519'",
    ],
  },
  {
    key: "gdpr discover",
    usage: "gdpr discover --company <path> (--cvr <DK...> | --subject <id> | --name <text>) [--as-of <YYYY-MM-DD>]",
    description: "Subject-discovery: lister hvor en data-subject optræder pr. tabel (kunder, leverandører, bilag, banktransaktioner). Hurtigere end 'gdpr export' fordi den ikke beriger med retention-status. Read-only.",
    allowedFlags: ["--company", "--cvr", "--subject", "--name", "--as-of"],
    inputNotes: [
      "Den registrerede identificeres med --cvr/--subject og/eller --name",
      "Output er deterministisk og audit-loggable: byTable-tæller + rækker pr. tabel",
      "--json/--format json-outputtets feltliste er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå skemaet op dér før maskinel parsing.",
    ],
  },
  {
    key: "gdpr export",
    usage: "gdpr export --company <path> (--cvr <DK...> | --subject <id> | --name <text>) [--as-of <YYYY-MM-DD>] [--out <dir>]",
    description: "Samler alle persondata Rentemester har om en kunde/leverandør i én indsigtsrapport med opbevaringsvurdering. Read-only.",
    allowedFlags: ["--company", "--cvr", "--subject", "--name", "--as-of", "--out"],
    inputNotes: [
      "Den registrerede identificeres med --cvr/--subject og/eller --name",
      "Hver post markeres med opbevaringsfrist og om den stadig er under bogføringspligt",
      "Med --out <dir> skrives indsigtsrapporten som én JSON-fil 'gdpr-export-<subject>-<as-of>.json' i mappen",
      "--json/--format json-outputtets feltliste (indsigtsrapportens form) er dokumenteret i docs/cli-contract.md afsnit 3 og docs/mcp-tool-surface.md — slå skemaet op dér før maskinel parsing.",
    ],
  },
  {
    key: "gdpr erase",
    usage: "gdpr erase --company <path> (--cvr <DK...> | --name <text>) [--as-of <YYYY-MM-DD>]",
    description: "Legacy navn for gdpr forget — sletter/redigerer persondata der ikke længere er under bogføringsmæssig opbevaringspligt. Append-only ledger og audit-kæde røres aldrig.",
    allowedFlags: ["--company", "--cvr", "--subject", "--name", "--as-of"],
    inputNotes: [
      "Foretræk 'gdpr forget --after-retention-expiry' — samme handling med eksplicit accept af kontrakten",
      "Poster under opbevaringsfrist afvises — bogføringsloven går forud for sletteret",
      "Sletning skrives som append-only tombstone; finansposteringer ændres ikke",
    ],
  },
  {
    key: "gdpr forget",
    usage: "gdpr forget --company <path> (--cvr <DK...> | --subject <id> | --name <text>) [--as-of <YYYY-MM-DD>] --after-retention-expiry",
    description: "Sletter/redigerer persondata der ikke længere er under bogføringsmæssig opbevaringspligt; afviser klart data der stadig skal opbevares. Kræver --after-retention-expiry som operatørbekræftelse af kontrakten.",
    allowedFlags: ["--company", "--cvr", "--subject", "--name", "--as-of", "--after-retention-expiry"],
    inputNotes: [
      "Den registrerede identificeres med --cvr/--subject og/eller --name",
      "--after-retention-expiry er PÅKRÆVET: bekræfter at poster stadig under retention IKKE redigeres (håndhæves pr. post af kernen)",
      "Poster under opbevaringsfrist afvises — bogføringsloven går forud for sletteret",
      "Sletning skrives som append-only tombstone; finansposteringer ændres ikke",
    ],
  },
  // ===== END GDPR (#184) =====
];
