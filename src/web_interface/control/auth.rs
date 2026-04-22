//! Control API ? Bearer ????????????????
//!
//! ??????**???** ? **????** ????:
//!   - ??? ([`TokenSource`]): ???? > ?????? > ????
//!   - ???? ([`ControlApiRuntime::require_token_for_loopback`] / `..._non_loopback`):
//!     ??????? loopback ??????????
//!
//! ???????????? PC Tauri ?????????? LAN ??????? Bearer ???????
//! ??????????? 1 ???????????

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::web::Data;
use actix_web::{Error, HttpResponse};
use anyhow::{Context, Result};
use base64::Engine as _;
use std::path::PathBuf;

use crate::conf::{Conf, ControlApiConf};
use crate::SharedState;

/// Bearer ??????????? `/whoami` ???????
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
 /// `VAC_CONTROL_API_BEARER_TOKEN` ???????????
 Env,
 /// `[control_api].bearer_token` ???????
 Config,
 /// ???????????`<runtime_root>/control-token.txt` ?????????
 Generated,
}

/// Control API ?????????actix-web ? `Data<ControlApiRuntime>` ?????????????
#[derive(Debug, Clone)]
pub struct ControlApiRuntime {
 /// ????? Bearer ??????????????? require_token ? false ????????
 pub token: String,
 /// ?????/?????
 pub token_source: TokenSource,
 /// loopback (127.0.0.1 / ::1) ?????? Bearer ?????????
 pub require_token_for_loopback: bool,
 /// ? loopback (LAN ?) ?????? Bearer ?????????
 pub require_token_for_non_loopback: bool,
 /// ?????????????????????????`Generated` ????? `Some`??
 pub written_token_file: Option<PathBuf>,
}

impl ControlApiRuntime {
 /// ????????????????????????
 ///
 /// ????????? `<runtime_root>/control-token.txt` ??????????????
 /// ??????????????? OS ???? ACL ?????Windows `%LOCALAPPDATA%` ??????????
 pub async fn init(conf: &Conf, state: &SharedState) -> Result<Self> {
  let policy = conf.control_api.clone().unwrap_or_default();

  let (token, token_source, written_token_file) = resolve_token(&policy, state).await?;

  Ok(Self {
   token,
   token_source,
   require_token_for_loopback: policy.require_token_for_loopback,
   require_token_for_non_loopback: policy.require_token_for_non_loopback,
   written_token_file,
  })
 }

 /// `peer_addr` ? loopback ??????????????? Bearer ?????????
 pub fn require_token_for(&self, is_loopback: bool) -> bool {
  if is_loopback {
   self.require_token_for_loopback
  } else {
   self.require_token_for_non_loopback
  }
 }
}

/// ?????????????????????????????
async fn resolve_token(
 policy: &ControlApiConf,
 state: &SharedState,
) -> Result<(String, TokenSource, Option<PathBuf>)> {
 // 1) ?????????
 if let Ok(t) = std::env::var("VAC_CONTROL_API_BEARER_TOKEN") {
  let t = t.trim().to_string();
  if !t.is_empty() {
   return Ok((t, TokenSource::Env, None));
  }
 }
 // 2) ??????
 if let Some(t) = policy.bearer_token.as_ref() {
  let t = t.trim().to_string();
  if !t.is_empty() {
   return Ok((t, TokenSource::Config, None));
  }
 }
 // 3) ???? + ????
 let token = generate_token();
 let runtime_paths = state.read().await.runtime_paths.clone();
 let path = runtime_paths.root.join("control-token.txt");
 // ?????????????????????????? RuntimePaths ?????????
 std::fs::write(&path, &token)
  .with_context(|| format!("Control API ????????????????: {:?}", path))?;
 Ok((token, TokenSource::Generated, Some(path)))
}

