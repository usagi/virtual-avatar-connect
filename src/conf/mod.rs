mod motion;
mod processor_conf;
mod runtime_mode;

pub use anyhow::{bail, Result};
pub use runtime_mode::{
	AiModeOverlay, FlowgraphGroupsModeSpec, ManagedAppsModeDirective, NotificationsModeOverlay,
	RuntimeModeDefinition,
};
pub use motion::{MotionConf, VmcPassthroughSpec};
pub use processor_conf::*;

use crate::ai::AiConf;
use crate::flowgraph::FlowgraphInstanceConfig;
use crate::{utility::bool_true, Arc, Args, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn default_twitch_oauth_redirect_ports() -> Vec<u16> {
	vec![22300, 32300, 42300]
}

fn default_twitch_oauth_timeout_secs() -> u64 {
	600
}

/// Phase ζ-1: OAuth DCF 認可待ちタイムアウトの VAC 既定値（秒）を返す public ヘルパ。
///
/// `TwitchTokenKeySpec.oauth_timeout_secs` が未指定時、broadcaster 以外の key では
/// これをフォールバックとして使う。
pub fn default_twitch_oauth_timeout_secs_value() -> u64 {
	default_twitch_oauth_timeout_secs()
}

pub type SharedConf = Arc<RwLock<Conf>>;

pub const DEFAULT_WEB_UI_ADDRESS: &str = "127.0.0.1:57000";

/// Phase ε-2: `run_with` エントリの **終了処理ポリシー**。
///
/// VAC シャットダウン時／`POST /managed_apps/{id}/stop` 時に、対応する子プロセスに対して
/// どのようにクローズを送り、応答を待ち、最終的に強制終了するかを制御する。
///
/// 既定値は歴史的互換のため **`CloseAndWait`**（WM_CLOSE 送信 → `grace_ms` 経過後 `TerminateProcess`）。
/// ただし OBS / CoeiroInk のように「終了確認ダイアログを出す」アプリでは `CloseAndWait` だと
/// ダイアログ表示中に強制終了されてクラッシュ扱いになるため、`CloseOnly` を推奨する。
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunWithShutdownAction {
	/// 既定。WM_CLOSE / SC_CLOSE 送信 → `grace_ms` 経過後も残っているなら `TerminateProcess`。
	/// シンプルな CLI／サービス系に向く。ダイアログで止まるアプリには向かない。
	#[default]
	CloseAndWait,
	/// WM_CLOSE / SC_CLOSE 送信 → `grace_ms` ポーリング → それでも残っていても **放置** する。
	/// OBS の「未保存変更あり」確認ダイアログのように、ユーザー応答が必要で時間のかかる終了が
	/// 想定されるアプリ向け。VAC は先に exit し、対象アプリは画面に残ったまま自然に終わる。
	CloseOnly,
	/// VAC シャットダウンでは **何もしない**（WM_CLOSE も送らない）。ユーザーが独立に管理するアプリ向け。
	LeaveRunning,
}

/// Phase ε-2: クローズ通知を送るために使う Win32 メッセージの種別。
///
/// ×ボタン押下に最も近いのは `Syscommand`（`WM_SYSCOMMAND` の `SC_CLOSE`）で、アプリ側は
/// DefWindowProc を通じて `WM_CLOSE` に変換して受け取る。両者は多くのアプリで等価だが、
/// `WM_SYSCOMMAND` を hook している実装では差が出ることがあるため選択肢として残す。
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunWithShutdownMethod {
	/// 既定。`SendMessageTimeoutW(WM_SYSCOMMAND, SC_CLOSE)` を使う（×ボタン相当 + タイムアウト付き）。
	#[default]
	Syscommand,
	/// `PostMessageW(WM_CLOSE)`。非同期。`Syscommand` で反応しないアプリの fallback 用途。
	WmClose,
	/// 両方試す: まず `Syscommand` を送り、同時に `WmClose` も送る。対象ウィンドウが片方しか
	/// 処理しないケースの保険。
	Both,
}

/// Phase ε-2: `run_with` エントリごとのシャットダウン設定。TOML 表現。
///
/// 全フィールド optional で、未指定は VAC 既定値（`CloseAndWait` / `Syscommand` / 10_000ms）で埋まる。
/// resolved 後の値は [`crate::managed_app::ShutdownCfg`] を参照。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RunWithShutdownSpec {
	#[serde(default)]
	pub action: Option<RunWithShutdownAction>,
	/// このエントリに対して WM_CLOSE 送信後に exit 完了を何 ms 待つか。未指定なら VAC 既定（10_000）。
	/// `CloseOnly` の場合はこの時間だけポーリングし、未終了でも放置して続行する。
	#[serde(default)]
	pub grace_ms: Option<u64>,
	#[serde(default)]
	pub method: Option<RunWithShutdownMethod>,
	/// Phase ε-3: **アプリ固有のシャットダウン後処理**を識別するキー。
	///
	/// 指定するとクローズ送信後のポーリングループの毎 tick で該当ハンドラが呼ばれ、
	/// アプリ固有の「終了確認ダイアログの "終了" ボタンをクリック」等を試みる（冪等）。
	/// 未指定なら何もしない（既存挙動）。
	///
	/// 現在サポートする値（ホワイトリスト方式）:
	///   - `"coeiroink"` — `#32770` TaskDialog の「終了」ボタン HWND に `BM_CLICK` 送信。
	///     CoeiroInk v2 が出す「終了の確認」ダイアログが対象。
	///
	/// 実装は [`crate::managed_app::app_specific`]。未知の値はログ警告のうえ無視。
	#[serde(default)]
	pub app_specific: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RunWith {
	Command(String),
	CommandIfProcessIsNotRunning {
		command: String,
		if_not_running: Option<String>,
		run_as_admin: Option<bool>,
		working_dir: Option<String>,
		/// `true` のとき、可能な範囲でウィンドウを最小化した状態で起動する（主に Windows。http(s) URL では無視）
		minimized: Option<bool>,
		/// Phase VI-γ-2b: **Managed App** としての安定 ID。GUI の持続ドロワーや `/api/v1/control/managed_apps/:id/*`
		/// で参照する識別子。未指定時は `run-with-<index>` を自動採番（順序依存）。安定な API 操作を望むなら必ず指定する。
		#[serde(default)]
		id: Option<String>,
		/// Phase VI-γ-2b: GUI 表示用のラベル。未指定時は `if_not_running` → ファイル名 → command から自動生成される。
		#[serde(default)]
		label: Option<String>,
		/// Phase ε-2: シャットダウン時の挙動（action / method / grace_ms）。未指定なら既定。
		/// OBS / CoeiroInk など確認ダイアログ付きアプリには `{ action = "close_only", grace_ms = 30000 }` を推奨。
		#[serde(default)]
		shutdown: Option<RunWithShutdownSpec>,
	},
}

impl RunWith {
	/// このエントリの起動コマンド（引数込み生文字列）。
	pub fn command(&self) -> &str {
		match self {
			RunWith::Command(c) => c,
			RunWith::CommandIfProcessIsNotRunning { command, .. } => command,
		}
	}

	/// プロセス名照合に使うマーカー（`if_not_running` と同義）。`Command(..)` だけの形式では `None`。
	///
	/// これが `None` の entry は **Managed App としての状態監視ができない**（=「起動中か」を判定できない）。
	/// そのため API ではそのような entry を `supports_status=false` として扱い、停止/最小化は提供しない。
	pub fn process_marker(&self) -> Option<&str> {
		match self {
			RunWith::Command(_) => None,
			RunWith::CommandIfProcessIsNotRunning { if_not_running, .. } => if_not_running.as_deref(),
		}
	}

	/// Phase VI-γ-2b: 明示された Managed App ID（`id`）。無ければ `None`。
	/// 自動採番込みで確定 ID が欲しい場合は呼び出し側で `run-with-<idx>` を使う。
	pub fn explicit_id(&self) -> Option<&str> {
		match self {
			RunWith::Command(_) => None,
			RunWith::CommandIfProcessIsNotRunning { id, .. } => id.as_deref(),
		}
	}

	/// Phase VI-γ-2b: GUI 表示用ラベル。明示値がなければ `if_not_running` → 実行ファイル名 → command 先頭単語で推定。
	pub fn display_label(&self) -> String {
		match self {
			RunWith::Command(c) => label_from_command(c),
			RunWith::CommandIfProcessIsNotRunning {
				label,
				if_not_running,
				command,
				..
			} => label
				.clone()
				.or_else(|| if_not_running.clone())
				.unwrap_or_else(|| label_from_command(command)),
		}
	}

	pub fn minimized(&self) -> bool {
		match self {
			RunWith::Command(_) => false,
			RunWith::CommandIfProcessIsNotRunning { minimized, .. } => minimized.unwrap_or(false),
		}
	}

	pub fn run_as_admin(&self) -> bool {
		match self {
			RunWith::Command(_) => false,
			RunWith::CommandIfProcessIsNotRunning { run_as_admin, .. } => run_as_admin.unwrap_or(false),
		}
	}

	pub fn working_dir(&self) -> Option<&str> {
		match self {
			RunWith::Command(_) => None,
			RunWith::CommandIfProcessIsNotRunning { working_dir, .. } => working_dir.as_deref(),
		}
	}

	/// Phase ε-2: このエントリに明示されたシャットダウン仕様（raw）。未指定なら `None`。
	/// resolved なデフォルト付きの値は [`crate::managed_app::ShutdownCfg::from_spec`] で得る。
	pub fn shutdown_spec(&self) -> Option<&RunWithShutdownSpec> {
		match self {
			RunWith::Command(_) => None,
			RunWith::CommandIfProcessIsNotRunning { shutdown, .. } => shutdown.as_ref(),
		}
	}
}

/// `command` 文字列の先頭のパスらしき部分からベース名を抜き出して UI 向けラベルにする。
/// 例: `"C:\\...\\COEIROINKv2.exe"` → "COEIROINKv2"、`"steam://rungameid/1234"` → "steam://rungameid/1234"。
fn label_from_command(command: &str) -> String {
	let trimmed = command.trim().trim_matches('"');
	// URL/URI スキームはそのまま
	if trimmed.contains("://") {
		return trimmed.to_string();
	}
	// 最初の空白で区切って引数を捨てる（パスに空白がある場合は " でクォートされている前提）
	let first = trimmed.split_whitespace().next().unwrap_or(trimmed);
	let first = first.trim_matches('"');
	let base = std::path::Path::new(first).file_stem().and_then(|s| s.to_str()).unwrap_or(first);
	base.to_string()
}

/// 《Twitch》 EventSub（WebSocket）。`user_access_token` に **配信者本人**のユーザーアクセストークンが必要（Helix 作成時は App トークン不可）。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TwitchEventSubConfig {
	#[serde(default = "bool_true")]
	pub enabled: bool,
	/// Twitch Client ID。未指定時は VAC ビルドに埋め込んだ既定値を使う。上書き: 環境変数 `VAC_TWITCH_CLIENT_ID`。
	#[serde(default)]
	pub client_id: Option<String>,
	/// 機密クライアント用。環境変数 `VAC_TWITCH_CLIENT_SECRET` が優先。未設定ならパブリッククライアントとして PKCE を使う。
	pub client_secret: Option<String>,
	/// ユーザーアクセストークン。環境変数 `VAC_TWITCH_USER_ACCESS_TOKEN` が優先。
	pub user_access_token: Option<String>,
	/// トークンが未設定・検証失敗時に、ローカル HTTP コールバックで OAuth しブラウザ同意を取る（既定: true）。
	#[serde(default = "bool_true")]
	pub oauth_auto: bool,
	/// OAuth リダイレクト用にバインドする `localhost` ポート（空なら 22300 / 32300 / 42300）。
	#[serde(default = "default_twitch_oauth_redirect_ports")]
	pub oauth_redirect_ports: Vec<u16>,
	/// ブラウザで同意してからコールバックを待つ最大秒数。
	#[serde(default = "default_twitch_oauth_timeout_secs")]
	pub oauth_timeout_secs: u64,
	/// `authorize` に渡すスコープ（スペース区切り）。未指定時は EventSub 用の既定セット。
	pub oauth_scopes: Option<String>,
	/// Device Code Flow のブラウザ起動コマンドテンプレート。`{url}` が認可 URL に置換される。
	///
	/// **Twitch の activate ページは `login_hint` 非対応**なので「このアカウントで認可」を強制する公式手段は無い。
	/// 実用上は、特定アカウントがログイン済みの **ブラウザプロファイル**で開くことで実現する。
	///
	/// 例:
	/// - `"msedge --inprivate {url}"`（プライベートウィンドウで broadcaster セッションを汚さない）
	/// - `"msedge --profile-directory=\"Profile 2\" {url}"`（ボット用プロファイル常駐運用）
	/// - `"firefox -P BotAccount -no-remote {url}"`
	///
	/// 省略時は OS 既定のブラウザで開く。
	pub oauth_browser_command: Option<String>,
	/// 配信者ログイン名。未指定時は IRC の `username` / `twitch_username` と同じ。
	pub broadcaster_login: Option<String>,
	/// 通知を流す VAC チャンネル。未指定時は IRC の `channel_to` と同じ。
	pub channel_to: Option<String>,
	#[serde(default = "bool_true")]
	pub channel_points: bool,
	#[serde(default = "bool_true")]
	pub follow: bool,
	#[serde(default = "bool_true")]
	pub subscribe: bool,
	#[serde(default = "bool_true")]
	pub raid: bool,
	/// `channel.channel_points_custom_reward_redemption.add` 用。未指定なら Helix でカスタム報酬を列挙し、**各 ID に個別購読**する。1 件だけに絞るときだけ指定する。
	pub channel_points_reward_id: Option<String>,

	#[serde(default = "bool_true")]
	pub stream_online: bool,
	#[serde(default = "bool_true")]
	pub stream_offline: bool,
	#[serde(default = "bool_true")]
	pub channel_cheer: bool,
	#[serde(default = "bool_true")]
	pub channel_subscription_gift: bool,
	#[serde(default = "bool_true")]
	pub hype_train_begin: bool,
	#[serde(default = "bool_true")]
	pub hype_train_progress: bool,
	#[serde(default = "bool_true")]
	pub hype_train_end: bool,
	/// リサブのチャットメッセージ付き（`channel.subscription.message`）。
	#[serde(default = "bool_true")]
	pub channel_subscription_message: bool,

	/// ζ-2c: `flowgraph.ingress.twitch_eventsub` が **同一 broadcaster** を購読している場合に
	/// V1 の EventSub ループ（ChannelDatum に 1 行テキストを流す経路）を自動で無効化するのを
	/// 無効化するスイッチ。既定 `false`（＝ ingress ノードを置くと V1 はスキップされる）。
	///
	/// V1 ChannelDatum に依存する既存 `[[processors]]` を残したい移行期だけ `true` にする。
	/// そのとき WebSocket は 2 本張られる（V1 + V2）。
	#[serde(default)]
	pub force_v1_loop: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BrowserSourceConfig {
	/// OBS ブラウザソース用 HTML のルート（相対は cwd）。`GET /output`・`GET /browser-output/...` が参照する。未指定時は `output`。
	pub document_root: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Twitch {
	/// Twitch IRC で接続するアカウントのログイン名。単一チャンネル受信時はこのチャットに join する。
	///
	/// **Phase ζ-1 以降 deprecated**: IRC ingress は `flowgraph.ingress.twitch` ノードの
	/// `login` property へ移行する。設定を残してもエラーにはならないが警告が出る。
	pub username: String,
	/// 受信した発言を流し込む VAC チャンネル名。
	///
	/// **Phase ζ-1 以降 deprecated**: Flowgraph 経路では `fixed_channel` property ないし
	/// 下流ノードの接続先で表現する。`channel_to` は V1 EventSub ループを走らせるときの
	/// 既定 channel としてのみ参照され、未指定時は `username` が使われる（[`Twitch::effective_channel_to`]）。
	#[serde(default)]
	pub channel_to: Option<String>,
	/// `username` のチャットに加えて join する追加チャンネル（GitHub #47）。未指定時は `username` のみ。
	///
	/// **Phase ζ-1 以降 deprecated**: Flowgraph 経路では `channels` property を使う。
	#[serde(default)]
	pub reads: Option<Vec<String>>,
	/// EventSub WebSocket（IRC とは別タスク）。未指定なら起動しない。
	#[serde(default)]
	pub eventsub: Option<TwitchEventSubConfig>,
	/// 《Twitch》 発話中継・モデレーションアクション用のボット/モデレーターアカウント（Phase V）。
	/// 配信者（`eventsub`）とはトークンを別ファイルで持ち、スコープも別枠。
	#[serde(default)]
	pub moderator: Option<TwitchModeratorConfig>,
	/// 《Twitch》 IRC ingress で無視する送信者 login の明示指定（小文字化して比較）。
	///
	/// Phase V-b で AI 発話を `twitch-out` 経由で自分の chat に送るようになったため、放置すると自分のボット投稿を
	/// ingress が読み戻して TTS にかけてしまう（エコー）。これを防ぐには:
	///   - `[twitch.moderator].login` を書けば自動で無視対象に入る
	///   - `TwitchOut` 初期化時に Helix `validate` で解決した自 login も自動で無視対象に入る
	/// このフィールドは **さらに他 bot を明示的に外したい場合** の追加手段。
	#[serde(default)]
	pub ignore_logins: Option<Vec<String>>,

	/// Phase ζ-1: このインスタンスで管理する OAuth token の識別キー群。
	///
	/// 例: `token_keys = ["broadcaster", "moderator", "alt_bot"]`
	///
	/// - 各 key につき 1 本のトークンを `twitch-token.<key>.json` に保存する。
	/// - 設定画面 / CLI の OAuth フローを key 単位で実行してキャッシュを作る。
	/// - Flowgraph の `flowgraph.twitch.get_token` ノードと、本体機能（AI tool 等）の
	///   `token_key` 指定が同じ名前空間を参照する。
	///
	/// 未指定時は後方互換のため `eventsub` / `moderator` 設定の有無から
	/// `["broadcaster", "moderator"]` を推測する（[`Twitch::resolved_token_keys`] 参照）。
	#[serde(default)]
	pub token_keys: Option<Vec<String>>,

	/// Phase ζ-1: `token_keys` で宣言した key ごとの個別設定。
	///
	/// `broadcaster` / `moderator` は shim が既存 `eventsub` / `moderator` ブロックから
	/// defaults を拾うので省略可。ユーザー追加 key では少なくとも `scopes` の指定推奨。
	#[serde(default)]
	pub tokens: Option<BTreeMap<String, TwitchTokenKeySpec>>,
}

impl Twitch {
	/// Phase ζ-1: 実効的に有効な token_keys を返す。
	///
	/// - `token_keys` が明示されていればそれを使う（空リストでも空のまま返す）
	/// - 未指定時は後方互換推測:
	///   - `eventsub` が設定されていれば `"broadcaster"` を含める
	///   - `moderator` が設定されていれば `"moderator"` を含める
	///
	/// 重複は除去し、入力順は保つ。
	pub fn resolved_token_keys(&self) -> Vec<String> {
		if let Some(ref keys) = self.token_keys {
			let mut seen = std::collections::HashSet::new();
			let mut out = Vec::new();
			for k in keys {
				let k = k.trim();
				if k.is_empty() {
					continue;
				}
				if seen.insert(k.to_string()) {
					out.push(k.to_string());
				}
			}
			return out;
		}
		let mut out = Vec::new();
		if self.eventsub.is_some() {
			out.push("broadcaster".to_string());
		}
		if self.moderator.is_some() {
			out.push("moderator".to_string());
		}
		out
	}

	/// 指定 key の spec を `tokens` テーブルから取り出す。未設定なら `None`。
	pub fn token_spec(&self, key: &str) -> Option<&TwitchTokenKeySpec> {
		self.tokens.as_ref()?.get(key)
	}

	/// 実効的な `channel_to`。未指定時は `username` にフォールバック。
	///
	/// V1 EventSub ループが ChannelDatum を発行する際の既定 channel として使う。
	pub fn effective_channel_to(&self) -> &str {
		self.channel_to
			.as_deref()
			.map(str::trim)
			.filter(|s| !s.is_empty())
			.unwrap_or(self.username.as_str())
	}
}

/// Phase ζ-1: 個別 token_key の OAuth 設定。
///
/// 全フィールド optional。未指定フィールドは key 名と既定値（broadcaster / moderator
/// は旧 `eventsub` / `moderator` ブロックから継承、それ以外は VAC グローバル既定）で埋まる。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TwitchTokenKeySpec {
	/// スペース区切りの OAuth スコープ。未指定時は key 名に応じた VAC 既定:
	/// - `broadcaster` → `DEFAULT_BROADCASTER_OAUTH_SCOPES`
	/// - `moderator`   → `DEFAULT_MODERATOR_OAUTH_SCOPES`
	/// - その他        → 空（ユーザーが明示する必要あり）
	#[serde(default)]
	pub scopes: Option<String>,
	/// 上書き Client ID。未指定なら VAC 同梱の既定 + 環境変数 `VAC_TWITCH_CLIENT_ID`。
	#[serde(default)]
	pub client_id: Option<String>,
	/// DCF の認可待ちタイムアウト（秒）。未指定なら [`default_twitch_oauth_timeout_secs`]。
	#[serde(default)]
	pub oauth_timeout_secs: Option<u64>,
	/// DCF のブラウザ起動コマンドテンプレート（`{url}` 置換）。プロファイル切替などに使う。
	#[serde(default)]
	pub oauth_browser_command: Option<String>,
	/// GUI / CLI 表示用のログイン名ヒント（どのアカウントで認可させたいかの案内）。
	/// 強制はしない（`login_hint` 非対応のため）。
	#[serde(default)]
	pub login_hint: Option<String>,
	/// 手動で事前取得したユーザーアクセストークン。conf に直書きしたい場合の逃げ道。
	/// 環境変数 `VAC_TWITCH_TOKEN_<KEY>` も同等に扱う。
	#[serde(default)]
	pub user_access_token: Option<String>,
}

/// Phase V で追加したモデレーター/ボット用アカウントの設定。
///
/// - このアカウントは配信者ではないので、**VAC は受信タスクを起動しない**。発話中継・BAN/timeout/削除 のためだけに使う。
/// - `client_id` は `TwitchEventSubConfig::client_id` と同じ VAC アプリを流用する（Client ID を分けたい場合は環境変数
///   `VAC_TWITCH_CLIENT_ID` で上書き可能）。
/// - 保存トークンファイルは broadcaster 側と別（`twitch-token.moderator.json`）。環境変数
///   `VAC_TWITCH_MODERATOR_USER_ACCESS_TOKEN` / `VAC_TWITCH_MODERATOR_TOKEN_FILE` が個別に効く。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TwitchModeratorConfig {
	#[serde(default = "bool_true")]
	pub enabled: bool,
	/// このアカウントのログイン名（小文字）。未指定でも OAuth 自体は通るが、設定ミスで誤爆しないよう記述推奨。
	pub login: Option<String>,
	/// 手動で事前取得したユーザーアクセストークン。環境変数 `VAC_TWITCH_MODERATOR_USER_ACCESS_TOKEN` が優先。
	pub user_access_token: Option<String>,
	/// Device Code Flow による対話的認可を許可する（既定: true）。
	#[serde(default = "bool_true")]
	pub oauth_auto: bool,
	/// 空白区切りのスコープ文字列。未指定時は VAC 既定のモデレータースコープ。
	pub oauth_scopes: Option<String>,
	#[serde(default = "default_twitch_oauth_timeout_secs")]
	pub oauth_timeout_secs: u64,
	/// DCF のブラウザ起動コマンドテンプレート（`{url}` 置換）。`TwitchEventSubConfig::oauth_browser_command` と同じ書式。
	///
	/// broadcaster とは **別アカウント**で認可したいので、ここは専用プロファイルを指定するのが最も実用的:
	/// 例: `"msedge --profile-directory=\"Profile 2\" {url}"`
	pub oauth_browser_command: Option<String>,
}

