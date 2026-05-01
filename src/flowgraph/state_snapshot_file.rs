//! Flowgraph state snapshot file envelope.
//!
//! This module defines the on-disk JSON wrapper used before automatic reload restore
//! or profile-local persistence is wired into the runtime.

use crate::flowgraph::ProgramStateSnapshot;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const FLOWGRAPH_STATE_SNAPSHOT_FILE_KIND: &str = "vac.flowgraph.state_snapshot";
pub const FLOWGRAPH_STATE_SNAPSHOT_FILE_SCHEMA_VERSION: u8 = 1;

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
		let dir = std::env::temp_dir().join(format!(
			"vac-state-snapshot-file-{}-{}",
			std::process::id(),
			unix_time_ms()
		));
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
}