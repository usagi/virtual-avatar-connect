//! Responses API client 本体。
//!
//! # 責務
//!
//! - `POST /v1/responses` を叩いて [`super::types::response::Response`] を返す `create`（non-stream）
//! - `Bearer` 認証 / `OpenAI-Organization` / `OpenAI-Project` ヘッダ
//! - エラー body の truncate を噛ませた構造化エラー
//!
//! ストリーミング（`create_stream`）は **χ-2** で追加予定。

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;

use super::types::request::CreateResponseRequest;
use super::types::response::Response;
use super::util::truncate_error_body;

/// Responses API エンドポイントのホスト URL（末尾スラッシュなし）。
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// 既定タイムアウト（2 分）。ストリーミングを含めても 2 分あれば十分なはず。
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// [`ResponsesClient`] のコンフィグ。
///
/// `crate::*` に依存しないため、VAC 内の `AiConf` からは呼び出し側で
/// 必要フィールドを抽出してこの struct に詰め替える。
#[derive(Debug, Clone)]
pub struct ResponsesClientConfig
{
 pub api_key: String,
 pub base_url: String,
 pub organization: Option<String>,
 pub project: Option<String>,
 pub timeout: Duration,
 /// 冪等キー prefix（`Idempotency-Key` ヘッダ用、option）。
 pub idempotency_prefix: Option<String>,
}

impl Default for ResponsesClientConfig
{
 fn default() -> Self
 {
  Self {
   api_key: String::new(),
   base_url: DEFAULT_BASE_URL.to_string(),
   organization: None,
   project: None,
   timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
   idempotency_prefix: None,
  }
 }
}

/// `/v1/responses` を叩く自前 client。`crate::*` には依存しない。
///
/// 複数クエリで `http` を共有できるよう `Arc` 保持している。
#[derive(Debug, Clone)]
pub struct ResponsesClient
{
 http: Arc<reqwest::Client>,
 cfg: Arc<ResponsesClientConfig>,
}

impl ResponsesClient
{
 /// 内部 `reqwest::Client` を自前で構築する。
 pub fn new(cfg: ResponsesClientConfig) -> Result<Self, ResponsesClientError>
 {
  let http = reqwest::Client::builder()
   .timeout(cfg.timeout)
   .build()
   .map_err(|e| ResponsesClientError::Build(e.to_string()))?;
  Ok(Self {
   http: Arc::new(http),
   cfg: Arc::new(cfg),
  })
 }

 /// 既存の `reqwest::Client` を使い回す。VAC 内で http client を共有したい場合に使う。
 pub fn with_http(http: Arc<reqwest::Client>, cfg: ResponsesClientConfig) -> Self
 {
  Self {
   http,
   cfg: Arc::new(cfg),
  }
 }

 pub fn config(&self) -> &ResponsesClientConfig
 {
  &self.cfg
 }

 pub fn http(&self) -> &reqwest::Client
 {
  &self.http
 }

 /// `POST /v1/responses`（non-stream）。
 ///
 /// 呼び出し時に `request.stream` は強制的に `Some(false)` に上書きされる。
 pub async fn create(
  &self,
  mut request: CreateResponseRequest,
 ) -> Result<Response, ResponsesClientError>
 {
  request.stream = Some(false);
  let url = format!("{}/responses", self.cfg.base_url.trim_end_matches('/'));
  let body = serde_json::to_vec(&request)
   .map_err(|e| ResponsesClientError::Encode(e.to_string()))?;

  let mut req = self
   .http
   .post(&url)
   .bearer_auth(&self.cfg.api_key)
   .header(reqwest::header::CONTENT_TYPE, "application/json")
   .body(body);
  if let Some(org) = &self.cfg.organization
  {
   req = req.header("OpenAI-Organization", org);
  }
  if let Some(project) = &self.cfg.project
  {
   req = req.header("OpenAI-Project", project);
  }
  let res = req
   .send()
   .await
   .map_err(|e| ResponsesClientError::Transport(e.to_string()))?;

  let status = res.status();
  let body_text = res
   .text()
   .await
   .map_err(|e| ResponsesClientError::Transport(e.to_string()))?;

  if !status.is_success()
  {
   return Err(ResponsesClientError::Api {
    status: status.as_u16(),
    body: truncate_error_body(&body_text),
   });
  }

  serde_json::from_str::<Response>(&body_text).map_err(|e| ResponsesClientError::Decode {
   error: e.to_string(),
   body: truncate_error_body(&body_text),
  })
 }
}

/// 一度だけ JSON シリアライズしてログに流すためのヘルパ（デバッグ用途）。
/// `log::trace!` で body を見たいときに使う。pub 保持するが公開 API としては補助。
pub fn serialize_compact<T: Serialize>(value: &T) -> Result<String, ResponsesClientError>
{
 serde_json::to_string(value).map_err(|e| ResponsesClientError::Encode(e.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum ResponsesClientError
{
 #[error("failed to build HTTP client: {0}")]
 Build(String),

 #[error("failed to encode request: {0}")]
 Encode(String),

 #[error("transport error: {0}")]
 Transport(String),

 #[error("OpenAI API error ({status}): {body}")]
 Api
 {
  status: u16,
  body: String,
 },

 #[error("failed to decode response: {error} (body: {body})")]
 Decode
 {
  error: String,
  body: String,
 },
}
