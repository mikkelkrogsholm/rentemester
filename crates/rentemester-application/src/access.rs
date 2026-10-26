//! Pure, non-disclosing access decisions, never a credential or a reusable commit grant.
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Eq)]
enum Scope {
    Public,
    Workspace,
    Company,
}

// Serde's default unit-enum decoder also accepts tagged objects. Policy wire values
// must be strings; one definition keeps serialization, decoding and spelling together.
macro_rules! string_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        pub enum $name { $(#[serde(rename = $wire)] $variant),+ }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                match value.as_str() {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::unknown_variant(&value, &[$($wire),+])),
                }
            }
        }
    };
}
string_enum! { Permission {
    PublicRead => "public.read",
    PublicInvitationClaim => "public.invitation.claim",
    WorkspaceRead => "workspace.read",
    WorkspaceGroupRead => "workspace.group.read",
    WorkspaceManage => "workspace.manage",
    WorkspaceMembersRead => "workspace.members.read",
    WorkspaceMembersManage => "workspace.members.manage",
    WorkspaceTasksRead => "workspace.tasks.read",
    WorkspaceTasksManage => "workspace.tasks.manage",
    CompanyRead => "company.read",
    CompanyDocumentsRead => "company.documents.read",
    CompanyDocumentsUpload => "company.documents.upload",
    CompanyMasterData => "company.master-data",
    CompanyDraftWrite => "company.draft.write",
    CompanyLedgerPost => "company.ledger.post",
    CompanyReview => "company.review",
    CompanyPeriodForceClose => "company.period.force-close",
    CompanyExport => "company.export",
    CompanyExternalLookup => "company.external-lookup",
    CompanyExternalSend => "company.external-send",
    CompanyAdmin => "company.admin",
    CompanyKnowledgeRead => "company.knowledge.read",
    CompanyKnowledgeManage => "company.knowledge.manage",
    CompanyOwnershipRead => "company.ownership.read",
    CompanyOwnershipManage => "company.ownership.manage",
    CompanyTasksRead => "company.tasks.read",
    CompanyTasksWrite => "company.tasks.write",
    CompanyTasksManage => "company.tasks.manage",
} }
impl Permission {
    pub const ALL: [Self; 28] = [
        Self::PublicRead,
        Self::PublicInvitationClaim,
        Self::WorkspaceRead,
        Self::WorkspaceGroupRead,
        Self::WorkspaceManage,
        Self::WorkspaceMembersRead,
        Self::WorkspaceMembersManage,
        Self::CompanyRead,
        Self::CompanyDocumentsRead,
        Self::CompanyDocumentsUpload,
        Self::CompanyMasterData,
        Self::CompanyDraftWrite,
        Self::CompanyLedgerPost,
        Self::CompanyReview,
        Self::CompanyPeriodForceClose,
        Self::CompanyExport,
        Self::CompanyExternalLookup,
        Self::CompanyExternalSend,
        Self::CompanyAdmin,
        Self::CompanyKnowledgeRead,
        Self::CompanyKnowledgeManage,
        Self::CompanyOwnershipRead,
        Self::CompanyOwnershipManage,
        Self::WorkspaceTasksRead,
        Self::WorkspaceTasksManage,
        Self::CompanyTasksRead,
        Self::CompanyTasksWrite,
        Self::CompanyTasksManage,
    ];
    fn scope(self) -> Scope {
        match self {
            Self::PublicRead | Self::PublicInvitationClaim => Scope::Public,
            Self::WorkspaceRead
            | Self::WorkspaceGroupRead
            | Self::WorkspaceManage
            | Self::WorkspaceMembersRead
            | Self::WorkspaceMembersManage
            | Self::WorkspaceTasksRead
            | Self::WorkspaceTasksManage => Scope::Workspace,
            Self::CompanyRead
            | Self::CompanyDocumentsRead
            | Self::CompanyDocumentsUpload
            | Self::CompanyMasterData
            | Self::CompanyDraftWrite
            | Self::CompanyLedgerPost
            | Self::CompanyReview
            | Self::CompanyPeriodForceClose
            | Self::CompanyExport
            | Self::CompanyExternalLookup
            | Self::CompanyExternalSend
            | Self::CompanyAdmin
            | Self::CompanyKnowledgeRead
            | Self::CompanyKnowledgeManage
            | Self::CompanyOwnershipRead
            | Self::CompanyOwnershipManage
            | Self::CompanyTasksRead
            | Self::CompanyTasksWrite
            | Self::CompanyTasksManage => Scope::Company,
        }
    }
    pub(crate) fn is_company(self) -> bool {
        self.scope() == Scope::Company
    }
    fn is_public(self) -> bool {
        self.scope() == Scope::Public
    }
    fn is_workspace(self) -> bool {
        self.scope() == Scope::Workspace
    }
}
string_enum! { WorkspaceRole { WorkspaceOwner => "workspace_owner", Member => "member" } }
string_enum! { CompanyRole { Owner => "owner", Bookkeeper => "bookkeeper", Reviewer => "reviewer", Reader => "reader" } }

