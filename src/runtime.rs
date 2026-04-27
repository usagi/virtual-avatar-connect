//! アプリ設定から core runtime path primitives を初期化する adapter。

use crate::Conf;
use anyhow::Result;

pub use vac_core::runtime::{RuntimePathConfig, RuntimePaths};

pub fn runtime_path_config_from_conf(conf: &Conf) -> RuntimePathConfig {
	RuntimePathConfig {
		runtime_dir: conf.runtime_dir.clone(),
		attachment_inline_max_bytes: conf.attachment_inline_max_bytes,
	}
}

pub fn init_runtime_paths(conf: &Conf) -> Result<RuntimePaths> {
	RuntimePaths::init(runtime_path_config_from_conf(conf))
}