/// VoicePeak CLI のグローバル既定（`flowgraph.tts.speak` で `endpoint` が空のときに State が埋める）。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct VoicepeakConfig {
	/// `voicepeak.exe` の絶対パス。未指定・空文字のときは OS 既定（Windows: `%ProgramFiles%\\VOICEPEAK\\voicepeak.exe`、他: `voicepeak`）。
	#[serde(default)]
	pub path: Option<String>,
}

/// `[voicepeak].path` が非空ならそれを返し、空なら Windows は `%ProgramFiles%\\VOICEPEAK\\voicepeak.exe`、それ以外は `voicepeak`。
pub fn resolve_voicepeak_fallback_executable(conf: &Conf) -> String {
	let from_conf = conf
		.voicepeak
		.as_ref()
		.and_then(|v| v.path.as_deref())
		.map(str::trim)
		.filter(|s| !s.is_empty());
	if let Some(p) = from_conf {
		return p.to_string();
	}
	#[cfg(windows)]
	{
		let base = std::env::var("ProgramW6432")
			.or_else(|_| std::env::var("PROGRAMFILES"))
			.unwrap_or_else(|_| "C:\\Program Files".to_string());
		std::path::PathBuf::from(base)
			.join("VOICEPEAK")
			.join("voicepeak.exe")
			.to_string_lossy()
			.into_owned()
	}
	#[cfg(not(windows))]
	{
		"voicepeak".to_string()
	}
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Conf {
	pub workers: Option<usize>,

	pub web_ui_address: Option<String>,
	/// actix-web のレスポンス圧縮（`Accept-Encoding` に応じて br / gzip / deflate 等）。`false` で無効。
	#[serde(default = "bool_true")]
	pub web_ui_compress: bool,
	// デフォルト値を Some("resources".to_string()) に指定
	#[serde(default = "default_web_ui_resources_path")]
	pub web_ui_resources_path: Option<String>,

	/// GUI (Phase VI-β) の `npm run build` 成果物を `/gui/` 配下で配信する場合のディレクトリ。
	/// 省略時は `./gui/dist`。存在しないときは `/gui/*` 全体が「未ビルド」の親切な HTML を返す
	/// （ビルドせずに VAC だけ起動するケースを壊さない）。
	#[serde(default = "default_gui_dist_path")]
	pub gui_dist_path: Option<String>,

	pub state_data_auto_save: Option<bool>,
	pub state_data_path: Option<PathBuf>,
	pub state_data_capacity: Option<usize>,
	pub state_data_pretty: Option<bool>,

	pub twitch: Option<Twitch>,

	/// OBS ブラウザソース向けの静的ファイルルートなど。
	#[serde(default)]
	pub browser_source: Option<BrowserSourceConfig>,

	/// Phase δ-6: Flowgraph の TOML ルートディレクトリ。`*.flowgraph.toml` を再帰ウォークする基点。
	/// 未指定時は `flowgraph`（cwd 相対）。存在しなくても起動は継続し、Control API / GUI からは
	/// 「ノード 0・診断 0」の空ランタイムとして見える（opt-in）。
	#[serde(default = "default_flowgraph_dir")]
	pub flowgraph_dir: Option<PathBuf>,

	/// Phase π-4c: Flowgraph ランタイム instance スコープの config。
	///
	/// conf.toml 上は `[flowgraph]` テーブルで指定する:
	///
	/// ```toml
	/// [flowgraph]
	/// default_timezone = "+09:00"
	/// ```
	///
	/// 現状は `default_timezone` のみ保持（未指定時 UTC）。π-5 の
	/// `flowgraph.datetime.parse` ノードが naive datetime を解釈する際の既定 TZ として
	/// 参照する。将来のロケール設定 / 単位系好み等もここに集約予定。
	#[serde(default, skip_serializing_if = "Option::is_none", rename = "flowgraph")]
	pub flowgraph_config: Option<FlowgraphInstanceConfig>,

	/// VoicePeak CLI のグローバルパス（`[voicepeak]`）。未指定時は [`resolve_voicepeak_fallback_executable`] と同じ既定。
	#[serde(default)]
	pub voicepeak: Option<VoicepeakConfig>,

	/// Phase M0: VMC 生 UDP パススルー等。未指定時は motion ワーカーを起動しない。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub motion: Option<MotionConf>,

	#[serde(default)]
	pub run_with: Vec<RunWith>,

	/// RM-1: Runtime Mode 宣言（`[modes.<id>]`）。空なら mode 定義なし（後続の Mode Manager まで実質 no-op）。
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub modes: BTreeMap<String, RuntimeModeDefinition>,

	/// 起動直後の既定 Runtime Mode ID（`modes` のキーと一致）。未指定なら loader は従来どおり。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub default_runtime_mode: Option<String>,

	pub log_level: Option<String>,

	/// `ChannelDatum::attachments` の `Inline` 添付が許容される最大バイト数。これを超えるバイナリは
	/// 自動で `File` 添付（`runtime_dir` 配下）に降格する運用を想定する。既定 32 KiB。
	#[serde(default)]
	pub attachment_inline_max_bytes: Option<u64>,

	/// ランタイム一時ディレクトリのルート。未指定なら OS 既定（Windows: `%LOCALAPPDATA%\virtual-avatar-connect\runtime`）。
	/// セッションごとに `<runtime_dir>/<session_id>/` が作られ、古いセッションは起動時にベストエフォートで削除される。
	#[serde(default)]
	pub runtime_dir: Option<PathBuf>,

	/// V1 `[[processors]]` 定義。**δ-9 (v0.9.x) で実行経路ごと除去**。
	/// 互換性のためパース時には残っているが、State 側では利用されず、あればロード時に警告が出る。
	/// 次期以降で完全除去予定。
	#[serde(default)]
	pub processors: Vec<ProcessorConf>,

	/// 常駐 AI サービスの設定（`[[ai.personas]]`）。旧 `feature = "openai-chat"` プロセッサーの昇格先。
	#[serde(default)]
	pub ai: AiConf,

	/// Phase VI (2026-04+): ブラウザ/GUI からランタイム状態を覗いたり制御したりするための Control API。
	/// アドレスは既存の `web_ui_address` を流用（既定 `127.0.0.1:57000`）。ここでは**認証ポリシー**のみ指定する。
	/// 未設定時は既定（ループバック無認証 + LAN は Bearer 必須）で `/api/v1/control/*` が有効化される。
	#[serde(default)]
	pub control_api: Option<ControlApiConf>,

	/// Phase VI-γ-1: この `Conf` インスタンスが読み込まれた元ファイルのパス。
	///
	/// `Conf::new` で CLI 引数由来のパスを記録する。`/api/v1/control/restart` でプロファイル切替時に
	/// 「現在の conf」を識別したり、`/api/v1/control/profiles` で同ディレクトリの候補列挙の軸に使ったりする。
	///
	/// `#[serde(skip)]` で toml への書き出し対象から除外（conf 自身に自分のパスを書くのはメタ的にも気持ち悪いし、
	/// プロファイル移動で齟齬が出る）。`#[serde(default)]` は `Deserialize` で常に `None` から始めるため。
	#[serde(skip, default)]
	pub source_path: Option<PathBuf>,
}

/// Control API（`/api/v1/control/*` と `/ws/control`）の認証ポリシー。
///
/// 「どこから接続できるか」は既存の [`Conf::web_ui_address`] で制御し、「その接続に Bearer トークンが要るか」
/// をこの構造体で決める、という二軸分離。
///
/// 既定値の狙い:
///   - `require_token_for_loopback = false`: 同 PC からの Tauri / ブラウザ GUI を無認証で動かす（最頻のユースケース）
///   - `require_token_for_non_loopback = true`: LAN からの接続は半信頼前提で Bearer 必須に倒す
///
/// トークン解決の優先度:
///   1. 環境変数 `VAC_CONTROL_API_BEARER_TOKEN`
///   2. `bearer_token` フィールド
///   3. 未指定時は起動ごとにランダム生成し、`<runtime_root>/control-token.txt` に書き出す
///     - GUI クライアントはこのファイルを読み取って `Authorization: Bearer <token>` を組み立てる想定
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ControlApiConf {
	/// 固定 Bearer トークン。未指定時は上記の通り自動生成する。
	#[serde(default)]
	pub bearer_token: Option<String>,
	/// ループバック（127.0.0.1 / ::1）からのリクエストに Bearer 検証を要求するか。既定 `false`。
	#[serde(default)]
	pub require_token_for_loopback: bool,
	/// 非ループバック（LAN など）からのリクエストに Bearer 検証を要求するか。既定 `true`。
	#[serde(default = "bool_true")]
	pub require_token_for_non_loopback: bool,

	/// Phase φ-1: Table CRUD API (`/api/v1/control/table/*`) から編集を許可する TSV ファイルの allow-list。
	///
	/// 登録されていないファイルは API からは **存在しないもの**として扱う（404）。
	/// GUI の Dictionary Editor Pane / Live Quick-Add はここに並んだものだけをカタログ表示する。
	#[serde(default)]
	pub tables: Vec<ControlTableEntry>,
}

/// Phase φ-1: Control API Table 編集対象エントリ。`[[control_api.tables]]` 配列で列挙する。
///
/// 設計:
///   - `key` は URL 上の安定した一意キー（spec §3.1 の `fq_path` 相当）。英数 + `.` + `-` + `_` に限定。
///   - `path` はディスク上の実ファイル。cwd 相対で解決する。書き込みは同一ディレクトリでの atomic rename。
///   - `editable` を `false` にすると書き込み系は 403 を返す（GUI からは read-only 表示）。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ControlTableEntry {
	/// URL 上の識別子（`/api/v1/control/table/{key}`）。
	pub key: String,
	/// ディスク上の TSV ファイル。cwd 相対 or 絶対パス。
	pub path: PathBuf,
	/// GUI カタログ表示用ラベル。省略時は `key` を表示。
	#[serde(default)]
	pub label: Option<String>,
	/// 役割ヒント（`"dictionary"` | `"generic"` など）。GUI の skin 切替用。
	#[serde(default)]
	pub role: Option<String>,
	/// GUI からの編集許可。`false` なら全 mutation API で 403。
	#[serde(default = "bool_true")]
	pub editable: bool,
	/// Live Quick-Add 連携（φ-2 で利用）。未設定なら Quick-Add 対象外。
	#[serde(default)]
	pub quick_add: Option<ControlTableQuickAdd>,
}