pub fn workspace_role_allows(role: WorkspaceRole, permission: Permission) -> bool {
    match role {
        WorkspaceRole::WorkspaceOwner => permission.is_public() || permission.is_workspace(),
        WorkspaceRole::Member => matches!(
            permission,
            Permission::WorkspaceRead | Permission::WorkspaceTasksRead
        ),
    }
}
pub fn company_role_allows(role: CompanyRole, permission: Permission) -> bool {
    use Permission::*;
    if permission.is_public() || permission.is_workspace() {
        return false;
    }
    match role {
        CompanyRole::Owner => true,
        CompanyRole::Bookkeeper => matches!(
            permission,
            CompanyTasksRead
                | CompanyTasksWrite
                | CompanyTasksManage
                | CompanyRead
                | CompanyDocumentsRead
                | CompanyDocumentsUpload
                | CompanyMasterData
                | CompanyDraftWrite
                | CompanyLedgerPost
                | CompanyExport
                | CompanyExternalLookup
                | CompanyKnowledgeRead
                | CompanyKnowledgeManage
                | CompanyOwnershipRead
                | CompanyOwnershipManage
        ),
        CompanyRole::Reviewer => matches!(
            permission,
            CompanyTasksRead
                | CompanyTasksWrite
                | CompanyRead
                | CompanyDocumentsRead
                | CompanyReview
                | CompanyExport
                | CompanyKnowledgeRead
                | CompanyKnowledgeManage
                | CompanyOwnershipRead
                | CompanyOwnershipManage
        ),
        CompanyRole::Reader => matches!(
            permission,
            CompanyTasksRead
                | CompanyRead
                | CompanyDocumentsRead
                | CompanyExport
                | CompanyKnowledgeRead
                | CompanyOwnershipRead
        ),
    }
}

