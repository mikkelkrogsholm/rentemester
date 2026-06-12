// #348/#350/#351 — Bilagsmail.

export type BilagsmailInboxRow = {
  id: number;
  documentNo: string | null;
  source: string;
  uploadDatetime: string | null;
  senderName: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  retainUntil: string | null;
};

export type CompanyBilagsmail = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  imapConfigured: boolean;
  imapStatus: {
    host: string;
    port: number;
    secure: boolean;
    username: string;
    mailbox: string;
  } | null;
  mailAlias: string | null;
  inbox: BilagsmailInboxRow[];
};

export type BilagsmailResponse = {
  ok: true;
  bilagsmail: CompanyBilagsmail;
};
