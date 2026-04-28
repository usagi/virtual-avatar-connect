//! Flowgraph ノードカタログ（spec §6.5 / §8.2-4）。
//!
//! `feature` 文字列（例 `flowgraph.literal.string`）から
//! `NodeSpec` と `NodeImpl` を取得するためのグローバルレジストリ。
//!
//! Loader（δ-5）は `*.flowgraph.toml` の `[[nodes]] feature = "..."` を
//! ここで解決する。GUI（δ-6）もパレットをここから生成する。
//!
//! ## 設計
//!
//! - 各組み込みノードは singleton Arc で一度だけ構築し、
//!   同じ feature に属するインスタンスは Arc clone でコストゼロで複製する。
//! - Stateful ノードも「driver は singleton、state slot は add 時に毎回 `init_state`」で扱える
//!   （`NodeImpl::stateful` が内部で `init_state` を呼ぶ）。
//! - `NodeSpec` はノードごとに静的（`SequenceNode` のように出力数が動的に変わるものは除外）。
//!
//! ## 未登録ノード
//!
//! - `flowgraph.flow.sequence`: 出力ポート数が instance 固有のため、現状では loader 経由で
//!   生成不可。テスト用に `SequenceNode::new(n)` を直接使う。
//!   δ-6 以降で properties 駆動化する予定。

use crate::flowgraph::node::{EffectfulNode, NodeImpl, NodeSpec, PureNode, StatefulNode};
use crate::flowgraph::nodes;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

/// 登録時に singleton として保持する Arc。
enum NodeArc {
	Pure(Arc<dyn PureNode>),
	Stateful(Arc<dyn StatefulNode>),
	Effectful(Arc<dyn EffectfulNode>),
}

impl NodeArc {
	fn describe(&self) -> NodeSpec {
		match self {
			NodeArc::Pure(n) => n.describe(),
			NodeArc::Stateful(n) => n.describe(),
			NodeArc::Effectful(n) => n.describe(),
		}
	}

	fn control_triggerable(&self) -> bool {
		match self {
			NodeArc::Pure(n) => n.control_triggerable(),
			NodeArc::Stateful(n) => n.control_triggerable(),
			NodeArc::Effectful(n) => n.control_triggerable(),
		}
	}

	fn make_impl(&self) -> NodeImpl {
		match self {
			NodeArc::Pure(n) => NodeImpl::pure(n.clone()),
			NodeArc::Stateful(n) => NodeImpl::stateful(n.clone()),
			NodeArc::Effectful(n) => NodeImpl::effectful(n.clone()),
		}
	}
}

/// ノードカタログ。`feature` 文字列をキーに singleton を引く。
pub struct NodeRegistry {
	map: HashMap<String, NodeArc>,
}

impl NodeRegistry {
	pub fn empty() -> Self {
		Self { map: HashMap::new() }
	}

	pub fn register_pure(&mut self, node: Arc<dyn PureNode>) {
		let feature = node.describe().feature;
		self.map.insert(feature, NodeArc::Pure(node));
	}

	pub fn register_stateful(&mut self, node: Arc<dyn StatefulNode>) {
		let feature = node.describe().feature;
		self.map.insert(feature, NodeArc::Stateful(node));
	}

	pub fn register_effectful(&mut self, node: Arc<dyn EffectfulNode>) {
		let feature = node.describe().feature;
		self.map.insert(feature, NodeArc::Effectful(node));
	}

	/// feature が登録されているか。
	pub fn contains(&self, feature: &str) -> bool {
		self.map.contains_key(feature)
	}

	/// 登録済み feature の `NodeSpec`。
	pub fn spec(&self, feature: &str) -> Option<NodeSpec> {
		self.map.get(feature).map(|n| n.describe())
	}

	/// Phase φ-2: Control API の Flowgraph Trigger Endpoint が当該 feature の
	/// 外部発火を許可するかどうか。`NodeDescriptor::control_triggerable()` の投影。
	///
	/// 未登録 feature は `false` を返す（知らないノードは発火不可）。
	pub fn is_control_triggerable(&self, feature: &str) -> bool {
		self.map.get(feature).map(|n| n.control_triggerable()).unwrap_or(false)
	}

