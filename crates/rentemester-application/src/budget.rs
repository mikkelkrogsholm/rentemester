//! Current cookie authority is fenced with the immutable budget revision write.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::Value;
pub enum Action<'a> {
    Read {
        year: Option<&'a str>,
        compare: bool,
    },
    Dimensions {
        year: Option<&'a str>,
        dimension: Option<&'a str>,
        member: Option<&'a str>,
    },
    Set(&'a rentemester_storage::budget::Set),
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let permission = match action {
        Action::Read { .. } | Action::Dimensions { .. } => Permission::CompanyRead,
        Action::Set(_) => Permission::CompanyAdmin,
    };
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
        |tx, actor| match action {
            Action::Read { year, compare } => {
                rentemester_storage::budget::read(tx, slug, today, year, compare)
            }
            Action::Dimensions {
                year,
                dimension,
                member,
            } => rentemester_storage::budget::dimensions::read(
                tx, slug, today, year, dimension, member,
            ),
            Action::Set(input) => rentemester_storage::budget::set(tx, input, actor),
        },
    )
}
