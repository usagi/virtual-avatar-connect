//! δ-3d: Ingress ノードのスケルトン。
//!
//! 3 種（WebInput / Voice / Twitch）を同一パターンで表現する:
//!
//! - 外部コード（HTTP ハンドラ / voice エンジン / Twitch IRC クライアント）が
//!   `TriggerHandle::send(TriggerEvent)` でエンジンを叩く
//! - `fired_exec = ["__trigger__"]`、`data_overrides` に各フィールド値を載せる
//! - エンジンは fire_node で overrides を inputs に merge → ノードの compute が
//!   対応する outputs を echo して `exec_out` を発火
//!
//! ## 設計メモ
//!
//! - これらの「echo 型」ノードは `PureNode` として実装して十分（compute は決定論的）。
//! - 実際の外部 I/O は engine の外側（V1 Twitch/Voice/WebInput のブリッジレイヤー、δ-4d）が担う。
//! - 各 ingress の出力フィールドは V1 の `ChannelDatum` を参考に代表的なものを揃える:
//!   `content`（本文）/ `source_actor`（発信者識別子）/ `source_kind`（種別）/ `meta`（自由）
//! - **Phase M1**: `flowgraph.ingress.vmc_udp` + `bridges::vmc_ingress` — VMC 互換の生 UDP。
//! - **Phase ρ**: `flowgraph.ingress.osc_udp` + `bridges::osc_ingress` — 汎用 OSC/任意 UDP（`__meta__.profile = "osc_udp"`）。
//!
//! ## 入力ポートが `__` プレフィックス始まりの理由
//!
//! ユーザがエディタで誤接続しないように、内部ポートであることを命名規則で示す。
//! δ-6 GUI で `__` 始まりポートを自動的に hide する予定。

use crate::flowgraph::node::{
	ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------
// ヘルパ: 共通の ingress ポート群
// ---------------------------------------------------------------------

fn ingress_inputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_input("__trigger__", "(internal trigger)"),
		PortSpec::input("__content__", "(internal) content", SocketType::String).with_default(SocketValue::String(String::new())),
		PortSpec::input("__source_actor__", "(internal) actor", SocketType::String).with_default(SocketValue::String(String::new())),
		PortSpec::input("__source_kind__", "(internal) kind", SocketType::String).with_default(SocketValue::String(String::new())),
		PortSpec::input("__meta__", "(internal) meta", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
	]
}

fn ingress_outputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_output("exec_out", "Exec Out"),
		PortSpec::output("content", "Content", SocketType::String),
		PortSpec::output("source_actor", "Source Actor", SocketType::String),
		PortSpec::output("source_kind", "Source Kind", SocketType::String),
		PortSpec::output("meta", "Meta", SocketType::Json),
	]
}

async fn ingress_compute(inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
	if !fired_exec.contains("__trigger__") {
		return Ok(NodeOutput::new());
	}
	let content = inputs.get("__content__").cloned().unwrap_or(SocketValue::String(String::new()));
	let actor = inputs
		.get("__source_actor__")
		.cloned()
		.unwrap_or(SocketValue::String(String::new()));
	let kind = inputs.get("__source_kind__").cloned().unwrap_or(SocketValue::String(String::new()));
	let meta = inputs.get("__meta__").cloned().unwrap_or(SocketValue::Json(JsonValue::Null));
	Ok(NodeOutput::new()
		.set_data("content", content)
		.set_data("source_actor", actor)
		.set_data("source_kind", kind)
		.set_data("meta", meta)
		.fire_exec("exec_out"))
}

// ---------------------------------------------------------------------
// ingress.web_input
// ---------------------------------------------------------------------

pub struct WebInputIngressNode;

