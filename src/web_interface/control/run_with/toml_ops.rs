//! `run_with` 配列の `toml_edit` 操作と DTO → `Value` 変換。

use actix_web::HttpResponse;
use toml_edit::{DocumentMut, InlineTable, Item, Value};

use super::util::err_response;
use super::RunWithDto;

/// `run_with` 配列（inline array）への可変参照を取得する。存在しなければ空配列を挿入。
pub(crate) fn run_with_array(doc: &mut DocumentMut) -> Result<&mut toml_edit::Array, HttpResponse> {
	// None なら空配列を作る。
	if !doc.contains_key("run_with") {
		doc["run_with"] = Item::Value(Value::Array(toml_edit::Array::new()));
	}

	// ArrayOfTables (`[[run_with]]` 記法) にも一応対応したいところだが、conf.example は inline array を
	// 公式サンプルにしているので、ここでは inline array のみサポート。混在は今回は非対応で 400 に倒す。
	let item = &mut doc["run_with"];
	match item {
		Item::Value(Value::Array(arr)) => Ok(arr),
		Item::ArrayOfTables(_) => Err(err_response(
			actix_web::http::StatusCode::NOT_IMPLEMENTED,
			"array_of_tables_not_supported",
			"conf.toml の run_with は `[[run_with]]` テーブル形式で書かれています。\
    現在 GUI からの編集はインライン配列形式 (`run_with = [ ... ]`) のみ対応しています。",
		)),
		_ => Err(err_response(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"invalid_run_with_shape",
			"conf.toml の `run_with` が配列でありません。手動修正してください。",
		)),
	}
}

/// `RunWithDto` を `toml_edit` の `Value` に変換。
pub(crate) fn dto_to_value(dto: &RunWithDto) -> Value {
	match dto {
		RunWithDto::Simple(s) => Value::from(s.as_str()),
		RunWithDto::Table(t) => {
			let mut it = InlineTable::new();
			it.insert("command", Value::from(t.command.as_str()));
			if let Some(v) = &t.if_not_running {
				it.insert("if_not_running", Value::from(v.as_str()));
			}
			if let Some(v) = t.run_as_admin {
				it.insert("run_as_admin", Value::from(v));
			}
			if let Some(v) = &t.working_dir {
				it.insert("working_dir", Value::from(v.as_str()));
			}
			if let Some(v) = t.minimized {
				it.insert("minimized", Value::from(v));
			}
			if let Some(v) = &t.id {
				it.insert("id", Value::from(v.as_str()));
			}
			if let Some(v) = &t.label {
				it.insert("label", Value::from(v.as_str()));
			}
			Value::InlineTable(it)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use super::super::RunWithTableDto;
	use toml_edit::DocumentMut;

	fn doc(src: &str) -> DocumentMut {
		src.parse::<DocumentMut>().expect("parse")
	}

	#[test]
	fn push_to_empty_array_creates_it() {
		let mut d = doc("# empty file\n");
		let arr = run_with_array(&mut d).expect("get");
		arr.push(Value::from("notepad"));
		let out = d.to_string();
		assert!(out.contains("run_with"));
		assert!(out.contains("notepad"));
	}

	#[test]
	fn push_to_inline_array_preserves_others() {
		let mut d = doc(r#"
# keep this comment
speech_floor = 0
run_with = [
 "notepad",
]
other_key = "x"
"#);
		let arr = run_with_array(&mut d).expect("get");
		arr.push(Value::from("http://example.com"));
		let out = d.to_string();
		assert!(out.contains("# keep this comment"));
		assert!(out.contains("notepad"));
		assert!(out.contains("http://example.com"));
		assert!(out.contains("other_key = \"x\""));
	}

	#[test]
	fn remove_entry_by_index() {
		let mut d = doc(r#"run_with = [
 "a",
 "b",
 "c",
]
"#);
		let arr = run_with_array(&mut d).expect("get");
		arr.remove(1);
		let out = d.to_string();
		assert!(out.contains("\"a\""));
		assert!(!out.contains("\"b\""));
		assert!(out.contains("\"c\""));
	}

	#[test]
	fn array_of_tables_is_rejected() {
		let mut d = doc(r#"[[run_with]]
command = "notepad"
"#);
		let err = run_with_array(&mut d);
		assert!(err.is_err());
	}

	#[test]
	fn dto_to_inline_table_only_emits_present_fields() {
		let v = dto_to_value(&RunWithDto::Table(RunWithTableDto {
			command: "foo.exe".into(),
			if_not_running: Some("foo".into()),
			run_as_admin: None,
			working_dir: None,
			minimized: Some(true),
			id: Some("foo-app".into()),
			label: None,
		}));
		let rendered = v.to_string();
		assert!(rendered.contains("command = \"foo.exe\""));
		assert!(rendered.contains("if_not_running = \"foo\""));
		assert!(rendered.contains("minimized = true"));
		assert!(rendered.contains("id = \"foo-app\""));
		assert!(!rendered.contains("run_as_admin"));
		assert!(!rendered.contains("working_dir"));
		assert!(!rendered.contains("label"));
	}
}
