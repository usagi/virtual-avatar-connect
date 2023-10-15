mod processor_conf;

pub use anyhow::{bail, Result};
pub use processor_conf::*;

use crate::{Arc, Args, RwLock};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type SharedConf = Arc<RwLock<Conf>>;

pub const DEFAULT_WEB_UI_ADDRESS: &str = "127.0.0.1:57000";

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RunWith {
 Command(String),
 CommandIfProcessIsNotRunning {
  command: String,
  if_not_running: Option<String>,
  run_as_admin: Option<bool>,
  working_dir: Option<String>,
 },
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Conf {
 pub workers: Option<usize>,

 pub web_ui_address: Option<String>,
 // デフォルト値を Some("resources".to_string()) に指定
 #[serde(default = "default_web_ui_resources_path")]
 pub web_ui_resources_path: Option<String>,

 pub state_data_auto_save: Option<bool>,
 pub state_data_path: Option<PathBuf>,
 pub state_data_capacity: Option<usize>,
 pub state_data_pretty: Option<bool>,

 #[serde(default)]
 pub run_with: Vec<RunWith>,

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

  // processors に同一の id が指定されていないかチェック
  let mut already_used_ids = std::collections::HashSet::new();
  for processor in conf.processors.iter() {
   if let Some(pid) = processor.id.as_ref() {
    if !already_used_ids.insert(pid.clone()) {
     log::error!(
      "プロセッサーID {:?} が複数回指定されています。プロセッサーIDを定義する場合は同じIDが複数回指定されないように設定して下さい。",
      pid
     );
     bail!("プロセッサーID {:?} が複数回指定されています。", pid);
    }
   }
  }

  Ok(conf)
 }

 pub fn execute_run_with(&self) -> Result<()> {
  use sysinfo::{ProcessExt, ProcessRefreshKind, SystemExt};
  let mut system = sysinfo::System::new();
  system.refresh_processes_specifics(ProcessRefreshKind::everything().without_cpu());

  for run_with in self.run_with.iter() {
   let (command, if_not_running, run_as_admin, working_dir) = match run_with {
    RunWith::Command(command) => (command, None, false, None),
    RunWith::CommandIfProcessIsNotRunning {
     command,
     if_not_running,
     run_as_admin,
     working_dir,
    } => (
     command,
     if_not_running.as_ref(),
     run_as_admin.unwrap_or_default(),
     working_dir.as_ref(),
    ),
   };

   // if_not_running が指定されている場合は、プロセスが実行中か確認して実行中ならスキップ
   if let Some(if_not_running) = if_not_running {
    if system
     .processes()
     .iter()
     .any(|(_, process)| process.name().contains(if_not_running))
    {
     log::info!(
      "run_with: 既に {} を含むプロセスが実行中のため {} の実行はスキップされます。",
      if_not_running,
      command
     );
     continue;
    }
   }

   // command (引数がある場合も考慮)を実行、またはURLを開く
   if command.starts_with("http://") || command.starts_with("https://") {
    use webbrowser::{Browser, BrowserOptions};
    log::info!("run_with: {:?} を URL としてブラウザーで開きます。", command);
    if let Err(e) = webbrowser::open_browser_with_options(Browser::Default, command, BrowserOptions::new().with_target_hint("vac")) {
     log::error!("run_with: URL を開く際にエラーが発生しました: {:?}", e);
    }
   } else {
    let original_dir = match change_working_dir(working_dir) {
     Ok(original_dir) => original_dir.map(|p| p.to_string_lossy().to_string()),
     Err(e) => {
      log::error!("run_with: 作業ディレクトリーの変更に失敗しました: {:?}", e);
      continue;
     },
    };
    if run_as_admin {
     log::warn!("run_with: {:?} を管理者権限で実行を試みます。", command);
     if let Err(e) = runas::Command::new(command).status() {
      log::error!("run_with: 管理者権限でコマンドを実行する際にエラーが発生しました: {:?}", e);
     }
    } else {
     log::info!("run_with: {:?} をコマンドとして実行します。", command);
     if let Err(e) = duct::cmd!(command).start() {
      log::error!("run_with: コマンドを実行する際にエラーが発生しました: {:?}", e);
     }
    }
    change_working_dir(original_dir.as_ref())?;
   }
  }
  Ok(())
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

fn default_web_ui_resources_path() -> Option<String> {
 Some("resources".to_string())
}

/// 現在の作業ディレクトリを変更して、変更前の作業ディレクトリを返します。
fn change_working_dir(working_dir: Option<&String>) -> Result<Option<std::path::PathBuf>> {
 if let Some(working_dir) = working_dir {
  let original_dir = std::env::current_dir()?;
  std::env::set_current_dir(working_dir)?;
  Ok(Some(original_dir))
 } else {
  Ok(None)
 }
}