/// Phase φ-1/φ-4: Quick-Add ウィジェットの対応先ノード指定。
///
/// 実際の trigger は φ-2 (`POST /control/flowgraph/.../trigger/{node_id}`) で行う。
/// `forget_node_id` は φ-4 の Undo 機能で `dictionary.forget` ノードを指すために導入。
/// 未指定時は Undo ボタンを GUI から無効化する。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ControlTableQuickAdd {
	/// 対象 `dictionary.learn` ノードの fq ID（例 `"main::learn"`）。
	pub node_id: String,
	/// 既定の `kind`（`"literal"` | `"regex"`）。
	#[serde(default)]
	pub kind: Option<String>,
	/// Phase φ-4: 対応する `dictionary.forget` ノードの fq ID。
	/// 未設定なら履歴 [Undo] を無効化する（一方向 learn 運用）。
	#[serde(default)]
	pub forget_node_id: Option<String>,
}

impl Conf {
	fn load<P: Into<PathBuf>>(path: P) -> Result<Self> {
		let path = path.into();
		let conf_str = std::fs::read_to_string(&path)?;
		let mut conf: Self = toml::from_str(&conf_str)?;
		// Phase VI-γ-1: 自分の出自を覚えておく。CLI 解決後の絶対パス寄りに正規化しておくと、
		// カレントディレクトリ違いでの再起動でも取り違えない（失敗しても入力パスをそのまま採用）。
		conf.source_path = Some(std::fs::canonicalize(&path).unwrap_or(path));
		runtime_mode::validate_runtime_modes(&conf)?;
		Ok(conf)
	}

