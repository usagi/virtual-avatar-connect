//! Flowgraph state snapshot file envelope.
//!
//! This module defines the on-disk JSON wrapper used before automatic reload restore
//! or profile-local persistence is wired into the runtime.

use crate::flowgraph::ProgramStateSnapshot;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND: &str = "vac.flowgraph.state_snapshot";
pub const FLOWGRAPH_STATE_SNAPSHOT_FILE_SCHEMA_VERSION: u8 = 1;
pub const FLOWGRAPH_STATE_SNAPSHOT_DIR: &str = "flowgraph-state";
pub const FLOWGRAPH_STATE_SNAPSHOT_FILE_NAME: &str = "state.snapshot.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProgramStateSnapshotFile {
	pub kind: String,
	pub schema_version: u8,
	pub created_at_unix_ms: u64,
	pub snapshot: ProgramStateSnapshot,
}

impl ProgramStateSnapshotFile {
	pub fn new(snapshot: ProgramStateSnapshot) -> Self {
		Self::with_created_at_unix_ms(snapshot, unix_time_ms())
	}

	pub fn with_created_at_unix_ms(snapshot: ProgramStateSnapshot, created_at_unix_ms: u64) -> Self {
		Self {
			kind: FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND.into(),
			schema_version: FLOWGRAPH_STATE_SNAPSHOT_FILE_SCHEMA_VERSION,
			created_at_unix_ms,
			snapshot,
		}
	}

	pub fn validate(&self) -> Result<(), StateSnapshotFileError> {
		if self.kind != FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND {
			return Err(StateSnapshotFileError::KindMismatch { actual: self.kind.clone() });
		}
		if self.schema_version != FLOWGRAPH_STATE_SNAPSHOT_FILE_SCHEMA_VERSION {
			return Err(StateSnapshotFileError::UnsupportedSchemaVersion {
				actual: self.schema_version,
			});
		}
		if self.snapshot.snapshot_node_count != self.snapshot.nodes.len() {
			return Err(StateSnapshotFileError::SnapshotCountMismatch {
				declared: self.snapshot.snapshot_node_count,
				actual: self.snapshot.nodes.len(),
			});
		}
		Ok(())
	}

	pub fn to_json_pretty(&self) -> Result<String, StateSnapshotFileError> {
		self.validate()?;
		let mut json = serde_json::to_string_pretty(self)?;
		json.push('\n');
		Ok(json)
	}

	pub fn from_json_str(raw: &str) -> Result<Self, StateSnapshotFileError> {
		let file: Self = serde_json::from_str(raw)?;
		file.validate()?;
		Ok(file)
	}
}

pub fn write_state_snapshot_file(path: impl AsRef<Path>, file: &ProgramStateSnapshotFile) -> Result<(), StateSnapshotFileError> {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(path, file.to_json_pretty()?)?;
	Ok(())
}

pub fn read_state_snapshot_file(path: impl AsRef<Path>) -> Result<ProgramStateSnapshotFile, StateSnapshotFileError> {
	let raw = std::fs::read_to_string(path)?;
	ProgramStateSnapshotFile::from_json_str(&raw)
}

pub fn profile_local_state_snapshot_path(
	runtime_root: impl AsRef<Path>,
	profile_path: impl AsRef<Path>,
	flowgraph_root: impl AsRef<Path>,
) -> PathBuf {
	runtime_root
		.as_ref()
		.join(FLOWGRAPH_STATE_SNAPSHOT_DIR)
		.join(profile_local_state_snapshot_dir_name(profile_path, flowgraph_root))
		.join(FLOWGRAPH_STATE_SNAPSHOT_FILE_NAME)
}

pub fn profile_local_state_snapshot_dir_name(profile_path: impl AsRef<Path>, flowgraph_root: impl AsRef<Path>) -> String {
	let profile_path = profile_path.as_ref();
	let key = format!(
		"profile={}\nflowgraph={}",
		normalize_path_for_key(profile_path),
		normalize_path_for_key(flowgraph_root.as_ref())
	);
	let digest = blake3::hash(key.as_bytes());
	let digest_hex = hex_prefix(digest.as_bytes(), 16);
	format!("{}-{digest_hex}", profile_slug(profile_path))
}

#[derive(Debug, Error)]
pub enum StateSnapshotFileError {
	#[error("state snapshot file io failed: {0}")]
	Io(#[from] std::io::Error),
	#[error("state snapshot file json parse failed: {0}")]
	Json(#[from] serde_json::Error),
	#[error("state snapshot file kind mismatch: expected '{expected}', actual '{actual}'", expected = FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND)]
	KindMismatch { actual: String },
	#[error("state snapshot file schema version is unsupported: expected {expected}, actual {actual}", expected = FLOWGRAPH_STATE_SNAPSHOT_FILE_SCHEMA_VERSION)]
	UnsupportedSchemaVersion { actual: u8 },
	#[error("state snapshot file node count mismatch: declared {declared}, actual {actual}")]
	SnapshotCountMismatch { declared: usize, actual: usize },
}

fn unix_time_ms() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or(Duration::ZERO)
		.as_millis()
		.min(u128::from(u64::MAX)) as u64
}

fn normalize_path_for_key(path: &Path) -> String {
	path.to_string_lossy().replace('\\', "/")
}

fn profile_slug(profile_path: &Path) -> String {
	let stem = profile_path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("profile");
	let mut slug = String::new();
	let mut prev_dash = false;
	for ch in stem.chars() {
		let next = if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
			prev_dash = false;
			Some(ch)
		} else if !prev_dash {
			prev_dash = true;
			Some('-')
		} else {
			None
		};
		if let Some(ch) = next {
			slug.push(ch);
		}
		if slug.len() >= 48 {
			break;
		}
	}
	let slug = slug.trim_matches('-');
	if slug.is_empty() { "profile".into() } else { slug.into() }
}

