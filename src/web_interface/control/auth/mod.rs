//! Control API の Bearer トークン認証。
//!
//! トークンは **優先順位** と **接続元ポリシー** の二軸:
//!   - 優先順位 ([`TokenSource`]): 環境変数 → 設定 → 生成
//!   - ポリシー ([`ControlApiRuntime::require_token_for`]):
//!     loopback / 非 loopback で Bearer 必須かを切り替える。
//!
//! 典型用途は同一 PC の Tauri GUI からは無認証、LAN からは Bearer 必須、といった分割。
//! ミドルウェアは `/api/v1/control/*` スコープに 1 本だけかかる。

mod middleware;
mod runtime;

pub use middleware::control_api_auth;
pub use runtime::{ControlApiRuntime, TokenSource};
