/**
 * Bilagsmail intake (#122) — the first deterministic e-mail receipt slice.
 *
 * Scope: accept a local maildrop directory OR a single `.eml` file, parse
 * attachments (PDF/JPG/PNG) plus sender/subject/date/message-id metadata,
 * feed each attachment into the existing document-ingest pipeline, and
 * deduplicate by stable message-id + attachment hash so reruns are
 * idempotent. Messages with no usable attachment or ambiguous metadata
 * are routed into the exception queue instead of guessing.
 *
 * IMAP / hosted-mailbox / provider sync is explicitly OUT OF SCOPE — see
 * ROADMAP. EML is parsed with Bun/Node stdlib only (no new npm dependency)
 * via the minimal MIME parser below.
 *
 * Determinism: the same EML/maildrop input produces identical ingest
 * output across reruns — files are processed in sorted order, attachments
 * in sorted (filename, hash) order, and dedup is keyed on content hashes.
 */

import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { ingestDocument, type DocumentMetadata, type IngestDocumentOptions } from "./documents";
import { recordException } from "./exceptions";

export const MAIL_INTAKE_RULES = {
  TRANSPORT: "DK-MAIL-INTAKE-TRANSPORT-001",
  DEDUP: "DK-MAIL-INTAKE-DEDUP-001",
  EXCEPTION: "DK-MAIL-INTAKE-EXCEPTION-001",
} as const;

/** Attachment MIME types this slice forwards to the document pipeline. */
const SUPPORTED_ATTACHMENT_EXTENSIONS = new Set([".pdf", ".jpg", ".jpeg", ".png"]);

export type MailAttachment = {
  filename: string;
  mimeType: string;
  content: Buffer;
  /** sha256 of the decoded attachment bytes — stable across reparses. */
  sha256: string;
};

export type ParsedEml = {
  messageId: string | null;
  from: string | null;
  /** #352 — Reply-To bevarer ofte den oprindelige afsender på en forwardet mail. */
  replyTo: string | null;
  /**
   * #352 — X-Forwarded-For (eller den klassiske "Resent-From") afslører at
   * mailen er forwardet; cockpittet kan vise det som "videresendt fra <x>".
   */
  forwardedFrom: string | null;
  subject: string | null;
  date: string | null;
  /**
   * #352 — Den samlede rå-størrelse i bytes inden parsing. Bruges af
   * MAX_EML_SIZE_BYTES-checket så vi kan rejse en eksception før vi parser
   * et helt 200 MB attached-PDF-monster.
   */
  rawByteSize: number;
  attachments: MailAttachment[];
};

/**
 * #352 — Default maks-størrelse for en enkelt mail (rå bytes). Mails der
 * overskrider grænsen får en MAIL_INTAKE_TOO_LARGE-eksception i stedet for
 * at blive forsøgt parset. Kan overstyres via IngestMailDropOptions.
 */
export const DEFAULT_MAX_EML_SIZE_BYTES = 25 * 1024 * 1024;

export type MailIntakeMetadataInput = Omit<DocumentMetadata, "source">;

export type IngestMailDropOptions = {
  /** Metadata applied to every ingested attachment. */
  metadata?: MailIntakeMetadataInput;
  /** Per-message metadata keyed on message-id; overrides `metadata`. */
  metadataPerMessage?: Record<string, MailIntakeMetadataInput>;
  /** Forwarded to ingestDocument for forced logical-duplicate scans. */
  ingestOptions?: IngestDocumentOptions;
  /**
   * #352 — Maks-størrelse pr. eml-fil i bytes. Default
   * `DEFAULT_MAX_EML_SIZE_BYTES`. Mails over grænsen får en
   * MAIL_INTAKE_TOO_LARGE-eksception i stedet for at blive parset.
   */
  maxEmlSizeBytes?: number;
};

export type IngestMailDropResult = {
  ok: boolean;
  messagesProcessed: number;
  attachmentsIngested: number;
  attachmentsSkipped: number;
  exceptionsCreated: number;
  documents: Array<{ messageId: string; documentNo: string; sha256: string }>;
  errors: string[];
};

// ----------------------------------------------------------------------
// Minimal MIME / multipart parser (stdlib only).
// ----------------------------------------------------------------------

