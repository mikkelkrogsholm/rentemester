//! Custody admission and stopped operator maintenance. No HTTP authority.
use super::*;
use rentemester_runtime::bootstrap::Operator;
use rentemester_storage::namespace_custody::{self as storage, Identity};
impl Owner {
    fn custody_identity(&mut self) -> Result<Identity,OwnerError> {
        self.observe()?;
        let id=self.held.0.as_mut().unwrap().guard.held_root_identity().map_err(OwnerError::Namespace)?;
        Ok(Identity{device:id.device,inode:id.inode})
    }
    pub(super) fn verify_namespace_custody(&mut self) -> Result<(),OwnerError> {
        let id=self.custody_identity()?;
        storage::verify(self.held.0.as_ref().unwrap().control.as_ref().unwrap(),id).map_err(|_|OwnerError::Database)
    }
    pub(super) fn initialize_namespace_custody(&mut self) -> Result<(),OwnerError> {
        let id=self.custody_identity()?;
        storage::initialize_private(self.held.0.as_mut().unwrap().control.as_mut().unwrap(),id).map_err(|_|OwnerError::Database)
    }
    /// A stopped process obtains the same exclusive lifetime lease as startup.
    /// Never returns a reusable Owner, even on a current exact no-op.
    pub fn enroll_namespace_custody(root:&Path, confirmed:&Path, uid:u32, workspace:&str, slug:&str, profile:NamespaceProfile, operator:&Operator) -> Result<(),OwnerError> {
        if root != confirmed || operator.workspace()!=workspace { return Err(OwnerError::Context); }
        let guard=NamespaceGuard::acquire_with_profile(root,uid,workspace,slug,profile).map_err(OwnerError::Namespace)?;
        let mut owner=Self::open_private_guarded(root,workspace,slug,guard)?;
        let id=owner.custody_identity()?;
        storage::enroll_stopped(owner.held.0.as_mut().unwrap().control.as_mut().unwrap(),id).map_err(|_|OwnerError::Database)?;
        owner.try_close().map_err(|_|OwnerError::Database)
    }
    /// Mail extension is intentionally only v30 -> v31, not custody enrollment.
    pub fn maintain_mail_schema(root:&Path,uid:u32,workspace:&str,slug:&str,profile:NamespaceProfile)->Result<(),OwnerError>{
        let guard=NamespaceGuard::acquire_with_profile(root,uid,workspace,slug,profile).map_err(OwnerError::Namespace)?;
        let mut owner=Self::open_private_guarded(root,workspace,slug,guard)?;
        rentemester_storage::upgrade_mail_control(owner.held.0.as_mut().unwrap().control.as_mut().unwrap()).map_err(|_|OwnerError::Database)?;
        owner.try_close().map_err(|_|OwnerError::Database)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(target_os="macos")]
    fn namespace_custody_fresh_reopen_exclusive_lease_and_wrong_workspace() {
        let dir=tempfile::tempdir().unwrap();
        let root=dir.path().canonicalize().unwrap().join("fresh");
        let uid=std::process::Command::new("id").arg("-u").output().unwrap();
        let uid=std::str::from_utf8(&uid.stdout).unwrap().trim().parse().unwrap();
        let profile=NamespaceProfile::LocalUserMacos;
        let (owner,metadata)=Owner::create_new_with_profile(&root,uid,"w","a","Synthetic","2026-01-01T00:00:00Z",profile).unwrap();
        assert_eq!(metadata.control_version,32);
        let op=Operator::from_policy("w","agent:test", &["agent:test".into()]).unwrap();
        assert!(Owner::enroll_namespace_custody(&root,&root,uid,"w","a",profile,&op).is_err());
        owner.try_close().unwrap();
        Owner::enroll_namespace_custody(&root,&root,uid,"w","a",profile,&op).unwrap();
        let wrong=Operator::from_policy("other","agent:test", &["agent:test".into()]).unwrap();
        assert!(Owner::enroll_namespace_custody(&root,&root,uid,"w","a",profile,&wrong).is_err());
        let owner=Owner::open_ready_with_profile(&root,uid,"w","a",profile).unwrap();
        owner.try_close().unwrap();
    }
    #[test]
    fn namespace_custody_wrong_confirmation_refuses_without_creation(){
        let temp=tempfile::tempdir().unwrap(); let root=temp.path().join("absent");
        let operator=Operator::from_policy("synthetic","agent:test", &["agent:test".to_owned()]).unwrap();
        assert!(Owner::enroll_namespace_custody(&root,temp.path(),10001,"synthetic","company",NamespaceProfile::DedicatedLinux,&operator).is_err());
        assert!(!root.exists());
    }
}
