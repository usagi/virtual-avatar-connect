//! v1 `conf.toml` → v2 Flowgraph 変換ツール（best-effort、δ-8d）。
//!
//! 設計:
//! - 入力 `conf.toml` は **`toml_edit::DocumentMut`** で読み、processors 配列を抽出。
//! - トップレベル設定（`[twitch]` / `[[ai.personas]]` / `[libretranslate]` / `[control_api]` /
//!   `[browser_source]` / `workers` / `log_level` / `web_ui_address` / `run_with` 等）はそのまま保持。
//! - `[[processors]]` は **全削除** して、`flowgraph_dir = "flowgraph"` を新規キーとして付ける。
//! - 各 `[[processors]]` エントリは **feature 別に flowgraph フラグメント**として書き出す
//!   （`<out>/flowgraph/<topic>/main.flowgraph.toml`）。
//! - `voice` は暫定的に `[[processors]]` を残す（δ-9 で `[voice]` サービス化予定）。
//!
//! 非対応 feature は warning を出して skip。`--migrate-strict` なら warning も exit 1。

use std::path::{Path, PathBuf};
use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, Value};

// ---------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
	Converted,
	Warning,
	Error,
}

#[derive(Debug, Clone)]
pub struct ReportEntry {
	pub kind: EntryKind,
	pub feature: Option<String>,
	pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
	pub entries: Vec<ReportEntry>,
	pub written_files: Vec<PathBuf>,
	pub strict: bool,
}

impl MigrationReport {
	pub fn converted(&mut self, feature: impl Into<String>, msg: impl Into<String>) {
		self.entries.push(ReportEntry {
			kind: EntryKind::Converted,
			feature: Some(feature.into()),
			message: msg.into(),
		});
	}
	pub fn warn(&mut self, feature: Option<String>, msg: impl Into<String>) {
		self.entries.push(ReportEntry {
			kind: EntryKind::Warning,
			feature,
			message: msg.into(),
		});
	}
	pub fn error(&mut self, feature: Option<String>, msg: impl Into<String>) {
		self.entries.push(ReportEntry {
			kind: EntryKind::Error,
			feature,
			message: msg.into(),
		});
	}

	pub fn has_errors(&self) -> bool {
		self.entries.iter().any(|e| e.kind == EntryKind::Error)
			|| (self.strict && self.entries.iter().any(|e| e.kind == EntryKind::Warning))
	}

	pub fn summary(&self) -> String {
		let converted = self.entries.iter().filter(|e| e.kind == EntryKind::Converted).count();
		let warnings = self.entries.iter().filter(|e| e.kind == EntryKind::Warning).count();
		let errors = self.entries.iter().filter(|e| e.kind == EntryKind::Error).count();
		let mut s = format!(
			"=== Migration Summary ===\n  converted: {converted}\n  warnings:  {warnings}\n  errors:    {errors}\n  files:     {} written\n",
			self.written_files.len()
		);
		for e in &self.entries {
			let tag = match e.kind {
				EntryKind::Converted => "[ok]  ",
				EntryKind::Warning => "[warn]",
				EntryKind::Error => "[err] ",
			};
			let feat = e.feature.as_deref().unwrap_or("-");
			s.push_str(&format!("  {tag} ({feat}) {}\n", e.message));
		}
		s
	}