// Intentionally no Deserialize on observations: these belong to trusted in-process adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityKind {
    Human {
        email_verified: bool,
        mfa_enabled: bool,
    },
    Service {
        registered: bool,
        live_credential: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFacts<'a> {
    pub workspace: &'a str,
    pub subject: &'a str,
    pub authenticated: bool,
    pub kind: IdentityKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAccessFacts<'a> {
    pub workspace: &'a str,
    pub subject: &'a str,
    pub active: bool,
    pub role: WorkspaceRole,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipFacts<'a> {
    pub workspace: &'a str,
    pub subject: &'a str,
    pub company: &'a str,
    pub active: bool,
    pub role: CompanyRole,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyFacts<'a> {
    pub workspace: &'a str,
    pub slug: &'a str,
    pub registered: bool,
    pub archived: bool,
    pub membership: Option<MembershipFacts<'a>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessFacts<'a> {
    pub identity: Option<IdentityFacts<'a>>,
    pub workspace_access: Option<WorkspaceAccessFacts<'a>>,
    pub company: Option<CompanyFacts<'a>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request<'a> {
    pub workspace: &'a str,
    pub company: Option<&'a str>,
    pub permission: Permission,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub allowed: bool,
}

fn valid_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}
/// Caller must observe current identity/control state under the eventual revocation fence.
/// This function does not verify observations or protect later work from revocation.
pub fn authorize(request: Request<'_>, facts: &AccessFacts<'_>) -> Decision {
    Decision {
        allowed: allowed(request, facts),
    }
}
fn allowed(request: Request<'_>, facts: &AccessFacts<'_>) -> bool {
    if request.permission.is_public() {
        return true;
    }
    let Some(identity) = &facts.identity else {
        return false;
    };
    if request.workspace.is_empty()
        || identity.workspace != request.workspace
        || identity.subject.is_empty()
        || !identity.authenticated
    {
        return false;
    }
    let ready = match identity.kind {
        IdentityKind::Human {
            email_verified,
            mfa_enabled,
        } => email_verified && mfa_enabled,
        IdentityKind::Service {
            registered,
            live_credential,
        } => registered && live_credential,
    };
    if !ready {
        return false;
    }
    let Some(workspace) = &facts.workspace_access else {
        return false;
    };
    if workspace.workspace != request.workspace
        || workspace.subject != identity.subject
        || !workspace.active
    {
        return false;
    }
    if request.permission.is_workspace() {
        return workspace_role_allows(workspace.role, request.permission);
    }
    let Some(slug) = request.company.filter(|s| valid_slug(s)) else {
        return false;
    };
    let Some(company) = &facts.company else {
        return false;
    };
    if company.workspace != request.workspace
        || company.slug != slug
        || !company.registered
        || company.archived
    {
        return false;
    }
    let Some(membership) = &company.membership else {
        return false;
    };
    membership.workspace == request.workspace
        && membership.subject == identity.subject
        && membership.company == slug
        && membership.active
        && company_role_allows(membership.role, request.permission)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_vocabulary_and_slug_grammar() {
        for p in Permission::ALL {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(serde_json::from_str::<Permission>(&json).unwrap(), p);
        }
        for p in [
            "company.write",
            "company.READ",
            " company.read",
            "company.read ",
            "",
            "workspace.future",
        ] {
            assert!(serde_json::from_value::<Permission>(serde_json::json!(p)).is_err());
        }
        for slug in ["", "A", "../a", "a/b", "a\\b", "æ", "a_", "a ", "a\0", "-a"] {
            assert!(!valid_slug(slug));
        }
        for slug in ["a", "0", "a--", "company-2"] {
            assert!(valid_slug(slug));
        }
    }
    #[test]
    fn unknown_roles_and_nonstring_permissions_refuse() {
        for value in [
            serde_json::json!("admin"),
            serde_json::json!("OWNER"),
            serde_json::json!(" owner"),
            serde_json::json!(null),
            serde_json::json!(1),
            serde_json::json!({"role":"owner"}),
            serde_json::json!({"owner":null}),
            serde_json::json!({"workspace_owner":null}),
        ] {
            assert!(serde_json::from_value::<WorkspaceRole>(value.clone()).is_err());
            assert!(serde_json::from_value::<CompanyRole>(value).is_err());
        }
        for value in [
            serde_json::json!(null),
            serde_json::json!(1),
            serde_json::json!(true),
            serde_json::json!({"permission":"company.read"}),
            serde_json::json!({"company.read":null}),
        ] {
            assert!(serde_json::from_value::<Permission>(value).is_err());
        }
        assert_eq!(
            serde_json::to_string(&Decision { allowed: false }).unwrap(),
            r#"{"allowed":false}"#
        );
    }
    #[test]
    fn reevaluation_observes_revocation_and_never_changes_prior_facts() {
        let mut facts = AccessFacts {
            identity: Some(IdentityFacts {
                workspace: "w",
                subject: "s",
                authenticated: true,
                kind: IdentityKind::Human {
                    email_verified: true,
                    mfa_enabled: true,
                },
            }),
            workspace_access: Some(WorkspaceAccessFacts {
                workspace: "w",
                subject: "s",
                active: true,
                role: WorkspaceRole::Member,
            }),
            company: Some(CompanyFacts {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
                membership: Some(MembershipFacts {
                    workspace: "w",
                    subject: "s",
                    company: "a",
                    active: true,
                    role: CompanyRole::Bookkeeper,
                }),
            }),
        };
        let request = Request {
            workspace: "w",
            company: Some("a"),
            permission: Permission::CompanyLedgerPost,
        };
        let before = facts.clone();
        assert!(authorize(request, &facts).allowed);
        assert_eq!(facts, before);
        facts
            .company
            .as_mut()
            .unwrap()
            .membership
            .as_mut()
            .unwrap()
            .active = false;
        assert!(!authorize(request, &facts).allowed);
        // A restored membership still cannot survive global workspace disable.
        facts
            .company
            .as_mut()
            .unwrap()
            .membership
            .as_mut()
            .unwrap()
            .active = true;
        facts.workspace_access.as_mut().unwrap().active = false;
        assert!(!authorize(request, &facts).allowed);
    }
    #[test]
    fn company_roles_never_grant_workspace_or_public_permissions() {
        for role in [
            CompanyRole::Owner,
            CompanyRole::Bookkeeper,
            CompanyRole::Reviewer,
            CompanyRole::Reader,
        ] {
            for p in Permission::ALL
                .into_iter()
                .filter(|p| p.is_workspace() || p.is_public())
            {
                assert!(!company_role_allows(role, p));
            }
        }
    }
}
