//! Synthetic fixture adapter only. Not an authentication/authorization endpoint.
use rentemester_application::access::*;
use serde_json::{Value, json};
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap()
}
fn flag(v: &Value, key: &str) -> bool {
    v[key].as_bool().unwrap()
}
fn execute(v: &Value) -> Decision {
    let fault = text(v, "fault");
    let workspace = if fault == "empty-workspace" {
        ""
    } else {
        "workspace-a"
    };
    let subject = if fault == "empty-subject" {
        ""
    } else {
        "subject-a"
    };
    let slug = if fault == "bad-slug" {
        "../a"
    } else {
        "company-a"
    };
    let identity = (text(v, "kind") != "missing").then(|| IdentityFacts {
        workspace: if fault == "identity-workspace" {
            "workspace-b"
        } else {
            "workspace-a"
        },
        subject,
        authenticated: flag(v, "authenticated"),
        kind: if text(v, "kind") == "human" {
            IdentityKind::Human {
                email_verified: flag(v, "verified"),
                mfa_enabled: flag(v, "mfa"),
            }
        } else {
            IdentityKind::Service {
                registered: flag(v, "service"),
                live_credential: flag(v, "live"),
            }
        },
    });
    let workspace_access = (fault != "no-workspace").then(|| WorkspaceAccessFacts {
        workspace: if fault == "access-workspace" {
            "workspace-b"
        } else {
            "workspace-a"
        },
        subject: if fault == "access-subject" {
            "subject-b"
        } else {
            subject
        },
        active: fault != "disabled",
        role: serde_json::from_value(v["workspaceRole"].clone()).unwrap(),
    });
    let membership = v["companyRole"].as_str().map(|_| MembershipFacts {
        workspace: if fault == "membership-workspace" {
            "workspace-b"
        } else {
            "workspace-a"
        },
        subject: if fault == "membership-subject" {
            "subject-b"
        } else {
            subject
        },
        company: if fault == "membership-company" {
            "company-b"
        } else {
            slug
        },
        active: fault != "revoked",
        role: serde_json::from_value(v["companyRole"].clone()).unwrap(),
    });
    let company = (fault != "no-company").then_some(CompanyFacts {
        workspace: if fault == "company-workspace" {
            "workspace-b"
        } else {
            "workspace-a"
        },
        slug,
        registered: fault != "unregistered",
        archived: fault == "archived",
        membership,
    });
    let facts = AccessFacts {
        identity,
        workspace_access,
        company,
    };
    let request = Request {
        workspace,
        company: if fault == "no-request-company" {
            None
        } else if fault == "request-company" {
            Some("company-b")
        } else {
            Some(slug)
        },
        permission: serde_json::from_value(v["permission"].clone()).unwrap(),
    };
    let before = facts.clone();
    let result = authorize(request, &facts);
    assert_eq!(result, authorize(request, &facts));
    assert_eq!(facts, before);
    result
}
#[test]
#[ignore = "independent access corpus explicitly selected by Bun harness"]
fn vectors() {
    let input = std::fs::read_to_string(std::env::var("RM_ACCESS_INPUT").unwrap()).unwrap();
    let mut out = String::new();
    for line in input.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        let result = execute(&v);
        let permission: Permission = serde_json::from_value(v["permission"].clone()).unwrap();
        let role: WorkspaceRole = serde_json::from_value(v["workspaceRole"].clone()).unwrap();
        assert_eq!(
            workspace_role_allows(role, permission),
            flag(&v, "workspaceRoleExpected")
        );
        let company_role: Option<CompanyRole> =
            serde_json::from_value(v["companyRole"].clone()).unwrap();
        assert_eq!(
            company_role.is_some_and(|r| company_role_allows(r, permission)),
            flag(&v, "companyRoleExpected")
        );
        assert_eq!(result.allowed, flag(&v, "expected"), "{}", text(&v, "id"));
        out.push_str(&json!({"id":v["id"],"allowed":result.allowed}).to_string());
        out.push('\n');
    }
    std::fs::write(std::env::var("RM_ACCESS_OUTPUT").unwrap(), out).unwrap();
}
