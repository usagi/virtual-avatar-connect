//! イベント駆動型の入力起点（ingress）のうち、**まだ Flowgraph bridge が揃っていないもの**を
//! `[[processors]]` 設定から並行起動する。
//!
//! - **Twitch IRC**: ζ-1 で Flowgraph bridge (`flowgraph.ingress.twitch` + `src/bridges/twitch.rs`) に
//!   完全移行済み。V1 経路（`[[processors]] feature="twitch"` / トップレベル `[twitch]` の IRC フィールド）
//!   は deprecation 警告のみ出して何も起動しない。
//! - **Twitch EventSub**: ζ-2 で Flowgraph 化予定。ζ-1 時点では V1 経路で起動する。
//! - **Voice**: δ-9 Part E.2 で Flowgraph 移行完了。V1 経路は廃止済み。`[[processors]] feature="voice"` が残っていれば警告のみ。
//! - **WebInput**: V1 `[[processors]] feature="webinput"` からの登録は `web_input::WebInputRegistry` 側で処理する。

use crate::conf::Conf;
use crate::twitch::eventsub as twitch_eventsub;
use crate::web_interface::web_input::WebInputRegistry;
use crate::SharedState;
use std::collections::BTreeSet;
use std::sync::Arc;

pub(crate) struct IngressHandles {
	pub eventsub: Vec<tokio::task::JoinHandle<()>>,
}

/// Voice / Twitch / WebInput の V1 ingress を初期化する。
///
/// Voice / Twitch IRC は Flowgraph 経路に完全移行したため、`[[processors]] feature="voice"` /
/// `feature="twitch"` が残っていた場合は warn ログで移行を促す（動作はしない）。
///
/// ζ-2c: Twitch EventSub については、`v2_eventsub_skip_broadcasters` に **同じ broadcaster_login
/// を購読する Flowgraph ingress が存在** することが示されている場合、V1 ループを自動で起動しない
/// （`[twitch.eventsub].force_v1_loop = true` で強制起動できる）。
pub(crate) async fn prepare(
	conf: &Conf,
	state: SharedState,
	v2_eventsub_skip_broadcasters: &BTreeSet<String>,
) -> anyhow::Result<(IngressHandles, Arc<WebInputRegistry>)> {
	let registry = Arc::new(WebInputRegistry::from_conf(conf)?);
	let mut eventsub_handles = Vec::new();
	let mut twitch_processor_detected = false;

	for pc in &conf.processors {
		if !pc.is_enabled {
			continue;
		}
		if matches!(pc.feature.as_deref().map(|s| s.eq_ignore_ascii_case("voice")), Some(true)) {
			log::warn!(
				"《Voice》: [[processors]] feature=\"voice\" は δ-9 Part E.2 で廃止されました。\
				 `flowgraph.ingress.voice` ノード + `flowgraph.local/*.flowgraph.toml` へ移行してください。id={:?}",
				pc.id
			);
			continue;
		}
		if !matches!(pc.feature.as_deref().map(|s| s.eq_ignore_ascii_case("twitch")), Some(true)) {
			continue;
		}
		// ζ-1: Twitch IRC ingress は Flowgraph bridge が担当する。V1 processor は deprecation。
		log::warn!(
			"《Twitch》: [[processors]] feature=\"twitch\" の IRC ingress は ζ-1 で廃止されました。\
			 `flowgraph.ingress.twitch`（mode=\"irc\"）＋ `flowgraph.local/*.flowgraph.toml` へ移行してください。id={:?}",
			pc.id
		);
		twitch_processor_detected = true;
		// EventSub 設定が processor 内に書かれていた場合は V1 経路で動かす。
		// ζ-2c: ただし同じ broadcaster_login を購読する Flowgraph ingress があり、
		// `force_v1_loop = false`（既定）なら V1 はスキップして WS の二重化を避ける。
		if let Some(ref es) = pc.twitch_eventsub {
			let u = pc.twitch_username.as_deref().unwrap_or("");
			let ch = pc.channel_to.as_deref().unwrap_or("");
			let bl_key = es
				.broadcaster_login
				.as_deref()
				.unwrap_or(u)
				.trim()
				.trim_start_matches('#')
				.to_lowercase();
			let should_skip = !bl_key.is_empty() && !es.force_v1_loop && v2_eventsub_skip_broadcasters.contains(&bl_key);
			if should_skip {
				log::info!(
					"《Twitch》: EventSub V1 ループは Flowgraph ingress により自動スキップされました \
					 (processor id={:?}, broadcaster_login={:?})。V1 も並走させたい場合は \
					 `[twitch.eventsub].force_v1_loop = true` を設定してください。",
					pc.id,
					bl_key
				);
			} else if let Some(resolved) = twitch_eventsub::resolve_from_processor(u, ch, es).await {
				match resolved {
					Ok(r) => {
						log::info!("《Twitch》: EventSub を開始します (processor id={:?})", pc.id);
						eventsub_handles.push(twitch_eventsub::spawn_eventsub_loop(state.clone(), r));
					}
					Err(e) => log::error!("《Twitch》 EventSub: {:?}", e),
				}
			}
		}
	}

	// トップレベル [twitch] の IRC フィールドは deprecated。Flowgraph 側で定義し直すよう促す。
	if let Some(ref tw) = conf.twitch {
		let irc_fields_set = !tw.username.trim().is_empty()
			|| tw.channel_to.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false)
			|| tw.reads.as_ref().map(|r| !r.is_empty()).unwrap_or(false);
		if irc_fields_set {
			log::warn!(
				"《Twitch》: トップレベル `[twitch]` の IRC フィールド（username / channel_to / reads）は ζ-1 で廃止されました。\
				 `flowgraph.ingress.twitch` ノードで定義してください（Flowgraph bridge が担当します）。"
			);
		}
		// EventSub はトップレベル定義からも V1 経路で起動する。
		// ζ-2c: 同じ broadcaster_login を扱う Flowgraph ingress があれば自動スキップ。
		if !twitch_processor_detected {
			if let Some(ref es) = tw.eventsub {
				let bl_key = es
					.broadcaster_login
					.as_deref()
					.unwrap_or(&tw.username)
					.trim()
					.trim_start_matches('#')
					.to_lowercase();
				let should_skip = !bl_key.is_empty() && !es.force_v1_loop && v2_eventsub_skip_broadcasters.contains(&bl_key);
				if should_skip {
					log::info!(
						"《Twitch》: トップレベル EventSub V1 ループは Flowgraph ingress により自動スキップされました \
						 (broadcaster_login={:?})。V1 も並走させたい場合は \
						 `[twitch.eventsub].force_v1_loop = true` を設定してください。",
						bl_key
					);
				} else if let Some(resolved) = twitch_eventsub::resolve_from_top_level(&tw.username, tw.effective_channel_to(), es).await {
					match resolved {
						Ok(r) => {
							log::info!("《Twitch》: トップレベル EventSub を開始します");
							eventsub_handles.push(twitch_eventsub::spawn_eventsub_loop(state.clone(), r));
						}
						Err(e) => log::error!("《Twitch》 EventSub: {:?}", e),
					}
				}
			}
		} else if tw.eventsub.is_some() {
			log::warn!(
				"《Twitch》: [[processors]] で twitch を定義しているため、設定トップレベルの twitch.eventsub は無視されます。EventSub は [[processors]] の twitch ブロックに twitch_eventsub = {{ ... }} を書いてください。"
			);
		}
	}

	Ok((
		IngressHandles {
			eventsub: eventsub_handles,
		},
		registry,
	))
}
