//! Immutable trusted configuration binding, never selected from request JSON or socket UID.
use super::*;
pub(super) fn operator(config: &Config) -> Option<rentemester_runtime::bootstrap::Operator> {
    select(
        &config.workspace,
        config.restore_actor.as_deref(),
        config.setup_actor.as_deref(),
        &config.operator_actors,
    )
}
fn select(
    workspace: &str,
    restore: Option<&str>,
    setup: Option<&str>,
    allowed: &[String],
) -> Option<rentemester_runtime::bootstrap::Operator> {
    if matches!((restore, setup), (Some(a),Some(b)) if a != b) {
        return None;
    }
    rentemester_runtime::bootstrap::Operator::from_policy(workspace, restore.or(setup)?, allowed)
        .ok()
}
impl Backend {
    /// Trusted backend code must additionally require verified Unix transport and current
    /// authenticated operator/company authorization. This binding grants no request authority.
    pub(crate) fn original_restore_operator(
        &self,
    ) -> Option<&rentemester_runtime::bootstrap::Operator> {
        self.original_restore_operator.as_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_missing_ambiguous_or_unlisted_operator_refuses() {
        let allowed = vec!["user:operator".into()];
        assert!(select("w", None, None, &allowed).is_none());
        assert!(select("w", Some("user:operator"), Some("user:other"), &allowed).is_none());
        assert!(select("w", Some("user:other"), None, &allowed).is_none());
        assert_eq!(
            select("w", Some("user:operator"), None, &allowed)
                .unwrap()
                .actor(),
            "user:operator"
        );
    }
}
