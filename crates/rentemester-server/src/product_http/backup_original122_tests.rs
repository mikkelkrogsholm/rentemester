use super::*;
#[test]
fn backup_original122_http_query_and_strict_body(){
 assert_eq!(query(None,false),Ok(None));
 assert_eq!(query(Some("asOf=invalid"),false),Ok(Some("invalid".into())));
 for q in ["","asOf=a&asOf=b","actor=u","asOf=x&company=y"]{assert!(query(Some(q),false).is_err());}
 for q in ["","asOf=x"]{assert!(query(Some(q),true).is_err());}
 for body in [r#"{"confirm":true,"actor":"fake"}"#,r#"{"confirm":true,"graceDays":"3"}"#,r#"{"enforced":false}"#]{assert!(serde_json::from_str::<Input>(body).is_err());}
 assert!(serde_json::from_str::<Input>(r#"{"confirm":true,"graceDays":-1,"at":"invalid"}"#).is_ok());
}
