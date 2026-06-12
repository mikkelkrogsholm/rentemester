import type { CommandSpec } from "./_shared";

// ===== MAIL INTAKE (#122) =====
export const mailIntakeSpecs: CommandSpec[] = [
  {
    key: "mail-intake ingest",
    usage: "mail-intake ingest --company <path> --source <eml-file-or-maildrop-dir> [--metadata <file.json>] [--force]",
    description: "Indlæser bilag fra en lokal .eml-fil eller maildrop-mappe (første deterministiske intake-slice; ikke IMAP/hosted mailbox).",
    allowedFlags: ["--company", "--source", "--metadata", "--force"],
    examplePath: "examples/bilagsmail.metadata.json",
    exampleNote: "Eksemplet er KUN den valgfrie --metadata-payload, ikke et komplet kald: gem det til en fil og send den med --metadata sammen med --company og --source.",
  },
  // ===== IMAP INTAKE (#181) =====
  {
    key: "imap-intake poll",
    usage:
      "imap-intake poll --company <path> [--metadata <file.json>] [--imap-host <host>] [--imap-port <n>] [--imap-username <user>] [--imap-mailbox <name>] [--since-uid <n>] [--force]",
    description:
      "Poller en hosted IMAP-postkasse og videresender nye beskeder til den eksisterende bilagsmail-pipeline (#122). Dedup er rerun-stabil; gentaget poll skaber ingen dubletter.",
    allowedFlags: [
      "--company", "--metadata", "--imap-host", "--imap-port",
      "--imap-username", "--imap-mailbox", "--since-uid", "--force",
    ],
    examplePath: "examples/bilagsmail.metadata.json",
    exampleNote: "Eksemplet er KUN den valgfrie --metadata-payload, ikke et komplet kald: gem det til en fil og send den med --metadata sammen med --company og IMAP-flagsene.",
    inputNotes: [
      "IMAP-credentials læses fra --imap-* flags eller RENTEMESTER_IMAP_* miljøvariabler",
      "RENTEMESTER_IMAP_PASSWORD er kun miljøvariabel — aldrig et CLI-flag eller i ledger",
      "Standard: TLS (IMAPS) på port 993, mailbox INBOX",
      "Dedup deler mail_intake_messages-tabellen med 'mail-intake ingest' (#122)",
    ],
  },
  // ===== END IMAP INTAKE (#181) =====
];
