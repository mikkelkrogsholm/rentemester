import { useState } from 'react';
import { ConfirmDialog } from '../components/ConfirmDialog';
import { useUnsavedChanges } from './useUnsavedChanges';

/** Protects a form's own dismiss button as well as route navigation and unload. */
export function useDiscardGuard(dirty: boolean, onDismiss: () => void) {
  const [checking, setChecking] = useState(false);
  const markSaved = useUnsavedChanges(dirty);
  const dismiss = () => { markSaved(); setChecking(false); onDismiss(); };
  const onClose = () => { if (dirty) setChecking(true); else dismiss(); };
  const confirmation = checking ? <ConfirmDialog title="Kassér ændringer?" body="Du har ændringer, som ikke er gemt. Hvis du lukker formularen, bliver de kasseret." confirmLabel="Kassér ændringer" confirmKind="danger" onClose={() => setChecking(false)} onConfirm={async () => dismiss()} /> : null;
  return { onClose, dismiss, confirmation };
}
