import { createContext, useContext } from "react";
import type { Transfer } from "./api/workspace-transfer";
export type Lifecycle = { enabled: boolean; transfer: Transfer | null; error: string; loading: boolean; writable: boolean; scope: string; refresh: () => Promise<void>; adopt: (value: Transfer) => void };
export const Context = createContext<Lifecycle | null>(null);
export const useWorkspaceTransfer = () => useContext(Context);
