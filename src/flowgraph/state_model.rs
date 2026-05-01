//! Flowgraph state model metadata.
//!
//! LF-7 keeps this as read-only policy metadata. Runtime snapshot / restore behavior is not implemented here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateScope {
	None,
	NodeInstance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateStorage {
	None,
	Volatile,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateLifetime {
	None,
	ProgramInstance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateSnapshotPolicy {
	Unsupported,
	Explicit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateSnapshotFormat {
	None,
	Json,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateRestorePolicy {
	Unsupported,
	Explicit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateMigrationPolicy {
	None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatePersistencePolicy {
	None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowgraphStateModel {
	pub version: u8,
	pub stateful: bool,
	pub scope: StateScope,
	pub storage: StateStorage,
	pub lifetime: StateLifetime,
	pub reinitialized_on_reload: bool,
	pub snapshot_supported: bool,
	pub snapshot_policy: StateSnapshotPolicy,
	pub snapshot_format: StateSnapshotFormat,
	pub restore_supported: bool,
	pub restore_policy: StateRestorePolicy,
	pub migration_policy: StateMigrationPolicy,
	pub persistence_policy: StatePersistencePolicy,
}

impl FlowgraphStateModel {
	pub fn for_effect_class(effect_class: &str) -> Self {
		if effect_class == "stateful" {
			Self::volatile_node_instance()
		} else {
			Self::stateless()
		}
	}

	pub fn stateless() -> Self {
		Self {
			version: 1,
			stateful: false,
			scope: StateScope::None,
			storage: StateStorage::None,
			lifetime: StateLifetime::None,
			reinitialized_on_reload: false,
			snapshot_supported: false,
			snapshot_policy: StateSnapshotPolicy::Unsupported,
			snapshot_format: StateSnapshotFormat::None,
			restore_supported: false,
			restore_policy: StateRestorePolicy::Unsupported,
			migration_policy: StateMigrationPolicy::None,
			persistence_policy: StatePersistencePolicy::None,
		}
	}

	pub fn volatile_node_instance() -> Self {
		Self {
			version: 1,
			stateful: true,
			scope: StateScope::NodeInstance,
			storage: StateStorage::Volatile,
			lifetime: StateLifetime::ProgramInstance,
			reinitialized_on_reload: true,
			snapshot_supported: false,
			snapshot_policy: StateSnapshotPolicy::Unsupported,
			snapshot_format: StateSnapshotFormat::None,
			restore_supported: false,
			restore_policy: StateRestorePolicy::Unsupported,
			migration_policy: StateMigrationPolicy::None,
			persistence_policy: StatePersistencePolicy::None,
		}
	}

	pub fn volatile_node_instance_json_snapshot() -> Self {
		Self {
			version: 1,
			stateful: true,
			scope: StateScope::NodeInstance,
			storage: StateStorage::Volatile,
			lifetime: StateLifetime::ProgramInstance,
			reinitialized_on_reload: true,
			snapshot_supported: true,
			snapshot_policy: StateSnapshotPolicy::Explicit,
			snapshot_format: StateSnapshotFormat::Json,
			restore_supported: true,
			restore_policy: StateRestorePolicy::Explicit,
			migration_policy: StateMigrationPolicy::None,
			persistence_policy: StatePersistencePolicy::None,
		}
	}
}