	/// Phase VI-γ-1: 再起動 API が新 conf の妥当性を事前確認するための軽量ロード。
	///
	/// 目的は「deserialize が通る = 起動時に即死しないことの保証」であり、ロガーや
	/// 外部サービスとの通信など副作用のある処理は一切走らせない。現プロセスの ログレベル等も
	/// いじらない。成功時は単に破棄する（ここで返した値は使わない）。
	pub fn new_noop_probe<P: Into<PathBuf>>(path: P) -> Result<Self> {
		Self::load(path)
	}

	pub fn new(args: &Args) -> Result<Self> {
		let conf = Self::load(&args.conf)?;
		// ログレベルの設定(argsで--debugが指定されていない場合)
		if let Some(log_filter) = conf.log_level.as_ref() {
			let v = match log_filter.to_lowercase().as_str() {
				"off" => Some(log::LevelFilter::Off),
				"error" => Some(log::LevelFilter::Error),
				"warn" => Some(log::LevelFilter::Warn),
				"info" => Some(log::LevelFilter::Info),
				"debug" => Some(log::LevelFilter::Debug),
				"trace" => Some(log::LevelFilter::Trace),
				_ => None,
			};
			match v {
				Some(llf) if !args.debug => {
					log::info!("ログレベルを {} に設定します。", llf);
					log::set_max_level(llf);
				}
				Some(_) if args.debug => {
					log::warn!("設定ファイルでログレベルが {} に設定されていますが、コマンドライン引数で -D/--debug が指定されているためログレベルは Trace が維持されます。", log_filter);
				}
				_ => {
					log::warn!(
						"設定ファイルでログレベルが {} に設定されていますが、不正な値として無視されました。",
						log_filter
					);
				}
			}
		}

		// processors に同一の id が指定されていないかチェック
		let mut already_used_ids = std::collections::HashSet::new();
		for processor in conf.processors.iter() {
			if let Some(pid) = processor.id.as_ref() {
				if !already_used_ids.insert(pid.clone()) {
					log::error!(
      "プロセッサーID {:?} が複数回指定されています。プロセッサーIDを定義する場合は同じIDが複数回指定されないように設定して下さい。",
      pid
     );
					bail!("プロセッサーID {:?} が複数回指定されています。", pid);
				}
			}
		}

		Ok(conf)
	}