fn hex_prefix(bytes: &[u8], hex_len: usize) -> String {
	const HEX: &[u8; 16] = b"0123456789abcdef";
	let mut out = String::with_capacity(hex_len);
	for byte in bytes {
		if out.len() >= hex_len {
			break;
		}
		out.push(HEX[(byte >> 4) as usize] as char);
		if out.len() >= hex_len {
			break;
		}
		out.push(HEX[(byte & 0x0f) as usize] as char);
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::{ProgramStateSnapshotNode, StateSnapshotFormat};

	fn snapshot() -> ProgramStateSnapshot {
		ProgramStateSnapshot {
			snapshot_node_count: 1,
			nodes: vec![ProgramStateSnapshotNode {
				node: "main::counter".into(),
				feature: "flowgraph.state.int_counter".into(),
				version: 42,
				format: StateSnapshotFormat::Json,
				value: serde_json::json!({ "value": 42 }),
			}],
		}
	}

	fn temp_path(name: &str) -> std::path::PathBuf {
		let dir = std::env::temp_dir().join(format!("vac-state-snapshot-file-{}-{}", std::process::id(), unix_time_ms()));
		std::fs::create_dir_all(&dir).expect("create temp dir");
		dir.join(name)
	}

	#[test]
	fn snapshot_file_json_round_trips() {
		let file = ProgramStateSnapshotFile::with_created_at_unix_ms(snapshot(), 1234);
		let json = file.to_json_pretty().expect("json");
		assert!(json.ends_with('\n'));
		assert!(json.contains("vac.flowgraph.state_snapshot"));
		let decoded = ProgramStateSnapshotFile::from_json_str(&json).expect("decode");
		assert_eq!(decoded, file);
	}

	#[test]
	fn snapshot_file_read_write_round_trips() {
		let path = temp_path("state.snapshot.json");
		let file = ProgramStateSnapshotFile::with_created_at_unix_ms(snapshot(), 5678);
		write_state_snapshot_file(&path, &file).expect("write");
		let decoded = read_state_snapshot_file(&path).expect("read");
		assert_eq!(decoded, file);
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn snapshot_file_validation_rejects_unknown_schema_and_bad_count() {
		let unknown_schema = serde_json::json!({
			"kind": FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND,
			"schema_version": 99,
			"created_at_unix_ms": 1234,
			"snapshot": { "snapshot_node_count": 0, "nodes": [] },
		})
		.to_string();
		assert!(matches!(
			ProgramStateSnapshotFile::from_json_str(&unknown_schema),
			Err(StateSnapshotFileError::UnsupportedSchemaVersion { actual: 99 })
		));

		let bad_count = ProgramStateSnapshotFile::with_created_at_unix_ms(
			ProgramStateSnapshot {
				snapshot_node_count: 2,
				nodes: snapshot().nodes,
			},
			1234,
		);
		assert!(matches!(
			bad_count.validate(),
			Err(StateSnapshotFileError::SnapshotCountMismatch { declared: 2, actual: 1 })
		));
	}

	#[test]
	fn profile_local_snapshot_path_is_stable_and_profile_scoped() {
		let path = profile_local_state_snapshot_path(
			"C:/vac/runtime",
			"C:/Users/me/vac/conf.local.toml",
			"C:/Users/me/vac/flowgraph.example",
		);
		let text = path.to_string_lossy().replace('\\', "/");
		assert!(
			text.ends_with("/flowgraph-state/conf-local-4397fb2756fd0d7f/state.snapshot.json"),
			"{text}"
		);

		let other_profile = profile_local_state_snapshot_path(
			"C:/vac/runtime",
			"C:/Users/me/vac/conf.streaming.toml",
			"C:/Users/me/vac/flowgraph.example",
		);
		assert_ne!(path, other_profile);

		let other_flowgraph = profile_local_state_snapshot_path(
			"C:/vac/runtime",
			"C:/Users/me/vac/conf.local.toml",
			"C:/Users/me/vac/flowgraph.local",
		);
		assert_ne!(path, other_flowgraph);
	}

	#[test]
	fn profile_local_snapshot_dir_name_sanitizes_non_ascii_stems() {
		let name = profile_local_state_snapshot_dir_name("C:/vac/設定.toml", "C:/vac/flowgraph.example");
		assert!(name.starts_with("profile-"), "{name}");
		assert!(name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'), "{name}");
	}
}
