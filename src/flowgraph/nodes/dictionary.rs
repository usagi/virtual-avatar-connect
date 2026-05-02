//! η-3 / GRN: `flowgraph.glossary.*` 4 ノード（旧 `flowgraph.dictionary.*`）。
//!
//! - [`DictionaryReplaceNode`] (Stateful): `Table` を用語集として受け取り、literal(AC)+regex 統合置換
//! - [`DictionaryMatchNode`] (Stateful): 照合・キャプチャ出力・exec 分岐
//! - [`DictionaryLearnNode`] (Pure): 11 カラム append、重複検出
//! - [`DictionaryForgetNode`] (Pure): mode=latest/all/exact、is_locked 尊重
//!
//! Stateful Replace/Match はそれぞれ AC/Regex の再コンパイルを避けるため、
//! 入力 Table の Arc identity → version → content_hash の 3 段階でキャッシュ再利用する。

use crate::flowgraph::node::{
	get_optional_string, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
	PropertySpec, PureNode, StatefulCtx, StatefulNode,
};
use crate::flowgraph::nodes::table_ops::dictionary_schema;
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::table::{Row, Table};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value as JsonValue};
use std::any::Any;
use std::sync::Arc;

// ---------------------------------------------------------------------
// 共通: 11 カラム辞書スキーマの row ヘルパ
// ---------------------------------------------------------------------

/// 既知の辞書カラム。
mod col {
	pub const SOURCE: &str = "source";
	pub const REPLACEMENT: &str = "replacement";
	pub const KIND: &str = "kind";
	pub const PRIORITY: &str = "priority";
	pub const IS_LOCKED: &str = "is_locked";
	pub const ENABLED: &str = "enabled";
	pub const BY: &str = "by";
	pub const CREATED_AT: &str = "created_at";
	pub const EXPIRES_AT: &str = "expires_at";
	pub const TAGS: &str = "tags";
	pub const NOTE: &str = "note";
}

/// Row から列名経由で値を取り出す（Table::schema と突き合わせ）。
fn row_get<'a>(table: &'a Table, row: &'a Row, name: &str) -> Option<&'a JsonValue> {
	let idx = table.schema().column_index(name)?;
	row.get(idx)
}

fn row_str<'a>(table: &'a Table, row: &'a Row, name: &str) -> &'a str {
	row_get(table, row, name).and_then(|v| v.as_str()).unwrap_or("")
}

fn row_opt_str<'a>(table: &'a Table, row: &'a Row, name: &str) -> Option<&'a str> {
	row_get(table, row, name).and_then(|v| v.as_str())
}