/// 32 byte ????? URL-safe Base64 (no pad) ?????????
fn generate_token() -> String {
 let mut bytes = [0u8; 32];
 for b in &mut bytes {
  *b = rand::random::<u8>();
 }
 base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// actix-web `middleware::from_fn` ??????
///
/// ???????? `peer_addr` ?? loopback/? loopback ?????policy ???? Bearer ??????
pub async fn control_api_auth(
 req: ServiceRequest,
 next: Next<impl MessageBody + 'static>,
) -> std::result::Result<ServiceResponse<BoxBody>, Error> {
 let Some(runtime) = req.app_data::<Data<ControlApiRuntime>>().cloned() else {
  log::error!("?ControlAPI? ControlApiRuntime ? app_data ????????????");
  return Ok(req.into_response(unauthorized("server misconfigured")));
 };

 let is_loopback = req.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
 let require_token = runtime.require_token_for(is_loopback);

 if require_token {
  // 1) Authorization: Bearer <token>?HTTP / fetch ??
  let from_header = req
   .headers()
   .get("Authorization")
   .and_then(|v| v.to_str().ok())
   .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
   .map(|s| s.trim().to_string());

  // 2) ?token=<token> ????????? WebSocket ??????????????WS upgrade ? fallback?
  let from_query = extract_query_token(req.query_string());

  let provided = from_header.or(from_query).unwrap_or_default();
  if !constant_time_eq(provided.as_bytes(), runtime.token.as_bytes()) {
   log::warn!(
    "?ControlAPI?Bearer ????: peer={:?} loopback={} path={}",
    req.peer_addr(),
    is_loopback,
    req.path()
   );
   return Ok(req.into_response(unauthorized("invalid bearer token")));
  }
 }

 next.call(req).await.map(|res| res.map_into_boxed_body())
}

fn unauthorized(reason: &str) -> HttpResponse {
 HttpResponse::Unauthorized()
  .insert_header(("WWW-Authenticate", "Bearer"))
  .content_type("application/json")
  .body(format!(r#"{{"error":"unauthorized","reason":"{}"}}"#, reason))
}

/// query string ?? `token=...` / `access_token=...` ??????percent-decoded ???????
///
/// WebSocket ? `new WebSocket("ws://.../events?token=xxx")` ????????????????????
/// `Authorization` ????????????????????????HTTP GET ???????
fn extract_query_token(query: &str) -> Option<String> {
 for part in query.split('&') {
  let mut it = part.splitn(2, '=');
  let (k, v) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
  if k == "token" || k == "access_token" {
   return Some(percent_decode(v));
  }
 }
 None
}

fn percent_decode(s: &str) -> String {
 let bytes = s.as_bytes();
 let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
 let mut i = 0;
 while i < bytes.len() {
  if bytes[i] == b'%' && i + 2 < bytes.len() {
   let hi = from_hex(bytes[i + 1]);
   let lo = from_hex(bytes[i + 2]);
   if let (Some(h), Some(l)) = (hi, lo) {
    out.push((h << 4) | l);
    i += 3;
    continue;
   }
  } else if bytes[i] == b'+' {
   out.push(b' ');
   i += 1;
   continue;
  }
  out.push(bytes[i]);
  i += 1;
 }
 String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
 match b {
  b'0'..=b'9' => Some(b - b'0'),
  b'a'..=b'f' => Some(b - b'a' + 10),
  b'A'..=b'F' => Some(b - b'A' + 10),
  _ => None,
 }
}

/// ?????????????`==` ??? return ????????
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
 if a.len() != b.len() {
  return false;
 }
 let mut acc: u8 = 0;
 for (x, y) in a.iter().zip(b.iter()) {
  acc |= x ^ y;
 }
 acc == 0
}

#[cfg(test)]
mod tests {
 use super::*;
 use actix_web::body::to_bytes;
 use actix_web::middleware::from_fn;
 use actix_web::test::{self, TestRequest};
 use actix_web::{web, App, HttpResponse};
 use std::net::SocketAddr;

 /// ??????? peer_addr ? policy ???????????????????
 /// middleware ????????????TestRequest ? peer_addr ??????????????
 fn make_runtime(loopback_req: bool, non_loopback_req: bool) -> ControlApiRuntime {
  ControlApiRuntime {
   token: "correct-token-xxxxxxxx".to_string(),
   token_source: TokenSource::Generated,
   require_token_for_loopback: loopback_req,
   require_token_for_non_loopback: non_loopback_req,
   written_token_file: None,
  }
 }

 async fn exercise(
  runtime: ControlApiRuntime,
  peer: Option<SocketAddr>,
  authorization: Option<&str>,
 ) -> u16 {
  exercise_uri(runtime, peer, authorization, "/g/ok").await
 }

 async fn exercise_uri(
  runtime: ControlApiRuntime,
  peer: Option<SocketAddr>,
  authorization: Option<&str>,
  uri: &str,
 ) -> u16 {
  let app = test::init_service(
   App::new()
    .app_data(Data::new(runtime))
    .service(web::scope("/g").wrap(from_fn(control_api_auth)).route(
     "/ok",
     web::get().to(|| async { HttpResponse::Ok().body("ok") }),
    )),
  )
  .await;

  let mut req = TestRequest::get().uri(uri);
  if let Some(addr) = peer {
   req = req.peer_addr(addr);
  }
  if let Some(auth) = authorization {
   req = req.insert_header(("Authorization", auth));
  }
  let resp = test::call_service(&app, req.to_request()).await;
  let status = resp.status().as_u16();
  let _ = to_bytes(resp.into_body()).await;
  status
 }

 fn loopback() -> SocketAddr {
  "127.0.0.1:12345".parse().unwrap()
 }
 fn lan() -> SocketAddr {
  "192.168.1.42:12345".parse().unwrap()
 }

 #[actix_web::test]
 async fn loopback_no_token_required_allows_without_header() {
  let rt = make_runtime(false, true);
  assert_eq!(exercise(rt, Some(loopback()), None).await, 200);
 }

 #[actix_web::test]
 async fn lan_token_required_without_header_rejects() {
  let rt = make_runtime(false, true);
  assert_eq!(exercise(rt, Some(lan()), None).await, 401);
 }

 #[actix_web::test]
 async fn lan_token_required_with_correct_bearer_passes() {
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise(rt, Some(lan()), Some("Bearer correct-token-xxxxxxxx")).await,
   200
  );
 }

 #[actix_web::test]
 async fn lan_token_required_with_wrong_bearer_rejects() {
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise(rt, Some(lan()), Some("Bearer WRONG-token-xxxxxxxx")).await,
   401
  );
 }

 #[actix_web::test]
 async fn loopback_paranoid_policy_requires_token() {
  let rt = make_runtime(true, true);
  assert_eq!(exercise(rt.clone(), Some(loopback()), None).await, 401);
  assert_eq!(
   exercise(rt, Some(loopback()), Some("Bearer correct-token-xxxxxxxx")).await,
   200
  );
 }

 #[actix_web::test]
 async fn full_trust_lan_policy_allows_without_token() {
  // require_token_for_non_loopback = false?LAN ???????????????? LAN ???
  let rt = make_runtime(false, false);
  assert_eq!(exercise(rt, Some(lan()), None).await, 200);
 }

 #[actix_web::test]
 async fn case_insensitive_bearer_prefix() {
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise(rt, Some(lan()), Some("bearer correct-token-xxxxxxxx")).await,
   200
  );
 }

 #[actix_web::test]
 async fn lan_token_via_query_string_passes() {
  // Authorization ???????token=... ????? (WebSocket upgrade ???????)
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise_uri(rt, Some(lan()), None, "/g/ok?token=correct-token-xxxxxxxx").await,
   200
  );
 }

 #[actix_web::test]
 async fn lan_token_via_access_token_query_passes() {
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise_uri(rt, Some(lan()), None, "/g/ok?access_token=correct-token-xxxxxxxx").await,
   200
  );
 }

 #[actix_web::test]
 async fn lan_token_via_wrong_query_rejects() {
  let rt = make_runtime(false, true);
  assert_eq!(
   exercise_uri(rt, Some(lan()), None, "/g/ok?token=WRONG").await,
   401
  );
 }

 #[test]
 fn percent_decode_basic() {
  assert_eq!(percent_decode("abc"), "abc");
  assert_eq!(percent_decode("a%20b"), "a b");
  assert_eq!(percent_decode("a+b"), "a b");
  // UTF-8 3-byte: U+3042 (HIRAGANA LETTER A) = E3 81 82
  let bytes = [0xE3u8, 0x81, 0x82];
  let expected = std::str::from_utf8(&bytes).unwrap();
  assert_eq!(percent_decode("%E3%81%82"), expected);
  // ??? percent ????????????????
  assert_eq!(percent_decode("%ZZ"), "%ZZ");
 }

 #[test]
 fn extract_query_token_picks_token_or_access_token() {
  assert_eq!(extract_query_token("foo=bar&token=xyz"), Some("xyz".to_string()));
  assert_eq!(extract_query_token("access_token=abc"), Some("abc".to_string()));
  assert_eq!(extract_query_token("nothing=here"), None);
  assert_eq!(extract_query_token(""), None);
 }

 #[test]
 fn constant_time_eq_length_mismatch_rejects() {
  assert!(!constant_time_eq(b"short", b"longer"));
  assert!(constant_time_eq(b"same", b"same"));
  assert!(!constant_time_eq(b"same", b"samE"));
 }
}
