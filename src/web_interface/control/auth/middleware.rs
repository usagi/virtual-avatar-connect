//! Control API の認証ミドルウェア（`control_api_auth`）。

use actix_web::body::{BoxBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::web::Data;
use actix_web::{Error, HttpResponse};

use super::runtime::ControlApiRuntime;

/// actix-web `middleware::from_fn` 用ハンドラ。
///
/// 接続元 `peer_addr` が loopback / 非 loopback かに応じてポリシーどおり Bearer を検証する。
pub async fn control_api_auth(
	req: ServiceRequest,
	next: Next<impl MessageBody + 'static>,
) -> std::result::Result<ServiceResponse<BoxBody>, Error> {
	let Some(runtime) = req.app_data::<Data<ControlApiRuntime>>().cloned() else {
		log::error!("《ControlAPI》 app_data に ControlApiRuntime がありません");
		return Ok(req.into_response(unauthorized("server misconfigured")));
	};

	let is_loopback = req.peer_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
	let require_token = runtime.require_token_for(is_loopback);

	if require_token {
		// 1) Authorization: Bearer <token>（HTTP / fetch 用）
		let from_header = req
			.headers()
			.get("Authorization")
			.and_then(|v| v.to_str().ok())
			.and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
			.map(|s| s.trim().to_string());

		// 2) ?token=<token>（ブラウザ WebSocket 等で `Authorization` が付けにくい場合のフォールバック）
		let from_query = extract_query_token(req.query_string());

		let provided = from_header.or(from_query).unwrap_or_default();
		if !constant_time_eq(provided.as_bytes(), runtime.token.as_bytes()) {
			log::warn!(
				"《ControlAPI》 Bearer 不一致: peer={:?} loopback={} path={}",
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

/// query string から `token=...` / `access_token=...` を取り、percent-decode する。
///
/// WebSocket で `new WebSocket("ws://.../events?token=xxx")` とする場合など、
/// `Authorization` が付けられないクライアント向け。HTTP GET のクエリにも使われる。
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

/// 定時間比較。長さが違っても分岐タイミングで内容が漏れないよう `==` だけにしない。
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

	use crate::conf::ControlTableEntry;
	use super::super::runtime::{ControlApiRuntime, TokenSource};

	/// テスト用に `peer_addr` とポリシーを切り替えたランタイムを組む。
	/// ミドルウェア結合テストでは `TestRequest` に `peer_addr` を載せる。
	fn make_runtime(loopback_req: bool, non_loopback_req: bool) -> ControlApiRuntime {
		ControlApiRuntime {
			token: "correct-token-xxxxxxxx".to_string(),
			token_source: TokenSource::Generated,
			require_token_for_loopback: loopback_req,
			require_token_for_non_loopback: non_loopback_req,
			written_token_file: None,
			tables: Vec::<ControlTableEntry>::new(),
		}
	}

	async fn exercise(runtime: ControlApiRuntime, peer: Option<SocketAddr>, authorization: Option<&str>) -> u16 {
		exercise_uri(runtime, peer, authorization, "/g/ok").await
	}

	async fn exercise_uri(runtime: ControlApiRuntime, peer: Option<SocketAddr>, authorization: Option<&str>, uri: &str) -> u16 {
		let app = test::init_service(
			App::new().app_data(Data::new(runtime)).service(
				web::scope("/g")
					.wrap(from_fn(control_api_auth))
					.route("/ok", web::get().to(|| async { HttpResponse::Ok().body("ok") })),
			),
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
		assert_eq!(exercise(rt, Some(lan()), Some("Bearer correct-token-xxxxxxxx")).await, 200);
	}

	#[actix_web::test]
	async fn lan_token_required_with_wrong_bearer_rejects() {
		let rt = make_runtime(false, true);
		assert_eq!(exercise(rt, Some(lan()), Some("Bearer WRONG-token-xxxxxxxx")).await, 401);
	}

	#[actix_web::test]
	async fn loopback_paranoid_policy_requires_token() {
		let rt = make_runtime(true, true);
		assert_eq!(exercise(rt.clone(), Some(loopback()), None).await, 401);
		assert_eq!(exercise(rt, Some(loopback()), Some("Bearer correct-token-xxxxxxxx")).await, 200);
	}

	#[actix_web::test]
	async fn full_trust_lan_policy_allows_without_token() {
		// require_token_for_non_loopback = false → LAN からもトークン不要
		let rt = make_runtime(false, false);
		assert_eq!(exercise(rt, Some(lan()), None).await, 200);
	}

	#[actix_web::test]
	async fn case_insensitive_bearer_prefix() {
		let rt = make_runtime(false, true);
		assert_eq!(exercise(rt, Some(lan()), Some("bearer correct-token-xxxxxxxx")).await, 200);
	}

	#[actix_web::test]
	async fn lan_token_via_query_string_passes() {
		// Authorization なしで token=... のみ（WebSocket upgrade 想定）
		let rt = make_runtime(false, true);
		assert_eq!(exercise_uri(rt, Some(lan()), None, "/g/ok?token=correct-token-xxxxxxxx").await, 200);
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
		assert_eq!(exercise_uri(rt, Some(lan()), None, "/g/ok?token=WRONG").await, 401);
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
		// 不正な percent シーケンスはそのまま残す
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
