//! Control API の Bearer トークン解決と `ControlApiRuntime`。
//!
//! トークンは次の **優先順位** で決まる:
//!   - 環境変数 ([`TokenSource::Env`]): `VAC_CONTROL_API_BEARER_TOKEN` → 設定 → 生成
//!   - 設定 ([`TokenSource::Config`]): `[control_api].bearer_token`
//!   - 生成 ([`TokenSource::Generated`]): `<runtime_root>/control-token.txt` に書き込み
//!
//! ポリシー（[`ControlApiRuntime::require_token_for`]）:
//!   接続元が loopback か否かで Bearer 必須かを切り替える。
//!
//! 典型用途は同一 PC の Tauri GUI からは無認証、LAN からは Bearer 必須、など。

use anyhow::{Context, Result};
use base64::Engine as _;
use std::path::PathBuf;

use crate::conf::{Conf, ControlApiConf, ControlTableEntry};
use crate::SharedState;

/// Bearer トークンがどこから来たか（`/whoami` 表示用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
	/// `VAC_CONTROL_API_BEARER_TOKEN` 環境変数。
	Env,
	/// `[control_api].bearer_token` 設定値。
	Config,
	/// 生成して `<runtime_root>/control-token.txt` に書いたもの。
	Generated,
}

/// Control API 認証のランタイム。actix-web の `Data<ControlApiRuntime>` として登録する。
#[derive(Debug, Clone)]
pub struct ControlApiRuntime {
	/// 期待する Bearer トークン。`require_token_*` がいずれも false なら実質未使用。
	pub token: String,
	/// 上記トークンの出自。
	pub token_source: TokenSource,
	/// loopback (127.0.0.1 / ::1) からの接続で Bearer を必須にするか。
	pub require_token_for_loopback: bool,
	/// 非 loopback（LAN 等）からの接続で Bearer を必須にするか。
	pub require_token_for_non_loopback: bool,
	/// 生成トークンを書いたファイルパス。`Generated` のときだけ `Some`。
	pub written_token_file: Option<PathBuf>,
	/// Phase φ-1: Control API Table CRUD の allow-list スナップショット。
	///
	/// `ControlApiConf::tables` を init 時にコピーしたもの。`[[control_api.tables]]` を増やすには
	/// 現状 VAC の再起動 (`POST /api/v1/control/restart`) が必要。GUI からの動的追加は φ 後続で検討。
	pub tables: Vec<ControlTableEntry>,
}

impl ControlApiRuntime {
	/// 設定と共有状態からランタイムを構築する。
	///
	/// 生成トークンは `<runtime_root>/control-token.txt` に書く。ディレクトリは [`crate::runtime::RuntimePaths`] 初期化で確保済み想定。
	/// ファイル ACL は OS 任せ（Windows では `%LOCALAPPDATA%` 配下など）。
	pub async fn init(conf: &Conf, state: &SharedState) -> Result<Self> {
		let policy = conf.control_api.clone().unwrap_or_default();

		let (token, token_source, written_token_file) = resolve_token(&policy, state).await?;

		Ok(Self {
			token,
			token_source,
			require_token_for_loopback: policy.require_token_for_loopback,
			require_token_for_non_loopback: policy.require_token_for_non_loopback,
			written_token_file,
			tables: policy.tables.clone(),
		})
	}

	/// `peer_addr` が loopback かどうかに応じて Bearer を要求するか。
	pub fn require_token_for(&self, is_loopback: bool) -> bool {
		if is_loopback {
			self.require_token_for_loopback
		} else {
			self.require_token_for_non_loopback
		}
	}
}

/// トークン文字列とその出自を決める。
async fn resolve_token(policy: &ControlApiConf, state: &SharedState) -> Result<(String, TokenSource, Option<PathBuf>)> {
	// 1) 環境変数
	if let Ok(t) = std::env::var("VAC_CONTROL_API_BEARER_TOKEN") {
		let t = t.trim().to_string();
		if !t.is_empty() {
			return Ok((t, TokenSource::Env, None));
		}
	}
	// 2) 設定ファイル
	if let Some(t) = policy.bearer_token.as_ref() {
		let t = t.trim().to_string();
		if !t.is_empty() {
			return Ok((t, TokenSource::Config, None));
		}
	}
	// 3) 生成 + 書き込み
	let token = generate_token();
	let runtime_paths = state.read().await.runtime_paths.clone();
	let path = runtime_paths.root.join("control-token.txt");
	// 親ディレクトリは起動時に確保済み想定（`crate::runtime::RuntimePaths` 初期化）。
	std::fs::write(&path, &token).with_context(|| format!("Control API トークンファイルの書き込みに失敗: {:?}", path))?;
	Ok((token, TokenSource::Generated, Some(path)))
}

/// 32 byte 乱数を URL-safe Base64 (no pad) にしたトークン。
fn generate_token() -> String {
	let mut bytes = [0u8; 32];
	for b in &mut bytes {
		*b = rand::random::<u8>();
	}
	base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