	pub fn execute_run_with(&self) -> Result<()> {
		use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
		let mut system = sysinfo::System::new();
		system.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything().without_cpu());

		for run_with in self.run_with.iter() {
			Self::run_with_entry(run_with, Some(&system))?;
		}
		Ok(())
	}

	/// Phase VI-γ-2b: `run_with` の単一 entry 相当を実行する。
	///
	/// 呼び出し側:
	///   1. `Conf::execute_run_with`（起動時の一括実行、`system` を事前走査して渡す）
	///   2. `/api/v1/control/managed_apps/<id>/start`（ユーザー操作、`system=None` → 内部で走査）
	///
	/// `if_not_running` マッチで既に起動中なら `command` 発行をスキップする挙動は従来通り
	/// （Managed App API は呼び出し前に自前でチェックするので二重安全）。
	pub fn run_with_entry(run_with: &RunWith, system: Option<&sysinfo::System>) -> Result<()> {
		use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
		// system を持たない呼び出しのための on-demand 走査用バッファ。
		// Some 分岐では未使用だがライフタイムを揃えるためマッチ外で let しておく。
		#[allow(unused_assignments)]
		let mut owned_system: Option<sysinfo::System> = None;
		let system: &sysinfo::System = match system {
			Some(s) => s,
			None => {
				let mut s = sysinfo::System::new();
				s.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything().without_cpu());
				owned_system = Some(s);
				owned_system.as_ref().unwrap()
			}
		};

		let (command, if_not_running, run_as_admin, working_dir, minimized) = match run_with {
			RunWith::Command(command) => (command, None, false, None, false),
			RunWith::CommandIfProcessIsNotRunning {
				command,
				if_not_running,
				run_as_admin,
				working_dir,
				minimized,
				..
			} => (
				command,
				if_not_running.as_ref(),
				run_as_admin.unwrap_or_default(),
				working_dir.as_ref(),
				minimized.unwrap_or_default(),
			),
		};

		// if_not_running が指定されている場合は、プロセスが実行中か確認して実行中なら起動をスキップ
		if let Some(if_not_running) = if_not_running {
			let matching_pids: Vec<u32> = system
				.processes()
				.iter()
				.filter(|(_, process)| process.name().to_string_lossy().contains(if_not_running))
				.map(|(pid, _)| pid.as_u32())
				.collect();

			if !matching_pids.is_empty() {
				log::info!(
					"run_with: 既に {} を含むプロセスが実行中のため {} の起動はスキップします。",
					if_not_running,
					command
				);
				#[cfg(target_os = "windows")]
				if minimized {
					for pid in matching_pids {
						log::info!(
							"run_with: minimized のため既存プロセス (pid={}) のウィンドウ最小化をスケジュールします。",
							pid
						);
						run_with_schedule_minimize_child_windows(pid);
					}
				}
				return Ok(());
			}
		}

		// command (引数がある場合も考慮)を実行、またはURL/URIを開く
		let is_http_url = command.starts_with("http://") || command.starts_with("https://");
		let is_app_scheme = command.starts_with("steam://");

		if is_http_url {
			// 通常の http(s) URL は既存どおりブラウザーに任せる
			use webbrowser::{Browser, BrowserOptions};
			if minimized {
				log::debug!("run_with: minimized は http(s) URL では未対応のため無視します。");
			}
			log::info!("run_with: {:?} を URL として既定ブラウザーで開きます。", command);
			if let Err(e) = webbrowser::open_browser_with_options(Browser::Default, command, BrowserOptions::new().with_target_hint("vac"))
			{
				log::error!("run_with: URL を開く際にエラーが発生しました: {:?}", e);
			}
		} else if is_app_scheme {
			// steam:// などのアプリ用スキームはブラウザー経由ではなく OS の既定 URL ハンドラで開く
			#[cfg(target_os = "windows")]
			{
				use windows::core::PCWSTR;
				use windows::Win32::UI::Shell::ShellExecuteW;
				use windows::Win32::UI::WindowsAndMessaging::{SW_SHOWMINIMIZED, SW_SHOWNORMAL};

				log::info!("run_with: {:?} を OS の既定 URL ハンドラで開きます。", command);
				let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
				let show = if minimized { SW_SHOWMINIMIZED } else { SW_SHOWNORMAL };
				unsafe {
					let result = ShellExecuteW(
						None,
						PCWSTR::null(), // "open"
						PCWSTR(wide.as_ptr()),
						PCWSTR::null(),
						PCWSTR::null(),
						show,
					);
					let hinst = result.0 as isize;
					if hinst <= 32 {
						log::error!("run_with: ShellExecuteW による URI オープンに失敗しました (code = {})", hinst);
					}
				}
			}
			#[cfg(not(target_os = "windows"))]
			{
				if minimized {
					log::debug!("run_with: minimized はアプリ用スキームではこの OS では未対応です。");
				}
				log::warn!(
					"run_with: {:?} はアプリ用スキームと判定されましたが、この OS では特別な処理は行われません。",
					command
				);
			}
		} else {
			let original_dir = match change_working_dir(working_dir) {
				Ok(original_dir) => original_dir.map(|p| p.to_string_lossy().to_string()),
				Err(e) => {
					log::error!("run_with: 作業ディレクトリーの変更に失敗しました: {:?}", e);
					return Ok(());
				}
			};
			if run_as_admin {
				log::warn!("run_with: {:?} を管理者権限で実行を試みます。", command);
				#[cfg(target_os = "windows")]
				{
					if minimized {
						log::info!("run_with: 管理者権限・最小化のため ShellExecuteW (runas) を使います（プロセス終了は待機しません）。");
						if let Err(e) = run_with_shell_execute_runas(command, true) {
							log::error!("run_with: 管理者権限で最小化起動する際にエラーが発生しました: {:?}", e);
						}
					} else if let Err(e) = runas::Command::new(command).status() {
						log::error!("run_with: 管理者権限でコマンドを実行する際にエラーが発生しました: {:?}", e);
					}
				}
				#[cfg(not(target_os = "windows"))]
				{
					if minimized {
						log::warn!("run_with: minimized は管理者起動ではこの OS では未対応のため無視します。");
					}
					if let Err(e) = runas::Command::new(command).status() {
						log::error!("run_with: 管理者権限でコマンドを実行する際にエラーが発生しました: {:?}", e);
					}
				}
			} else {
				log::info!("run_with: {:?} をコマンドとして実行します。", command);
				#[cfg(target_os = "windows")]
				{
					let r = if minimized {
						// `start /MIN` は新規プロセスグループで起動し親子が切れるため使わない。
						// `CreateProcessW` + STARTUPINFO で子として起動し、WPF 等は無視するため PID でウィンドウ列挙→最小化も試す。
						match run_with_create_process_minimized(command) {
							Ok(pid) => {
								run_with_schedule_minimize_child_windows(pid);
								Ok(())
							}
							Err(e) => Err(e),
						}
					} else {
						duct::cmd!(command).start().map(|_| ())
					};
					if let Err(e) = r {
						log::error!("run_with: コマンドを実行する際にエラーが発生しました: {:?}", e);
					}
				}
				#[cfg(not(target_os = "windows"))]
				{
					if minimized {
						log::warn!("run_with: minimized はこの OS では未対応のため通常起動します。");
					}
					if let Err(e) = duct::cmd!(command).start() {
						log::error!("run_with: コマンドを実行する際にエラーが発生しました: {:?}", e);
					}
				}
			}
			change_working_dir(original_dir.as_ref())?;
		}
		Ok(())
	}

	pub fn to_shared(self) -> SharedConf {
		Arc::new(RwLock::new(self))
	}

	pub fn get_workers(&self) -> usize {
		self.workers.unwrap_or_else(|| num_cpus::get())
	}

	pub fn get_web_ui_address(&self) -> &str {
		match self.web_ui_address.as_ref() {
			Some(a) => a,
			None => DEFAULT_WEB_UI_ADDRESS,
		}
	}
}