	pub fn as_notes_md(&self) -> String {
		let mut s = String::from("# Migration Notes\n\nこのファイルは `virtual-avatar-connect --migrate` によって自動生成されました。\n\n");
		s.push_str(&format!(
			"- converted: {}\n- warnings: {}\n- errors: {}\n- files written: {}\n\n",
			self.entries.iter().filter(|e| e.kind == EntryKind::Converted).count(),
			self.entries.iter().filter(|e| e.kind == EntryKind::Warning).count(),
			self.entries.iter().filter(|e| e.kind == EntryKind::Error).count(),
			self.written_files.len()
		));
		s.push_str("## Entries\n\n");
		for e in &self.entries {
			let tag = match e.kind {
				EntryKind::Converted => "OK",
				EntryKind::Warning => "WARN",
				EntryKind::Error => "ERROR",
			};
			let feat = e.feature.as_deref().unwrap_or("-");
			s.push_str(&format!("- **{tag}** `{feat}` — {}\n", e.message));
		}
		s.push_str("\n## Next Steps\n\n");
		s.push_str("- `migrated/conf.toml` を確認し、環境変数や Bearer token 等が正しいか見直してください。\n");
		s.push_str("- `migrated/flowgraph/` の各サンプルは **トークンや詳細パラメータがプレースホルダ**のままです。必要に応じて GUI（Flowgraph タブ）で編集してください。\n");
		s.push_str("- `WARN` / `ERROR` が付いた項目は手動対応が必要です。\n");
		s
	}
}