fn row_i64(table: &Table, row: &Row, name: &str) -> i64 {
	row_get(table, row, name).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn row_bool(table: &Table, row: &Row, name: &str, default: bool) -> bool {
	row_get(table, row, name).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn is_expired(row: &Row, table: &Table, now: &jiff::Timestamp) -> bool {
	let Some(s) = row_opt_str(table, row, col::EXPIRES_AT) else {
		return false;
	};
	if s.is_empty() {
		return false;
	}
	match s.parse::<jiff::Timestamp>() {
		Ok(dt) => dt <= *now,
		Err(_) => false,
	}
}

fn current_utc_rfc3339() -> String {
	jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

// ---------------------------------------------------------------------
// CompiledDictionary: AC + Regex の統合コンパイル結果
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CompiledEntry {
	source: String,
	replacement: String,
	priority: i64,
	#[allow(dead_code)]
	locked: bool,
}

#[derive(Debug)]
struct CompiledDictionary {
	ac: Option<AhoCorasick>,
	literal_entries: Vec<CompiledEntry>,
	regex_entries: Vec<(Regex, CompiledEntry)>,
}

impl CompiledDictionary {
	/// Table → CompiledDictionary。enabled && !expired だけを採用、priority desc → created_at desc でソート。
	fn compile(table: &Table) -> Self {
		let now = jiff::Timestamp::now();
		let mut indexed: Vec<(usize, &Row)> = table.rows().iter().enumerate().collect();
		indexed.retain(|(_, r)| row_bool(table, r, col::ENABLED, true) && !is_expired(r, table, &now));
		indexed.sort_by(|(ia, a), (ib, b)| {
			let pa = row_i64(table, a, col::PRIORITY);
			let pb = row_i64(table, b, col::PRIORITY);
			pb.cmp(&pa)
				.then_with(|| {
					let ca = row_str(table, a, col::CREATED_AT);
					let cb = row_str(table, b, col::CREATED_AT);
					cb.cmp(ca)
				})
				.then_with(|| ia.cmp(ib))
		});

		let mut literal_patterns: Vec<String> = Vec::new();
		let mut literal_entries: Vec<CompiledEntry> = Vec::new();
		let mut regex_entries: Vec<(Regex, CompiledEntry)> = Vec::new();

		for (_, row) in &indexed {
			let source = row_str(table, row, col::SOURCE);
			if source.is_empty() {
				continue;
			}
			let replacement = row_str(table, row, col::REPLACEMENT);
			let kind = {
				let k = row_str(table, row, col::KIND);
				if k.is_empty() {
					"literal"
				} else {
					k
				}
			};
			let entry = CompiledEntry {
				source: source.to_string(),
				replacement: replacement.to_string(),
				priority: row_i64(table, row, col::PRIORITY),
				locked: row_bool(table, row, col::IS_LOCKED, false),
			};
			match kind {
				"literal" => {
					literal_patterns.push(source.to_string());
					literal_entries.push(entry);
				}
				"regex" => {
					if let Ok(re) = Regex::new(source) {
						regex_entries.push((re, entry));
					}
				}
				_ => {
					// 未知 kind は literal にフォールバック
					literal_patterns.push(source.to_string());
					literal_entries.push(entry);
				}
			}
		}

		let ac = if literal_patterns.is_empty() {
			None
		} else {
			AhoCorasickBuilder::new()
				// LeftmostLongest: 位置 conflict は最長マッチ優先（辞書としての典型動作）
				.match_kind(MatchKind::LeftmostLongest)
				.build(&literal_patterns)
				.ok()
		};

		Self {
			ac,
			literal_entries,
			regex_entries,
		}
	}

	fn is_empty(&self) -> bool {
		self.literal_entries.is_empty() && self.regex_entries.is_empty()
	}
}

// ---------------------------------------------------------------------
// DictionaryCache: Stateful Replace/Match の共通 state
// ---------------------------------------------------------------------

#[derive(Default)]
struct DictionaryCache {
	last_arc_ptr: usize, // Arc::as_ptr を usize で保持（NonNull ではないので 0 = 未セット）
	last_version: Option<u64>,
	last_hash: Option<[u8; 32]>,
	compiled: Option<Arc<CompiledDictionary>>,
}

impl DictionaryCache {
	/// Table を受けて compiled を取得。必要なら再コンパイル。
	fn get_or_compile(&mut self, table: &Table) -> Arc<CompiledDictionary> {
		let ptr = table.arc_ptr() as usize;
		let version = table.version();

		// Fast path 1: Arc identity 一致
		if ptr == self.last_arc_ptr && self.compiled.is_some() {
			return self.compiled.as_ref().unwrap().clone();
		}
		// Fast path 2: version 一致（同じ Arc を別経路で受け取った場合）
		if Some(version) == self.last_version && ptr == self.last_arc_ptr {
			if let Some(c) = &self.compiled {
				return c.clone();
			}
		}
		// Fast path 3: content_hash 一致（別 Arc だが内容同一）
		let hash = *table.content_hash();
		if Some(hash) == self.last_hash {
			if let Some(c) = &self.compiled {
				// identity を更新して次回以降 fast path に乗せる
				self.last_arc_ptr = ptr;
				self.last_version = Some(version);
				return c.clone();
			}
		}
		// Miss: 再コンパイル
		let compiled = Arc::new(CompiledDictionary::compile(table));
		self.last_arc_ptr = ptr;
		self.last_version = Some(version);
		self.last_hash = Some(hash);
		self.compiled = Some(compiled.clone());
		compiled
	}
}

fn get_required_table<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a Table, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_table().map_err(|_| NodeExecError::TypeMismatch {
		port: key.into(),
		expected: SocketType::Table,
		actual: v.type_of(),
	})
}

// ---------------------------------------------------------------------
// flowgraph.glossary.replace (Stateful)
// ---------------------------------------------------------------------

pub struct DictionaryReplaceNode;

impl NodeDescriptor for DictionaryReplaceNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.glossary.replace".into(),
			title: "Glossary Replace".into(),
			category: "glossary".into(),
			description: Some(
				"Glossary Table（11 カラム）で content を literal(AC) + regex 統合で逐次置換。Stateful（AC/Regex キャッシュ）".into(),
			),
			inputs: vec![
				PortSpec::input("content", "Content", SocketType::String),
				PortSpec::input("dictionary", "Glossary", SocketType::Table).with_default(SocketValue::Table(Table::empty())),
			],
			outputs: vec![
				PortSpec::output("result", "Result", SocketType::String),
				PortSpec::output("applied_count", "Applied Count", SocketType::Int),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for DictionaryReplaceNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(DictionaryCache::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let cache = state.downcast_mut::<DictionaryCache>().expect("DictionaryCache");
		let content = get_required_string(inputs, "content")?;
		let table = get_required_table(inputs, "dictionary")?;
		let compiled = cache.get_or_compile(table);
		let (result, applied) = replace_with(&content, &compiled);
		Ok(NodeOutput::new()
			.set_data("result", SocketValue::String(result))
			.set_data("applied_count", SocketValue::Int(applied as i64)))
	}
}

fn replace_with(content: &str, dict: &CompiledDictionary) -> (String, usize) {
	if dict.is_empty() {
		return (content.to_string(), 0);
	}
	let mut applied = 0usize;
	// Phase 1: literal (AC) を一括置換
	let phase1: String = match &dict.ac {
		Some(ac) => {
			let mut out = String::with_capacity(content.len());
			let mut last_end = 0;
			for m in ac.find_iter(content) {
				out.push_str(&content[last_end..m.start()]);
				let entry = &dict.literal_entries[m.pattern().as_usize()];
				out.push_str(&entry.replacement);
				applied += 1;
				last_end = m.end();
			}
			out.push_str(&content[last_end..]);
			out
		}
		None => content.to_string(),
	};
	// Phase 2: regex を priority 順に適用（高い順に既にソート済）
	let mut phase2 = phase1;
	for (re, entry) in &dict.regex_entries {
		let before_applied = applied;
		let replaced = re.replace_all(&phase2, entry.replacement.as_str());
		if matches!(replaced, std::borrow::Cow::Owned(_)) {
			let occurrences = re.find_iter(&phase2).count();
			applied += occurrences;
		}
		let s = replaced.into_owned();
		phase2 = s;
		let _ = before_applied;
	}
	(phase2, applied)
}

// ---------------------------------------------------------------------
// flowgraph.glossary.match (Stateful)
// ---------------------------------------------------------------------

pub struct DictionaryMatchNode;

impl NodeDescriptor for DictionaryMatchNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.glossary.match".into(),
			title: "Glossary Match".into(),
			category: "glossary".into(),
			description: Some("Glossary Table で text を照合し、一致エントリと captures を取り出す。exec 分岐可能。Stateful".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("text", "Text", SocketType::String),
				PortSpec::input("dictionary", "Glossary", SocketType::Table).with_default(SocketValue::Table(Table::empty())),
			],
			outputs: vec![
				PortSpec::exec_output("on_match", "On Match"),
				PortSpec::exec_output("on_no_match", "On No Match"),
				PortSpec::output("matched_entries", "Matched Entries", SocketType::List(Box::new(SocketType::Json))),
				PortSpec::output("matched_count", "Matched Count", SocketType::Int),
				PortSpec::output(
					"captures",
					"Captures",
					SocketType::List(Box::new(SocketType::List(Box::new(SocketType::String)))),
				),
				PortSpec::output("first_replacement", "First Replacement", SocketType::String),
			],
			properties: vec![
				PropertySpec::new(
					"match_policy",
					"Match Policy",
					SocketType::String,
					SocketValue::String("first".into()),
				)
				.description("first / all / longest"),
				PropertySpec::new("anchor", "Anchor", SocketType::String, SocketValue::String("anywhere".into()))
					.description("anywhere / prefix / full"),
			],
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchPolicy {
	First,
	All,
	Longest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
	Anywhere,
	Prefix,
	Full,
}

fn parse_policy(s: &str) -> MatchPolicy {
	match s {
		"all" => MatchPolicy::All,
		"longest" => MatchPolicy::Longest,
		_ => MatchPolicy::First,
	}
}

fn parse_anchor(s: &str) -> Anchor {
	match s {
		"prefix" => Anchor::Prefix,
		"full" => Anchor::Full,
		_ => Anchor::Anywhere,
	}
}

#[async_trait]
impl StatefulNode for DictionaryMatchNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(DictionaryCache::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let cache = state.downcast_mut::<DictionaryCache>().expect("DictionaryCache");
		let text = get_required_string(inputs, "text")?;
		let table = get_required_table(inputs, "dictionary")?;
		let policy = parse_policy(&get_optional_string(props, "match_policy", "first")?);
		let anchor = parse_anchor(&get_optional_string(props, "anchor", "anywhere")?);
		let compiled = cache.get_or_compile(table);

		let hits = match_with(&text, &compiled, policy, anchor, table);

		let matched_count = hits.len() as i64;
		let mut entries_out: Vec<SocketValue> = Vec::with_capacity(hits.len());
		let mut captures_out: Vec<SocketValue> = Vec::with_capacity(hits.len());
		let mut first_replacement = String::new();
		for (idx, hit) in hits.iter().enumerate() {
			entries_out.push(SocketValue::Json(hit.entry_json.clone()));
			let caps_list: Vec<SocketValue> = hit.captures.iter().map(|c| SocketValue::String(c.clone())).collect();
			captures_out.push(SocketValue::List(caps_list));
			if idx == 0 {
				first_replacement = hit.replacement.clone();
			}
		}

		let mut out = NodeOutput::new()
			.set_data("matched_entries", SocketValue::List(entries_out))
			.set_data("matched_count", SocketValue::Int(matched_count))
			.set_data("captures", SocketValue::List(captures_out))
			.set_data("first_replacement", SocketValue::String(first_replacement));
		if matched_count > 0 {
			out = out.fire_exec("on_match");
		} else {
			out = out.fire_exec("on_no_match");
		}
		Ok(out)
	}
}

struct MatchHit {
	entry_json: JsonValue,
	captures: Vec<String>,
	replacement: String,
	start: usize,
	end: usize,
}

fn match_with(text: &str, dict: &CompiledDictionary, policy: MatchPolicy, anchor: Anchor, source_table: &Table) -> Vec<MatchHit> {
	let mut hits: Vec<MatchHit> = Vec::new();

	// literal (AC)
	if let Some(ac) = &dict.ac {
		for m in ac.find_iter(text) {
			let entry = &dict.literal_entries[m.pattern().as_usize()];
			if !anchor_ok(anchor, text, m.start(), m.end()) {
				continue;
			}
			hits.push(MatchHit {
				entry_json: entry_to_json(entry, source_table, "literal"),
				captures: Vec::new(),
				replacement: entry.replacement.clone(),
				start: m.start(),
				end: m.end(),
			});
		}
	}
	// regex
	for (re, entry) in &dict.regex_entries {
		for caps in re.captures_iter(text) {
			let m = caps.get(0).unwrap();
			if !anchor_ok(anchor, text, m.start(), m.end()) {
				continue;
			}
			let cap_vec: Vec<String> = caps
				.iter()
				.skip(1)
				.map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
				.collect();
			hits.push(MatchHit {
				entry_json: entry_to_json(entry, source_table, "regex"),
				captures: cap_vec,
				replacement: entry.replacement.clone(),
				start: m.start(),
				end: m.end(),
			});
		}
	}

	// ポリシー適用
	// 位置順に並べる
	hits.sort_by_key(|h| (h.start, usize::MAX - (h.end - h.start)));
	match policy {
		MatchPolicy::All => hits,
		MatchPolicy::First => hits.into_iter().take(1).collect(),
		MatchPolicy::Longest => {
			// 入力内で「一番長い」マッチを 1 つ選ぶ（位置問わず）
			let best = hits.into_iter().max_by_key(|h| (h.end - h.start, usize::MAX - h.start));
			best.into_iter().collect()
		}
	}
}

fn anchor_ok(anchor: Anchor, text: &str, start: usize, end: usize) -> bool {
	match anchor {
		Anchor::Anywhere => true,
		Anchor::Prefix => start == 0,
		Anchor::Full => start == 0 && end == text.len(),
	}
}

fn entry_to_json(entry: &CompiledEntry, _table: &Table, kind: &str) -> JsonValue {
	// compiled の情報だけでエントリを再構築（source/replacement/kind/priority）。
	// 全 11 カラムが必要なら下流で Table.filter を書けばよい。
	json!({
		col::SOURCE: entry.source,
		col::REPLACEMENT: entry.replacement,
		col::KIND: kind,
		col::PRIORITY: entry.priority,
	})
}

// ---------------------------------------------------------------------
// flowgraph.glossary.learn (Pure)
// ---------------------------------------------------------------------

pub struct DictionaryLearnNode;

impl NodeDescriptor for DictionaryLearnNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.glossary.learn".into(),
			title: "Glossary Learn".into(),
			category: "glossary".into(),
			description: Some("Glossary Table に 11 カラムエントリを append。同値エントリは duplicate 検出して no-op".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("dictionary", "Glossary", SocketType::Table).with_default(SocketValue::Table(Table::empty())),
				PortSpec::input("source", "Source", SocketType::String),
				PortSpec::input("replacement", "Replacement", SocketType::String),
				PortSpec::input("kind", "Kind", SocketType::String).with_default(SocketValue::String("literal".into())),
				PortSpec::input("priority", "Priority", SocketType::Int).with_default(SocketValue::Int(0)),
				PortSpec::input("by", "By", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("tags", "Tags", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("note", "Note", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("expires_at", "Expires At", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("on_learned", "On Learned"),
				PortSpec::exec_output("on_duplicate", "On Duplicate"),
				PortSpec::output("updated_dictionary", "Updated Glossary", SocketType::Table),
				PortSpec::output("added_entry", "Added Entry", SocketType::Json),
				PortSpec::output("feedback", "Feedback", SocketType::String),
			],
			properties: vec![],
		}
	}

	// Phase φ-2: Live Quick-Add ウィジェットから 1 shot で「新しい用語集行を追加」する用途に限り
	// 外部トリガを許可する。副作用は dictionary Table の append のみで、失敗しても Table を破壊しない。
	fn control_triggerable(&self) -> bool {
		true
	}
}

#[async_trait]
impl PureNode for DictionaryLearnNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let base_table = get_required_table(inputs, "dictionary")?.clone();
		let source = get_required_string(inputs, "source")?;
		let replacement = get_required_string(inputs, "replacement")?;
		let kind = get_optional_string(inputs, "kind", "literal")?;
		let priority = inputs.get("priority").and_then(|v| v.as_i64().ok()).unwrap_or(0);
		let by = get_optional_string(inputs, "by", "")?;
		let tags = get_optional_string(inputs, "tags", "")?;
		let note = get_optional_string(inputs, "note", "")?;
		let expires_at = get_optional_string(inputs, "expires_at", "")?;

		let mut table = ensure_dictionary_schema(base_table);

		// duplicate 検出: (source, replacement, kind) が完全一致 & enabled & !expired
		let now = jiff::Timestamp::now();
		let is_duplicate = table.rows().iter().any(|r| {
			row_bool(&table, r, col::ENABLED, true)
				&& !is_expired(r, &table, &now)
				&& row_str(&table, r, col::SOURCE) == source
				&& row_str(&table, r, col::REPLACEMENT) == replacement
				&& (row_str(&table, r, col::KIND).is_empty() && kind == "literal" || row_str(&table, r, col::KIND) == kind)
		});

		if is_duplicate {
			let feedback = format!("既に同一エントリが登録済み: source='{source}'");
			return Ok(NodeOutput::new()
				.set_data("updated_dictionary", SocketValue::Table(table))
				.set_data("added_entry", SocketValue::Json(JsonValue::Null))
				.set_data("feedback", SocketValue::String(feedback))
				.fire_exec("on_duplicate"));
		}

		let created_at = current_utc_rfc3339();
		let expires_json = if expires_at.is_empty() {
			JsonValue::Null
		} else {
			JsonValue::String(expires_at)
		};
		let row_values = vec![
			JsonValue::String(source.clone()),
			JsonValue::String(replacement.clone()),
			JsonValue::String(kind.clone()),
			JsonValue::Number(priority.into()),
			JsonValue::Bool(false), // is_locked
			JsonValue::Bool(true),  // enabled
			JsonValue::String(by.clone()),
			JsonValue::String(created_at.clone()),
			expires_json.clone(),
			JsonValue::String(tags.clone()),
			JsonValue::String(note.clone()),
		];
		let new_row = Row::new(row_values.clone());
		table.push_row(new_row.clone());

		let added = json!({
			col::SOURCE: source,
			col::REPLACEMENT: replacement,
			col::KIND: kind,
			col::PRIORITY: priority,
			col::IS_LOCKED: false,
			col::ENABLED: true,
			col::BY: by,
			col::CREATED_AT: created_at,
			col::EXPIRES_AT: expires_json,
			col::TAGS: tags,
			col::NOTE: note,
		});
		let feedback = format!("学習: source='{}'", row_values[0].as_str().unwrap_or(""));

		Ok(NodeOutput::new()
			.set_data("updated_dictionary", SocketValue::Table(table))
			.set_data("added_entry", SocketValue::Json(added))
			.set_data("feedback", SocketValue::String(feedback))
			.fire_exec("on_learned"))
	}
}

