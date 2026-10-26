//! Read-only startup schema admission and probes over admitted connections.
use super::*;
use rentemester_storage::readiness::{Code, Readiness};

impl Owner {
    /// Namespace/security failures remain ordinary constructor errors. Only a
    /// completed protected read-only storage probe yields StorageUnusable.
    pub fn open_ready_with_profile(
        root: &Path, broker_uid: u32, workspace: &str, slug: &str,
        profile: NamespaceProfile,
    ) -> Result<Self, OwnerError> {
        let guard = NamespaceGuard::acquire_with_profile(root, broker_uid, workspace, slug, profile)
            .map_err(OwnerError::Namespace)?;
        let mut held = Held(Some(Slots { control: None, company: None, pending: None, guard }));
        let slots = held.0.as_mut().unwrap();
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        // No CREATE, recovery, writable connection, or provider action on failure.
        slots.control = Some(Connection::open_with_flags(root.join(".rentemester/workspace-control.sqlite"), flags)
            .map_err(|_| OwnerError::StorageUnusable(Readiness::new(Code::StorageUnavailable)))?);
        slots.company = Some(Connection::open_with_flags(root.join(slug).join("data/ledger.sqlite"), flags)
            .map_err(|_| OwnerError::StorageUnusable(Readiness::new(Code::StorageUnavailable)))?);
        slots.guard.ensure_connections(slots.control.as_ref().unwrap(), slots.company.as_ref().unwrap())
            .map_err(OwnerError::Namespace)?;
        let identity = slots.guard.held_root_identity().map_err(OwnerError::Namespace)?;
        rentemester_storage::namespace_custody::verify(slots.control.as_ref().unwrap(), rentemester_storage::namespace_custody::Identity {device: identity.device, inode: identity.inode}).map_err(|_| OwnerError::Database)?;
        let readiness = rentemester_storage::readiness::probe(slots.control.as_ref().unwrap(), slots.company.as_ref().unwrap());
        slots.guard.ensure_connections(slots.control.as_ref().unwrap(), slots.company.as_ref().unwrap())
            .map_err(OwnerError::Namespace)?;
        if readiness.unusable() { return Err(OwnerError::StorageUnusable(readiness)); }
        if !slots.close() { return Err(OwnerError::Database); }
        let slots = held.0.take().unwrap();
        Self::open_guarded(root, workspace, slug, slots.guard)
    }

    /// Uses the existing lifetime lease, poison/retirement state and connections.
    pub fn readiness(&mut self) -> Readiness {
        if self.admit().is_err() { return Readiness::new(Code::StorageUnavailable); }
        let slots = self.held.0.as_ref().unwrap();
        let result = rentemester_storage::readiness::probe(slots.control.as_ref().unwrap(), slots.company.as_ref().unwrap());
        if self.finish(true) { return Readiness::new(Code::StorageUnavailable); }
        if result.unusable() { self.reusable = false; }
        result
    }
}
