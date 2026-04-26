//! `TtsDriver` のプロセス全体レジストリ。
//!
//! LazyLock で 1 回だけ初期化し、以後はスレッド越しに readonly で共有する。
//! 動的な登録解除は想定しない（静的登録のみ）。VoicePeak 等新エンジンを足すときは
//! [`default_registry`] のリストに 1 行追加するだけ。

use super::driver::TtsDriver;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub struct TtsRegistry {
	by_name: HashMap<&'static str, Arc<dyn TtsDriver>>,
}

impl TtsRegistry {
	pub fn empty() -> Self {
		Self { by_name: HashMap::new() }
	}

	pub fn register(&mut self, driver: Arc<dyn TtsDriver>) {
		self.by_name.insert(driver.name(), driver);
	}

	pub fn get(&self, name: &str) -> Option<Arc<dyn TtsDriver>> {
		self.by_name.get(name).cloned()
	}

	pub fn names(&self) -> Vec<&'static str> {
		let mut v: Vec<_> = self.by_name.keys().copied().collect();
		v.sort();
		v
	}

	pub fn len(&self) -> usize {
		self.by_name.len()
	}

	pub fn is_empty(&self) -> bool {
		self.by_name.is_empty()
	}
}

/// 同梱ドライバ一覧。新規追加は `super::drivers::<name>` を作ってからここに 1 行。
fn default_registry() -> TtsRegistry {
	use super::drivers;
	let mut r = TtsRegistry::empty();
	r.register(Arc::new(drivers::os::OsDriver));
	r.register(Arc::new(drivers::bouyomichan::BouyomichanDriver));
	r.register(Arc::new(drivers::voicevox::VoicevoxDriver));
	r.register(Arc::new(drivers::aivis_speech::AivisSpeechDriver));
	r.register(Arc::new(drivers::coeiroink::CoeiroinkDriver));
	r.register(Arc::new(drivers::voicepeak::VoicepeakDriver));
	r
}

static REGISTRY: LazyLock<TtsRegistry> = LazyLock::new(default_registry);

/// プロセス全体の TTS レジストリ。
pub fn registry() -> &'static TtsRegistry {
	&REGISTRY
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_registry_has_expected_engines() {
		let r = registry();
		let names = r.names();
		assert!(names.contains(&"os"));
		assert!(names.contains(&"bouyomichan"));
		assert!(names.contains(&"voicevox"));
		assert!(names.contains(&"aivis_speech"));
		assert!(names.contains(&"coeiroink"));
		assert!(names.contains(&"voicepeak"));
		assert_eq!(r.len(), 6);
	}

	#[test]
	fn unknown_engine_returns_none() {
		assert!(registry().get("nonexistent_engine").is_none());
	}

	#[test]
	fn lookup_returns_correct_name() {
		let d = registry().get("voicevox").unwrap();
		assert_eq!(d.name(), "voicevox");
	}
}