/// Table が辞書スキーマでない場合は新しい辞書スキーマで空テーブル化。
/// すでに辞書スキーマ互換ならそのまま返す。
fn ensure_dictionary_schema(table: Table) -> Table {
	let schema = table.schema();
	let dict = dictionary_schema();
	// カラム名が辞書スキーマをすべて含むかどうかで判定
	let ok = dict.columns.iter().all(|c| schema.column_index(&c.name).is_some());
	if ok && schema.len() == dict.len() {
		table
	} else if table.is_empty() {
		Table::new(dict, Vec::new())
	} else {
		// 既存 rows を辞書スキーマに射影
		let mut rows = Vec::with_capacity(table.len());
		for row in table.rows() {
			let mut vals = Vec::with_capacity(dict.len());
			for col in &dict.columns {
				let v = schema
					.column_index(&col.name)
					.and_then(|i| row.get(i).cloned())
					.unwrap_or(JsonValue::Null);
				vals.push(v);
			}
			rows.push(Row::new(vals));
		}
		Table::new(dict, rows)
	}
}

// ---------------------------------------------------------------------
// flowgraph.glossary.forget (Pure)
// ---------------------------------------------------------------------

pub struct DictionaryForgetNode;

impl NodeDescriptor for DictionaryForgetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.glossary.forget".into(),
			title: "Glossary Forget".into(),
			category: "glossary".into(),
			description: Some("Glossary Table から source (+ replacement) 一致行を削除。mode=latest/all/exact、is_locked 保護".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("dictionary", "Glossary", SocketType::Table).with_default(SocketValue::Table(Table::empty())),
				PortSpec::input("source", "Source", SocketType::String),
				PortSpec::input("replacement", "Replacement", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("mode", "Mode", SocketType::String).with_default(SocketValue::String("latest".into())),
			],
			outputs: vec![
				PortSpec::exec_output("on_forgotten", "On Forgotten"),
				PortSpec::exec_output("on_nothing", "On Nothing"),
				PortSpec::exec_output("on_locked", "On Locked"),
				PortSpec::output("updated_dictionary", "Updated Glossary", SocketType::Table),
				PortSpec::output("removed_count", "Removed Count", SocketType::Int),
				PortSpec::output("locked_count", "Locked Count", SocketType::Int),
				PortSpec::output("feedback", "Feedback", SocketType::String),
			],
			properties: vec![],
		}
	}

	// Phase φ-2: Glossary Editor から「1 行消す」操作のために opt-in する。
	// is_locked な行は compute() 内で保護されるため、trigger 経由でも破壊は起きない。
	fn control_triggerable(&self) -> bool {
		true
	}
}