// ---------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum MigrateError {
	#[error("入力 conf を開けません: {0}")]
	Io(#[from] std::io::Error),
	#[error("conf のパースに失敗: {0}")]
	Parse(#[from] toml_edit::TomlError),
	#[error("出力ディレクトリが既に存在します: {0}。別パスを指定するか、ディレクトリを消してから再実行してください。")]
	OutputDirExists(PathBuf),
}

// ---------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------

pub fn migrate_conf_file(
	input_path: &Path,
	out_dir: &Path,
	strict: bool,
) -> Result<MigrationReport, MigrateError> {
	let src = std::fs::read_to_string(input_path)?;
	migrate_conf_str(&src, out_dir, strict)
}

pub fn migrate_conf_str(
	input_toml: &str,
	out_dir: &Path,
	strict: bool,
) -> Result<MigrationReport, MigrateError> {
	let mut doc: DocumentMut = input_toml.parse()?;
	let mut report = MigrationReport {
		strict,
		..Default::default()
	};

	if out_dir.exists() {
		return Err(MigrateError::OutputDirExists(out_dir.to_path_buf()));
	}
	std::fs::create_dir_all(out_dir)?;
	let flowgraph_root = out_dir.join("flowgraph");
	std::fs::create_dir_all(&flowgraph_root)?;

	// --- 1. [[processors]] を抽出（後でトップレベルから削除） ---
	let processors: Vec<Table> = match doc.remove("processors") {
		Some(Item::ArrayOfTables(arr)) => arr.iter().cloned().collect(),
		Some(other) => {
			report.error(None, format!("トップレベル `processors` が [[processors]] でない: {:?}", other.type_name()));
			Vec::new()
		},
		None => Vec::new(),
	};

	// --- 2. feature ごとに分類して flowgraph フラグメント or voice 暫定保持 ---
	let mut voice_processors_to_keep: Vec<Table> = Vec::new();
	let mut topic_counter: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

	for (idx, proc_t) in processors.iter().enumerate() {
		let feature = proc_t
			.get("feature")
			.and_then(|i| i.as_str())
			.unwrap_or("")
			.to_string();

		if feature.is_empty() {
			report.warn(None, format!("processors[{idx}] に feature が無いため無視"));
			continue;
		}

		if feature == "voice" {
			voice_processors_to_keep.push(proc_t.clone());
			report.warn(
				Some(feature.clone()),
				"voice は v2 でも暫定的に [[processors]] のまま保持しました（δ-9 で [voice] サービスに移行予定）。`conf.example-voice.toml` 参照。",
			);
			continue;
		}

		let topic = feature_to_topic(&feature);
		let counter = topic_counter.entry(topic.clone()).or_insert(0);
		let suffix = if *counter == 0 { String::new() } else { format!("-{}", counter) };
		*counter += 1;
		let topic_dir = flowgraph_root.join(format!("{topic}{suffix}"));

		match render_flowgraph_for(&feature, proc_t, idx) {
			Some(toml_content) => {
				std::fs::create_dir_all(&topic_dir)?;
				let file = topic_dir.join("main.flowgraph.toml");
				std::fs::write(&file, toml_content)?;
				report.written_files.push(file);
				report.converted(&feature, format!("processors[{idx}] → flowgraph/{topic}{suffix}/main.flowgraph.toml"));
			},
			None => {
				report.warn(
					Some(feature.clone()),
					format!(
						"processors[{idx}] feature=\"{feature}\" は v2 Flowgraph への自動変換をサポートしていません。手動で書き換えてください。"
					),
				);
			},
		}
	}

	// --- 3. voice だけ processors に戻す ---
	if !voice_processors_to_keep.is_empty() {
		let mut arr = toml_edit::ArrayOfTables::new();
		for t in voice_processors_to_keep {
			arr.push(t);
		}
		doc.insert("processors", Item::ArrayOfTables(arr));
	}

	// --- 4. flowgraph_dir を付与 ---
	if !doc.contains_key("flowgraph_dir") {
		doc["flowgraph_dir"] = toml_edit::value("flowgraph");
	}

	// --- 5. ヘッダコメントを差し込む ---
	let header = "# =========================================================================\n\
	              #  Migrated conf.toml (v2)\n\
	              # =========================================================================\n\
	              # `virtual-avatar-connect --migrate` によって自動生成されました。\n\
	              # `MIGRATION-NOTES.md` で変換結果の詳細を確認し、必要に応じて手動調整してください。\n\
	              # =========================================================================\n\n";
	let conf_path = out_dir.join("conf.toml");
	std::fs::write(&conf_path, format!("{header}{doc}"))?;
	report.written_files.push(conf_path);

	// --- 6. MIGRATION-NOTES.md ---
	let notes_path = out_dir.join("MIGRATION-NOTES.md");
	std::fs::write(&notes_path, report.as_notes_md())?;
	report.written_files.push(notes_path);

	Ok(report)
}

// ---------------------------------------------------------------------
// feature → topic name
// ---------------------------------------------------------------------

fn feature_to_topic(feature: &str) -> String {
	match feature {
		"modify" => "chat-filters".into(),
		"command" => "chat-filters-command".into(),
		"dictionary-command" => "chat-dictionary-command".into(),
		"gas-translation" => "translate-gas".into(),
		"libre-translation" => "translate-libre".into(),
		"screenshot" => "screenshot".into(),
		"ocr" => "ocr".into(),
		"os-tts" | "OS-TTS" => "tts-os".into(),
		"voicevox" => "tts-voicevox".into(),
		"aivis-speech" | "aivisspeech" => "tts-aivis-speech".into(),
		"coeiroink" => "tts-coeiroink".into(),
		"bouyomichan" | "BouyomiChan" => "tts-bouyomichan".into(),
		"twitch" => "twitch-echo".into(),
		"twitch-out" => "twitch-chat-send".into(),
		"web-input" | "webinput" => "web-input".into(),
		other => other.replace('_', "-"),
	}
}

// ---------------------------------------------------------------------
// feature → flowgraph TOML string
// ---------------------------------------------------------------------

fn render_flowgraph_for(feature: &str, proc_t: &Table, idx: usize) -> Option<String> {
	let id = proc_t.get("id").and_then(|i| i.as_str()).map(|s| s.to_string());
	let channel_from = proc_t.get("channel_from").and_then(|i| i.as_str()).map(|s| s.to_string());
	let channel_to = proc_t.get("channel_to").and_then(|i| i.as_str()).map(|s| s.to_string());
	let header = |title: &str, desc: &str, tags: &[&str]| -> String {
		let id_note = id
			.as_ref()
			.map(|s| format!("# 旧 processors.id = \"{}\"\n", s))
			.unwrap_or_default();
		let from_note = channel_from
			.as_ref()
			.map(|s| format!("# 旧 channel_from = \"{}\"\n", s))
			.unwrap_or_default();
		let to_note = channel_to
			.as_ref()
			.map(|s| format!("# 旧 channel_to = \"{}\"\n", s))
			.unwrap_or_default();
		format!(
			"# =========================================================================\n\
			 # migrated from processors[{idx}] feature=\"{feature}\"\n\
			 {id_note}{from_note}{to_note}# =========================================================================\n\n\
			 [meta]\n\
			 title = \"{title}\"\n\
			 description = \"{desc}\"\n\
			 tags = [{tag_list}]\n\n",
			tag_list = tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", ")
		)
	};

	match feature {
		"modify" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"dict\"\n\
feature = \"flowgraph.dictionary.replace\"\n\
position = [400, 200]\n\
# TODO: v1 の dictionary_files / regex_files をここに配線する必要があるが、\n\
# 現状 `list<json>` を構築できる literal ノードが未整備のため空辞書のまま。\n\
# δ-9 で `flowgraph.literal.list_json` 予定。\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 200]\n\
\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"dict:content\"\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"dict:result\"\n\
to = \"log:value\"\n",
			hdr = header("Migrated: modify", "v1 modify processor の骨格。辞書/正規表現の配線は手動で行ってください。", &["migrated", "modify"])
		)),
		"command" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"prefix\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"/\"\n\
\n\
[[nodes]]\n\
id = \"cmd\"\n\
feature = \"flowgraph.command.match\"\n\
position = [400, 200]\n\
\n\
[[nodes]]\n\
id = \"log_cmd\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 120]\n\
\n\
[[nodes]]\n\
id = \"log_other\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 280]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"cmd:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"cmd:content\"\n\
[[edges]]\n\
from = \"prefix:value\"\n\
to = \"cmd:prefix\"\n\
[[edges]]\n\
from = \"cmd:on_command\"\n\
to = \"log_cmd:exec_in\"\n\
[[edges]]\n\
from = \"cmd:command\"\n\
to = \"log_cmd:value\"\n\
[[edges]]\n\
from = \"cmd:on_other\"\n\
to = \"log_other:exec_in\"\n\
[[edges]]\n\
from = \"cmd:original\"\n\
to = \"log_other:value\"\n\
# TODO: v1 の response_mod / set 機能は自動変換されていません。\n\
# GUI でコマンド別の応答テンプレートを配線してください。\n",
			hdr = header("Migrated: command", "v1 command processor の骨格（/prefix 分岐のみ）。response_mod / set は手動配線が必要。", &["migrated", "command"])
		)),
		"dictionary-command" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"cmd\"\n\
feature = \"flowgraph.dictionary.command\"\n\
position = [400, 200]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 200]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"cmd:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"cmd:content\"\n\
[[edges]]\n\
from = \"cmd:on_parsed\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"cmd:feedback\"\n\
to = \"log:value\"\n\
# TODO: v1 の target_modify_id / allowed_logins / writable_dictionary_file は自動変換されていません。\n\
# 更新後の辞書（updated_dictionary）の永続化は、現状 fs 書き込みノードが未実装のため手動対応が必要です（δ-9）。\n",
			hdr = header("Migrated: dictionary-command", "v1 dictionary-command processor の骨格。権限設定と辞書永続化は手動対応が必要。", &["migrated", "dictionary-command"])
		)),
		"gas-translation" => {
			let from_ = proc_t.get("translate_from").and_then(|i| i.as_str()).unwrap_or("auto");
			let to_ = proc_t.get("translate_to").and_then(|i| i.as_str()).unwrap_or("en");
			let script_id = proc_t.get("script_id").and_then(|i| i.as_str()).unwrap_or("YOUR_GAS_SCRIPT_ID_HERE");
			Some(format!(
				"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"script_id\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"{script_id}\"\n\
\n\
[[nodes]]\n\
id = \"from_lang\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 440]\n\
properties.value = \"{from_}\"\n\
\n\
[[nodes]]\n\
id = \"to_lang\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 500]\n\
properties.value = \"{to_}\"\n\
\n\
[[nodes]]\n\
id = \"gas\"\n\
feature = \"flowgraph.translate.gas\"\n\
position = [440, 260]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [760, 260]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"gas:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"gas:text\"\n\
[[edges]]\n\
from = \"script_id:value\"\n\
to = \"gas:script_id\"\n\
[[edges]]\n\
from = \"from_lang:value\"\n\
to = \"gas:translate_from\"\n\
[[edges]]\n\
from = \"to_lang:value\"\n\
to = \"gas:translate_to\"\n\
[[edges]]\n\
from = \"gas:on_success\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"gas:translated\"\n\
to = \"log:value\"\n",
				hdr = header("Migrated: gas-translation", "v1 gas-translation processor の骨格。", &["migrated", "translate", "gas"])
			))
		},
		"libre-translation" => {
			let from_ = proc_t.get("translate_from").and_then(|i| i.as_str()).unwrap_or("auto");
			let to_ = proc_t.get("translate_to").and_then(|i| i.as_str()).unwrap_or("en");
			let url = proc_t.get("libretranslate_url").and_then(|i| i.as_str()).unwrap_or("http://127.0.0.1:5000");
			Some(format!(
				"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"base_url\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"{url}\"\n\
\n\
[[nodes]]\n\
id = \"from_lang\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 440]\n\
properties.value = \"{from_}\"\n\
\n\
[[nodes]]\n\
id = \"to_lang\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 500]\n\
properties.value = \"{to_}\"\n\
\n\
[[nodes]]\n\
id = \"libre\"\n\
feature = \"flowgraph.translate.libre\"\n\
position = [440, 260]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [760, 260]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"libre:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"libre:text\"\n\
[[edges]]\n\
from = \"base_url:value\"\n\
to = \"libre:base_url\"\n\
[[edges]]\n\
from = \"from_lang:value\"\n\
to = \"libre:translate_from\"\n\
[[edges]]\n\
from = \"to_lang:value\"\n\
to = \"libre:translate_to\"\n\
[[edges]]\n\
from = \"libre:on_success\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"libre:translated\"\n\
to = \"log:value\"\n",
				hdr = header("Migrated: libre-translation", "v1 libre-translation processor の骨格。", &["migrated", "translate", "libre"])
			))
		},
		"screenshot" => {
			let title = proc_t.get("title").and_then(|i| i.as_str()).unwrap_or("メモ帳");
			Some(format!(
				"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"title\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"{title}\"\n\
\n\
[[nodes]]\n\
id = \"cap\"\n\
feature = \"flowgraph.screenshot.capture\"\n\
position = [400, 200]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 200]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"cap:exec_in\"\n\
[[edges]]\n\
from = \"title:value\"\n\
to = \"cap:title\"\n\
[[edges]]\n\
from = \"cap:on_success\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"cap:data_url\"\n\
to = \"log:value\"\n\
# TODO: v1 の crops / paths / client_only / bitblt は自動変換されていません。\n",
				hdr = header("Migrated: screenshot", "v1 screenshot processor の骨格（Windows 専用）。", &["migrated", "screenshot"])
			))
		},
		"ocr" => {
			let lang = proc_t.get("lang").and_then(|i| i.as_str()).unwrap_or("ja-JP");
			Some(format!(
				"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"lang\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"{lang}\"\n\
\n\
[[nodes]]\n\
id = \"ocr\"\n\
feature = \"flowgraph.ocr.recognize\"\n\
position = [400, 200]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [720, 200]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"ocr:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"ocr:data_url\"\n\
[[edges]]\n\
from = \"lang:value\"\n\
to = \"ocr:lang\"\n\
[[edges]]\n\
from = \"ocr:on_success\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"ocr:text\"\n\
to = \"log:value\"\n\
# TODO: v1 の load_from / lines / check_result_lang は自動変換されていません。\n\
# 入力として `data_url` には screenshot.capture などが直接繋がる想定です。\n",
				hdr = header("Migrated: ocr", "v1 ocr processor の骨格（Windows 専用）。", &["migrated", "ocr"])
			))
		},
		"os-tts" | "OS-TTS" | "voicevox" | "aivis-speech" | "aivisspeech" | "coeiroink" | "bouyomichan" | "BouyomiChan" => {
			let (engine_value, engine_note) = match feature {
				"os-tts" | "OS-TTS" => ("os", ""),
				"voicevox" => ("voicevox", ""),
				"aivis-speech" | "aivisspeech" => ("aivis_speech", ""),
				"coeiroink" => ("coeiroink", ""),
				"bouyomichan" | "BouyomiChan" => ("bouyomichan", ""),
				_ => ("os", ""),
			};
			let _ = engine_note;
			let voice = guess_tts_voice(feature, proc_t);
			let endpoint = guess_tts_endpoint(feature, proc_t);
			let endpoint_line = endpoint
				.map(|e| format!(
					"[[nodes]]\n\
id = \"endpoint\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 500]\n\
properties.value = \"{e}\"\n\n\
[[edges]]\n\
from = \"endpoint:value\"\n\
to = \"speak:endpoint\"\n"
				))
				.unwrap_or_default();
			Some(format!(
				"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"engine\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"{engine_value}\"\n\
\n\
[[nodes]]\n\
id = \"voice\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 440]\n\
properties.value = \"{voice}\"\n\
\n\
[[nodes]]\n\
id = \"speak\"\n\
feature = \"flowgraph.tts.speak\"\n\
position = [440, 260]\n\
\n\
{endpoint_line}\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [760, 260]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"speak:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"speak:text\"\n\
[[edges]]\n\
from = \"engine:value\"\n\
to = \"speak:engine\"\n\
[[edges]]\n\
from = \"voice:value\"\n\
to = \"speak:voice\"\n\
[[edges]]\n\
from = \"speak:on_success\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"speak:audio_path\"\n\
to = \"log:value\"\n\
# TODO: v1 の speed_scale / pitch_scale / volume_scale / intonation_scale などは自動変換されていません。\n\
# 必要なら speed / pitch / volume 入力を追加、詳細パラメータは extra(map<json>) を使います（map<json> literal は δ-9 で実装予定）。\n",
				hdr = header(
					&format!("Migrated: {feature} → tts.speak"),
					&format!("v1 {feature} processor の骨格。エンジン別パラメータは手動調整を推奨。"),
					&["migrated", "tts", engine_value]
				),
			))
		},
		"twitch-out" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"access_token\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 380]\n\
properties.value = \"TWITCH_ACCESS_TOKEN_HERE\"\n\
\n\
[[nodes]]\n\
id = \"client_id\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 440]\n\
properties.value = \"TWITCH_CLIENT_ID_HERE\"\n\
\n\
[[nodes]]\n\
id = \"broadcaster_id\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 500]\n\
properties.value = \"TWITCH_BROADCASTER_ID_HERE\"\n\
\n\
[[nodes]]\n\
id = \"sender_id\"\n\
feature = \"flowgraph.literal.string\"\n\
position = [80, 560]\n\
properties.value = \"TWITCH_SENDER_USER_ID_HERE\"\n\
\n\
[[nodes]]\n\
id = \"send\"\n\
feature = \"flowgraph.twitch.chat_send\"\n\
position = [440, 320]\n\
\n\
[[nodes]]\n\
id = \"log_ok\"\n\
feature = \"flowgraph.util.log\"\n\
position = [760, 280]\n\
\n\
[[nodes]]\n\
id = \"log_err\"\n\
feature = \"flowgraph.util.log\"\n\
position = [760, 360]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"send:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"send:text\"\n\
[[edges]]\n\
from = \"access_token:value\"\n\
to = \"send:access_token\"\n\
[[edges]]\n\
from = \"client_id:value\"\n\
to = \"send:client_id\"\n\
[[edges]]\n\
from = \"broadcaster_id:value\"\n\
to = \"send:broadcaster_id\"\n\
[[edges]]\n\
from = \"sender_id:value\"\n\
to = \"send:sender_user_id\"\n\
[[edges]]\n\
from = \"send:on_success\"\n\
to = \"log_ok:exec_in\"\n\
[[edges]]\n\
from = \"send:on_error\"\n\
to = \"log_err:exec_in\"\n\
[[edges]]\n\
from = \"send:error\"\n\
to = \"log_err:value\"\n\
# TODO: トークン類は暫定的に literal 直書きです。外部管理（env / token_source）への差し替えを推奨（δ-9 予定）。\n\
# TODO: v1 の twitch_out_rate_limit_per_30s 相当は flowgraph.util.rate_limit を追加してください。\n",
			hdr = header("Migrated: twitch-out", "v1 twitch-out processor の骨格。トークンは手動で差し替えてください。", &["migrated", "twitch", "chat_send"])
		)),
		"twitch" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.twitch\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [400, 200]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"log:value\"\n\
# INFO: v1 の `[[processors]] feature = \"twitch\"` は、v2 では `[twitch]` サービス設定 + `ingress.twitch` Flowgraph ノードに分離されました。\n\
# `conf.toml` の `[twitch]` / `[twitch.eventsub]` / `[twitch.moderator]` は既に残してあります。\n",
			hdr = header("Migrated: twitch → ingress.twitch", "v1 twitch processor の骨格。OAuth 設定は [twitch] セクションに残してあります。", &["migrated", "twitch", "ingress"])
		)),
		"web-input" | "webinput" => Some(format!(
			"{hdr}\
[[nodes]]\n\
id = \"in\"\n\
feature = \"flowgraph.ingress.web_input\"\n\
position = [80, 200]\n\
\n\
[[nodes]]\n\
id = \"log\"\n\
feature = \"flowgraph.util.log\"\n\
position = [400, 200]\n\
\n\
[[edges]]\n\
from = \"in:exec_out\"\n\
to = \"log:exec_in\"\n\
[[edges]]\n\
from = \"in:content\"\n\
to = \"log:value\"\n\
# TODO: v1 の web_input_path / web_input_method / web_input_body_format は自動変換されていません。\n\
# v2 では GET/POST はエンドポイント側で受けて ingress.web_input に統合される想定です。\n",
			hdr = header("Migrated: web-input", "v1 web-input processor の骨格。", &["migrated", "ingress", "web-input"])
		)),
		_ => None,
	}
}

