//! Fixed pure report interface; no assertion of compliance from this scaffold.
use crate::Error;
use serde_json::Value;
pub fn render(_: &Value) -> Result<String,Error> { Err(Error::new("compliance_unavailable","compliance implementation pending")) }