#[async_trait]
impl PureNode for DictionaryForgetNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let base_table = get_required_table(inputs, "dictionary")?.clone();
		let source = get_required_string(inputs, "source")?;
		let replacement = get_optional_string(inputs, "replacement", "")?;
		let mode = get_optional_string(inputs, "mode", "latest")?;

		let mut table = ensure_dictionary_schema(base_table);

		// 対象候補: source 一致（replacement 非空なら replacement も一致）
		let match_pred = |row: &Row, t: &Table| -> bool {
			if row_str(t, row, col::SOURCE) != source {
				return false;
			}
			if !replacement.is_empty() && row_str(t, row, col::REPLACEMENT) != replacement {
				return false;
			}
			true
		};

		// locked カウント（削除候補で is_locked=true の件数）
		let locked_count: usize = table
			.rows()
			.iter()
			.filter(|r| match_pred(r, &table) && row_bool(&table, r, col::IS_LOCKED, false))
			.count();

		let mut removed = 0usize;
		match mode.as_str() {
			"all" => {
				let pred_table_clone = table.clone();
				removed =
					table.remove_all_where(|r| match_pred(r, &pred_table_clone) && !row_bool(&pred_table_clone, r, col::IS_LOCKED, false));
			}
			"exact" => {
				// replacement 必須
				if replacement.is_empty() {
					let feedback = "mode=exact では replacement が必須".to_string();
					return Ok(NodeOutput::new()
						.set_data("updated_dictionary", SocketValue::Table(table))
						.set_data("removed_count", SocketValue::Int(0))
						.set_data("locked_count", SocketValue::Int(locked_count as i64))
						.set_data("feedback", SocketValue::String(feedback))
						.fire_exec("on_nothing"));
				}
				let pred_table_clone = table.clone();
				let had =
					table.remove_last_where(|r| match_pred(r, &pred_table_clone) && !row_bool(&pred_table_clone, r, col::IS_LOCKED, false));
				if had {
					removed = 1;
				}
			}
			_ => {
				// "latest" or default
				let pred_table_clone = table.clone();
				let had =
					table.remove_last_where(|r| match_pred(r, &pred_table_clone) && !row_bool(&pred_table_clone, r, col::IS_LOCKED, false));
				if had {
					removed = 1;
				}
			}
		}

		let mut out = NodeOutput::new()
			.set_data("updated_dictionary", SocketValue::Table(table))
			.set_data("removed_count", SocketValue::Int(removed as i64))
			.set_data("locked_count", SocketValue::Int(locked_count as i64));
		let feedback = if removed > 0 {
			format!("忘却: source='{source}' {removed} 件削除（{locked_count} 件ロック保護）")
		} else if locked_count > 0 {
			format!("忘却対象あるがロック: source='{source}' {locked_count} 件保護")
		} else {
			format!("忘却対象なし: source='{source}'")
		};
		out = out.set_data("feedback", SocketValue::String(feedback));
		if removed > 0 {
			out = out.fire_exec("on_forgotten");
		} else {
			out = out.fire_exec("on_nothing");
		}
		if locked_count > 0 {
			out = out.fire_exec("on_locked");
		}
		Ok(out)
	}
}