impl NodeDescriptor for WebInputIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
   feature: "flowgraph.ingress.web_input".into(),
   title: "Web Input Ingress".into(),
   category: "ingress".into(),
   description: Some("HTTP エンドポイント経由の入力（V1 の /input/* 相当）".into()),
   inputs: ingress_inputs(),
   outputs: ingress_outputs(),
   properties: vec![
    PropertySpec::new("path", "Path", SocketType::String, SocketValue::String(String::new()))
     .description("HTTP パス（例 \"/input/chat\"）。空なら `/input/flowgraph/<node_id>` を自動採番。"),
    PropertySpec::new("method", "Method", SocketType::String, SocketValue::String("POST".into()))
     .description("HTTP メソッド。\"POST\"/\"GET\"/\"PUT\" 等（大文字小文字不問）。"),
    PropertySpec::new(
     "body_format",
     "Body Format",
     SocketType::String,
     SocketValue::String("plain".into()),
    )
    .description("リクエストボディの解釈。\"plain\"（本文をそのまま content）/\"json\"（`content` フィールド）/\"form\"（`content` パラメータ）。"),
    PropertySpec::new(
     "fixed_channel",
     "Fixed Channel",
     SocketType::String,
     SocketValue::String(String::new()),
    )
    .description("ingress が echo で出す `source_kind`（V1 の channel 相当）。空なら `web_input`。"),
   ],
  }
	}
}

#[async_trait]
impl PureNode for WebInputIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		ingress_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.voice
// ---------------------------------------------------------------------

pub struct VoiceIngressNode;

impl NodeDescriptor for VoiceIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.voice".into(),
			title: "Voice Ingress".into(),
			category: "ingress".into(),
			description: Some("音声認識（Vosk / Whisper）経由の入力".into()),
			inputs: ingress_inputs(),
			outputs: ingress_outputs(),
			properties: vec![
				PropertySpec::new("engine", "Engine", SocketType::String, SocketValue::String("vosk".into()))
					.description("\"vosk\" か \"whisper\"。"),
				PropertySpec::new("model_path", "Model Path", SocketType::String, SocketValue::String(String::new()))
					.description("Vosk/Whisper のモデル格納ディレクトリ。bridge 起動時にロードする。"),
				PropertySpec::new("source", "Audio Source", SocketType::String, SocketValue::String(String::new()))
					.description("入力音声ソース（マイクデバイス名 / ファイルパス / 空＝既定デバイス）。"),
				PropertySpec::new("sample_rate", "Sample Rate", SocketType::Int, SocketValue::Int(16000))
					.description("サンプリングレート（Hz）。vosk は通常 16000、whisper は任意。"),
				PropertySpec::new("language_code", "Language", SocketType::String, SocketValue::String("ja".into()))
					.description("言語コード（whisper で使用）。例 \"ja\" / \"en\"。"),
				PropertySpec::new(
					"grammar_json",
					"Grammar (JSON)",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("vosk の constrained grammar を JSON 文字列で指定（省略可）。"),
				PropertySpec::new(
					"fixed_channel",
					"Fixed Channel",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("ingress が echo で出す `source_kind`。空なら `voice`。"),
			],
		}
	}
}

#[async_trait]
impl PureNode for VoiceIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		ingress_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.twitch
// ---------------------------------------------------------------------

pub struct TwitchIngressNode;

