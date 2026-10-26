//! Stable source capture -> credential-free business codecs -> fresh control.
//! Conversion is evidence only. Host owns source retirement, publication and fresh identity.
use crate::{Snapshot, control_migration, portable_backup as business};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::path::Path;

/// No Deserialize/Debug or raw SQLite escape. All retained sections are the
/// existing fixed credential-free business codecs, never historical authority.
pub struct ConvertedControlBusiness {
    source_logical_digest: String,
    from_version: usize,
    business: business::Control,
    retirement: Option<crate::legacy_retirement::RetirementBinding>,
}
fn refused(error: Error) -> Error {
    Error::new(error.code, "legacy control adoption conversion refused")
}
fn invalid(_: impl std::fmt::Display) -> Error {
    Error::new(
        "legacy_control_adoption_refused",
        "fresh legacy control initialization refused",
    )
}
impl ConvertedControlBusiness {
    pub fn source_logical_digest(&self) -> &str {
        &self.source_logical_digest
    }
    pub fn from_version(&self) -> usize {
        self.from_version
    }
    pub fn to_version(&self) -> usize {
        31
    }
    pub fn retirement_binding(&self) -> Option<&crate::legacy_retirement::RetirementBinding> {
        self.retirement.as_ref()
    }
    /// Stable versioned, length-framed digest of every business section; no auth bytes included.
    pub fn payload_sha256(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"rentemester-legacy-control-business/3\0");
        for bytes in [
            &self.business.bytes,
            &self.business.knowledge_bytes,
            &self.business.parties_bytes,
            &self.business.inbox_bytes,
            &self.business.corporate_bytes,
            &self.business.ownership_bytes,
            &self.business.intercompany_bytes,
            &self.business.consolidation_bytes,
            &self.business.task_bytes,
            &self.business.knowledge_page_bytes,
        ] {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        format!("{:x}", hash.finalize())
    }
    /// The host may consume only the credential-free codecs and verified binding.
    pub fn into_parts(
        self,
    ) -> (
        business::Control,
        Option<crate::legacy_retirement::RetirementBinding>,
    ) {
        (self.business, self.retirement)
    }
    /// Caller owns a new private file and its entire namespace lifetime. Multiple
    /// codec commits are not atomic: on failure quarantine the stage, never retry
    /// into or publish a partially initialized control. No identity/access seeded.
    pub fn initialize_new_control(
        &self,
        db: &mut Connection,
        registered: &[String],
        receipt_hash: &str,
        actor: &str,
    ) -> Result<(), Error> {
        if !crate::workspace_transfer::hex(receipt_hash, 64)
            || actor.trim().is_empty()
            || actor.len() > 1024
            || actor.chars().any(char::is_control)
            || registered.is_empty()
            || registered.len() > 64
            || registered.iter().any(|s| s.is_empty())
            || registered
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != registered.len()
        {
            return Err(invalid("context"));
        }
        if !db.is_autocommit() || db.is_readonly(rusqlite::MAIN_DB).map_err(invalid)? {
            return Err(invalid("connection"));
        }
        for sql in [
            "SELECT count(*) FROM sqlite_temp_schema",
            "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
            "PRAGMA user_version",
            "PRAGMA application_id",
            "PRAGMA query_only",
        ] {
            if db
                .query_row::<i64, _, _>(sql, [], |r| r.get(0))
                .map_err(invalid)?
                != 0
            {
                return Err(invalid("fresh target required"));
            }
        }
        control_migration::initialize_empty(db).map_err(refused)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(invalid)?;
        business::initialize_audit_business(&tx, &self.business.bytes).map_err(refused)?;
        tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor) VALUES('legacy_control_adoption','legacy_adoption',?1,?2)",rusqlite::params![receipt_hash,actor]).map_err(invalid)?;
        tx.commit().map_err(invalid)?;
        business::initialize_parties(db, &self.business.parties_bytes, registered)
            .map_err(refused)?;
        business::initialize_knowledge(db, &self.business.knowledge_bytes).map_err(refused)?;
        business::initialize_inbox(db, &self.business.inbox_bytes, registered).map_err(refused)?;
        business::initialize_corporate(db, &self.business.corporate_bytes, registered)
            .map_err(refused)?;
        business::initialize_ownership(db, &self.business.ownership_bytes, registered)
            .map_err(refused)?;
        business::initialize_intercompany(db, &self.business.intercompany_bytes, registered)
            .map_err(refused)?;
        business::initialize_consolidation(db, &self.business.consolidation_bytes, registered)
            .map_err(refused)?;
        business::tasks::initialize(db, &self.business.task_bytes, registered).map_err(refused)?;
        business::knowledge_pages::initialize(db, &self.business.knowledge_page_bytes, registered).map_err(refused)?;
        control_migration::validate_current_control(db).map_err(refused)
    }
}
/// Existing bounded stable NOFOLLOW capture and private writable-copy cleanup.
/// A missing retirement binding is truthful conversion evidence, not permission
/// for host publication. Host must require matching retired control/companies.
pub fn convert_control_for_adoption(workspace: &Path) -> Result<ConvertedControlBusiness, Error> {
    Snapshot::capture_control(workspace)
        .map_err(refused)?
        .with_writable_copy(|db| {
            let source_logical_digest = crate::legacy_retirement::source_digest(db)?;
            let retirement = crate::legacy_retirement::strip_verified_overlay(db)?;
            let (from_version, business) = control_migration::convert_adoption_copy(db)?;
            Ok(ConvertedControlBusiness {
                source_logical_digest,
                from_version,
                business,
                retirement,
            })
        })
        .map_err(refused)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use serde_json::{Value, json};
    fn fixture(version: usize) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".rentemester")).unwrap();
        let db =
            Connection::open(dir.path().join(".rentemester/workspace-control.sqlite")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        let identities: Value =
            serde_json::from_str(include_str!("legacy-workspace-schema.json")).unwrap();
        for (raw, identity) in control_migration::ARTIFACTS
            .iter()
            .zip(identities.as_array().unwrap())
            .take(version)
        {
            let value: Value =
                serde_json::from_str(raw).unwrap();
            db.execute_batch(value["sql"].as_str().unwrap()).unwrap();
            db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version)VALUES(?1,?2,?3,'synthetic-source')",params![identity["id"].as_i64().unwrap(),identity["name"].as_str().unwrap(),identity["checksum"].as_str().unwrap()]).unwrap();
        }
        (dir, db)
    }
    #[test]
    fn every_exact_control_prefix_converts_without_mutating_source_or_inheriting_authority() {
        for version in 0..=31 {
            let (dir, _db) = fixture(version);
            let path = dir.path().join(".rentemester/workspace-control.sqlite");
            let before = std::fs::read(&path).unwrap();
            let value = convert_control_for_adoption(dir.path()).unwrap();
            assert_eq!(value.from_version(), version);
            assert_eq!(value.to_version(), 31);
            assert!(value.retirement_binding().is_none());
            let mut target = Connection::open_in_memory().unwrap();
            value
                .initialize_new_control(
                    &mut target,
                    &["synthetic".into()],
                    &"a".repeat(64),
                    "system:synthetic-adopter",
                )
                .unwrap();
            assert_eq!(std::fs::read(path).unwrap(), before);
            for table in [
                "user",
                "account",
                "session",
                "twoFactor",
                "apikey",
                "rm_company_membership_events",
                "rm_workspace_bootstrap_events",
            ] {
                assert_eq!(
                    target
                        .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r
                            .get(0))
                        .unwrap(),
                    0,
                    "{version}/{table}"
                );
            }
        }
    }
    #[test]
    fn populated_business_actors_survive_but_secrets_and_old_grants_do_not() {
        fn audit_prefix(db:&Connection,table:&str,last:i64)->Vec<Vec<rusqlite::types::Value>> {
            let mut q=db.prepare(&format!("SELECT * FROM {table} WHERE id<=?1 ORDER BY id")).unwrap();let n=q.column_count();
            q.query_map([last],|r|(0..n).map(|i|r.get(i)).collect::<Result<Vec<_>,_>>()).unwrap().map(Result::unwrap).collect()
        }
        let (dir, db) = fixture(31);
        db.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('old','Synthetic old owner','old@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO account(id,issuer,accountId,providerId,userId,password,createdAt,updatedAt)VALUES('credential','local:credential','old','credential','old','synthetic-password-secret','2026-01-01','2026-01-01');INSERT INTO session(id,token,userId,createdAt,updatedAt,expiresAt)VALUES('s','synthetic-token-secret','old','2026-01-01','2026-01-01','2099-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic-totp-secret','[]','old',1);INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('old','activate','workspace_owner','user:old');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('old','synthetic','grant','owner','user:old');INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('synthetic-history','synthetic','preserved','user:old')").unwrap();
        // Historical v31 data is seeded as historical rows. A current writer must
        // continue to refuse this old schema until explicit private conversion.
        let payload=json!({"name":"Synthetic preserved party","aliases":["Synthetic alias"],"identifiers":[],"source":"manual","observedAt":"2026-01-01T00:00:00.000Z","reviewAssertion":"approved"});
        db.execute("INSERT INTO rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at) VALUES('synthetic-party','created','organization',?1,?2,'user:old','2026-01-01T00:00:00.000Z')",params![crate::parties::hash(&payload),crate::parties::canonical(&payload)]).unwrap();
        let source_party=db.query_row("SELECT canonical_payload,payload_hash,actor FROM rm_party_events",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).unwrap();
        let source_audit=["workspace_audit","rm_workspace_document_access_events","rm_workspace_authorization_events"].map(|table| {
            let last=db.query_row::<i64,_,_>(&format!("SELECT COALESCE(MAX(id),0) FROM {table}"),[],|r|r.get(0)).unwrap();(table,last,audit_prefix(&db,table,last))
        });
        let original =
            std::fs::read(dir.path().join(".rentemester/workspace-control.sqlite")).unwrap();
        let converted = convert_control_for_adoption(dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(".rentemester/workspace-control.sqlite")).unwrap(),
            original
        );
        for bytes in [
            &converted.business.bytes,
            &converted.business.knowledge_bytes,
            &converted.business.parties_bytes,
            &converted.business.inbox_bytes,
            &converted.business.corporate_bytes,
            &converted.business.ownership_bytes,
            &converted.business.intercompany_bytes,
            &converted.business.consolidation_bytes,
            &converted.business.task_bytes,
            &converted.business.knowledge_page_bytes,
        ] {
            let text = std::str::from_utf8(bytes).unwrap();
            for secret in [
                "synthetic-password-secret",
                "synthetic-token-secret",
                "synthetic-totp-secret",
            ] {
                assert!(!text.contains(secret));
            }
        }
        assert_eq!(converted.payload_sha256().len(), 64);
        let mut target = Connection::open_in_memory().unwrap();
        converted
            .initialize_new_control(
                &mut target,
                &["synthetic".into()],
                &"b".repeat(64),
                "system:synthetic-adopter",
            )
            .unwrap();
        assert_eq!(
            target
                .query_row::<String, _, _>(
                    "SELECT actor FROM workspace_audit WHERE event_type='synthetic-history'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            "user:old"
        );
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_party_events", [], |r| r.get(0))
                .unwrap(),
            1
        );
        assert_eq!(target.query_row("SELECT canonical_payload,payload_hash,actor FROM rm_party_events",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).unwrap(),source_party);
        for (table,last,expected) in source_audit {assert_eq!(audit_prefix(&target,table,last),expected,"historical audit prefix: {table}");}
        for table in [
            "user",
            "account",
            "session",
            "twoFactor",
            "apikey",
            "rm_workspace_user_access_events",
            "rm_company_membership_events",
            "rm_workspace_bootstrap_events",
        ] {
            assert_eq!(
                target
                    .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r
                        .get(0))
                    .unwrap(),
                0,
                "{table}"
            );
        }
        assert_eq!(target.query_row::<i64,_,_>("SELECT count(*) FROM workspace_audit WHERE event_type='native_portable_restore'",[],|r|r.get(0)).unwrap(),0);
        assert_eq!(target.query_row::<i64,_,_>("SELECT count(*) FROM workspace_audit WHERE event_type='legacy_control_adoption'",[],|r|r.get(0)).unwrap(),1);
        assert!(
            converted
                .initialize_new_control(
                    &mut target,
                    &["synthetic".into()],
                    &"b".repeat(64),
                    "system:synthetic-adopter"
                )
                .is_err()
        );
    }
    #[test]
    fn unknown_schema_and_nonempty_uncoded_tables_refuse_without_source_changes() {
        for extra in [
            "CREATE TABLE unknown_source_history(id INTEGER PRIMARY KEY);INSERT INTO unknown_source_history VALUES(1)",
            "CREATE TRIGGER unknown_guard BEFORE INSERT ON workspace_audit BEGIN SELECT 1;END",
            "INSERT INTO rm_party_events(party_id,event_type,canonical_kind,payload_hash,canonical_payload,actor,created_at)VALUES('synthetic-bad','created','organization','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','{}','user:old','2026-01-01T00:00:00.000Z')",
        ] {
            let (dir, db) = fixture(31);
            db.execute_batch(extra).unwrap();
            let path = dir.path().join(".rentemester/workspace-control.sqlite");
            let before = std::fs::read(&path).unwrap();
            assert!(convert_control_for_adoption(dir.path()).is_err());
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
    }
}
#[cfg(test)]mod knowledge31_tests{
 use super::*;use rusqlite::params;use serde_json::{Value,json};
 #[test]fn explicit_main31_page_lineage_projects_without_rebinding_or_authority_restore(){
  let dir=tempfile::tempdir().unwrap();std::fs::create_dir(dir.path().join(".rentemester")).unwrap();let file=dir.path().join(".rentemester/workspace-control.sqlite");let db=Connection::open(&file).unwrap();let native:Vec<crate::Migration>=serde_json::from_str(include_str!("legacy-workspace-schema.json")).unwrap();for (raw,m)in control_migration::ARTIFACTS.iter().zip(native).take(30){let a:Value=serde_json::from_str(raw).unwrap();db.execute_batch(a["sql"].as_str().unwrap()).unwrap();db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version)VALUES(?1,?2,?3,'actual-source-prefix')",params![m.id,m.name,m.checksum]).unwrap();}
  let raw=include_str!("../../../src/core/workspace-migrations/0031-knowledge-pages-v31.json");let a:Value=serde_json::from_str(raw).unwrap();db.execute_batch(a["sql"].as_str().unwrap()).unwrap();db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version)VALUES(31,?1,?2,'actual-main31')",params![a["name"].as_str().unwrap(),format!("{:x}",Sha256::digest(raw.as_bytes()))]).unwrap();
  // Independently produced actual-main rows and hashes from the engine oracle.
  let v:Value=serde_json::from_str(include_str!("../../rentemester-engine/src/knowledge_pages/main-pages.json")).unwrap();for key in ["first","second","target"]{let row=&v[key];let p=&row["result"];let scope=if p["scope"]["kind"]=="company"{Some(p["scope"]["companySlug"].as_str().unwrap())}else{None};db.execute("INSERT INTO rm_knowledge_page_events(page_id,version,event_type,scope_kind,company_slug,slug,title,body_markdown,provenance_kind,provenance_ref,effective_from,effective_to_exclusive,prior_event_hash,event_hash,actor,principal,created_at)VALUES(?1,?2,?3,'company',?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'user:synthetic','synthetic-member',?14)",params![p["pageId"].as_str().unwrap(),p["version"].as_i64().unwrap(),p["eventType"].as_str().unwrap(),scope,p["slug"].as_str().unwrap(),p["title"].as_str().unwrap(),p["bodyMarkdown"].as_str().unwrap(),p["provenance"]["kind"].as_str().unwrap(),p["provenance"]["ref"].as_str().unwrap(),p["effectiveFrom"].as_str().unwrap(),p["effectiveToExclusive"].as_str(),p["priorEventHash"].as_str(),p["eventHash"].as_str().unwrap(),row["at"].as_str().unwrap()]).unwrap();}
  db.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('historical-owner','Synthetic historical owner','old@example.invalid',1,'2026-01-01','2026-01-01');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('historical-owner','activate','workspace_owner','user:historical-owner');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('historical-owner','synthetic','grant','owner','user:historical-owner');").unwrap();
  drop(db);let before=std::fs::read(&file).unwrap();let converted=convert_control_for_adoption(dir.path()).unwrap();assert_eq!(converted.from_version(),31);let mut target=Connection::open_in_memory().unwrap();converted.initialize_new_control(&mut target,&["synthetic".into()],&"a".repeat(64),"system:synthetic-adopter").unwrap();assert_eq!(std::fs::read(&file).unwrap(),before);let page=crate::knowledge_pages::read_page(&target,&rentemester_engine::knowledge_pages::Scope::Company{company_slug:"synthetic".into()},"playbook","2026-10-26").unwrap().unwrap();assert_eq!(page,v["second"]["result"]);assert_eq!(target.query_row::<i64,_,_>("SELECT count(*) FROM user",[],|r|r.get(0)).unwrap(),0);let name:String=target.query_row("SELECT name FROM workspace_schema_migrations WHERE id=31",[],|r|r.get(0)).unwrap();assert_eq!(name,"rentemester-workspace-mail-intake-v31");assert_eq!(target.query_row::<i64,_,_>("SELECT count(*) FROM workspace_schema_migrations",[],|r|r.get(0)).unwrap(),34);
  let db=Connection::open(&file).unwrap();db.execute_batch("CREATE TABLE unencoded_extra(data TEXT)").unwrap();assert!(convert_control_for_adoption(dir.path()).is_err());let _=json!({"main32plus":"not accepted; original financial/mapping/tasks lineage still requires explicit conversion"});
 }
}
