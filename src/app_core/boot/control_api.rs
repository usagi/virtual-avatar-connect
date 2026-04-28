use crate::conf::Conf;
use crate::state::SharedState;
use crate::{web_interface, Result};

pub(super) async fn init_control_api_runtime(conf: &Conf, state: &SharedState) -> Result<web_interface::control::ControlApiRuntime> {
	let control_api_runtime = web_interface::control::ControlApiRuntime::init(conf, state).await?;
	log_control_api_policy(&control_api_runtime);
	Ok(control_api_runtime)
}

fn log_control_api_policy(control_api_runtime: &web_interface::control::ControlApiRuntime) {
	log::info!(
		"《ControlAPI》 認証ポリシー: loopback={}, non_loopback={} (token_source={:?})",
		if control_api_runtime.require_token_for_loopback {
			"token-required"
		} else {
			"allow"
		},
		if control_api_runtime.require_token_for_non_loopback {
			"token-required"
		} else {
			"allow"
		},
		control_api_runtime.token_source
	);
	if let Some(path) = control_api_runtime.written_token_file.as_ref() {
		log::info!(
			"《ControlAPI》 自動生成トークンを書き出しました: {:?}（GUI クライアントはこのファイルを読み取って Authorization: Bearer <token> に使う）",
			path
		);
	}
}
