//! Flowgraph 組込みノード群。
//!
//! δ-1:
//! - `literal`: 各型のリテラルソース
//! - `log`: exec 発火で `ctx.trace` に書き出す唯一の Effectful 実装
//! - `flow`: Branch / Sequence / Gate（制御フロー、Pure）
//!
//! δ-2:
//! - `logic`: Bool 論理演算
//! - `compare`: 比較演算
//! - `math`: 数値演算
//! - `string_ops`: 文字列操作
//! - `json_ops`: JSON 操作
//! - `collection`: List / Map 操作
//! - `convert`: 型変換
//!
//! δ-3:
//! - `delay`: util.delay（self-ingress StatefulNode）
//! - `state`: state.bool / state.int_counter / state.latch / state.accumulator
//! - `ingress`: ingress.web_input / ingress.voice / ingress.twitch（外部 trigger source スケルトン）
//!
//! δ-4a:
//! - `command`: command.match（V1 Command 由来、verb + args パーサ）
//! - `regex_ops`: regex.replace（V1 Modify 由来、正規表現逐次置換）
//!
//! η（Dictionary/Table Unification）:
//! - `table_ops`: table.from_json / table.to_json（Pure）、table.load_tsv / table.write_tsv（Effectful）
//!   汎用 Table 型の I/O と JSON 相互変換。辞書以外にも scene registry / credential store 等で利用可能。
//! - `dictionary`: dictionary.replace / .match (Stateful, AC + Regex キャッシュ) と
//!   dictionary.learn / .forget (Pure)。V1 dictionary.command は η-4 で廃止。
//!
//! δ-4b:
//! - `screenshot`: screenshot.capture（Windows 限定、V1 Screenshot 由来の EffectfulNode）
//! - `ocr`: ocr.recognize（Windows 限定、V1 Ocr 由来）
//! - `translate_gas`: translate.gas（V1 GasTranslation 由来、reqwest GET）
//! - `translate_libre`: translate.libre（V1 LibreTranslation 由来、reqwest POST via crate::libretranslate）
//!
//! δ-4c:
//! - `tts`: tts.speak（V1 OsTts / Bouyomichan / VoiceVox / AivisSpeech / CoeiroInk / VoicePeak 統合、
//!   driver 差替え可能な EffectfulNode）
//!
//! δ-4d:
//! - `twitch`: twitch.validate_token / twitch.user_id_by_login / twitch.chat_send
//!   （V1 TwitchOut 由来。単一 processor ではなく Helix 単機能 EffectfulNode への分解）
//! - `rate_limit`: util.rate_limit（汎用トークンバケット StatefulNode、
//!   Twitch の 100req/30s 系ゲートなどに使う）
//!
//! δ-9 Part C:
//! - `channel`: channel.emit（Flowgraph → V1 ChannelDatum push 終端。ws / browser-output への出力経路）
//!
//! δ-9 Part E:
//! - `ingress.channel_subscribe`: V1 ChannelDatum → Flowgraph 入口。`State.channel_datum_tx` を bridges 経由で
//!   subscribe し、`TriggerEvent` として graph に投入する。`channel.emit` と対称。
//! - `ingress.vmc_udp` (Phase M1): 生 UDP → Base64 `content` + `TriggerEvent`。ブリッジは `bridges::vmc_ingress`。

pub mod channel;
pub mod collection;
pub mod command;
pub mod compare;
pub mod convert;
pub mod datetime;
pub mod delay;
pub mod dictionary;
pub mod easing;
pub mod flow;
pub mod ingress;
pub mod json_ops;
pub mod library_boundary;
pub mod literal;
pub mod log;
pub mod logic;
pub mod math;
pub mod mode;
pub mod motion_vmc;
pub mod obs;
pub mod ocr;
pub mod osc_send;
pub mod vmc_send;
pub mod vmc_extract;
pub mod vrchat_osc;
pub mod random_noise;
pub mod rate_limit;
pub mod regex_ops;
pub mod screenshot;
pub mod signal_util;
pub mod state;
pub mod string_ops;
pub mod table_ops;
pub mod timer_interval;
pub mod translate_gas;
pub mod translate_libre;
pub mod tts;
pub mod twitch;
pub mod unit;
pub mod util_format;
pub mod vec;
