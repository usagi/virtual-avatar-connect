//! `POST /shutdown` のリクエスト・レスポンス型。

use serde::{Deserialize, Serialize};

/// `POST /api/v1/control/shutdown` の body。現状フィールドは無いが、将来の拡張余地として
/// `graceful_ms`（cleanup 完了までの最大待ち時間ヒント）を受け付けられるようにする。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShutdownRequest {
	/// 予約済み。現時点では無視する（cleanup の各段は内部で固定値を使う）。
	#[serde(default)]
	pub graceful_ms: Option<u64>,
}

/// `POST /api/v1/control/shutdown` のレスポンス。
#[derive(Debug, Serialize)]
pub struct ShutdownResponse {
	/// 常に `"shutting_down"`。broker.trigger が冪等なので 2 回目以降でも同じ文字列を返す。
	pub status: &'static str,
	/// 停止要求を受け付けた側（呼び出し先）の PID。デバッグ用。
	pub current_pid: u32,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn shutdown_request_accepts_empty_body() {
		let parsed: ShutdownRequest = serde_json::from_str("{}").unwrap();
		assert!(parsed.graceful_ms.is_none());
	}

	#[test]
	fn shutdown_request_partial() {
		let parsed: ShutdownRequest = serde_json::from_str(r#"{"graceful_ms":500}"#).unwrap();
		assert_eq!(parsed.graceful_ms, Some(500));
	}
}
