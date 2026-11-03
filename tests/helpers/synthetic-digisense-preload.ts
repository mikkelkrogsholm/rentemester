import { appendFileSync } from "node:fs";
import { syntheticDigisenseFetch } from "./synthetic-digisense-fetch";
globalThis.fetch = syntheticDigisenseFetch((route, body) => appendFileSync(process.env.SYNTHETIC_DIGISENSE_LOG!, `${JSON.stringify({ route, body })}\n`)) as typeof fetch;