fn guess_tts_voice(feature: &str, proc_t: &Table) -> String {
	match feature {
		"voicevox" | "aivis-speech" | "aivisspeech" => {
			// style_id (あるいは speaker_uuid:style_id)
			let uuid = proc_t.get("speaker_uuid").and_then(|i| i.as_str());
			let style = proc_t.get("style_id").and_then(item_to_i64);
			match (uuid, style) {
				(Some(u), Some(s)) => format!("{u}:{s}"),
				(_, Some(s)) => format!("{s}"),
				_ => String::from(""),
			}
		},
		"coeiroink" => {
			let uuid = proc_t.get("speaker_uuid").and_then(|i| i.as_str()).unwrap_or("");
			let style = proc_t.get("style_id").and_then(item_to_i64);
			match style {
				Some(s) => format!("{uuid}:{s}"),
				None => uuid.to_string(),
			}
		},
		"bouyomichan" | "BouyomiChan" => proc_t
			.get("voice")
			.and_then(item_to_i64)
			.map(|v| v.to_string())
			.unwrap_or_default(),
		"os-tts" | "OS-TTS" => proc_t
			.get("voice_name")
			.and_then(|i| i.as_str())
			.or_else(|| proc_t.get("voice_id").and_then(|i| i.as_str()))
			.unwrap_or("")
			.to_string(),
		_ => String::new(),
	}
}

