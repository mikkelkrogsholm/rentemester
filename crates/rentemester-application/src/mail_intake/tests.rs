use super::*;
#[test]
fn aliases_follow_source_grammar_and_empty_deletion() {
    assert_eq!(
        normalize_alias(Some("  Synthetic.Inbox-1  ")).unwrap(),
        Some("synthetic.inbox-1".into())
    );
    assert_eq!(normalize_alias(None).unwrap(), None);
    assert_eq!(normalize_alias(Some("")).unwrap(), None);
    for value in ["ab", "-abc", "abc-", "a@b", "æbc", "a\nb", "   "] {
        assert!(normalize_alias(Some(value)).is_err());
    }
}
#[test]
fn config_envelope_and_audit_proof_never_expose_config_secrets() {
    let config = json!({"host":"imap.example.invalid","port":993,"username":"synthetic","password":"private-test-password"});
    let runtime = imap::Config::parse(&serde_json::to_vec(&config).unwrap()).unwrap();
    let bytes=serde_json::to_vec(&json!({"format":"rentemester-imap-config/1","epoch":"synthetic-epoch","nonce":"a".repeat(43),"config":config})).unwrap();
    let grant = Grant {
        epoch: "synthetic-epoch".into(),
        file_hash: sha(&bytes),
        status: serde_json::to_value(runtime.status()).unwrap(),
        grantor: "synthetic-owner".into(),
        authority: "b".repeat(64),
        service: None,
    };
    let audit = serde_json::to_string(&grant).unwrap();
    assert!(!audit.contains("private-test-password"));
    assert!(!audit.contains("nonce"));
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert_ne!(sha(&changed), grant.file_hash);
    let decoded: Envelope = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.format, "rentemester-imap-config/1");
    let recovered: Grant = serde_json::from_str(&audit).unwrap();
    assert!(recovered == grant);
}

