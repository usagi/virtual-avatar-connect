//! Phase M0: `[motion]` — VMC 生 UDP パススルー等（Flowgraph 非依存）。
//!
//! 正本のロードマップ: [`docs/roadmap/phase-mu-vmc-motion-m0.md`](../../docs/roadmap/phase-mu-vmc-motion-m0.md)

use serde::{Deserialize, Serialize};

/// `conf.toml` の `[motion]` ルート。省略時は `None`（motion タスクは起動しない）。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct MotionConf {
	/// VMC 互換の **生 UDP パススルー**（受信 → そのまま複数宛先へ `send_to`）。
	#[serde(default)]
	pub vmc_passthrough: Vec<VmcPassthroughSpec>,
}

/// `[[motion.vmc_passthrough]]` 1 エントリ。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VmcPassthroughSpec {
	/// `false` のときこのエントリは無視される（既定 `true`）。
	#[serde(default = "crate::utility::bool_true")]
	pub enabled: bool,
	/// 受信 UDP の bind 先。例: `"0.0.0.0:39539"` / `"127.0.0.1:39539"`。
	pub bind: String,
	/// 転送先ごとの `"host:port"`。空のときこのエントリは起動しない（警告ログ）。
	#[serde(default)]
	pub forward_to: Vec<String>,
	/// ログ用の短い識別子。省略・空のときは `bind` が文脈として使われる。
	#[serde(default)]
	pub label: Option<String>,
}