fn default_web_ui_resources_path() -> Option<String> {
	Some("resources".to_string())
}

fn default_gui_dist_path() -> Option<String> {
	Some("gui/dist".to_string())
}

fn default_flowgraph_dir() -> Option<PathBuf> {
	Some(PathBuf::from("flowgraph"))
}

/// 通常権限で子プロセスとして起動し、最初の表示を最小化する（`duct` + `start /MIN` と異なり親子関係を維持する）。
/// 戻り値は子プロセスの PID（最小化の列挙用）。
#[cfg(target_os = "windows")]
fn run_with_create_process_minimized(command: &str) -> std::io::Result<u32> {
	use std::ffi::OsStr;
	use std::os::windows::ffi::OsStrExt;
	use windows::core::PWSTR;
	use windows::Win32::Foundation::CloseHandle;
	use windows::Win32::System::Threading::{
		CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTF_USESHOWWINDOW, STARTUPINFOW,
	};
	use windows::Win32::UI::WindowsAndMessaging::SW_SHOWMINIMIZED;

	// lpCommandLine は一部の実装で書き換えられるため可変バッファにする（MSDN）
	let mut cmdline: Vec<u16> = OsStr::new(command).encode_wide().chain(std::iter::once(0)).collect();

	let mut si = STARTUPINFOW::default();
	si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
	si.dwFlags = STARTF_USESHOWWINDOW;
	si.wShowWindow = SW_SHOWMINIMIZED.0 as u16;

	let mut pi = PROCESS_INFORMATION::default();

	unsafe {
		CreateProcessW(
			None,
			Some(PWSTR(cmdline.as_mut_ptr())),
			None,
			None,
			false,
			PROCESS_CREATION_FLAGS(0),
			None,
			None,
			&si,
			&mut pi,
		)?;

		let pid = pi.dwProcessId;
		let _ = CloseHandle(pi.hThread);
		let _ = CloseHandle(pi.hProcess);
		Ok(pid)
	}
}