#[cfg(target_os = "macos")]
mod backup_lock {
    use super::*;
    use rentemester_runtime::{
        auth::{AuthError, CookieProfile, VerifiedCookie},
        namespace::NamespaceProfile,
    };
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000)
        }
    }
    struct Fixture {
        _dir: tempfile::TempDir,
        control: Connection,
        company: Connection,
        guard: NamespaceGuard,
        cookies: CookieVerifier,
        proof: VerifiedCookie,
        key: ServiceKey,
        credential: String,
    }
    fn selection() -> CompanySelection<'static> {
        CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        }
    }
    fn context(proof: &VerifiedCookie) -> Context<'_> {
        Context {
            cookie: proof,
            clock: &Fixed,
            selection: selection(),
        }
    }
    fn config() -> Value {
        json!({"host":"imap.example.invalid","port":993,"username":"synthetic","password":"synthetic-private-secret"})
    }
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("workspace");
        let uid = String::from_utf8(
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse()
        .unwrap();
        let guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic mail lock",
            "2026-01-01T00:00:00.000Z",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let mut control =
            Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
        let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
        for db in [&control, &company] {
            db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
                .unwrap();
        }
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic mail lock",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
        company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(id,slug,name,currency,ledger_account_no)VALUES(1,'synthetic','Synthetic','DKK','B');INSERT INTO bank_transactions(transaction_date,text,amount,currency,bank_account_id)VALUES('2025-12-01','Synthetic historical activity',1,'DKK',1)").unwrap();
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let issued = cookies.issue_session("synthetic-token").unwrap();
        let proof = cookies.verify(Some(issued.request_header()), None).unwrap();
        let receipt = crate::service_lifecycle::execute(
            &mut control,
            &cookies,
            &Fixed,
            &proof,
            &crate::service_lifecycle::Operation::Create {
                display_name: "Synthetic mailbox service".into(),
                operation_id: None,
                expires_in_seconds: None,
            },
        )
        .unwrap();
        let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
        let credential = issued["credentialId"].as_str().unwrap().to_owned();
        let subject = issued["serviceAccountId"].as_str().unwrap();
        let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
        control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','owner','user:u')",[subject]).unwrap();
        control
            .execute("UPDATE apikey SET remaining=30 WHERE id=?1", [&credential])
            .unwrap();
        let mut f = Fixture {
            _dir: dir,
            control,
            company,
            guard,
            cookies,
            proof,
            key,
            credential,
        };
        execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.cookies,
            context(&f.proof),
            Some(&f.key),
            Action::Configure(&config()),
        )
        .unwrap();
        f
    }
    fn lock(f: &mut Fixture) {
        let ct = f.control.transaction().unwrap();
        let lt = f.company.transaction().unwrap();
        rentemester_storage::backup_state::configure(
            &ct,
            &lt,
            "a",
            true,
            0,
            "user:u",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        lt.commit().unwrap();
        ct.commit().unwrap();
    }
    fn usage(f: &Fixture) -> (i64, i64) {
        f.control
            .query_row(
                "SELECT remaining,COALESCE(requestCount,0) FROM apikey WHERE id=?1",
                [&f.credential],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }
    #[test]
    fn locked_mail_configuration_alias_and_new_admissions_have_no_durable_effects() {
        let mut f = fixture();
        lock(&mut f);
        let original = f.guard.read_imap_config().unwrap();
        let before = (f.control.total_changes(), f.company.total_changes());
        let mut changed = config();
        changed["password"] = json!("different-private-secret");
        for action in [
            Action::Configure(&changed),
            Action::DeleteConfig,
            Action::Alias(Some("synthetic-new")),
        ] {
            assert_eq!(
                execute(
                    &mut f.control,
                    &mut f.company,
                    &mut f.guard,
                    &f.cookies,
                    context(&f.proof),
                    Some(&f.key),
                    action
                )
                .unwrap_err()
                .error
                .code,
                "backup_overdue"
            );
            assert_eq!(f.guard.read_imap_config().unwrap(), original);
            assert_eq!(
                (f.control.total_changes(), f.company.total_changes()),
                before
            );
        }
        execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.cookies,
            context(&f.proof),
            Some(&f.key),
            Action::Status,
        )
        .unwrap();
        assert_eq!(
            prepare(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.cookies,
                context(&f.proof)
            )
            .err()
            .unwrap()
            .error
            .code,
            "backup_overdue"
        );
        let before_usage = usage(&f);
        assert_eq!(
            prepare_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.key,
                &Fixed,
                selection()
            )
            .err()
            .unwrap()
            .error
            .code,
            "backup_overdue"
        );
        assert_eq!(usage(&f), before_usage);
        assert!(f.control.is_autocommit() && f.company.is_autocommit());
    }
    #[test]
    fn current_human_and_grant_authority_refuse_before_the_backup_lock() {
        let mut f = fixture();
        lock(&mut f);
        f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')").unwrap();
        let original = f.guard.read_imap_config().unwrap();
        let before = f.company.total_changes();
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.cookies,
                context(&f.proof),
                Some(&f.key),
                Action::DeleteConfig
            )
            .unwrap_err()
            .error
            .code,
            "access_denied"
        );
        assert_eq!(
            prepare(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.cookies,
                context(&f.proof)
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        let before_usage = usage(&f);
        assert_eq!(
            prepare_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.key,
                &Fixed,
                selection()
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        assert_eq!(usage(&f), before_usage);
        assert_eq!(f.company.total_changes(), before);
        assert_eq!(f.guard.read_imap_config().unwrap(), original);
    }
    #[test]
    fn already_admitted_failure_and_received_observation_finish_after_lock_activation() {
        let mut f = fixture();
        let human = prepare(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.cookies,
            context(&f.proof),
        )
        .unwrap()
        .value;
        let admitted = prepare_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.key,
            &Fixed,
            selection(),
        )
        .unwrap()
        .value;
        let epoch = admitted.grant.epoch.clone();
        let after_admission = usage(&f);
        assert_eq!(after_admission.0, 29);
        lock(&mut f);
        let failed = finish(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.cookies,
            context(&f.proof),
            human,
            Err(imap::Failure::Timeout),
        )
        .unwrap()
        .value;
        assert_eq!(failed["code"], "imap_timeout");
        let batch = imap::Batch {
            uid_validity: 7,
            messages: vec![imap::Message {
                uid: 1,
                raw: b"Synthetic invalid received EML retained for review".to_vec(),
            }],
            has_more: false,
            refusals: vec![],
        };
        let received = finish_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            &f.key,
            &Fixed,
            selection(),
            admitted,
            Ok(batch),
        )
        .unwrap()
        .value;
        assert_eq!(received["ok"], true);
        assert_eq!(usage(&f), after_admission);
        let ct = f.control.transaction().unwrap();
        assert_eq!(
            store::cursor(&ct, "a", &epoch, "INBOX").unwrap(),
            Some((7, 1))
        );
        ct.rollback().unwrap();
        assert_eq!(
            f.company
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM audit_log WHERE event_type='native_mail_poll'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            2
        );
        assert_eq!(
            prepare_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &f.key,
                &Fixed,
                selection()
            )
            .err()
            .unwrap()
            .error
            .code,
            "backup_overdue"
        );
        assert_eq!(usage(&f), after_admission);
    }
}