/**
 * Splits a raw RFC-822 message into its header block and body. Headers
 * are unfolded (continuation lines beginning with whitespace are joined
 * onto the previous header) and returned with lowercased keys.
 *
 * The header block is decoded as UTF-8 so non-ASCII sender names/subjects
 * survive; the body is kept as a `binary` (latin1) string so attachment
 * bytes round-trip losslessly through the multipart split.
 */
function splitHeadersAndBody(raw: string): { headers: Map<string, string>; body: string } {
  const separator = raw.indexOf("\r\n\r\n");
  const altSeparator = raw.indexOf("\n\n");
  let headerBlock: string;
  let body: string;
  if (separator >= 0 && (altSeparator < 0 || separator <= altSeparator)) {
    headerBlock = raw.slice(0, separator);
    body = raw.slice(separator + 4);
  } else if (altSeparator >= 0) {
    headerBlock = raw.slice(0, altSeparator);
    body = raw.slice(altSeparator + 2);
  } else {
    headerBlock = raw;
    body = "";
  }

  // The raw text is held as a `binary` string; re-decode the header block
  // as UTF-8 so multi-byte characters in From/Subject are not mangled.
  headerBlock = Buffer.from(headerBlock, "binary").toString("utf8");

  const headers = new Map<string, string>();
  const rawLines = headerBlock.split(/\r?\n/);
  const unfolded: string[] = [];
  for (const line of rawLines) {
    if (/^[ \t]/.test(line) && unfolded.length > 0) {
      unfolded[unfolded.length - 1] += " " + line.trim();
    } else {
      unfolded.push(line);
    }
  }
  for (const line of unfolded) {
    const idx = line.indexOf(":");
    if (idx < 0) continue;
    const key = line.slice(0, idx).trim().toLowerCase();
    const value = line.slice(idx + 1).trim();
    if (!headers.has(key)) headers.set(key, value);
  }
  return { headers, body };
}

/** Extracts a named parameter (e.g. `boundary`, `name`) from a header value. */
function headerParam(headerValue: string | undefined | null, param: string): string | null {
  if (!headerValue) return null;
  const quoted = new RegExp(`${param}\\s*=\\s*"([^"]*)"`, "i").exec(headerValue);
  if (quoted) return quoted[1] ?? null;
  const unquoted = new RegExp(`${param}\\s*=\\s*([^;\\s]+)`, "i").exec(headerValue);
  return unquoted ? (unquoted[1] ?? null) : null;
}

/** The bare MIME type (before the first `;`), lowercased. */
function baseMimeType(headerValue: string | undefined): string {
  if (!headerValue) return "text/plain";
  const semi = headerValue.indexOf(";");
  return (semi >= 0 ? headerValue.slice(0, semi) : headerValue).trim().toLowerCase();
}

/** Decodes a MIME body part according to its Content-Transfer-Encoding. */
function decodePart(body: string, encoding: string | undefined): Buffer {
  const enc = (encoding ?? "7bit").trim().toLowerCase();
  if (enc === "base64") {
    return Buffer.from(body.replace(/[\r\n\s]/g, ""), "base64");
  }
  if (enc === "quoted-printable") {
    const text = body
      .replace(/=\r?\n/g, "")
      .replace(/=([0-9A-Fa-f]{2})/g, (_m, hex) => String.fromCharCode(parseInt(hex, 16)));
    return Buffer.from(text, "binary");
  }
  return Buffer.from(body, "utf8");
}

/**
 * Splits a multipart body into its constituent parts using `boundary`.
 * Trailing boundary markers and the epilogue are discarded.
 */
function splitMultipart(body: string, boundary: string): string[] {
  const delimiter = `--${boundary}`;
  const segments = body.split(delimiter);
  const parts: string[] = [];
  for (let i = 1; i < segments.length; i += 1) {
    const segment = segments[i]!;
    if (segment.startsWith("--")) break; // closing delimiter — epilogue follows
    parts.push(segment.replace(/^\r?\n/, ""));
  }
  return parts;
}

function looksLikeAttachment(filename: string | null, contentType: string, disposition: string | null): boolean {
  if (disposition && /attachment/i.test(disposition)) return true;
  if (filename && SUPPORTED_ATTACHMENT_EXTENSIONS.has(extname(filename).toLowerCase())) return true;
  return contentType === "application/pdf" || contentType === "image/png" || contentType === "image/jpeg";
}

/**
 * Recursively collects attachment parts from a MIME part. `text/*` parts
 * without a filename are treated as message body, not attachments.
 */