/// STARTUPINFO を無視する GUI 向けに、起動後にトップレベルウィンドウを列挙して最小化する。
/// 子プロセスでメインウィンドウを出すアプリ向けに Toolhelp でプロセス木を追跡し、後半はタイトル一致フォールバックも試す。
/// Phase VI-γ-2b: Managed App からも単発の最小化として呼ぶので `pub(crate)` に。
#[cfg(target_os = "windows")]
pub(crate) fn run_with_schedule_minimize_child_windows(root_pid: u32) {
	use std::time::Duration;
	std::thread::spawn(move || {
		// COEIROINK 等はエンジン起動・子プロセス起動後にメイン窓が出るまで数秒かかる。即列挙では 0 件になりやすい。
		std::thread::sleep(Duration::from_millis(2000));
		/// 遅延表示・子プロセス起動に備え、最大約 15 秒まで再試行する。
		const ATTEMPTS: u32 = 60;
		const SLEEP_MS: u64 = 250;
		for attempt in 0..ATTEMPTS {
			if attempt > 0 {
				std::thread::sleep(Duration::from_millis(SLEEP_MS));
			}
			let tree = run_with_collect_process_tree_pids(root_pid);
			log::trace!(
				"run_with: minimize 試行 {} — プロセス木に {} 件の PID（root={}）。",
				attempt + 1,
				tree.len(),
				root_pid
			);

			// 前半は表示中のみ、後半は非表示だが矩形のあるウィンドウも対象（起直前の一瞬など）
			let require_visible = attempt < 36;
			match run_with_minimize_top_level_windows_for_pids(&tree, require_visible) {
				Ok(n) if n > 0 => {
					log::info!(
						"run_with: プロセス木に関連するウィンドウ {} 件を最小化しました（{} 試行目、root pid={}）。",
						n,
						attempt + 1,
						root_pid
					);
					return;
				}
				Ok(_) => {}
				Err(e) => log::trace!("run_with: EnumWindows 試行 {}: {:?}", attempt + 1, e),
			}

			// ランチャーが終了して子だけ残る等、親子が切れた場合のフォールバック（COEIROINK 向け）
			if attempt >= 40 {
				match run_with_minimize_windows_title_substring_ci("coeiroink") {
					Ok(n) if n > 0 => {
						log::info!(
							"run_with: ウィンドウタイトルに「coeiroink」を含む {} 件を最小化しました（{} 試行目・フォールバック）。",
							n,
							attempt + 1
						);
						return;
					}
					Ok(_) => {}
					Err(e) => log::trace!("run_with: タイトルフォールバック 試行 {}: {:?}", attempt + 1, e),
				}
			}
		}
		log::debug!(
			"run_with: root pid={} について最小化対象ウィンドウが見つかりませんでした（{} 試行）。",
			root_pid,
			ATTEMPTS
		);
	});
}

/// Toolhelp の親子関係から、`root` から辿れるすべてのプロセス PID（`root` 自身を含む）。
///
/// Phase VI-γ-2b: Managed App の停止時にも「root から辿れる子孫まとめて」扱いたいので `pub(crate)` で開く。
#[cfg(target_os = "windows")]
pub(crate) fn run_with_collect_process_tree_pids(root: u32) -> std::collections::HashSet<u32> {
	use std::collections::HashSet;
	use windows::Win32::Foundation::CloseHandle;
	use windows::Win32::System::Diagnostics::ToolHelp::{
		CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
	};

	let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(256);
	unsafe {
		let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
			let mut only = HashSet::new();
			only.insert(root);
			return only;
		};

		let mut pe = PROCESSENTRY32W::default();
		pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
		if Process32FirstW(snap, &mut pe).is_ok() {
			loop {
				pairs.push((pe.th32ProcessID, pe.th32ParentProcessID));
				if Process32NextW(snap, &mut pe).is_err() {
					break;
				}
			}
		}
		let _ = CloseHandle(snap);
	}

	let mut set = HashSet::new();
	set.insert(root);
	let mut changed = true;
	while changed {
		changed = false;
		for &(pid, ppid) in &pairs {
			if set.contains(&ppid) && !set.contains(&pid) {
				set.insert(pid);
				changed = true;
			}
		}
	}
	set
}

#[cfg(target_os = "windows")]
fn run_with_minimize_top_level_windows_for_pids(pids: &std::collections::HashSet<u32>, require_visible: bool) -> std::io::Result<usize> {
	use std::boxed::Box;
	use windows::core::BOOL;
	use windows::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
	use windows::Win32::UI::WindowsAndMessaging::{
		EnumWindows, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER, WNDENUMPROC,
	};

	struct EnumCtx {
		pids: std::collections::HashSet<u32>,
		hwnds: Vec<HWND>,
		require_visible: bool,
	}

	unsafe extern "system" fn enum_top_level_for_pids(hwnd: HWND, lparam: LPARAM) -> BOOL {
		let ctx = unsafe { &mut *(lparam.0 as *mut EnumCtx) };
		let mut wpid = 0u32;
		unsafe {
			GetWindowThreadProcessId(hwnd, Some(&mut wpid as *mut u32));
		}
		if !ctx.pids.contains(&wpid) {
			return TRUE;
		}
		if ctx.require_visible {
			if !unsafe { IsWindowVisible(hwnd).as_bool() } {
				return TRUE;
			}
		} else {
			if !unsafe { IsWindowVisible(hwnd).as_bool() } {
				let mut r = RECT::default();
				if GetWindowRect(hwnd, &mut r).is_err() {
					return TRUE;
				}
				let w = (r.right - r.left).abs();
				let h = (r.bottom - r.top).abs();
				if w * h < 64 {
					return TRUE;
				}
			}
		}
		let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
		if !owner.0.is_null() {
			return TRUE;
		}
		ctx.hwnds.push(hwnd);
		TRUE
	}

	let ctx = Box::new(EnumCtx {
		pids: pids.clone(),
		hwnds: Vec::new(),
		require_visible,
	});
	let ptr = Box::into_raw(ctx);
	unsafe {
		let enum_fn: WNDENUMPROC = Some(enum_top_level_for_pids);
		EnumWindows(enum_fn, LPARAM(ptr as isize))
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("EnumWindows: {}", e)))?;
		let ctx = Box::from_raw(ptr);
		let n = ctx.hwnds.len();
		run_with_apply_minimize_to_hwnds(&ctx.hwnds);
		Ok(n)
	}
}

/// タスクバーの「最小化」に近い経路を複数試す（`SW_FORCEMINIMIZE` だけでは WPF 等で効かないことがある）。
#[cfg(target_os = "windows")]
fn run_with_apply_minimize_to_hwnds(hwnds: &[windows::Win32::Foundation::HWND]) {
	use windows::Win32::Foundation::{LPARAM, WPARAM};
	use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, ShowWindow, SC_MINIMIZE, SW_FORCEMINIMIZE, SW_MINIMIZE, WM_SYSCOMMAND};

	for &h in hwnds {
		unsafe {
			let _ = ShowWindow(h, SW_MINIMIZE);
			let _ = ShowWindow(h, SW_FORCEMINIMIZE);
			let _ = PostMessageW(Some(h), WM_SYSCOMMAND, WPARAM(SC_MINIMIZE as usize), LPARAM(0));
		}
	}
}

