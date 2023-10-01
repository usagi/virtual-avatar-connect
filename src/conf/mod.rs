mod processor_conf;

pub use anyhow::Result;
pub use processor_conf::*;

use crate::{Arc, Args, RwLock};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type SharedConf = Arc<RwLock<Conf>>;

pub const DEFAULT_WEB_UI_ADDRESS: &str = "127.0.0.1:57000";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Conf {
 pub workers: Option<usize>,

 pub web_ui_address: Option<String>,
 pub web_ui_resources_path: Option<String>,

 pub state_data_auto_save: Option<bool>,
 pub state_data_path: Option<PathBuf>,
 pub state_data_capacity: Option<usize>,
 pub state_data_pretty: Option<bool>,

 #[serde(default)]
 pub run_with: Vec<String>,

 pub log_level: Option<String>,

 #[serde(default)]
 pub processors: Vec<ProcessorConf>,
}

impl Conf {
 fn load<P: Into<PathBuf>>(path: P) -> Result<Self> {
  let path = path.into();
  let conf = std::fs::read_to_string(&path)?;
  let conf = toml::from_str(&conf)?;
  Ok(conf)
 }

 pub fn new(args: &Args) -> Result<Self> {
  let conf = Self::load(&args.conf)?;
  // ログレベルの設定(argsで--debugが指定されていない場合)
  if let Some(log_filter) = conf.log_level.as_ref() {
   let v = match log_filter.to_lowercase().as_str() {
    "off" => Some(log::LevelFilter::Off),
    "error" => Some(log::LevelFilter::Error),
    "warn" => Some(log::LevelFilter::Warn),
    "info" => Some(log::LevelFilter::Info),
    "debug" => Some(log::LevelFilter::Debug),
    "trace" => Some(log::LevelFilter::Trace),
    _ => None,
   };
   match v {
    Some(llf) if !args.debug => {
     log::info!("ログレベルを {} に設定します。", llf);
     log::set_max_level(llf);
    },
    Some(_) if args.debug => {
     log::warn!("設定ファイルでログレベルが {} に設定されていますが、コマンドライン引数で -D/--debug が指定されているためログレベルは Trace が維持されます。", log_filter);
    },
    _ => {
     log::warn!(
      "設定ファイルでログレベルが {} に設定されていますが、不正な値として無視されました。",
      log_filter
     );
    },
   }
  }
  Ok(conf)
 }

 pub fn to_shared(self) -> SharedConf {
  Arc::new(RwLock::new(self))
 }

 pub fn get_workers(&self) -> usize {
  self.workers.unwrap_or_else(|| num_cpus::get())
 }

 pub fn get_web_ui_address(&self) -> &str {
  match self.web_ui_address.as_ref() {
   Some(a) => a,
   None => DEFAULT_WEB_UI_ADDRESS,
  }
 }
}
