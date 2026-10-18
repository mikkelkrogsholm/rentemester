//! Fixed company setup commands under the same current-session fence as posting.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
pub use rentemester_storage::company_setup::{Account, Role};
use rusqlite::Connection;
use serde_json::{Map, Value};
pub enum Action<'a> {
    Accounts,
    Profile,
    Create(&'a Account),
    Confirm(&'a Role),
    Update(&'a Map<String, Value>),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Accounts | Action::Profile => Permission::CompanyRead,
        _ => Permission::CompanyAdmin,
    };
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        |tx, subject| match action {
            Action::Accounts => rentemester_storage::company_setup::accounts(tx, slug),
            Action::Profile => rentemester_storage::company_setup::profile(tx),
            Action::Create(input) => {
                rentemester_storage::company_setup::create_account(tx, input, subject)
            }
            Action::Confirm(input) => {
                rentemester_storage::company_setup::confirm_role(tx, input, subject)
            }
            Action::Update(input) => {
                rentemester_storage::company_setup::update_profile(tx, input, subject)
            }
        },
    )
}
