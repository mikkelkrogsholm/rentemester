//! Fixed pure regulatory interface; fail-closed scaffold until scoped implementation.
use crate::Error;
use serde_json::Value;
pub fn bundled_coverage() -> Result<Value,Error> { Err(Error::new("regulatory_unavailable","regulatory implementation pending")) }
pub fn render_coverage(_: &Value) -> Result<String,Error> { Err(Error::new("regulatory_unavailable","regulatory implementation pending")) }
pub fn render_citations(_: &Value) -> Result<String,Error> { Err(Error::new("regulatory_unavailable","regulatory implementation pending")) }
