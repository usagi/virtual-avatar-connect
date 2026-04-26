//! Twitch 周りの snapshot 用サマリー計算。

use crate::conf::Twitch;
use crate::twitch::oauth::{try_load_valid_token_for, OAuthIdent};

/// Twitch の保存済みトークンが有効かどうかを判定する。
pub(super) async fn compute_twitch_authorized(t: &Twitch) -> (bool, Option<bool>) {
	// eventsub 設定が無ければ OAuthIdent を組めないので、moderator だけ false 相当で返す。
	let Some(es) = t.eventsub.as_ref() else {
		return (false, t.moderator.as_ref().map(|_| false));
	};

	let b_ident = OAuthIdent::for_broadcaster(es);
	let broadcaster_authorized = if b_ident.client_id.is_empty() {
		false
	} else {
		try_load_valid_token_for(&b_ident).await.is_some()
	};

	let moderator_authorized = if let Some(mc) = t.moderator.as_ref() {
		let m_ident = OAuthIdent::for_moderator(es, mc);
		let ok = if m_ident.client_id.is_empty() {
			false
		} else {
			try_load_valid_token_for(&m_ident).await.is_some()
		};
		Some(ok)
	} else {
		None
	};

	(broadcaster_authorized, moderator_authorized)
}
