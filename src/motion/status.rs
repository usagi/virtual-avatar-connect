//! VMC passthrough の Control API 向けランタイム状態。

use crate::conf::{Conf, VmcPassthroughSpec};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VmcPassthroughBindState {
	Configured,
	Skipped,
	Running,
	Failed,
	Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct VmcPassthroughStatusView {
	pub id: String,
	pub label: Option<String>,
	pub enabled: bool,
	pub bind: String,
	pub forward_to: Vec<String>,
	pub state: VmcPassthroughBindState,
	pub error: Option<String>,
	pub packets_received: u64,
	pub bytes_received: u64,
	pub packets_forwarded: u64,
	pub send_errors: u64,
	pub last_packet_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VmcStatusSnapshot {
	pub entries: Vec<VmcPassthroughStatusView>,
}

#[derive(Debug)]
pub struct VmcPassthroughStatusEntry {
	id: String,
	label: Option<String>,
	enabled: bool,
	bind: String,
	forward_to: Mutex<Vec<String>>,
	forward_addrs: Mutex<Vec<SocketAddr>>,
	state: Mutex<VmcPassthroughBindState>,
	error: Mutex<Option<String>>,
	packets_received: AtomicU64,
	bytes_received: AtomicU64,
	packets_forwarded: AtomicU64,
	send_errors: AtomicU64,
	last_packet_at: Mutex<Option<String>>,
}

impl VmcPassthroughStatusEntry {
	fn from_spec(index: usize, spec: &VmcPassthroughSpec) -> Arc<Self> {
		let label = spec.label.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
		let id = label.clone().unwrap_or_else(|| format!("vmc-passthrough-{}", index + 1));
		Arc::new(Self {
			id,
			label,
			enabled: spec.enabled,
			bind: spec.bind.clone(),
			forward_to: Mutex::new(spec.forward_to.clone()),
			forward_addrs: Mutex::new(parse_socket_addrs_lossy(&spec.forward_to)),
			state: Mutex::new(VmcPassthroughBindState::Configured),
			error: Mutex::new(None),
			packets_received: AtomicU64::new(0),
			bytes_received: AtomicU64::new(0),
			packets_forwarded: AtomicU64::new(0),
			send_errors: AtomicU64::new(0),
			last_packet_at: Mutex::new(None),
		})
	}

	pub(crate) fn mark_skipped(&self, reason: impl Into<String>) {
		self.set_state(VmcPassthroughBindState::Skipped, Some(reason.into()));
	}

	pub(crate) fn mark_running(&self) {
		self.set_state(VmcPassthroughBindState::Running, None);
	}

	pub(crate) fn mark_failed(&self, error: impl Into<String>) {
		self.set_state(VmcPassthroughBindState::Failed, Some(error.into()));
	}

	pub(crate) fn mark_stopped(&self) {
		self.set_state(VmcPassthroughBindState::Stopped, None);
	}

	pub(crate) fn record_receive(&self, bytes: usize, destinations: usize) {
		self.packets_received.fetch_add(1, Ordering::Relaxed);
		self.bytes_received.fetch_add(bytes as u64, Ordering::Relaxed);
		self.packets_forwarded.fetch_add(destinations as u64, Ordering::Relaxed);
		if let Ok(mut g) = self.last_packet_at.lock() {
			*g = Some(crate::datetime::DateTime::now().to_rfc3339());
		}
	}

	pub(crate) fn record_send_error(&self) {
		self.send_errors.fetch_add(1, Ordering::Relaxed);
	}

	pub(crate) fn is_running(&self) -> bool {
		self.state.lock().map(|g| *g == VmcPassthroughBindState::Running).unwrap_or(false)
	}

	pub(crate) fn forward_addrs_snapshot(&self) -> Vec<SocketAddr> {
		self.forward_addrs.lock().map(|g| g.clone()).unwrap_or_default()
	}

	pub(crate) fn add_forward(&self, raw: &str) -> Result<VmcPassthroughStatusView, String> {
		let addr = parse_socket_addr(raw)?;
		let normalized = addr.to_string();
		let mut addrs = self.forward_addrs.lock().map_err(|_| "forward address lock poisoned".to_string())?;
		if addrs.contains(&addr) {
			return Ok(self.snapshot());
		}
		addrs.push(addr);
		drop(addrs);

		let mut raw_list = self.forward_to.lock().map_err(|_| "forward list lock poisoned".to_string())?;
		raw_list.push(normalized);
		drop(raw_list);
		Ok(self.snapshot())
	}

	pub(crate) fn remove_forward(&self, raw: &str) -> Result<VmcPassthroughStatusView, String> {
		let addr = parse_socket_addr(raw)?;
		let mut addrs = self.forward_addrs.lock().map_err(|_| "forward address lock poisoned".to_string())?;
		let before = addrs.len();
		addrs.retain(|a| *a != addr);
		let removed = addrs.len() != before;
		drop(addrs);

		if removed {
			let mut raw_list = self.forward_to.lock().map_err(|_| "forward list lock poisoned".to_string())?;
			raw_list.retain(|s| parse_socket_addr(s).map(|a| a != addr).unwrap_or(true));
		}
		Ok(self.snapshot())
	}

	fn set_state(&self, state: VmcPassthroughBindState, error: Option<String>) {
		if let Ok(mut g) = self.state.lock() {
			*g = state;
		}
		if let Ok(mut g) = self.error.lock() {
			*g = error;
		}
	}

	fn snapshot(&self) -> VmcPassthroughStatusView {
		VmcPassthroughStatusView {
			id: self.id.clone(),
			label: self.label.clone(),
			enabled: self.enabled,
			bind: self.bind.clone(),
			forward_to: self.forward_to.lock().map(|g| g.clone()).unwrap_or_default(),
			state: self.state.lock().map(|g| g.clone()).unwrap_or(VmcPassthroughBindState::Failed),
			error: self.error.lock().ok().and_then(|g| g.clone()),
			packets_received: self.packets_received.load(Ordering::Relaxed),
			bytes_received: self.bytes_received.load(Ordering::Relaxed),
			packets_forwarded: self.packets_forwarded.load(Ordering::Relaxed),
			send_errors: self.send_errors.load(Ordering::Relaxed),
			last_packet_at: self.last_packet_at.lock().ok().and_then(|g| g.clone()),
		}
	}
}

#[derive(Debug)]
pub struct VmcPassthroughStatusRegistry {
	entries: Vec<Arc<VmcPassthroughStatusEntry>>,
}

impl VmcPassthroughStatusRegistry {
	pub fn from_conf(conf: &Conf) -> Arc<Self> {
		let entries = conf
			.motion
			.as_ref()
			.map(|m| Self::entries_from_specs(&m.vmc_passthrough))
			.unwrap_or_default();
		Arc::new(Self { entries })
	}

	fn entries_from_specs(specs: &[VmcPassthroughSpec]) -> Vec<Arc<VmcPassthroughStatusEntry>> {
		specs
			.iter()
			.enumerate()
			.map(|(index, spec)| VmcPassthroughStatusEntry::from_spec(index, spec))
			.collect()
	}

	pub(crate) fn entry(&self, index: usize) -> Option<Arc<VmcPassthroughStatusEntry>> {
		self.entries.get(index).cloned()
	}

	pub(crate) fn find(&self, id: &str) -> Option<Arc<VmcPassthroughStatusEntry>> {
		self.entries.iter().find(|entry| entry.id == id).cloned()
	}

	pub fn snapshot(&self) -> VmcStatusSnapshot {
		VmcStatusSnapshot {
			entries: self.entries.iter().map(|entry| entry.snapshot()).collect(),
		}
	}
}

fn parse_socket_addr(raw: &str) -> Result<SocketAddr, String> {
	raw.trim()
		.parse::<SocketAddr>()
		.map_err(|e| format!("invalid SocketAddr {:?}: {}", raw, e))
}

fn parse_socket_addrs_lossy(raw: &[String]) -> Vec<SocketAddr> {
	raw.iter().filter_map(|s| parse_socket_addr(s).ok()).collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::conf::VmcPassthroughSpec;

	#[test]
	fn snapshot_includes_configured_entries() {
		let registry = Arc::new(VmcPassthroughStatusRegistry {
			entries: VmcPassthroughStatusRegistry::entries_from_specs(&[VmcPassthroughSpec {
				enabled: true,
				label: Some("primary".into()),
				bind: "0.0.0.0:39539".into(),
				forward_to: vec!["127.0.0.1:39540".into()],
			}]),
		});
		let snapshot = registry.snapshot();
		assert_eq!(snapshot.entries.len(), 1);
		assert_eq!(snapshot.entries[0].id, "primary");
		assert_eq!(snapshot.entries[0].state, VmcPassthroughBindState::Configured);
	}

	#[test]
	fn entry_records_receive_stats() {
		let registry = Arc::new(VmcPassthroughStatusRegistry {
			entries: VmcPassthroughStatusRegistry::entries_from_specs(&[VmcPassthroughSpec {
				enabled: true,
				label: None,
				bind: "0.0.0.0:39539".into(),
				forward_to: vec!["127.0.0.1:39540".into(), "127.0.0.1:39541".into()],
			}]),
		});
		let entry = registry.entry(0).expect("entry");
		entry.mark_running();
		entry.record_receive(128, 2);
		entry.record_send_error();

		let view = registry.snapshot().entries.remove(0);
		assert_eq!(view.id, "vmc-passthrough-1");
		assert_eq!(view.state, VmcPassthroughBindState::Running);
		assert_eq!(view.packets_received, 1);
		assert_eq!(view.bytes_received, 128);
		assert_eq!(view.packets_forwarded, 2);
		assert_eq!(view.send_errors, 1);
		assert!(view.last_packet_at.is_some());
	}

	#[test]
	fn entry_add_remove_forward_updates_snapshot_and_addrs() {
		let registry = Arc::new(VmcPassthroughStatusRegistry {
			entries: VmcPassthroughStatusRegistry::entries_from_specs(&[VmcPassthroughSpec {
				enabled: true,
				label: Some("primary".into()),
				bind: "0.0.0.0:39539".into(),
				forward_to: vec!["127.0.0.1:39540".into()],
			}]),
		});
		let entry = registry.entry(0).expect("entry");
		entry.add_forward("127.0.0.1:39541").expect("add forward");
		assert_eq!(entry.forward_addrs_snapshot().len(), 2);
		assert_eq!(
			entry.snapshot().forward_to,
			vec!["127.0.0.1:39540".to_string(), "127.0.0.1:39541".to_string()]
		);

		entry.remove_forward("127.0.0.1:39540").expect("remove forward");
		assert_eq!(entry.forward_addrs_snapshot().len(), 1);
		assert_eq!(entry.snapshot().forward_to, vec!["127.0.0.1:39541".to_string()]);
	}
}
