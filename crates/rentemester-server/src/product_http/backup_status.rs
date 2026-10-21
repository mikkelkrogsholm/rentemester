use super::*;
use rentemester_application::backup_status::Action;
#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct Input{confirm:bool,enforced:bool,grace_days:u32}
pub(super) async fn handle(State(state):State<Product>,request:Request)->Response{
 let mutation=request.method()==axum::http::Method::POST;
 let cookie=match proof(&state,&request,mutation){Ok(v)=>v,Err(r)=>return *r};
 if !selected(&state,&request){return refused(StatusCode::NOT_FOUND,"company_not_found");}
 if request.uri().query().is_some()||request.headers().contains_key("content-encoding"){return refused(StatusCode::BAD_REQUEST,"invalid_input");}
 let slug=request.uri().path().split('/').nth(3).unwrap_or("").to_owned();
 let action=if mutation{
  if !one(request.headers(),"content-type").is_some_and(|v|v.eq_ignore_ascii_case("application/json")){return refused(StatusCode::BAD_REQUEST,"invalid_input");}
  let Ok(bytes)=to_bytes(request.into_body(),4096).await else{return refused(StatusCode::PAYLOAD_TOO_LARGE,"invalid_input")};
  let Ok(i)=serde_json::from_slice::<Input>(&bytes)else{return refused(StatusCode::BAD_REQUEST,"invalid_input")};
  if !i.confirm||i.grace_days>365{return refused(StatusCode::BAD_REQUEST,"invalid_input");}
  Action::Configure{enforced:i.enforced,grace_days:i.grace_days}
 }else{if to_bytes(request.into_body(),0).await.is_err(){return refused(StatusCode::BAD_REQUEST,"invalid_input");}Action::Read};
 let Ok(permit)=state.workers.clone().try_acquire_owned()else{return refused(StatusCode::SERVICE_UNAVAILABLE,"busy")};
 let result=tokio::task::spawn_blocking(move||{let _permit=permit;let mut owner=state.native.lock_company(&slug).map_err(|_|())?;owner.backup_status(&cookie,&SystemClock,action).map_err(|_|())}).await;
 match result{Ok(Ok(attempt))=>match super::documents::outcome(attempt){Ok(value)=>response(StatusCode::OK,value),Err(r)=>*r},_=>refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unconfirmed")}
}
