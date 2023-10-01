use env_logger::{Builder, Env};

pub fn init() {
 Builder::from_env(Env::default().default_filter_or("virtual_avatar_connect=trace")).init();
 log::info!("ロガーを初期化しました。");
}