function collectAttachments(partRaw: string): MailAttachment[] {
  const { headers, body } = splitHeadersAndBody(partRaw);
  const contentTypeRaw = headers.get("content-type");
  const contentType = baseMimeType(contentTypeRaw);

  if (contentType.startsWith("multipart/")) {
    const boundary = headerParam(contentTypeRaw, "boundary");
    if (!boundary) return [];
    return splitMultipart(body, boundary).flatMap(collectAttachments);
  }

  const disposition = headers.get("content-disposition") ?? null;
  const filename =
    headerParam(disposition, "filename") ?? headerParam(contentTypeRaw, "name") ?? null;

  if (!looksLikeAttachment(filename, contentType, disposition)) return [];

  const content = decodePart(body, headers.get("content-transfer-encoding"));
  const resolvedName = filename ?? `attachment${extensionForMime(contentType)}`;
  return [
    {
      filename: resolvedName,
      mimeType: contentType,
      content,
      sha256: createHash("sha256").update(content).digest("hex"),
    },
  ];
}

function extensionForMime(mimeType: string): string {
  if (mimeType === "application/pdf") return ".pdf";
  if (mimeType === "image/png") return ".png";
  if (mimeType === "image/jpeg") return ".jpg";
  return ".bin";
}

/**
 * Parses a raw `.eml` message buffer into its metadata and attachments.
 * Pure and deterministic: identical input bytes yield identical output,
 * including attachment hashes.
 */
export function parseEml(raw: Buffer | string): ParsedEml {
  const text = typeof raw === "string" ? raw : raw.toString("binary");
  const rawByteSize =
    typeof raw === "string" ? Buffer.byteLength(raw, "binary") : raw.length;
  const { headers } = splitHeadersAndBody(text);
  const attachments = collectAttachments(text).sort((a, b) => {
    if (a.filename !== b.filename) return a.filename < b.filename ? -1 : 1;
    return a.sha256 < b.sha256 ? -1 : a.sha256 > b.sha256 ? 1 : 0;
  });
  return {
    messageId: headers.get("message-id") ?? null,
    from: decodeHeaderText(headers.get("from")) ?? null,
    // #352 — bevar oprindelig afsender på forwardede mails. Reply-To
    // bruges af de fleste mail-klienter når man trykker "Videresend";
    // X-Forwarded-For / Resent-From fortæller specifikt at det ER en
    // videresendt mail.
    replyTo: decodeHeaderText(headers.get("reply-to")) ?? null,
    forwardedFrom:
      decodeHeaderText(headers.get("x-forwarded-for")) ??
      decodeHeaderText(headers.get("resent-from")) ??
      null,
    subject: decodeHeaderText(headers.get("subject")) ?? null,
    date: headers.get("date") ?? null,
    rawByteSize,
    attachments,
  };
}

/**
 * Decodes RFC-2047 encoded-word headers (`=?utf-8?B?...?=` / `?Q?`) so
 * non-ASCII sender names and subjects survive intake. Plain headers pass
 * through unchanged.
 */
function decodeHeaderText(value: string | undefined): string | undefined {
  if (value === undefined) return undefined;
  return value.replace(/=\?([^?]+)\?([BbQq])\?([^?]*)\?=/g, (_m, charset, enc, data) => {
    try {
      if (enc.toUpperCase() === "B") {
        return Buffer.from(data, "base64").toString(charset.toLowerCase() === "utf-8" ? "utf8" : "latin1");
      }
      const qp = data
        .replace(/_/g, " ")
        .replace(/=([0-9A-Fa-f]{2})/g, (_x: string, hex: string) => String.fromCharCode(parseInt(hex, 16)));
      return Buffer.from(qp, "binary").toString(charset.toLowerCase() === "utf-8" ? "utf8" : "latin1");
    } catch {
      return data;
    }
  });
}

// ----------------------------------------------------------------------
// Intake pipeline.
// ----------------------------------------------------------------------

/** Resolves the ordered list of `.eml` files for a file-or-directory input. */
function resolveEmlFiles(inputPath: string): string[] {
  const stat = statSync(inputPath);
  if (stat.isFile()) return [inputPath];
  if (stat.isDirectory()) {
    return readdirSync(inputPath)
      .filter((name) => name.toLowerCase().endsWith(".eml"))
      .sort()
      .map((name) => join(inputPath, name));
  }
  throw new Error(`mail intake source is neither a file nor a directory: ${inputPath}`);
}

