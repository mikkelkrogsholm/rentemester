/**
 * #566 — deterministic human-readable rendering of an attachmentless EML
 * evidence document.
 *
 * The original EML bytes are the immutable evidence; a rendering makes them
 * usable to a human bookkeeper without touching a mail client. The rendering
 * is PURE: identical EML bytes yield byte-identical HTML, so it can always be
 * re-derived and compared — a missing or diverged sibling
 * `<stored_path>.rendered.html` is detectable.
 *
 * Security stance (#566): the rendering must never mistake the message for a
 * trusted document. Everything is HTML-escaped, including a text/html body,
 * which is shown as source text — "+no OCR-style invented source document".
 * Header continuation lines are unfolded per RFC 5322 § 2.2.3.
 */

// (no size cap needed: ingestDocumentSnapshot already refused over-large or
// mismatched content before this renderer is called)

interface EmlFields {
  from: string | null;
  to: string | null;
  replyTo: string | null;
  subject: string | null;
  date: string | null;
  messageId: string | null;
  contentType: string | null;
  contentTransferEncoding: string | null;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/'/g, "&#39;")
    .replace(/"/g, "&quot;");
}

/** Decodes RFC 2047 encoded-words so foreign subjects/senders stay readable. */
function decodeHeaderText(value: string | undefined): string | undefined {
  if (value === undefined) return undefined;
  return value.replace(/=\?([^?]+)\?([BbQq])\?([^?]*)\?=/g, (_m, charset: string, enc: string, data: string) => {
    try {
      if (enc.toUpperCase() === "B") {
        return Buffer.from(data, "base64").toString(/utf-?8/i.test(charset) ? "utf8" : "latin1");
      }
      const qp = data
        .replace(/_/g, " ")
        .replace(/=([0-9A-Fa-f]{2})/g, (_x, hex: string) => String.fromCharCode(parseInt(hex, 16)));
      return Buffer.from(qp, "binary").toString(/utf-?8/i.test(charset) ? "utf8" : "latin1");
    } catch {
      return data;
    }
  });
}

/** Splits headers from body and unfolds continuation lines. */
function splitHeadersAndBody(raw: Buffer): { headers: string[]; body: Buffer } {
  const separator = raw.indexOf("\r\n\r\n");
  const altSeparator = raw.indexOf("\n\n");
  let splitIndex = -1;
  let bodyOffset = 0;
  if (separator >= 0 && (altSeparator < 0 || separator <= altSeparator)) {
    splitIndex = separator;
    bodyOffset = separator + 4;
  } else if (altSeparator >= 0) {
    splitIndex = altSeparator;
    bodyOffset = altSeparator + 2;
  }
  if (splitIndex < 0) return { headers: raw.toString("utf8").split(/\r?\n/), body: Buffer.alloc(0) };
  const headerBlock = raw.subarray(0, splitIndex).toString("utf8");
  return { headers: headerBlockToLines(headerBlock), body: raw.subarray(bodyOffset) };
}

function headerBlockToLines(headerBlock: string): string[] {
  const rawLines = headerBlock.split(/\r?\n/);
  const unfolded: string[] = [];
  for (const line of rawLines) {
    if (/^[ \t]/.test(line) && unfolded.length > 0) {
      unfolded[unfolded.length - 1] += " " + line.trim();
    } else {
      unfolded.push(line);
    }
  }
  return unfolded;
}

function readFields(headers: string[]): EmlFields {
  const get = (name: string): string | null => {
    const prefix = `${name.toLowerCase()}:`;
    const line = headers.find((h) => h.toLowerCase().startsWith(prefix));
    if (!line) return null;
    const idx = line.indexOf(":");
    return decodeHeaderText(line.slice(idx + 1).trim()) ?? null;
  };
  const contentTypeLine = headers.find((line) => line.toLowerCase().startsWith("content-type:")) ?? null;
  const cteLine = headers.find((line) => line.toLowerCase().startsWith("content-transfer-encoding:")) ?? null;
  return {
    from: get("from"),
    to: get("to"),
    replyTo: get("reply-to"),
    subject: get("subject"),
    date: get("date"),
    messageId: get("message-id"),
    contentType: contentTypeLine ? contentTypeLine.slice("content-type:".length).trim() : null,
    contentTransferEncoding: cteLine ? cteLine.slice("content-transfer-encoding:".length).trim().toLowerCase() : null,
  };
}

/** Decodes the message body per its Content-Transfer-Encoding. */
function decodeBody(body: Buffer, encoding: string | null, contentType: string | null): string {
  const enc = (encoding ?? "7bit").trim().toLowerCase();
  let text: Buffer;
  if (enc === "base64") text = Buffer.from(body.toString("utf8").replace(/\s+/g, ""), "base64");
  else if (enc === "quoted-printable") text = Buffer.from(body.toString("binary").replace(/=\r?\n/g, "").replace(/=([0-9A-Fa-f]{2})/g, (_m, hex) => String.fromCharCode(parseInt(hex, 16))), "binary");
  else text = body;
  const charset = /charset\s*=\s*"?([^";\r\n]+)"?/i.exec(contentType ?? "")?.[1] ?? "utf-8";
  return /utf-?8/i.test(charset) ? text.toString("utf8") : text.toString("latin1");
}

/**
 * Renders an EML's evidence bytes as a deterministic HTML document. Total on
 * any input: unparseable parts render as "(mangler)" rather than throwing, so
 * an ingest can never fail on rendering alone.
 */
export function renderEmlEvidence(raw: Buffer, sha256: string): string {
  const { headers, body } = splitHeadersAndBody(raw);
  const fields = readFields(headers);
  const bodyText = decodeBody(body, fields.contentTransferEncoding, fields.contentType);
  const esc = escapeHtml;

  const row = (label: string, value: string | null): string =>
    `<tr><th scope="row">${esc(label)}</th><td>${value === null ? "<em>(mangler)</em>" : esc(value)}</td></tr>`;

  const headerRows = [
    row("Fra", fields.from),
    row("Til", fields.to),
    row("Reply-To", fields.replyTo),
    row("Emne", fields.subject),
    row("Dato", fields.date),
    row("Message-ID", fields.messageId),
  ].join("");

  const bodyKind = /text\/html/i.test(fields.contentType ?? "")
    ? "text/html-krop vises som kilde-tekst, ikke gengivet som webindhold"
    : "decoded med Content-Transfer-Encoding";

  return [
    "<!DOCTYPE html>",
    `<html lang="da"><head><meta charset="utf-8"><title>Renderet EML-evidens ${esc(sha256)}</title></head>`,
    "<body>",
    `<h1>Renderet EML-evidens</h1>`,
    `<p>Dette er en <strong>deterministisk rendering</strong> af en immutabel EML-evidens. ` +
      `Den originale fil (sha256 <code>${esc(sha256)}</code>) er autoritativ; renderingen kan altid gen-males ud fra den.`,
    `<table>${headerRows}</table>`,
    `<h2>Krop (${esc(fields.contentType ?? "ukendt Content-Type")}; ${esc(bodyKind)})</h2>`,
    `<pre>${esc(bodyText)}</pre>`,
    "",
    "</body></html>",
    "",
  ].join("\n");
}