fn guess_tts_endpoint(feature: &str, proc_t: &Table) -> Option<String> {
	match feature {
		"voicevox" | "aivis-speech" | "aivisspeech" | "coeiroink" => proc_t
			.get("api_url")
			.and_then(|i| i.as_str())
			.map(|s| s.to_string()),
		"bouyomichan" | "BouyomiChan" => {
			let addr = proc_t.get("address").and_then(|i| i.as_str()).unwrap_or("127.0.0.1");
			let port = proc_t.get("port").and_then(item_to_i64).unwrap_or(50001);
			Some(format!("{addr}:{port}"))
		},
		_ => None,
	}
}

fn item_to_i64(item: &Item) -> Option<i64> {
	match item {
		Item::Value(Value::Integer(v)) => Some(*v.value()),
		_ => None,
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	fn tmp_dir(label: &str) -> PathBuf {
		let base = std::env::temp_dir().join(format!(
			"vac-migrate-test-{}-{}-{}",
			label,
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		));
		base
	}

	#[test]
	fn minimal_v1_conf_converts() {
		let src = r#"
workers = 8
web_ui_address = "127.0.0.1:57000"

[[processors]]
feature = "modify"
channel_from = "user"
channel_to = "tts"
dictionary_files = ["dict.txt"]

[[processors]]
feature = "os-tts"
channel_from = "tts"
voice_name = "Microsoft Haruka Desktop"
"#;
		let out = tmp_dir("minimal");
		let report = migrate_conf_str(src, &out, false).unwrap();
		assert_eq!(report.entries.iter().filter(|e| e.kind == EntryKind::Converted).count(), 2);
		let conf_out = std::fs::read_to_string(out.join("conf.toml")).unwrap();
		assert!(conf_out.contains("workers = 8"));
		assert!(!conf_out.contains("[[processors]]"));
		assert!(conf_out.contains("flowgraph_dir"));
		assert!(out.join("flowgraph/chat-filters/main.flowgraph.toml").exists());
		assert!(out.join("flowgraph/tts-os/main.flowgraph.toml").exists());
		std::fs::remove_dir_all(&out).ok();
	}

	#[test]
	fn voice_processor_is_kept() {
		let src = r#"
[[processors]]
feature = "voice"
channel_to = "user"
voice_vosk_model_path = "vosk-model-small-ja-0.22"
"#;
		let out = tmp_dir("voice");
		let report = migrate_conf_str(src, &out, false).unwrap();
		let conf_out = std::fs::read_to_string(out.join("conf.toml")).unwrap();
		assert!(conf_out.contains("[[processors]]"));
		assert!(conf_out.contains("voice_vosk_model_path"));
		assert!(report.entries.iter().any(|e| e.kind == EntryKind::Warning));
		std::fs::remove_dir_all(&out).ok();
	}

	#[test]
	fn unknown_feature_emits_warning() {
		let src = r#"
[[processors]]
feature = "no-such-feature"
"#;
		let out = tmp_dir("unknown");
		let report = migrate_conf_str(src, &out, false).unwrap();
		assert!(report.entries.iter().any(|e| e.kind == EntryKind::Warning));
		assert!(!report.has_errors());
		std::fs::remove_dir_all(&out).ok();
	}

	#[test]
	fn strict_mode_treats_warning_as_error() {
		let src = r#"
[[processors]]
feature = "no-such-feature"
"#;
		let out = tmp_dir("strict");
		let report = migrate_conf_str(src, &out, true).unwrap();
		assert!(report.has_errors());
		std::fs::remove_dir_all(&out).ok();
	}

	#[test]
	fn tts_voicevox_encodes_voice_and_endpoint() {
		let src = r#"
[[processors]]
feature = "voicevox"
channel_from = "ai"
style_id = 3
api_url = "http://127.0.0.1:50021"
"#;
		let out = tmp_dir("voicevox");
		migrate_conf_str(src, &out, false).unwrap();
		let fg = std::fs::read_to_string(out.join("flowgraph/tts-voicevox/main.flowgraph.toml")).unwrap();
		assert!(fg.contains("properties.value = \"voicevox\""));
		assert!(fg.contains("properties.value = \"3\""));
		assert!(fg.contains("properties.value = \"http://127.0.0.1:50021\""));
		std::fs::remove_dir_all(&out).ok();
	}

	#[test]
	fn duplicate_tts_processors_get_suffix() {
		let src = r#"
[[processors]]
feature = "os-tts"

[[processors]]
feature = "os-tts"
"#;
		let out = tmp_dir("duplicate");
		migrate_conf_str(src, &out, false).unwrap();
		assert!(out.join("flowgraph/tts-os/main.flowgraph.toml").exists());
		assert!(out.join("flowgraph/tts-os-1/main.flowgraph.toml").exists());
		std::fs::remove_dir_all(&out).ok();
	}
}