	/// 登録済み feature から **新しい** `NodeImpl` を作る（Stateful は state slot を新規確保）。
	pub fn make_impl(&self, feature: &str) -> Option<NodeImpl> {
		self.map.get(feature).map(|n| n.make_impl())
	}

	/// 登録されている全 feature 名。ソートされた順。
	pub fn features(&self) -> Vec<String> {
		let mut keys: Vec<String> = self.map.keys().cloned().collect();
		keys.sort();
		keys
	}

	/// 登録されている全 `NodeSpec`。feature 名ソート順。
	pub fn all_specs(&self) -> Vec<NodeSpec> {
		self.features().into_iter().filter_map(|f| self.spec(&f)).collect()
	}
}

/// δ-5 時点の組み込みノード一式を登録したレジストリを返す。
pub fn default_registry() -> NodeRegistry {
	let mut r = NodeRegistry::empty();

	// --- literal (§6.5 literal) ---
	r.register_pure(Arc::new(nodes::literal::BoolLiteralNode));
	r.register_pure(Arc::new(nodes::literal::IntLiteralNode));
	r.register_pure(Arc::new(nodes::literal::FloatLiteralNode));
	r.register_pure(Arc::new(nodes::literal::StringLiteralNode));
	// --- library boundary (Phase λ) ---
	r.register_pure(Arc::new(nodes::library_boundary::LibraryInputNode));
	r.register_pure(Arc::new(nodes::library_boundary::LibraryOutputNode));
	r.register_pure(Arc::new(nodes::literal::JsonLiteralNode));

	// --- flow ---
	r.register_pure(Arc::new(nodes::flow::BranchNode));
	r.register_pure(Arc::new(nodes::flow::GateNode));
	// --- mode (RM-2 / RM-5) ---
	r.register_pure(Arc::new(nodes::mode::ModeGetNode));
	r.register_pure(Arc::new(nodes::mode::ModeEqualsNode));
	r.register_effectful(Arc::new(nodes::mode::ModeTransitNode));
	// SequenceNode: 動的 schema のため登録しない（上記 doc 参照）。

	// --- logic ---
	r.register_pure(Arc::new(nodes::logic::AndNode));
	r.register_pure(Arc::new(nodes::logic::OrNode));
	r.register_pure(Arc::new(nodes::logic::XorNode));
	r.register_pure(Arc::new(nodes::logic::NotNode));

	// --- compare ---
	r.register_pure(Arc::new(nodes::compare::EqNode));
	r.register_pure(Arc::new(nodes::compare::NeqNode));
	r.register_pure(Arc::new(nodes::compare::IntLtNode));
	r.register_pure(Arc::new(nodes::compare::IntGtNode));
	r.register_pure(Arc::new(nodes::compare::IntLeNode));
	r.register_pure(Arc::new(nodes::compare::IntGeNode));
	r.register_pure(Arc::new(nodes::compare::FloatLtNode));
	r.register_pure(Arc::new(nodes::compare::FloatGtNode));

	// --- unit (Phase ξ-2) ---
	r.register_pure(Arc::new(nodes::unit::UnitAssignNode));
	r.register_pure(Arc::new(nodes::unit::UnitConvertNode));
	r.register_pure(Arc::new(nodes::unit::UnitStripNode));
	r.register_pure(Arc::new(nodes::unit::UnitGetUnitStringNode));
	r.register_pure(Arc::new(nodes::unit::UnitGetDimensionStringNode));
	r.register_pure(Arc::new(nodes::unit::UnitSameDimensionNode));
	r.register_pure(Arc::new(nodes::unit::UnitToJsonNode));

	// --- util.format (xi-4) ---
	r.register_pure(Arc::new(nodes::util_format::UtilFormatNode));

	// --- math ---
	r.register_pure(Arc::new(nodes::math::IntAddNode));
	r.register_pure(Arc::new(nodes::math::IntSubNode));
	r.register_pure(Arc::new(nodes::math::IntMulNode));
	r.register_pure(Arc::new(nodes::math::IntDivNode));
	r.register_pure(Arc::new(nodes::math::IntModNode));
	r.register_pure(Arc::new(nodes::math::FloatAddNode));
	r.register_pure(Arc::new(nodes::math::FloatSubNode));
	r.register_pure(Arc::new(nodes::math::FloatMulNode));
	r.register_pure(Arc::new(nodes::math::FloatDivNode));
	// Phase o-1: math 42-node expansion
	r.register_pure(Arc::new(nodes::math::IntMinNode));
	r.register_pure(Arc::new(nodes::math::IntMaxNode));
	r.register_pure(Arc::new(nodes::math::IntAbsNode));
	r.register_pure(Arc::new(nodes::math::IntSignNode));
	r.register_pure(Arc::new(nodes::math::IntClampNode));
	r.register_pure(Arc::new(nodes::math::FloatAbsNode));
	r.register_pure(Arc::new(nodes::math::FloatSignNode));
	r.register_pure(Arc::new(nodes::math::FloatFloorNode));
	r.register_pure(Arc::new(nodes::math::FloatCeilNode));
	r.register_pure(Arc::new(nodes::math::FloatRoundNode));
	r.register_pure(Arc::new(nodes::math::FloatSqrtNode));
	r.register_pure(Arc::new(nodes::math::FloatExpNode));
	r.register_pure(Arc::new(nodes::math::FloatLnNode));
	r.register_pure(Arc::new(nodes::math::FloatLog2Node));
	r.register_pure(Arc::new(nodes::math::FloatLog10Node));
	r.register_pure(Arc::new(nodes::math::FloatSinhNode));
	r.register_pure(Arc::new(nodes::math::FloatCoshNode));
	r.register_pure(Arc::new(nodes::math::FloatTanhNode));
	r.register_pure(Arc::new(nodes::math::FloatAsinhNode));
	r.register_pure(Arc::new(nodes::math::FloatAcoshNode));
	r.register_pure(Arc::new(nodes::math::FloatAtanhNode));
	r.register_pure(Arc::new(nodes::math::FloatMinNode));
	r.register_pure(Arc::new(nodes::math::FloatMaxNode));
	r.register_pure(Arc::new(nodes::math::FloatClampNode));
	r.register_pure(Arc::new(nodes::math::FloatSinNode));
	r.register_pure(Arc::new(nodes::math::FloatCosNode));
	r.register_pure(Arc::new(nodes::math::FloatTanNode));
	r.register_pure(Arc::new(nodes::math::FloatAsinNode));
	r.register_pure(Arc::new(nodes::math::FloatAcosNode));
	r.register_pure(Arc::new(nodes::math::FloatAtanNode));
	r.register_pure(Arc::new(nodes::math::FloatAtan2Node));
	r.register_pure(Arc::new(nodes::math::FloatPowNode));
	r.register_pure(Arc::new(nodes::math::FloatLerpNode));
	r.register_pure(Arc::new(nodes::math::FloatInverseLerpNode));
	r.register_pure(Arc::new(nodes::math::FloatRemapNode));
	r.register_pure(Arc::new(nodes::math::FloatSmoothstepNode));
	r.register_pure(Arc::new(nodes::math::FloatDegToRadNode));
	r.register_pure(Arc::new(nodes::math::FloatRadToDegNode));
	r.register_pure(Arc::new(nodes::math::FloatNormalizeAngleDeg0To360Node));
	r.register_pure(Arc::new(nodes::math::FloatNormalizeAngleDegSignedNode));
	r.register_pure(Arc::new(nodes::math::FloatNormalizeAngleRad0To2piNode));
	r.register_pure(Arc::new(nodes::math::FloatNormalizeAngleRadSignedNode));

	// --- easing (Phase o-2) ---
	r.register_pure(Arc::new(nodes::easing::EasingApplyNode));

	// --- datetime (Phase pi-5) ---
	r.register_pure(Arc::new(nodes::datetime::DateTimeNowNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeParseNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeFormatNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeAddDurationNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeSubDurationNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeDiffNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeEpochMsNode));
	r.register_pure(Arc::new(nodes::datetime::DateTimeFromEpochMsNode));

	// --- vec (Phase o-3) ---
	r.register_pure(Arc::new(nodes::vec::Vec2MakeNode));
	r.register_pure(Arc::new(nodes::vec::Vec2UnpackNode));
	r.register_pure(Arc::new(nodes::vec::Vec2AddNode));
	r.register_pure(Arc::new(nodes::vec::Vec2SubNode));
	r.register_pure(Arc::new(nodes::vec::Vec2ScaleNode));
	r.register_pure(Arc::new(nodes::vec::Vec2DotNode));
	r.register_pure(Arc::new(nodes::vec::Vec2LengthNode));
	r.register_pure(Arc::new(nodes::vec::Vec2NormalizeNode));
	r.register_pure(Arc::new(nodes::vec::Vec2LerpNode));
	r.register_pure(Arc::new(nodes::vec::Vec2DistanceNode));
	r.register_pure(Arc::new(nodes::vec::Vec3MakeNode));
	r.register_pure(Arc::new(nodes::vec::Vec3UnpackNode));
	r.register_pure(Arc::new(nodes::vec::Vec3AddNode));
	r.register_pure(Arc::new(nodes::vec::Vec3SubNode));
	r.register_pure(Arc::new(nodes::vec::Vec3ScaleNode));
	r.register_pure(Arc::new(nodes::vec::Vec3DotNode));
	r.register_pure(Arc::new(nodes::vec::Vec3LengthNode));
	r.register_pure(Arc::new(nodes::vec::Vec3NormalizeNode));
	r.register_pure(Arc::new(nodes::vec::Vec3LerpNode));
	r.register_pure(Arc::new(nodes::vec::Vec3DistanceNode));

	// --- string_ops ---
	r.register_pure(Arc::new(nodes::string_ops::StringConcatNode));
	r.register_pure(Arc::new(nodes::string_ops::StringLenNode));
	r.register_pure(Arc::new(nodes::string_ops::StringContainsNode));
	r.register_pure(Arc::new(nodes::string_ops::StringReplaceNode));
	r.register_pure(Arc::new(nodes::string_ops::StringSplitNode));
	r.register_pure(Arc::new(nodes::string_ops::StringJoinNode));

	// --- regex_ops ---
	r.register_pure(Arc::new(nodes::regex_ops::RegexReplaceNode));

	// --- convert ---
	r.register_pure(Arc::new(nodes::convert::BoolToStringNode));
	r.register_pure(Arc::new(nodes::convert::IntToStringNode));
	r.register_pure(Arc::new(nodes::convert::StringToIntNode));
	r.register_pure(Arc::new(nodes::convert::FloatToStringNode));
	r.register_pure(Arc::new(nodes::convert::StringToFloatNode));
	r.register_pure(Arc::new(nodes::convert::IntToFloatNode));
	r.register_pure(Arc::new(nodes::convert::FloatToIntNode));

	// --- json_ops ---
	r.register_pure(Arc::new(nodes::json_ops::JsonParseNode));
	r.register_pure(Arc::new(nodes::json_ops::JsonStringifyNode));
	r.register_pure(Arc::new(nodes::json_ops::JsonGetNode));
	r.register_effectful(Arc::new(nodes::http::HttpRequestNode));

	// --- collection ---
	r.register_pure(Arc::new(nodes::collection::ListLenNode));
	r.register_pure(Arc::new(nodes::collection::ListGetNode));
	r.register_pure(Arc::new(nodes::collection::ListIsEmptyNode));
	r.register_pure(Arc::new(nodes::collection::MapGetNode));
	r.register_pure(Arc::new(nodes::collection::MapKeysNode));
	r.register_pure(Arc::new(nodes::collection::MapHasNode));

	// --- command ---
	r.register_pure(Arc::new(nodes::command::CommandMatchNode));
	r.register_effectful(Arc::new(nodes::command::CommandSetNode));

	// --- table (η-2) ---
	r.register_pure(Arc::new(nodes::table_ops::TableFromJsonNode));
	r.register_pure(Arc::new(nodes::table_ops::TableToJsonNode));
	r.register_effectful(Arc::new(nodes::table_ops::TableLoadTsvNode));
	r.register_effectful(Arc::new(nodes::table_ops::TableWriteTsvNode));

	// --- dictionary (η-3: Stateful Replace/Match + Pure Learn/Forget) ---
	r.register_stateful(Arc::new(nodes::dictionary::DictionaryReplaceNode));
	r.register_stateful(Arc::new(nodes::dictionary::DictionaryMatchNode));
	r.register_pure(Arc::new(nodes::dictionary::DictionaryLearnNode));
	r.register_pure(Arc::new(nodes::dictionary::DictionaryForgetNode));

	// --- ingress (δ-3d, δ-9 Part E) ---
	r.register_pure(Arc::new(nodes::ingress::WebInputIngressNode));
	r.register_pure(Arc::new(nodes::ingress::VoiceIngressNode));
	r.register_pure(Arc::new(nodes::ingress::TwitchIngressNode));
	r.register_pure(Arc::new(nodes::ingress::TwitchEventsubIngressNode));
	r.register_pure(Arc::new(nodes::ingress::ChannelSubscribeIngressNode));
	r.register_pure(Arc::new(nodes::ingress::VmcUdpIngressNode));
	r.register_pure(Arc::new(nodes::ingress::OscUdpIngressNode));
	r.register_pure(Arc::new(nodes::motion_vmc::VmcParseNode));
	r.register_pure(Arc::new(nodes::motion_vmc::MotionFilterNode));
	r.register_pure(Arc::new(nodes::motion_vmc::MotionMapNode));
	r.register_effectful(Arc::new(nodes::osc_send::OscSendNode));
	r.register_effectful(Arc::new(nodes::obs::ObsRequestNode));
	r.register_effectful(Arc::new(nodes::obs::ObsSetCurrentProgramSceneNode));
	r.register_effectful(Arc::new(nodes::vmc_send::VmcSendBonePosNode));
	r.register_effectful(Arc::new(nodes::vmc_send::VmcSendRootPosNode));
	r.register_pure(Arc::new(nodes::vmc_extract::VmcExtractBonePosNode));
	r.register_pure(Arc::new(nodes::vmc_extract::VmcExtractRootPosNode));
	r.register_pure(Arc::new(nodes::vmc_extract::VmcExtractBlendshapeNode));
	r.register_effectful(Arc::new(nodes::vrchat_osc::VrchatAvatarParameterFloatNode));
	r.register_effectful(Arc::new(nodes::vrchat_osc::VrchatAvatarParameterIntNode));
	r.register_effectful(Arc::new(nodes::vrchat_osc::VrchatAvatarParameterBoolNode));
	r.register_effectful(Arc::new(nodes::vrchat_osc::VrchatChatboxInputNode));
	r.register_effectful(Arc::new(nodes::vrchat_osc::VrchatChatboxTypingNode));

	// --- state (δ-3c) ---
	r.register_stateful(Arc::new(nodes::state::BoolStateNode));
	r.register_stateful(Arc::new(nodes::state::IntCounterNode));
	r.register_stateful(Arc::new(nodes::state::LatchNode));
	r.register_stateful(Arc::new(nodes::state::AccumulatorNode));

	// --- delay / rate_limit / timer_interval (Phase o-4) ---
	r.register_stateful(Arc::new(nodes::delay::DelayNode));
	r.register_stateful(Arc::new(nodes::rate_limit::RateLimitNode));
	r.register_stateful(Arc::new(nodes::timer_interval::TimerIntervalNode));

	// --- signal util + random + noise (Phase o-5) ---
	r.register_stateful(Arc::new(nodes::signal_util::EdgeDetectNode));
	r.register_stateful(Arc::new(nodes::signal_util::PrevValueNode));
	r.register_stateful(Arc::new(nodes::signal_util::SampleHoldNode));
	r.register_stateful(Arc::new(nodes::signal_util::DebounceNode));
	r.register_stateful(Arc::new(nodes::signal_util::ThrottleNode));
	r.register_pure(Arc::new(nodes::random_noise::RandomUniformIntNode));
	r.register_pure(Arc::new(nodes::random_noise::RandomUniformFloatNode));
	r.register_pure(Arc::new(nodes::random_noise::RandomNormalNode));
	r.register_pure(Arc::new(nodes::random_noise::NoisePerlin1dNode));
	r.register_pure(Arc::new(nodes::random_noise::NoisePerlin2dNode));

	// --- log (effectful) ---
	r.register_effectful(Arc::new(nodes::log::LogNode));

	// --- translate ---
	r.register_effectful(Arc::new(nodes::translate_gas::TranslateGasNode));
	r.register_effectful(Arc::new(nodes::translate_libre::TranslateLibreNode));

	// --- screenshot / ocr ---
	r.register_effectful(Arc::new(nodes::screenshot::ScreenshotCaptureNode));
	r.register_effectful(Arc::new(nodes::ocr::OcrRecognizeNode));

	// --- tts ---
	r.register_effectful(Arc::new(nodes::tts::TtsSpeakNode));

	// --- twitch (δ-4d, ζ-1) ---
	r.register_effectful(Arc::new(nodes::twitch::GetTokenNode));
	r.register_effectful(Arc::new(nodes::twitch::ValidateTokenNode));
	r.register_effectful(Arc::new(nodes::twitch::UserIdByLoginNode));
	r.register_effectful(Arc::new(nodes::twitch::ChatSendNode));
	r.register_effectful(Arc::new(nodes::twitch::BanNode));
	r.register_effectful(Arc::new(nodes::twitch::TimeoutNode));

	// δ-9 Part C: channel.emit (Flowgraph → State.channel_data 終端)
	r.register_effectful(Arc::new(nodes::channel::ChannelEmitNode));

	r
}

