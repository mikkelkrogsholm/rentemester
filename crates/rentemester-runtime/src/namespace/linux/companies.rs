//! Narrow workspace-owner transitions, retaining the existing root lease.
use super::*;
impl NamespaceGuard {
    pub fn catalog(&mut self) -> Result<Vec<(String, String, bool)>, Error> {
        self.ensure_current()?;
        self.layout.catalog().map_err(|_| Error::Changed)
    }
    /// Host must have confirmed closure of its previous company connection.
    pub fn select_company(&mut self, slug: &str) -> Result<(), Error> {
        self.ensure_current()?;
        let (_, directory) = walk(&self.root, self.uid, true)?;
        metadata(&self.root, &directory, self.uid, slug)?;
        self.layout
            .select_company(slug)
            .map_err(|_| Error::Invalid)?;
        self.slug = slug.to_owned();
        self.ensure_current()
    }
    /// Trusted snapshot/restore traversal only. Host must close the prior DB,
    /// verify each opened connection, and restore ordinary selection before
    /// returning any Owner to request handling; failure retires that Owner.
    pub fn select_company_for_snapshot(&mut self, slug: &str) -> Result<(), Error> {
        self.ensure_current()?;
        let (_, directory) = walk(&self.root, self.uid, true)?;
        metadata(&self.root, &directory, self.uid, slug)?;
        self.layout
            .select_company_for_snapshot(slug)
            .map_err(|_| Error::Invalid)?;
        self.slug = slug.to_owned();
        self.ensure_current()
    }
    pub fn create_company_files(&mut self, slug: &str) -> Result<(), Error> {
        self.ensure_current()?;
        self.layout
            .create_company_files(slug)
            .map_err(|_| Error::Exists)?;
        let (_, directory) = walk(&self.root, self.uid, true)?;
        metadata(&self.root, &directory, self.uid, slug)?;
        self.ensure_current()
    }
    pub fn verify_pending_company(
        &mut self,
        slug: &str,
        db: &rusqlite::Connection,
    ) -> Result<(), Error> {
        self.ensure_current()?;
        let (_, directory) = walk(&self.root, self.uid, true)?;
        metadata(&self.root, &directory, self.uid, slug)?;
        self.layout
            .verify_pending_company(db)
            .map_err(|_| Error::Changed)?;
        self.ensure_current()
    }
    pub fn publish_company(
        &mut self,
        slug: &str,
        name: &str,
        created_at: i64,
    ) -> Result<(), Error> {
        self.ensure_current()?;
        self.layout
            .publish_company(
                slug,
                name,
                &chrono::DateTime::from_timestamp_millis(created_at)
                    .ok_or(Error::Invalid)?
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            )
            .map_err(|_| Error::Changed)?;
        self.ensure_current()
    }
}
