//! Frozen original backup HTTP boundary, no replay of configuration POSTs.
use super::*;
use rentemester_application::backup_original122::{Action,Input};
fn query(raw:Option<&str>,mutation:bool)->Result<Option<String>,()> {
 if mutation{return if raw.is_none(){Ok(None)}else{Err(())};}
 let mut value=None;
 for (k,v) in url::form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {if k!="asOf"||value.is_some(){return Err(());}value=Some(v.into_owned());}
 if raw==Some(""){return Err(());} Ok(value)
}
pub(super) async fn handle(State(state):State<Product>,request:Request)->Response{
 let mutation=request.method()==axum::http::Method::POST;
 let credential=match documents::credential(&state,&request,mutation){Ok(v)=>v,Err(r)=>return *r};
 let date=match query(request.uri().query(),mutation){Ok(v)=>v,Err(())=>return refused(StatusCode::BAD_REQUEST,"invalid_query")};
 if ["content-encoding","transfer-encoding","idempotency-key"].iter().any(|h|request.headers().contains_key(*h)){return refused(StatusCode::BAD_REQUEST,"invalid_input");}
 let slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
 let action=if mutation {
 if !one(request.headers(),"content-type").is_some_and(|v|v.eq_ignore_ascii_case("application/json")){return refused(StatusCode::BAD_REQUEST,"invalid_input");}
 let Ok(bytes)=to_bytes(request.into_body(),4096).await else{return refused(StatusCode::PAYLOAD_TOO_LARGE,"invalid_input")};
 let Ok(input)=serde_json::from_slice::<Input>(&bytes)else{return refused(StatusCode::BAD_REQUEST,"invalid_input")};
 if !input.confirm{return refused(StatusCode::BAD_REQUEST,"confirmation_required");} Action::Configure(input)
 }else {if !to_bytes(request.into_body(),1).await.is_ok_and(|b|b.is_empty()){return refused(StatusCode::BAD_REQUEST,"invalid_input");}Action::Status(date)};
 let Ok(permit)=state.workers.clone().try_acquire_owned()else{return refused(StatusCode::SERVICE_UNAVAILABLE,"busy")};
 let result=tokio::task::spawn_blocking(move||{
 let _permit=permit;
 let mut owner=state.native.lock_company(&slug).map_err(|_|Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable")))?;
 match credential {
 documents::Credential::Human(cookie)=>{let attempt=owner.backup_original122(&cookie,&SystemClock,action).map_err(|_|Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable")))?;if attempt.retired{return Err(Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unconfirmed")));}documents::outcome(attempt)},
 documents::Credential::Service(key)=>{let attempt=owner.backup_original122_service(&key,&SystemClock,action).map_err(|_|Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable")))?;if attempt.retired{return Err(Box::new(refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unconfirmed")));}documents::service_outcome(attempt)}
 }
 }).await;
 match result {Ok(Ok(core))=>response(StatusCode::OK,json!({"ok":true,(if mutation{"backupLock"}else{"backupStatus"}):core})),Ok(Err(r))=>*r,Err(_)=>refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unconfirmed")}
}
#[cfg(test)]#[path="backup_original122_tests.rs"]mod tests;
#[path="backup_original122/original717.rs"]mod artifact717;
pub(super) async fn original717(state:State<Product>,request:Request)->Response{artifact717::handle(state,request).await}