/**
 * Ingests a local maildrop directory or a single `.eml` file.
 *
 * For each parsed message:
 *  - missing message-id → MAIL_INTAKE_AMBIGUOUS_METADATA exception
 *  - no usable attachment → MAIL_INTAKE_NO_ATTACHMENT exception
 *  - each attachment is deduplicated on (message-id, attachment sha256);
 *    already-seen attachments are skipped so reruns are idempotent.
 *
 * The pipeline never guesses: anything that cannot be ingested cleanly is
 * surfaced in the exception queue. Returns aggregate counters and the list
 * of created documents.
 */
export function ingestMailDrop(
  db: Database,
  companyRoot: string,
  inputPath: string,
  options: IngestMailDropOptions = {},
): IngestMailDropResult {
  const result: IngestMailDropResult = {
    ok: true,
    messagesProcessed: 0,
    attachmentsIngested: 0,
    attachmentsSkipped: 0,
    exceptionsCreated: 0,
    documents: [],
    errors: [],
  };

  if (!existsSync(inputPath)) {
    return { ...result, ok: false, errors: [`mail intake source does not exist: ${inputPath}`] };
  }

  let emlFiles: string[];
  try {
    emlFiles = resolveEmlFiles(inputPath);
  } catch (error) {
    return { ...result, ok: false, errors: [error instanceof Error ? error.message : String(error)] };
  }

  const maxSize = options.maxEmlSizeBytes ?? DEFAULT_MAX_EML_SIZE_BYTES;

  for (const emlFile of emlFiles) {
    result.messagesProcessed += 1;
    const raw = readFileSync(emlFile);
    // #352 — refuse over-large mails BEFORE we parse them. A 200 MB
    // ".eml" is almost always a bug (giant signed-image attachment, or a
    // dump-of-everything) and parsing it can OOM the daemon.
    if (raw.length > maxSize) {
      const ex = recordException(db, {
        type: "MAIL_INTAKE_TOO_LARGE",
        severity: "medium",
        message: `Mail message in ${emlFile} is ${raw.length} bytes (> ${maxSize}); refused to parse`,
        requiredAction:
          "Forward only the receipt itself or raise the maxEmlSizeBytes limit.",
        sourceEvidence: {
          rule: MAIL_INTAKE_RULES.EXCEPTION,
          file: emlFile,
          rawByteSize: raw.length,
          maxEmlSizeBytes: maxSize,
        },
        postingPreview: { nextStep: "documents ingest" },
      });
      if (ex.ok && !ex.duplicate) result.exceptionsCreated += 1;
      continue;
    }
    const parsed = parseEml(raw);

    // Ambiguous metadata: without a stable message-id we cannot dedup.
    if (!parsed.messageId || parsed.messageId.trim().length === 0) {
      const ex = recordException(db, {
        type: "MAIL_INTAKE_AMBIGUOUS_METADATA",
        severity: "medium",
        message: `Mail message in ${emlFile} has no Message-ID and cannot be ingested deterministically`,
        requiredAction: "Add a Message-ID header to the .eml or ingest the attachment manually via 'documents ingest'.",
        sourceEvidence: {
          rule: MAIL_INTAKE_RULES.EXCEPTION,
          file: emlFile,
          from: parsed.from,
          subject: parsed.subject,
          date: parsed.date,
          attachmentCount: parsed.attachments.length,
        },
        postingPreview: { nextStep: "documents ingest" },
      });
      if (ex.ok && !ex.duplicate) result.exceptionsCreated += 1;
      continue;
    }

    const messageId = parsed.messageId.trim();
    const usable = parsed.attachments.filter((att) =>
      SUPPORTED_ATTACHMENT_EXTENSIONS.has(extname(att.filename).toLowerCase()),
    );

    if (usable.length === 0) {
      const ex = recordException(db, {
        type: "MAIL_INTAKE_NO_ATTACHMENT",
        severity: "medium",
        message: `Mail message ${messageId} has no usable PDF/JPG/PNG attachment`,
        requiredAction: "Forward the message with the receipt attached, or ingest the document manually.",
        sourceEvidence: {
          rule: MAIL_INTAKE_RULES.EXCEPTION,
          file: emlFile,
          messageId,
          from: parsed.from,
          subject: parsed.subject,
          date: parsed.date,
        },
        postingPreview: { nextStep: "documents ingest" },
      });
      if (ex.ok && !ex.duplicate) result.exceptionsCreated += 1;
      continue;
    }

    const metadata = options.metadataPerMessage?.[messageId] ?? options.metadata;

    for (const attachment of usable) {
      const alreadySeen = db
        .query("SELECT id FROM mail_intake_messages WHERE message_id = ? AND attachment_sha256 = ? LIMIT 1")
        .get(messageId, attachment.sha256) as { id: number } | null;
      if (alreadySeen) {
        result.attachmentsSkipped += 1;
        continue;
      }

      if (!metadata) {
        const ex = recordException(db, {
          type: "MAIL_INTAKE_AMBIGUOUS_METADATA",
          severity: "medium",
          message: `Mail message ${messageId} attachment ${attachment.filename} has no metadata for ingest`,
          requiredAction: "Provide --metadata (or per-message metadata) so the attachment can be booked.",
          sourceEvidence: {
            rule: MAIL_INTAKE_RULES.EXCEPTION,
            file: emlFile,
            messageId,
            attachmentFilename: attachment.filename,
            attachmentSha256: attachment.sha256,
          },
          postingPreview: { nextStep: "documents ingest" },
        });
        if (ex.ok && !ex.duplicate) result.exceptionsCreated += 1;
        continue;
      }

      const ingestResult = ingestAttachment(db, companyRoot, messageId, parsed, attachment, metadata, options.ingestOptions);
      if (ingestResult.ingested) {
        result.attachmentsIngested += 1;
        result.documents.push({
          messageId,
          documentNo: ingestResult.documentNo!,
          sha256: ingestResult.sha256!,
        });
      } else if (ingestResult.exceptionCreated) {
        result.exceptionsCreated += 1;
      }
    }
  }

  return result;
}

