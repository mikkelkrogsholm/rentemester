import { request, type ReadRequestOptions } from './_shared';
import type { listKnowledgePages } from '../../../../src/core/knowledge-pages';

export type KnowledgePage = Pick<ReturnType<typeof listKnowledgePages>[number], 'pageId' | 'version' | 'scope' | 'title' | 'provenance' | 'effectiveFrom' | 'effectiveToExclusive'> & { bodyMarkdown: string };
export const knowledgePagesApi = {
  knowledgePage(pageId: string, companySlug?: string, options?: ReadRequestOptions) {
    const base = companySlug ? `/api/companies/${encodeURIComponent(companySlug)}` : '/api';
    return request<{ page: KnowledgePage }>(`${base}/knowledge-pages/${encodeURIComponent(pageId)}`, options);
  },
};
