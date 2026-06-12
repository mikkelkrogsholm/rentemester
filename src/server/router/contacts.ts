// Contacts (customers + vendors) read handler.

import type { ServerConfig } from "../config";
import { buildCompanyContacts } from "../data";
import { okResponse } from "./_shared";

export function handleCompanyContacts(config: ServerConfig, slug: string): Response {
  const data = buildCompanyContacts(config.workspaceRoot, slug);
  return okResponse({ contacts: data });
}