/// プロセス全体で共有するデフォルト registry。
static REGISTRY: LazyLock<NodeRegistry> = LazyLock::new(default_registry);

/// デフォルト registry への参照を得る（Loader / GUI 用）。
pub fn registry() -> &'static NodeRegistry {
	&REGISTRY
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_registry_contains_core_features() {
		let r = registry();
		for feature in [
			"flowgraph.literal.string",
			"flowgraph.literal.int",
			"flowgraph.literal.bool",
			"flowgraph.literal.float",
			"flowgraph.literal.json",
			"flowgraph.flow.branch",
			"flowgraph.flow.gate",
			"flowgraph.mode.get",
			"flowgraph.mode.equals",
			"flowgraph.mode.transit",
			"flowgraph.logic.and",
			"flowgraph.logic.or",
			"flowgraph.logic.xor",
			"flowgraph.logic.not",
			"flowgraph.compare.eq",
			"flowgraph.compare.neq",
			"flowgraph.convert.bool_to_string",
			"flowgraph.string.concat",
			"flowgraph.string.len",
			"flowgraph.string.contains",
			"flowgraph.string.replace",
			"flowgraph.string.split",
			"flowgraph.string.join",
			"flowgraph.regex.replace",
			"flowgraph.json.parse",
			"flowgraph.json.stringify",
			"flowgraph.json.get",
			"flowgraph.http.request",
			"flowgraph.list.len",
			"flowgraph.list.get",
			"flowgraph.list.is_empty",
			"flowgraph.map.get",
			"flowgraph.map.keys",
			"flowgraph.map.has",
			"flowgraph.state.bool",
			"flowgraph.state.int_counter",
			"flowgraph.state.latch",
			"flowgraph.state.accumulator",
			"flowgraph.util.delay",
			"flowgraph.util.rate_limit",
			"flowgraph.util.timer_interval",
			"flowgraph.util.edge_detect",
			"flowgraph.util.prev_value",
			"flowgraph.util.sample_hold",
			"flowgraph.util.debounce",
			"flowgraph.util.throttle",
			"flowgraph.random.uniform_int",
			"flowgraph.random.uniform_float",
			"flowgraph.random.normal",
			"flowgraph.noise.perlin_1d",
			"flowgraph.noise.perlin_2d",
			"flowgraph.util.log",
			"flowgraph.tts.speak",
			"flowgraph.library.input",
			"flowgraph.library.output",
			"flowgraph.twitch.chat_send",
			"flowgraph.twitch.get_token",
			"flowgraph.twitch.validate_token",
			"flowgraph.twitch.user_id_by_login",
			"flowgraph.twitch.ban",
			"flowgraph.twitch.timeout",
			"flowgraph.translate.gas",
			"flowgraph.translate.libre",
			"flowgraph.ingress.web_input",
			"flowgraph.ingress.voice",
			"flowgraph.ingress.twitch",
			"flowgraph.ingress.twitch_eventsub",
			"flowgraph.ingress.channel_subscribe",
			"flowgraph.ingress.vmc_udp",
			"flowgraph.ingress.osc_udp",
			"flowgraph.motion.vmc_parse",
			"flowgraph.motion.filter",
			"flowgraph.motion.map",
			"flowgraph.osc.send",
			"flowgraph.obs.request",
			"flowgraph.obs.set_current_program_scene",
			"flowgraph.vmc.send_bone_pos",
			"flowgraph.vmc.send_root_pos",
			"flowgraph.vmc.extract_bone_pos",
			"flowgraph.vmc.extract_root_pos",
			"flowgraph.vmc.extract_blendshape",
			"flowgraph.vrchat.avatar_parameter_float",
			"flowgraph.vrchat.avatar_parameter_int",
			"flowgraph.vrchat.avatar_parameter_bool",
			"flowgraph.vrchat.chatbox_input",
			"flowgraph.vrchat.chatbox_typing",
			"flowgraph.channel.emit",
			"flowgraph.unit.assign",
			"flowgraph.unit.convert",
			"flowgraph.unit.strip",
			"flowgraph.unit.get_unit_string",
			"flowgraph.unit.get_dim_string",
			"flowgraph.unit.same_dimension",
			"flowgraph.unit.to_json",
			"flowgraph.util.format",
			// Phase o-1 representative samples
			"flowgraph.math.abs_float",
			"flowgraph.math.clamp_float",
			"flowgraph.math.lerp",
			"flowgraph.math.smoothstep",
			"flowgraph.math.sin",
			"flowgraph.math.atan2",
			"flowgraph.math.sqrt",
			"flowgraph.math.pow",
			"flowgraph.math.sinh",
			"flowgraph.math.asinh",
			"flowgraph.math.deg_to_rad",
			"flowgraph.math.normalize_angle_deg_0_360",
			"flowgraph.math.normalize_angle_rad_signed",
			"flowgraph.easing.apply",
			// Phase pi-5: datetime nodes
			"flowgraph.datetime.now",
			"flowgraph.datetime.parse",
			"flowgraph.datetime.format",
			"flowgraph.datetime.add_duration",
			"flowgraph.datetime.sub_duration",
			"flowgraph.datetime.diff",
			"flowgraph.datetime.epoch_ms",
			"flowgraph.datetime.from_epoch_ms",
			"flowgraph.vec2.make",
			"flowgraph.vec2.length",
			"flowgraph.vec2.normalize",
			"flowgraph.vec3.make",
			"flowgraph.vec3.dot",
			"flowgraph.vec3.lerp",
		] {
			assert!(r.contains(feature), "registry にコア feature '{feature}' が登録されていない");
			assert_eq!(r.spec(feature).unwrap().feature, feature);
		}
	}

	#[test]
	fn make_impl_pure_returns_pure() {
		let r = registry();
		let impl_ = r.make_impl("flowgraph.literal.string").unwrap();
		assert!(impl_.is_pure());
	}

	#[test]
	fn make_impl_stateful_returns_stateful_with_fresh_state() {
		let r = registry();
		let impl_a = r.make_impl("flowgraph.state.int_counter").unwrap();
		let impl_b = r.make_impl("flowgraph.state.int_counter").unwrap();
		assert!(impl_a.is_stateful());
		assert!(impl_b.is_stateful());
	}

	#[test]
	fn make_impl_effectful_returns_effectful() {
		let r = registry();
		let impl_ = r.make_impl("flowgraph.util.log").unwrap();
		assert!(impl_.is_effectful());
	}

	#[test]
	fn unknown_feature_returns_none() {
		let r = registry();
		assert!(r.make_impl("flowgraph.does.not.exist").is_none());
		assert!(r.spec("flowgraph.does.not.exist").is_none());
	}
}