// ---------------------------------------------------------------------
// 旧 dictionary.command は η-4 で削除、スタブも残さない。
// ---------------------------------------------------------------------

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::table::TableSchema;

	fn sample_dict() -> Table {
		let schema = dictionary_schema();
		let rows = vec![
			dict_row(
				"hello",
				"こんにちは",
				"literal",
				0,
				false,
				true,
				"user:a",
				"2026-04-01T00:00:00Z",
				None,
			),
			dict_row("Rust", "ラスト", "literal", 10, true, true, "system", "2026-04-01T00:00:00Z", None),
			dict_row(
				r"(\d+)円",
				"$1 yen",
				"regex",
				50,
				false,
				true,
				"system",
				"2026-04-01T00:00:00Z",
				None,
			),
			dict_row(
				"expired",
				"期限切れ",
				"literal",
				0,
				false,
				true,
				"system",
				"2026-04-01T00:00:00Z",
				Some("2026-04-02T00:00:00Z"),
			),
		];
		Table::new(schema, rows)
	}

	fn dict_row(
		source: &str,
		replacement: &str,
		kind: &str,
		priority: i64,
		is_locked: bool,
		enabled: bool,
		by: &str,
		created_at: &str,
		expires_at: Option<&str>,
	) -> Row {
		Row::new(vec![
			JsonValue::String(source.into()),
			JsonValue::String(replacement.into()),
			JsonValue::String(kind.into()),
			JsonValue::Number(priority.into()),
			JsonValue::Bool(is_locked),
			JsonValue::Bool(enabled),
			JsonValue::String(by.into()),
			JsonValue::String(created_at.into()),
			expires_at.map(|s| JsonValue::String(s.into())).unwrap_or(JsonValue::Null),
			JsonValue::String(String::new()),
			JsonValue::String(String::new()),
		])
	}

	fn fire(port: &str) -> ExecFireSet {
		let mut f = ExecFireSet::new();
		f.insert(port);
		f
	}

	fn sctx<'a>() -> StatefulCtx<'a> {
		StatefulCtx {
			node_id: "n",
			trigger: None,
		}
	}

	// ----- Replace -----

	#[tokio::test]
	async fn replace_literal_and_regex_mixed() {
		let node = DictionaryReplaceNode;
		let mut state = node.init_state();
		let mut inputs = InputMap::new();
		inputs.insert("content".into(), SocketValue::String("hello Rust! 100円".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		let SocketValue::String(result) = out.data.get("result").unwrap() else {
			panic!()
		};
		assert!(result.contains("こんにちは"));
		assert!(result.contains("ラスト"));
		assert!(result.contains("100 yen"));
	}

	#[tokio::test]
	async fn replace_caches_compiled_dictionary() {
		let node = DictionaryReplaceNode;
		let mut state = node.init_state();
		let dict = sample_dict();
		let mut inputs = InputMap::new();
		inputs.insert("content".into(), SocketValue::String("hello".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(dict.clone()));
		let _ = node
			.compute(state.as_mut(), &InputMap::new(), &inputs, &ExecFireSet::new(), &sctx())
			.await
			.unwrap();
		// 2 回目: キャッシュヒット（state の last_arc_ptr が更新されている）
		let cache = state.as_mut().downcast_mut::<DictionaryCache>().unwrap();
		assert!(cache.last_version.is_some());
		assert!(cache.compiled.is_some());
	}

	#[tokio::test]
	async fn replace_excludes_expired_entries() {
		let node = DictionaryReplaceNode;
		let mut state = node.init_state();
		let mut inputs = InputMap::new();
		inputs.insert("content".into(), SocketValue::String("expired entry".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &InputMap::new(), &inputs, &ExecFireSet::new(), &sctx())
			.await
			.unwrap();
		let SocketValue::String(result) = out.data.get("result").unwrap() else {
			panic!()
		};
		// "expired" は「期限切れ」に置換されない（expires_at が過去）
		assert_eq!(result, "expired entry");
	}

	// ----- Match -----

	#[tokio::test]
	async fn match_first_policy_returns_one_hit() {
		let node = DictionaryMatchNode;
		let mut state = node.init_state();
		let mut props = InputMap::new();
		props.insert("match_policy".into(), SocketValue::String("first".into()));
		props.insert("anchor".into(), SocketValue::String("anywhere".into()));
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("hello world".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &props, &inputs, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_match"));
		assert_eq!(out.data.get("matched_count"), Some(&SocketValue::Int(1)));
		assert_eq!(out.data.get("first_replacement"), Some(&SocketValue::String("こんにちは".into())));
	}

	#[tokio::test]
	async fn match_captures_regex_groups() {
		let node = DictionaryMatchNode;
		let mut state = node.init_state();
		let mut props = InputMap::new();
		props.insert("match_policy".into(), SocketValue::String("all".into()));
		props.insert("anchor".into(), SocketValue::String("anywhere".into()));
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("100円と200円".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &props, &inputs, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_match"));
		let SocketValue::List(caps) = out.data.get("captures").unwrap() else {
			panic!()
		};
		// 2 マッチ、各マッチに 1 キャプチャ
		assert_eq!(caps.len(), 2);
		if let SocketValue::List(g0) = &caps[0] {
			assert_eq!(g0.len(), 1);
			assert_eq!(g0[0], SocketValue::String("100".into()));
		}
	}

	#[tokio::test]
	async fn match_no_match_fires_on_no_match() {
		let node = DictionaryMatchNode;
		let mut state = node.init_state();
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("nothing here".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_no_match"));
		assert_eq!(out.data.get("matched_count"), Some(&SocketValue::Int(0)));
	}

	#[tokio::test]
	async fn match_anchor_prefix() {
		let node = DictionaryMatchNode;
		let mut state = node.init_state();
		let mut props = InputMap::new();
		props.insert("anchor".into(), SocketValue::String("prefix".into()));
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("hello world".into()));
		inputs.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out = node
			.compute(state.as_mut(), &props, &inputs, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_match"));
		// 先頭に "hello" なので OK
		assert_eq!(out.data.get("matched_count"), Some(&SocketValue::Int(1)));

		let mut inputs2 = InputMap::new();
		inputs2.insert("text".into(), SocketValue::String("say hello".into()));
		inputs2.insert("dictionary".into(), SocketValue::Table(sample_dict()));
		let out2 = node
			.compute(state.as_mut(), &props, &inputs2, &fire("exec_in"), &sctx())
			.await
			.unwrap();
		assert!(out2.fired_exec.contains("on_no_match"));
	}

	// ----- Learn / Forget -----

	#[tokio::test]
	async fn learn_appends_row() {
		let node = DictionaryLearnNode;
		let mut inputs = InputMap::new();
		inputs.insert("dictionary".into(), SocketValue::Table(Table::new(dictionary_schema(), Vec::new())));
		inputs.insert("source".into(), SocketValue::String("foo".into()));
		inputs.insert("replacement".into(), SocketValue::String("bar".into()));
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_learned"));
		let SocketValue::Table(t) = out.data.get("updated_dictionary").unwrap() else {
			panic!()
		};
		assert_eq!(t.len(), 1);
	}

	#[tokio::test]
	async fn learn_detects_duplicate() {
		let node = DictionaryLearnNode;
		let mut t = Table::new(dictionary_schema(), Vec::new());
		t.push_row(dict_row("foo", "bar", "literal", 0, false, true, "x", "2026-04-01T00:00:00Z", None));
		let mut inputs = InputMap::new();
		inputs.insert("dictionary".into(), SocketValue::Table(t));
		inputs.insert("source".into(), SocketValue::String("foo".into()));
		inputs.insert("replacement".into(), SocketValue::String("bar".into()));
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_duplicate"));
		assert!(!out.fired_exec.contains("on_learned"));
	}

	#[tokio::test]
	async fn forget_latest_removes_newest_then_undo_ish() {
		let learn = DictionaryLearnNode;
		let forget = DictionaryForgetNode;
		// Initial: (foo, old) at t=0
		let mut t0 = Table::new(dictionary_schema(), Vec::new());
		t0.push_row(dict_row("foo", "old", "literal", 0, false, true, "x", "2026-04-01T00:00:00Z", None));
		// Learn foo -> new
		let mut learn_in = InputMap::new();
		learn_in.insert("dictionary".into(), SocketValue::Table(t0.clone()));
		learn_in.insert("source".into(), SocketValue::String("foo".into()));
		learn_in.insert("replacement".into(), SocketValue::String("new".into()));
		let learned = learn
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&learn_in,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		let SocketValue::Table(t1) = learned.data.get("updated_dictionary").unwrap().clone() else {
			panic!()
		};
		assert_eq!(t1.len(), 2);

		// Forget (latest): newest "new" が消え、"old" が復活
		let mut forget_in = InputMap::new();
		forget_in.insert("dictionary".into(), SocketValue::Table(t1.clone()));
		forget_in.insert("source".into(), SocketValue::String("foo".into()));
		forget_in.insert("mode".into(), SocketValue::String("latest".into()));
		let forgotten = forget
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&forget_in,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		let SocketValue::Table(t2) = forgotten.data.get("updated_dictionary").unwrap().clone() else {
			panic!()
		};
		assert_eq!(t2.len(), 1);
		assert_eq!(
			t2.rows()[0].get(1).and_then(|v| v.as_str()),
			Some("old"),
			"UNDO: old が復活してるはず"
		);
	}

	#[tokio::test]
	async fn forget_all_removes_all_matching() {
		let forget = DictionaryForgetNode;
		let mut t = Table::new(dictionary_schema(), Vec::new());
		t.push_row(dict_row("foo", "a", "literal", 0, false, true, "x", "2026-04-01T00:00:00Z", None));
		t.push_row(dict_row("foo", "b", "literal", 0, false, true, "x", "2026-04-02T00:00:00Z", None));
		t.push_row(dict_row("bar", "c", "literal", 0, false, true, "x", "2026-04-03T00:00:00Z", None));
		let mut inp = InputMap::new();
		inp.insert("dictionary".into(), SocketValue::Table(t));
		inp.insert("source".into(), SocketValue::String("foo".into()));
		inp.insert("mode".into(), SocketValue::String("all".into()));
		let out = forget
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inp,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("removed_count"), Some(&SocketValue::Int(2)));
		let SocketValue::Table(t2) = out.data.get("updated_dictionary").unwrap() else {
			panic!()
		};
		assert_eq!(t2.len(), 1);
	}

	#[tokio::test]
	async fn forget_respects_is_locked() {
		let forget = DictionaryForgetNode;
		let mut t = Table::new(dictionary_schema(), Vec::new());
		t.push_row(dict_row("locked", "a", "literal", 0, true, true, "x", "2026-04-01T00:00:00Z", None));
		let mut inp = InputMap::new();
		inp.insert("dictionary".into(), SocketValue::Table(t));
		inp.insert("source".into(), SocketValue::String("locked".into()));
		inp.insert("mode".into(), SocketValue::String("all".into()));
		let out = forget
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inp,
				&fire("exec_in"),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_locked"));
		assert!(out.fired_exec.contains("on_nothing"));
		assert_eq!(out.data.get("removed_count"), Some(&SocketValue::Int(0)));
		assert_eq!(out.data.get("locked_count"), Some(&SocketValue::Int(1)));
	}

	#[test]
	fn ensure_schema_preserves_dictionary_schema_table() {
		let t = sample_dict();
		let orig_schema_len = t.schema().len();
		let t2 = ensure_dictionary_schema(t.clone());
		assert_eq!(t2.schema().len(), orig_schema_len);
		assert_eq!(t2.len(), t.len());
	}

	#[test]
	fn ensure_schema_reprojects_foreign_schema() {
		// カラム名が一致しないテーブル
		let custom = Table::new(
			TableSchema::new(vec![crate::flowgraph::table::ColumnSpec::new("something", SocketType::String)]),
			Vec::new(),
		);
		let t2 = ensure_dictionary_schema(custom);
		assert_eq!(t2.schema().len(), 11);
		assert_eq!(t2.len(), 0);
	}

	// ----- is_expired boundary tests (Phase pi-2 batch2) -----
	//
	// `is_expired(row, table, now)` は `expires_at <= now` を「期限切れ」とみなす。
	// jiff 移行後も chrono と同じ境界 (equal = expired) を保つことを回帰で守る。

	fn single_row_table_with_expires(expires_at: Option<&str>) -> Table {
		let schema = dictionary_schema();
		let row = dict_row("src", "dst", "literal", 0, false, true, "test", "2026-04-01T00:00:00Z", expires_at);
		Table::new(schema, vec![row])
	}

	fn now_fixed() -> jiff::Timestamp {
		"2026-04-24T12:00:00Z".parse().unwrap()
	}

	#[test]
	fn is_expired_returns_false_when_expires_at_is_absent() {
		let t = single_row_table_with_expires(None);
		let row = &t.rows()[0];
		assert!(!is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_returns_false_when_expires_at_is_empty_string() {
		let t = single_row_table_with_expires(Some(""));
		let row = &t.rows()[0];
		assert!(!is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_returns_false_for_unparseable_expires_at() {
		let t = single_row_table_with_expires(Some("not-a-timestamp"));
		let row = &t.rows()[0];
		assert!(!is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_returns_true_when_expires_at_is_in_the_past() {
		let t = single_row_table_with_expires(Some("2026-04-24T11:59:59Z"));
		let row = &t.rows()[0];
		assert!(is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_returns_true_when_expires_at_equals_now() {
		// 境界: expires_at == now は「期限切れ」扱い (dt <= now)。
		// chrono 実装と一致する半開区間 (now, ∞) を保つ。
		let t = single_row_table_with_expires(Some("2026-04-24T12:00:00Z"));
		let row = &t.rows()[0];
		assert!(is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_returns_false_when_expires_at_is_in_the_future() {
		let t = single_row_table_with_expires(Some("2026-04-24T12:00:01Z"));
		let row = &t.rows()[0];
		assert!(!is_expired(row, &t, &now_fixed()));
	}

	#[test]
	fn is_expired_respects_non_utc_offset_in_expires_at() {
		// "2026-04-24T21:00:00+09:00" == "2026-04-24T12:00:00Z" (same instant).
		// jiff は RFC3339 を任意オフセットで受理し、Timestamp は UTC absolute に正規化する。
		// 境界 (equal) なので expired と判定される。
		let t = single_row_table_with_expires(Some("2026-04-24T21:00:00+09:00"));
		let row = &t.rows()[0];
		assert!(is_expired(row, &t, &now_fixed()));

		// JST +09:00 で 1 ミリ秒未来は not expired。
		let t2 = single_row_table_with_expires(Some("2026-04-24T21:00:00.001+09:00"));
		let row2 = &t2.rows()[0];
		assert!(!is_expired(row2, &t2, &now_fixed()));
	}
}
