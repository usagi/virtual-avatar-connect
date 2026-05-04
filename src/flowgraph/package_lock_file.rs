//! Flowgraph package lockfile envelope.
//!
//! This module defines the on-disk JSON wrapper for LF-4 package lock preview
//! entries. The writer is intentionally separate from resolver execution so the
//! Control API can persist the current local preview before external registries
//! or compatibility solving are introduced.

use crate::flowgraph::loader::PackageLockEntry;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const FLOWGRAPH_PACKAGE_LOCK_FILE_KIND: &str = "vac.flowgraph.package_lock";
pub const FLOWGRAPH_PACKAGE_LOCK_FILE_SCHEMA_VERSION: u8 = 1;
pub const FLOWGRAPH_PACKAGE_LOCK_FILE_NAME: &str = "flowgraph.lock.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageLockFile {
	pub kind: String,
	pub schema_version: u8,
	pub created_at_unix_ms: u64,
	pub digest: Option<String>,
	pub entry_count: usize,
	pub entries: Vec<PackageLockEntry>,
}

impl PackageLockFile {
	pub fn new(digest: Option<String>, entries: Vec<PackageLockEntry>) -> Self {
		Self::with_created_at_unix_ms(digest, entries, unix_time_ms())
	}

	pub fn with_created_at_unix_ms(digest: Option<String>, entries: Vec<PackageLockEntry>, created_at_unix_ms: u64) -> Self {
		Self {
			kind: FLOWGRAPH_PACKAGE_LOCK_FILE_KIND.into(),
			schema_version: FLOWGRAPH_PACKAGE_LOCK_FILE_SCHEMA_VERSION,
			created_at_unix_ms,
			digest,
			entry_count: entries.len(),
			entries,
		}
	}

	pub fn validate(&self) -> Result<(), PackageLockFileError> {
		if self.kind != FLOWGRAPH_PACKAGE_LOCK_FILE_KIND {
			return Err(PackageLockFileError::KindMismatch { actual: self.kind.clone() });
		}
		if self.schema_version != FLOWGRAPH_PACKAGE_LOCK_FILE_SCHEMA_VERSION {
			return Err(PackageLockFileError::UnsupportedSchemaVersion {
				actual: self.schema_version,
			});
		}
		if self.entry_count != self.entries.len() {
			return Err(PackageLockFileError::EntryCountMismatch {
				declared: self.entry_count,
				actual: self.entries.len(),
			});
		}
		if self.entries.is_empty() && self.digest.is_some() {
			return Err(PackageLockFileError::EmptyDigestMismatch);
		}
		if !self.entries.is_empty() && self.digest.is_none() {
			return Err(PackageLockFileError::MissingDigest);
		}
		Ok(())
	}

	pub fn to_json_pretty(&self) -> Result<String, PackageLockFileError> {
		self.validate()?;
		let mut json = serde_json::to_string_pretty(self)?;
		json.push('\n');
		Ok(json)
	}

	pub fn from_json_str(raw: &str) -> Result<Self, PackageLockFileError> {
		let file: Self = serde_json::from_str(raw)?;
		file.validate()?;
		Ok(file)
	}
}

pub fn write_package_lock_file(path: impl AsRef<Path>, file: &PackageLockFile) -> Result<(), PackageLockFileError> {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(path, file.to_json_pretty()?)?;
	Ok(())
}

pub fn read_package_lock_file(path: impl AsRef<Path>) -> Result<PackageLockFile, PackageLockFileError> {
	let raw = std::fs::read_to_string(path)?;
	PackageLockFile::from_json_str(&raw)
}

#[derive(Debug, Error)]
pub enum PackageLockFileError {
	#[error("package lockfile io failed: {0}")]
	Io(#[from] std::io::Error),
	#[error("package lockfile json parse failed: {0}")]
	Json(#[from] serde_json::Error),
	#[error("package lockfile kind mismatch: expected '{expected}', actual '{actual}'", expected = FLOWGRAPH_PACKAGE_LOCK_FILE_KIND)]
	KindMismatch { actual: String },
	#[error("package lockfile schema version is unsupported: expected {expected}, actual {actual}", expected = FLOWGRAPH_PACKAGE_LOCK_FILE_SCHEMA_VERSION)]
	UnsupportedSchemaVersion { actual: u8 },
	#[error("package lockfile entry count mismatch: declared {declared}, actual {actual}")]
	EntryCountMismatch { declared: usize, actual: usize },
	#[error("package lockfile digest must be absent when entries are empty")]
	EmptyDigestMismatch,
	#[error("package lockfile digest is required when entries are present")]
	MissingDigest,
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
	use std::collections::BTreeMap;

	fn entry() -> PackageLockEntry {
		PackageLockEntry {
			id: "example.pkg".into(),
			version: Some("1.0.0".into()),
			source_fq: "main".into(),
			source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
			digest: "b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into(),
			dependencies: BTreeMap::new(),
		}
	}

	fn temp_path(name: &str) -> std::path::PathBuf {
		let dir = std::env::temp_dir().join(format!("vac-package-lock-file-{}-{}", std::process::id(), unix_time_ms()));
		std::fs::create_dir_all(&dir).expect("create temp dir");
		dir.join(name)
	}

	#[test]
	fn package_lock_file_json_round_trips() {
		let file = PackageLockFile::with_created_at_unix_ms(
			Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
			vec![entry()],
			1234,
		);
		let json = file.to_json_pretty().expect("json");
		assert!(json.ends_with('\n'));
		assert!(json.contains("vac.flowgraph.package_lock"));
		let decoded = PackageLockFile::from_json_str(&json).expect("decode");
		assert_eq!(decoded, file);
	}

	#[test]
	fn package_lock_file_read_write_round_trips() {
		let path = temp_path(FLOWGRAPH_PACKAGE_LOCK_FILE_NAME);
		let file = PackageLockFile::with_created_at_unix_ms(
			Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
			vec![entry()],
			5678,
		);
		write_package_lock_file(&path, &file).expect("write");
		let decoded = read_package_lock_file(&path).expect("read");
		assert_eq!(decoded, file);
		let _ = std::fs::remove_dir_all(path.parent().unwrap());
	}

	#[test]
	fn package_lock_file_validation_rejects_bad_shape() {
		let bad_count = PackageLockFile {
			entry_count: 2,
			..PackageLockFile::with_created_at_unix_ms(
				Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()),
				vec![entry()],
				1234,
			)
		};
		assert!(matches!(
			bad_count.validate(),
			Err(PackageLockFileError::EntryCountMismatch { declared: 2, actual: 1 })
		));

		let missing_digest = PackageLockFile::with_created_at_unix_ms(None, vec![entry()], 1234);
		assert!(matches!(missing_digest.validate(), Err(PackageLockFileError::MissingDigest)));
	}
}