impl NodeDescriptor for TwitchIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.twitch".into(),
			title: "Twitch Ingress".into(),
			category: "ingress".into(),
			description: Some("Twitch チャット / EventSub 経由の入力".into()),
			inputs: ingress_inputs(),
			outputs: ingress_outputs(),
			properties: vec![
				PropertySpec::new("mode", "Mode", SocketType::String, SocketValue::String("irc".into()))
					.description("\"irc\"（チャット受信）か \"eventsub\"（Helix/EventSub）。"),
				PropertySpec::new(
					"channels",
					"Channels",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description("IRC 購読するチャンネル名（`#` 不要）。"),
				PropertySpec::new(
					"access_token",
					"Access Token",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("OAuth トークン（空なら `token_key` による解決を試みる）。"),
				PropertySpec::new("token_key", "Token Key", SocketType::String, SocketValue::String(String::new()))
					.description("`conf.twitch.token_keys` で定義したキー。空なら `access_token` をそのまま使用。"),
				PropertySpec::new("client_id", "Client ID", SocketType::String, SocketValue::String(String::new()))
					.description("Twitch App の Client ID。EventSub で必須。"),
				PropertySpec::new(
					"broadcaster_id",
					"Broadcaster ID",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("EventSub subscribe 対象の broadcaster_user_id。"),
				PropertySpec::new("login", "Bot Login", SocketType::String, SocketValue::String(String::new()))
					.description("IRC ログイン名（小文字・`#` 不要）。"),
				PropertySpec::new(
					"fixed_channel",
					"Fixed Channel",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("ingress が echo で出す `source_kind`。空なら `twitch:chat` または `twitch:eventsub`。"),
			],
		}
	}
}

#[async_trait]
impl PureNode for TwitchIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		ingress_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.twitch_eventsub (ζ-2)
// ---------------------------------------------------------------------
//
// Twitch EventSub (WebSocket) の通知を **Flowgraph-native に** 受ける ingress。
// `src/bridges/twitch_eventsub.rs` が WS セッションを持ち、Helix で購読を張り、
// `notification` が届くたびに TriggerEvent を発火する。V1 `src/twitch/eventsub.rs`
// 経由の ChannelDatum パイプラインとは **独立** に動く:
//   - V1 = 人間可読 1 行を `channel_datum_tx` に push（既存 [[processors]] 用）
//   - V2 = 生 event JSON + 正規化メタを Flowgraph data output に流す（こっちは raw payload を持つ）
//
// 同一 broadcaster に対して両方を enable すると WS が 2 本張られる。不要なら
// `[twitch.eventsub].enabled = false` で V1 を落とすか、ingress ノード側を消す。

pub struct TwitchEventsubIngressNode;

impl NodeDescriptor for TwitchEventsubIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.twitch_eventsub".into(),
			title: "Twitch EventSub Ingress".into(),
			category: "ingress".into(),
			description: Some(
				"Twitch EventSub (WebSocket) の通知を受ける Flowgraph-native ingress。`token_key` で \
     conf.twitch.token_keys の OAuth トークンを引き、`event_types` に挙げた sub_type を購読する。\
     `payload` は生 event JSON、`meta` はサブスクリプション情報を含む補助マップ。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("__trigger__", "(internal trigger)"),
				PortSpec::input("__event_type__", "(internal) event_type", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("__actor_name__", "(internal) actor_name", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("__actor_login__", "(internal) actor_login", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("__broadcaster_login__", "(internal) broadcaster_login", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("__payload__", "(internal) payload", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
				PortSpec::input("__meta__", "(internal) meta", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("event_type", "Event Type", SocketType::String),
				PortSpec::output("actor_name", "Actor Name", SocketType::String),
				PortSpec::output("actor_login", "Actor Login", SocketType::String),
				PortSpec::output("broadcaster_login", "Broadcaster Login", SocketType::String),
				PortSpec::output("payload", "Raw Payload", SocketType::Json),
				PortSpec::output("meta", "Meta", SocketType::Json),
			],
			properties: vec![
				PropertySpec::new(
					"token_key",
					"Token Key",
					SocketType::String,
					SocketValue::String("broadcaster".into()),
				)
				.description("`conf.twitch.token_keys` で定義した OAuth key。EventSub 購読には user access token が必須。"),
				PropertySpec::new(
					"broadcaster_login",
					"Broadcaster Login",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"購読対象の配信者ログイン名。空なら `conf.twitch.username`、それも無ければ `token_key` の OAuth login が使われる。",
				),
				PropertySpec::new(
					"event_types",
					"Event Types",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description(
					"購読する sub_type リスト（例: `[\"channel.cheer\", \"channel.raid\"]`）。空なら conf.twitch.eventsub の \
      各 bool トグル（stream.online / channel.cheer ...）をフォールバックとして使う。",
				),
				PropertySpec::new(
					"channel_points_reward_id",
					"Channel Points Reward ID",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"`channel.channel_points_custom_reward_redemption.add` を個別 reward に絞りたい時の UUID。\
      空なら報酬を自動列挙してすべて購読する。",
				),
				PropertySpec::new(
					"fixed_channel",
					"Fixed Channel",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("（予約）将来 V1 channel に押し戻すブリッジ用。今はコメントのみ。"),
			],
		}
	}
}

async fn twitch_eventsub_compute(inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
	if !fired_exec.contains("__trigger__") {
		return Ok(NodeOutput::new());
	}
	let event_type = inputs.get("__event_type__").cloned().unwrap_or(SocketValue::String(String::new()));
	let actor_name = inputs.get("__actor_name__").cloned().unwrap_or(SocketValue::String(String::new()));
	let actor_login = inputs.get("__actor_login__").cloned().unwrap_or(SocketValue::String(String::new()));
	let broadcaster_login = inputs
		.get("__broadcaster_login__")
		.cloned()
		.unwrap_or(SocketValue::String(String::new()));
	let payload = inputs.get("__payload__").cloned().unwrap_or(SocketValue::Json(JsonValue::Null));
	let meta = inputs.get("__meta__").cloned().unwrap_or(SocketValue::Json(JsonValue::Null));
	Ok(NodeOutput::new()
		.set_data("event_type", event_type)
		.set_data("actor_name", actor_name)
		.set_data("actor_login", actor_login)
		.set_data("broadcaster_login", broadcaster_login)
		.set_data("payload", payload)
		.set_data("meta", meta)
		.fire_exec("exec_out"))
}

#[async_trait]
impl PureNode for TwitchEventsubIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		twitch_eventsub_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.channel_subscribe (δ-9 Part E)
// ---------------------------------------------------------------------
//
// V1 `State.channel_data` に push された `ChannelDatum` を **Flowgraph 側で再消費**するための ingress。
// 既存の web_input / voice / twitch と同じ「外部 trigger + echo compute」パターンに従うが、
// ポート構成が channel datum に特化している。Bridge 実装は `src/bridges/channel_subscribe.rs`。
//
// 出力:
//   - exec_out: trigger で発火
//   - channel / content / source_actor / is_final / meta:各 datum の該当フィールドを echo
//
// プロパティ（bridge が読む）:
//   - channels: List<String> — 監視対象のチャンネル名（空リスト = 全 channel）
//   - require_final: Bool = true — `is_final=true` のみ発火
//   - ignore_source_actors: List<String> — この actor 由来の datum を drop
//   - ignore_flowgraph_echo: Bool = true — `source_actor` が `flowgraph:` で始まる datum を drop
//   - require_flags: List<String> — 指定フラグを全部持つ datum のみ発火（空 = 無制限）
//   - drop_flags: List<String> — 指定フラグを 1 つでも持つ datum は drop

pub struct ChannelSubscribeIngressNode;

impl NodeDescriptor for ChannelSubscribeIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.channel_subscribe".into(),
			title: "Channel Subscribe Ingress".into(),
			category: "ingress".into(),
			description: Some(
				"V1 `State.channel_data` への push を監視し、channels / is_final / source_actor フィルタを通過した \
     ChannelDatum を TriggerEvent として流し込む。channel.emit の対称入口で、既存 Processor 時代の \
     channel_from 駆動パイプラインを Flowgraph で再現する基盤。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("__trigger__", "(internal trigger)"),
				PortSpec::input("__channel__", "(internal) channel", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("__content__", "(internal) content", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("__source_actor__", "(internal) source_actor", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("__is_final__", "(internal) is_final", SocketType::Bool).with_default(SocketValue::Bool(true)),
				PortSpec::input("__meta__", "(internal) meta", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("channel", "Channel", SocketType::String),
				PortSpec::output("content", "Content", SocketType::String),
				PortSpec::output("source_actor", "Source Actor", SocketType::String),
				PortSpec::output("is_final", "Is Final", SocketType::Bool),
				PortSpec::output("meta", "Meta", SocketType::Json),
			],
			properties: vec![
				PropertySpec::new(
					"channels",
					"Channels",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description("購読対象のチャンネル名リスト。空なら全チャンネルを受ける。"),
				PropertySpec::new("require_final", "Require is_final", SocketType::Bool, SocketValue::Bool(true))
					.description("true なら `is_final` フラグが立っている datum のみ発火。ストリーミング中間を無視する。"),
				PropertySpec::new(
					"ignore_source_actors",
					"Ignore Source Actors",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description("meta.source_actor がこの一覧に含まれる datum は drop。自 bot 由来のエコーを遮断する用途。"),
				PropertySpec::new(
					"ignore_flowgraph_echo",
					"Ignore Flowgraph Echo",
					SocketType::Bool,
					SocketValue::Bool(true),
				)
				.description(
					"true なら meta.source_actor が `flowgraph:` / `flowgraph` で始まる datum を drop。`channel.emit` \
      による自グラフ押し返しを防ぐ既定動作。",
				),
				PropertySpec::new(
					"require_flags",
					"Require Flags",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description("指定フラグをすべて持つ datum のみ発火。例: `[\"is_final\"]`。"),
				PropertySpec::new(
					"drop_flags",
					"Drop Flags",
					SocketType::List(Box::new(SocketType::String)),
					SocketValue::List(Vec::new()),
				)
				.description("指定フラグを 1 つでも持つ datum を drop。例: voice_vosk 由来を遮断する等。"),
			],
		}
	}
}

async fn channel_subscribe_compute(inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
	if !fired_exec.contains("__trigger__") {
		return Ok(NodeOutput::new());
	}
	let channel = inputs.get("__channel__").cloned().unwrap_or(SocketValue::String(String::new()));
	let content = inputs.get("__content__").cloned().unwrap_or(SocketValue::String(String::new()));
	let actor = inputs
		.get("__source_actor__")
		.cloned()
		.unwrap_or(SocketValue::String(String::new()));
	let is_final = inputs.get("__is_final__").cloned().unwrap_or(SocketValue::Bool(true));
	let meta = inputs.get("__meta__").cloned().unwrap_or(SocketValue::Json(JsonValue::Null));
	Ok(NodeOutput::new()
		.set_data("channel", channel)
		.set_data("content", content)
		.set_data("source_actor", actor)
		.set_data("is_final", is_final)
		.set_data("meta", meta)
		.fire_exec("exec_out"))
}

#[async_trait]
impl PureNode for ChannelSubscribeIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		channel_subscribe_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.vmc_udp（Phase M1）
// ---------------------------------------------------------------------
//
// VMC 互換の **生 UDP** を [`crate::bridges::vmc_ingress`] が受信し、各データグラムごとに
// `TriggerEvent` を投入する。`content` にはペイロードの **Base64**（`__content__` 経由で echo）、
// `meta` に `remote` / `byte_len` / `encoding` を載せる。

pub struct VmcUdpIngressNode;

impl NodeDescriptor for VmcUdpIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.vmc_udp".into(),
			title: "VMC UDP Ingress".into(),
			category: "ingress".into(),
			description: Some("VMC 互換の生 UDP を受信し、各データグラムを ingress echo で下流へ流す。`content` は Base64 文字列。".into()),
			inputs: ingress_inputs(),
			outputs: ingress_outputs(),
			properties: vec![
				PropertySpec::new("bind", "Bind", SocketType::String, SocketValue::String(String::new()))
					.description("受信 UDP の \"host:port\"（例 \"0.0.0.0:39539\"）。空のときブリッジは起動しない。"),
				PropertySpec::new(
					"fixed_channel",
					"Source Kind Override",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("空なら `source_kind` は `vmc_udp`。任意のラベルに上書き可能。"),
			],
		}
	}
}

#[async_trait]
impl PureNode for VmcUdpIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		ingress_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// ingress.osc_udp（Phase ρ）
// ---------------------------------------------------------------------
//
// 汎用 **OSC over UDP**（および任意バイナリ）の入口。[`crate::bridges::osc_ingress`] が受信し、
// `__meta__` に `profile: "osc_udp"` を載せる（VMC ingress のメタと区別）。

pub struct OscUdpIngressNode;

impl NodeDescriptor for OscUdpIngressNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.ingress.osc_udp".into(),
			title: "OSC UDP Ingress".into(),
			category: "ingress".into(),
			description: Some(
				"汎用 OSC（UDP データグラム）を受信し、ingress echo で下流へ流す。`content` は Base64。`__meta__.profile` は `osc_udp`。".into(),
			),
			inputs: ingress_inputs(),
			outputs: ingress_outputs(),
			properties: vec![
				PropertySpec::new("bind", "Bind", SocketType::String, SocketValue::String(String::new()))
					.description("受信 UDP の \"host:port\"。空のときブリッジは起動しない。"),
				PropertySpec::new(
					"fixed_channel",
					"Source Kind Override",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("空なら `source_kind` は `osc_udp`。任意のラベルに上書き可能。"),
			],
		}
	}
}

#[async_trait]
impl PureNode for OscUdpIngressNode {
	async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _props: &InputMap, inputs: &InputMap, fired_exec: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		ingress_compute(inputs, fired_exec).await
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::engine::{create_trigger_bus, FlowgraphBuilder, PortRef};
	use crate::flowgraph::node::{ExecCtx, NodeImpl, TriggerEvent};
	use crate::flowgraph::nodes::log::LogNode;
	use std::sync::Arc;
	use std::time::Duration;

	#[tokio::test]
	async fn web_input_ingress_forwards_content_to_log_via_external_trigger() {
		// graph: WebInputIngress --content--> Log
		//                         --exec_out--> Log.exec_in
		let mut b = FlowgraphBuilder::new();
		b.add_node("in", NodeImpl::pure(Arc::new(WebInputIngressNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("in", "exec_out"), PortRef::new("log", "exec_in"));
		b.connect(PortRef::new("in", "content"), PortRef::new("log", "value"));
		let mut prog = b.build().expect("build");

		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		// 250ms 後に shutdown、その前に 1 回 trigger を打つ
		let sender = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(30)).await;
			let _ = external.send(
				TriggerEvent::new("in")
					.with_exec("__trigger__")
					.with_override("__content__", SocketValue::String("hello via web".into()))
					.with_override("__source_actor__", SocketValue::String("alice".into())),
			);
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(150));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		assert_eq!(ctx.trace.len(), 1);
		assert!(ctx.trace[0].contains("hello via web"), "trace: {:?}", ctx.trace);
	}

	#[tokio::test]
	async fn multiple_external_triggers_process_sequentially() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("in", NodeImpl::pure(Arc::new(TwitchIngressNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("in", "exec_out"), PortRef::new("log", "exec_in"));
		b.connect(PortRef::new("in", "content"), PortRef::new("log", "value"));
		let mut prog = b.build().expect("build");

		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		let sender = tokio::spawn(async move {
			for i in 0..3 {
				tokio::time::sleep(Duration::from_millis(10)).await;
				let _ = external.send(
					TriggerEvent::new("in")
						.with_exec("__trigger__")
						.with_override("__content__", SocketValue::String(format!("chat {i}"))),
				);
			}
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(150));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		assert_eq!(ctx.trace.len(), 3);
		assert!(ctx.trace[0].contains("chat 0"));
		assert!(ctx.trace[1].contains("chat 1"));
		assert!(ctx.trace[2].contains("chat 2"));
	}

	#[tokio::test]
	async fn ingress_without_trigger_is_noop() {
		let node = VoiceIngressNode;
		let out = node.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
		assert!(out.data.is_empty());
	}

	#[tokio::test]
	async fn channel_subscribe_forwards_all_fields() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("sub", NodeImpl::pure(Arc::new(ChannelSubscribeIngressNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("sub", "exec_out"), PortRef::new("log", "exec_in"));
		b.connect(PortRef::new("sub", "content"), PortRef::new("log", "value"));
		let mut prog = b.build().expect("build");

		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		let sender = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(30)).await;
			let _ = external.send(
				TriggerEvent::new("sub")
					.with_exec("__trigger__")
					.with_override("__channel__", SocketValue::String("user".into()))
					.with_override("__content__", SocketValue::String("subscribed text".into()))
					.with_override("__source_actor__", SocketValue::String("alice".into()))
					.with_override("__is_final__", SocketValue::Bool(true)),
			);
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(150));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		assert_eq!(ctx.trace.len(), 1);
		assert!(ctx.trace[0].contains("subscribed text"), "trace: {:?}", ctx.trace);
	}

	#[tokio::test]
	async fn channel_subscribe_without_trigger_is_noop() {
		let node = ChannelSubscribeIngressNode;
		let out = node.compute(&crate::flowgraph::node::PureEvalHost::default(), &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
		assert!(out.data.is_empty());
	}

	#[tokio::test]
	async fn vmc_udp_ingress_echoes_base64_content() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("vmc", NodeImpl::pure(Arc::new(VmcUdpIngressNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("vmc", "exec_out"), PortRef::new("log", "exec_in"));
		b.connect(PortRef::new("vmc", "content"), PortRef::new("log", "value"));
		let mut prog = b.build().expect("build");

		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		let sender = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(30)).await;
			let _ = external.send(
				TriggerEvent::new("vmc")
					.with_exec("__trigger__")
					.with_override("__content__", SocketValue::String("QUJD".into()))
					.with_override("__source_actor__", SocketValue::String("127.0.0.1:39539".into()))
					.with_override("__source_kind__", SocketValue::String("vmc_udp".into())),
			);
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(150));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		assert_eq!(ctx.trace.len(), 1);
		assert!(ctx.trace[0].contains("QUJD"), "trace: {:?}", ctx.trace);
	}
}
