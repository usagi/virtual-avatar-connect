use crate::{utility::bool_true, Arc, RwLock};
use serde::{Deserialize, Serialize};

pub type SharedProcessorConf = Arc<RwLock<ProcessorConf>>;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContentWithChannel {
 pub channel: String,
 pub content: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CommandSet {
 pub name: String,
 #[serde(default)]
 pub pre: Vec<String>,
 #[serde(default)]
 pub post: Vec<String>,
 #[serde(default)]
 pub channel_contents: Vec<ContentWithChannel>,
}

// Note: 旧 `OpenAiChatFinetuning` / `OpenAiFewShotTurn` は Phase I で `crate::ai::config` に移設しました。

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProcessorConf {
 // Common
 /// プロセッサーの定義ごとに個別に名付けを行えます。
 /// IDを設定しなくてもたいていの機能は動作します。一部の高度な機能を使用する場合には必須となる場合があります。
 /// IDを設定すると同じ feature のプロセッサーを複数使用する場合にログで見分けやすくなったり、
 pub id: Option<String>,

 #[serde(default = "bool_true")]
 pub is_enabled: bool,
 #[serde(default)]
 pub group: Vec<String>,
 pub channel_from: Option<String>,
 pub channel_to: Option<String>,
 pub feature: Option<String>,
 #[serde(default)]
 pub pre_replace_regex_patterns: Vec<Vec<String>>,
 #[serde(default)]
 pub post_replace_regex_patterns: Vec<Vec<String>>,

 // command
 pub through_if_not_command: Option<bool>,
 #[serde(default)]
 pub response_mod: Vec<Vec<String>>,
 #[serde(default)]
 pub set: Vec<CommandSet>,

 // screenshot
 pub title: Option<String>,
 pub title_regex: Option<String>,
 #[serde(default)]
 pub crops: Vec<Vec<Option<i32>>>,
 #[serde(default)]
 pub paths: Vec<String>,
 #[serde(default)]
 pub client_only: bool,
 #[serde(default)]
 pub bitblt: bool,
 pub to_data_urls: Option<bool>,

 // ocr
 pub lang: Option<String>,
 #[serde(default)]
 pub load_from: Vec<String>,
 pub load_from_content: Option<bool>,
 pub lines: Option<bool>,
 pub auto_delete_processed_file: Option<bool>,
 pub check_result_lang: Option<bool>,

 // modify
 pub modify: Option<bool>,
 #[serde(default)]
 pub dictionary_files: Vec<String>,
 #[serde(default)]
 pub regex_files: Vec<String>,
 pub sort_dictionary: Option<String>,
 pub alkana: Option<bool>,
 /// Phase VI-γ-8a: 実行時 append / remove の書込先辞書ファイル。
 /// `dictionary_files` に含まれている必要がある。未指定なら Control API からの辞書書込は 409。
 pub writable_dictionary_file: Option<String>,
 /// Phase VI-γ-8a: 実行時 append / remove の書込先正規表現ファイル。
 /// `regex_files` に含まれている必要がある。未指定なら Control API からの正規表現書込は 409。
 pub writable_regex_file: Option<String>,

 // --- dictionary-command (Phase VI-γ-8b) ---
 /// 《DictionaryCommand》: 学習/忘却コマンドの反映先となる Modify processor の id。
 /// 対象 Modify 側で `writable_dictionary_file` が設定されている必要がある。未指定なら processor 初期化失敗。
 pub target_modify_id: Option<String>,
 /// 《DictionaryCommand》: 許可する `ChannelDatum.flags` の集合（OR 判定）。
 /// 空かつ `allowed_logins` も空なら全許可。現状 Twitch IRC は flags を埋めていないため、
 /// trusted ingress（VAC 内部 ingress / WebInput など）と組む前提。
 #[serde(default)]
 pub allowed_flags: Vec<String>,
 /// 《DictionaryCommand》: 許可する login（小文字比較）の集合（OR 判定、`allowed_flags` と合わせて OR）。
 /// meta["sender_login"] 優先、無ければ `strip_chat_prefix` で剥がした prefix。
 #[serde(default)]
 pub allowed_logins: Vec<String>,
 /// 《DictionaryCommand》: Twitch IRC 等の `"Name:msg"` プレフィックスを剥がしてから
 /// コマンド判定するか（既定 true）。
 pub strip_chat_prefix: Option<bool>,

 // --- OpenAI Chat は Phase I で `[[ai.personas]]` (`crate::ai::AiPersonaConf`) へ昇格しました。
 //     旧 ProcessorConf の openai-chat 系フィールドは削除済み。旧 `[[processors]] feature = "openai-chat"`
 //     のまま起動した conf は解析時エラーになって気付けます（State::init_processors のガードも参照）。

 // gas-translation
 pub script_id: Option<String>,
 pub translate_from: Option<String>,
 pub translate_to: Option<String>,
 pub process_incomplete_input: Option<bool>,

 // libre-translation（ローカル LibreTranslate HTTP）
 /// 例: `http://127.0.0.1:5000`。未指定かつ `libretranslate_embed_auto` が true のとき Windows で embeddable Python を展開して自動起動（試験的）。
 pub libretranslate_url: Option<String>,
 /// 自動起動時の待受ポート（未指定時 5000）。`libretranslate_url` 指定時は無視。
 pub libretranslate_port: Option<u16>,
 /// `false` のとき URL 未指定でも embed しない（手動起動のみ）。
 pub libretranslate_embed_auto: Option<bool>,

 // CoeiroInk
 pub api_url: Option<String>,
 pub speaker_uuid: Option<String>,
 pub style_id: Option<i64>,
 pub speed_scale: Option<f64>,
 pub volume_scale: Option<f64>,
 pub pitch_scale: Option<f64>,
 pub intonation_scale: Option<f64>,
 pub pre_phoneme_length: Option<f64>,
 pub post_phoneme_length: Option<f64>,
 pub output_sampling_rate: Option<u32>,
 pub audio_file_store_path: Option<String>,
 pub split_regex_pattern: Option<String>,
 pub processing_algorithm: Option<String>,

 // aivisspeech / voicevox（VOICEVOX 互換 HTTP。`api_url` はエンジン基底。`speaker_uuid` があるとき `style_id` はローカル番号(0〜)、ないときは GET /speakers のグローバル styles[].id。CoeiroInk と同じフィールドを流用）

 // BouyomiChan
 pub remote_talk_path: Option<String>,
 pub address: Option<String>,
 pub port: Option<u16>,
 pub voice: Option<i16>,
 pub speed: Option<i16>,
 pub tone: Option<i16>,
 pub volume: Option<i16>,

 // OsTTS
 pub voice_id: Option<String>,
 pub voice_name: Option<String>,
 pub tts_pitch: Option<f32>,
 pub tts_rate: Option<f32>,
 pub tts_volume: Option<f32>,

 /// 《WebInput》: HTTP のパス（`/` 始まり。省略時は `/input`）
 pub web_input_path: Option<String>,
 /// 《WebInput》: `get`（クエリ）または `post`（ボディ）。省略時は `post`。
 pub web_input_method: Option<String>,
 /// 《WebInput》: POST 時の `body_format`（`json` / `toml` / `msgpack`）。省略時は `json`。GET では無視。
 pub web_input_body_format: Option<String>,

 /// 《Twitch》: IRC ログイン名（`[[processors]] feature = "twitch"` のとき必須）
 pub twitch_username: Option<String>,
 /// 《Twitch》: 追加で join するチャンネル
 #[serde(default)]
 pub twitch_reads: Option<Vec<String>>,
 /// 《Twitch》 EventSub WebSocket。`feature = "twitch"` のとき、`twitch_username` / `channel_to` を既定の配信者・VAC チャンネルとして使う。
 #[serde(default)]
 pub twitch_eventsub: Option<super::TwitchEventSubConfig>,

 /// 《TwitchOut》: `feature = "twitch-out"` の送信先配信者 login（未指定時は `[twitch]` の `username` / `eventsub.broadcaster_login`）。
 pub twitch_out_broadcaster_login: Option<String>,
 /// 《TwitchOut》: 本文 UTF-16 前提の最大文字数クリップ（既定 500）。Twitch 制限内に収める。
 pub twitch_out_max_chars: Option<usize>,
 /// 《TwitchOut》: 送信前に除去する部分文字列（NG ワード等）。
 #[serde(default)]
 pub twitch_out_strip_substrings: Vec<String>,
 /// 《TwitchOut》: 30 秒あたりの送信回数ハードリミット（既定 20）。Twitch モデレーターで 100 まで緩和されるが保守的。
 pub twitch_out_rate_limit_per_30s: Option<u32>,

 /// 《Voice》: 認識エンジン。`vosk`（既定）または `whisper`（`voice-whisper` ビルド時）。
 pub voice_engine: Option<String>,
 /// 《Voice》: Vosk モデル。**展開済みディレクトリのパス**、または **alphacephei のモデル ID**（例: `vosk-model-small-ja-0.22`、初回に zip を取得してキャッシュへ展開）。`voice_engine` が vosk のとき必須。
 pub voice_vosk_model_path: Option<String>,
 /// 《Voice》: Whisper の言語コード（ISO 639-1）。例: `ja`, `en`。`ja-JP` のように渡した場合は先頭のみ使う。
 pub voice_language: Option<String>,
 /// 《Voice》: Whisper 用 GGML モデル。**ファイルパス**または **プリセット名**（`tiny` / `base` / …）。`voice_engine` が whisper のとき必須。
 pub voice_whisper_model_path: Option<String>,
 /// 《Voice》: まとめて文字起こしする秒数。未指定時は `5.0`。
 pub voice_chunk_seconds: Option<f32>,
 /// 《Voice》: 発話中にこのキーで speech floor を占有する（他プロセッサの `respect_speech_floor` と同じ文字列で対になる）
 pub speech_floor_key: Option<String>,
 /// 指定キーの speech floor が空くまで `process` 先頭で非同期待機する（スキップしない。解放は `Notify`）
 pub respect_speech_floor: Option<String>,
}

impl ProcessorConf {
 pub fn as_shared(&self) -> SharedProcessorConf {
  Arc::new(RwLock::new(self.clone()))
 }
}