/// 表示中のトップレベルウィンドウで、タイトルに部分文字列が含まれるものを最小化（親子が切れた場合の補助）。
#[cfg(target_os = "windows")]
fn run_with_minimize_windows_title_substring_ci(needle: &str) -> std::io::Result<usize> {
	use std::boxed::Box;
	use windows::core::BOOL;
	use windows::Win32::Foundation::{HWND, LPARAM, TRUE};
	use windows::Win32::UI::WindowsAndMessaging::{
		EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER, WNDENUMPROC,
	};

	let needle = needle.to_lowercase();

	struct TitleCtx {
		needle: String,
		hwnds: Vec<HWND>,
	}

	unsafe extern "system" fn enum_title(hwnd: HWND, lparam: LPARAM) -> BOOL {
		let ctx = unsafe { &mut *(lparam.0 as *mut TitleCtx) };
		let mut wpid = 0u32;
		unsafe {
			GetWindowThreadProcessId(hwnd, Some(&mut wpid as *mut u32));
		}
		if wpid == 0 {
			return TRUE;
		}
		if !unsafe { IsWindowVisible(hwnd).as_bool() } {
			return TRUE;
		}
		let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
		if !owner.0.is_null() {
			return TRUE;
		}
		let mut buf = [0u16; 512];
		let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
		if n == 0 {
			return TRUE;
		}
		let t = String::from_utf16_lossy(&buf[..n as usize]).to_lowercase();
		if !t.contains(ctx.needle.as_str()) {
			return TRUE;
		}
		ctx.hwnds.push(hwnd);
		TRUE
	}

	let ctx = Box::new(TitleCtx { needle, hwnds: Vec::new() });
	let ptr = Box::into_raw(ctx);
	unsafe {
		let enum_fn: WNDENUMPROC = Some(enum_title);
		EnumWindows(enum_fn, LPARAM(ptr as isize))
			.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("EnumWindows: {}", e)))?;
		let ctx = Box::from_raw(ptr);
		let n = ctx.hwnds.len();
		run_with_apply_minimize_to_hwnds(&ctx.hwnds);
		Ok(n)
	}
}

/// 管理者権限で起動（`ShellExecuteW` / `runas`）。`runas` クレートと異なりプロセス終了は待たない。
#[cfg(target_os = "windows")]
fn run_with_shell_execute_runas(command: &str, minimized: bool) -> std::io::Result<()> {
	use std::ffi::OsStr;
	use std::os::windows::ffi::OsStrExt;
	use windows::core::PCWSTR;
	use windows::Win32::UI::Shell::ShellExecuteW;
	use windows::Win32::UI::WindowsAndMessaging::{SW_NORMAL, SW_SHOWMINIMIZED};

	let verb: Vec<u16> = OsStr::new("runas").encode_wide().chain(std::iter::once(0)).collect();
	let file: Vec<u16> = OsStr::new(command).encode_wide().chain(std::iter::once(0)).collect();
	let n_show = if minimized { SW_SHOWMINIMIZED } else { SW_NORMAL };

	unsafe {
		let result = ShellExecuteW(
			None,
			PCWSTR(verb.as_ptr()),
			PCWSTR(file.as_ptr()),
			PCWSTR::null(),
			PCWSTR::null(),
			n_show,
		);
		let hinst = result.0 as isize;
		if hinst <= 32 {
			return Err(std::io::Error::new(
				std::io::ErrorKind::Other,
				format!("ShellExecuteW (runas) が失敗しました (code = {})", hinst),
			));
		}
	}
	Ok(())
}

/// 現在の作業ディレクトリを変更して、変更前の作業ディレクトリを返します。
fn change_working_dir(working_dir: Option<&String>) -> Result<Option<std::path::PathBuf>> {
	if let Some(working_dir) = working_dir {
		let original_dir = std::env::current_dir()?;
		std::env::set_current_dir(working_dir)?;
		Ok(Some(original_dir))
	} else {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// リポジトリ同梱の `conf.toml`（v2 配布既定）が必ずパースできることを保証する回帰テスト。
	///
	/// - v2 では `[[processors]]` を書かない前提なので processors は空配列になるはず。
	/// - `conf.toml` が壊れたまま commit されることを防ぎ、δ-8 以降の配布物刷新の
	///   セーフティネットとして機能する。
	#[test]
	fn distributed_conf_toml_parses_and_has_no_processors() {
		let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conf.toml");
		let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("conf.toml を読めない: {} ({e})", path.display()));
		let conf: Conf = toml::from_str(&text).unwrap_or_else(|e| panic!("conf.toml の TOML パースに失敗: {e}"));
		assert!(
			conf.processors.is_empty(),
			"v2 配布 conf.toml に `[[processors]]` が残っている ({} 件)。Flowgraph に移行してください。",
			conf.processors.len(),
		);
	}

	/// 開発者用の `conf.local*.toml`（未 commit ないし個人設定）をロード時エラーで落とさないための回帰。
	///
	/// CI やクリーンチェックアウトでは存在しないので、ファイルが無ければ silently skip する。
	/// `[[ai.personas]]` や `[twitch]` を含む v2-ready な形で書かれているはずなので、
	/// deserialize が通ることだけ検証する。
	#[test]
	fn conf_local_variants_parse_when_present() {
		let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
		for name in ["conf.local.toml", "conf.local.solo.toml"] {
			let path = root.join(name);
			let Ok(text) = std::fs::read_to_string(&path) else {
				continue;
			};
			let _conf: Conf = toml::from_str(&text).unwrap_or_else(|e| panic!("{name} の TOML パースに失敗: {e}"));
		}
	}

	/// Phase π-4c: `[flowgraph]` テーブル経由の FlowgraphInstanceConfig 取り込み確認。
	///
	/// conf.toml に `[flowgraph] default_timezone = "+09:00"` を書けば
	/// `Conf.flowgraph_config.default_timezone` に乗り、さらに resolve して Offset が取れること。
	#[test]
	fn conf_flowgraph_default_timezone_wires_through() {
		use jiff::tz::Offset;
		let src = r#"
[flowgraph]
default_timezone = "+09:00"
"#;
		let conf: Conf = toml::from_str(src).unwrap();
		let fc = conf
			.flowgraph_config
			.as_ref()
			.expect("[flowgraph] should deserialize into flowgraph_config");
		assert_eq!(fc.default_timezone.as_deref(), Some("+09:00"));
		assert_eq!(fc.resolve_default_timezone().unwrap(), Offset::constant(9));
	}

	/// `[flowgraph]` 未指定時は `flowgraph_config` が `None` のままで既存 conf.toml と互換性がある。
	#[test]
	fn conf_without_flowgraph_table_keeps_backward_compat() {
		let src = ""; // 空 conf
		let conf: Conf = toml::from_str(src).unwrap();
		assert!(conf.flowgraph_config.is_none());
	}

	#[test]
	fn voicepeak_conf_path_wires_resolve() {
		let src = r#"
[voicepeak]
path = "D:/tools/voicepeak.exe"
"#;
		let conf: Conf = toml::from_str(src).unwrap();
		assert_eq!(resolve_voicepeak_fallback_executable(&conf), "D:/tools/voicepeak.exe");
	}

	#[test]
	fn voicepeak_resolve_without_table_uses_os_heuristic() {
		let conf: Conf = toml::from_str("").unwrap();
		let p = resolve_voicepeak_fallback_executable(&conf);
		#[cfg(windows)]
		{
			assert!(
				p.ends_with(r"VOICEPEAK\voicepeak.exe") || p.ends_with("VOICEPEAK/voicepeak.exe"),
				"unexpected path: {p}"
			);
		}
		#[cfg(not(windows))]
		assert_eq!(p, "voicepeak");
	}
}
