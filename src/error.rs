use actix_web::{http::StatusCode, HttpResponse, ResponseError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
 #[error("内部エラーが発生したため処理は中断されました: {0}")]
 InternalError(#[from] anyhow::Error),

 #[error("OpenAI API エラーが発生しました: {0}")]
 OpenAIError(#[from] async_openai::error::OpenAIError),

 #[error("JSON エラーが発生しました: {0}")]
 SerdeJsonError(#[from] serde_json::Error),

 #[error("HTTP/Reqwest エラーが発生しました: {0}")]
 ReqwwestError(#[from] reqwest::Error),

 #[error("Rodio デコーダーでエラーが発生しました: {0}")]
 RodioDecoderError(#[from] rodio::decoder::DecoderError),

 #[error("Rodio ストリームでエラーが発生しました: {0}")]
 RodioStreamError(#[from] rodio::StreamError),

 #[error("Rodio プレイヤーでエラーが発生しました: {0}")]
 RodioPlayError(#[from] rodio::PlayError),

 #[error("IO エラーが発生しました: {0}")]
 IOError(#[from] std::io::Error),

 #[error("システム時間エラーが発生しました: {0}")]
 SystemTimeError(#[from] std::time::SystemTimeError),

 #[error("正規表現エラーが発生しました: {0}")]
 RegexError(#[from] regex::Error),

 #[error("OS-TTS エラーが発生しました: {0}")]
 OsTtsError(#[from] tts::Error),

 #[error("スレッド終了エラーが発生しました: {0}")]
 JoinError(#[from] tokio::task::JoinError),
}

impl ResponseError for Error {
 fn status_code(&self) -> StatusCode {
  match &self {
   Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::OpenAIError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::SerdeJsonError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::ReqwwestError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::RodioDecoderError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::RodioStreamError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::RodioPlayError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::IOError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::SystemTimeError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::RegexError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::OsTtsError(_) => StatusCode::INTERNAL_SERVER_ERROR,
   Self::JoinError(_) => StatusCode::INTERNAL_SERVER_ERROR,
  }
 }

 fn error_response(&self) -> HttpResponse {
  HttpResponse::build(self.status_code()).body(self.to_string())
 }
}

pub type Result<T> = std::result::Result<T, Error>;