/**
 * Writes a single attachment to a temp file, forwards it to the existing
 * document-ingest pipeline, records the dedup row, and — on a blocked
 * ingest — routes the failure into the exception queue.
 */
function ingestAttachment(
  db: Database,
  companyRoot: string,
  messageId: string,
  parsed: ParsedEml,
  attachment: MailAttachment,
  metadataInput: MailIntakeMetadataInput,
  ingestOptions?: IngestDocumentOptions,
): { ingested: boolean; exceptionCreated: boolean; documentNo?: string; sha256?: string } {
  const scratchDir = mkdtempSync(join(tmpdir(), "rentemester-mail-attachment-"));
  const ext = extname(attachment.filename).toLowerCase() || extensionForMime(attachment.mimeType);
  const scratchFile = join(scratchDir, `attachment${ext}`);
  writeFileSync(scratchFile, attachment.content);

  try {
    const metadata: DocumentMetadata = {
      ...metadataInput,
      source: `mail-intake:${messageId}`,
    };
    const ingest = ingestDocument(db, companyRoot, scratchFile, metadata, ingestOptions ?? {});
    if (!ingest.ok) {
      const ex = recordException(db, {
        type: "MAIL_INTAKE_INGEST_BLOCKED",
        severity: "medium",
        message: `Mail intake ingest blocked for ${attachment.filename} from ${messageId}`,
        requiredAction: "Fix the attachment metadata or duplicate handling, then re-run mail-intake.",
        sourceEvidence: {
          rule: MAIL_INTAKE_RULES.EXCEPTION,
          messageId,
          from: parsed.from,
          subject: parsed.subject,
          attachmentFilename: attachment.filename,
          attachmentSha256: attachment.sha256,
          errors: ingest.errors ?? [],
        },
        postingPreview: { retryCommand: "mail-intake ingest --company <path> --source <eml> --metadata <file.json>" },
      });
      return { ingested: false, exceptionCreated: ex.ok && !ex.duplicate };
    }

    // Record the dedup row only after a successful ingest so a blocked
    // attachment can be retried after the metadata is fixed.
    db.query(
      `INSERT INTO mail_intake_messages (
        message_id, attachment_sha256, attachment_filename, document_id,
        sender, subject, mail_date
      ) VALUES (?, ?, ?, ?, ?, ?, ?)`,
    ).run(
      messageId,
      attachment.sha256,
      attachment.filename,
      ingest.documentId ?? null,
      parsed.from,
      parsed.subject,
      parsed.date,
    );

    return { ingested: true, exceptionCreated: false, documentNo: ingest.documentNo, sha256: ingest.sha256 };
  } finally {
    rmSync(scratchDir, { recursive: true, force: true });
  }
}
