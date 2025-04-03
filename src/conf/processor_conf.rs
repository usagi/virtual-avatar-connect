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

/// ファインチューニング関連の設定オプションです。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum OpenAiChatFinetuning {
 /// Detail で path だけ設定した場合と同じ扱いになります。
 Path(String),

 /// すべてのオプションを設定可能な構造体タイプの設定です。
 Detail {
  /// message を入出力するファイルパスを指定します。(.jsonl, .csv)
  train_path: String,
  /// ファインチューニングに対して検証を行う場合は検証ファイルのパスを指定します。(.jsonl, .csv)
  validation_path: Option<String>,
  /// ベースモデルを指定します。未指定の場合は "gpt-3.5-turbo" が使用されます。
  model: Option<String>,
  /// ファインチューニングの結果生成されるモデル名を指定します。
  /// 未指定の場合はファイルパスの stem が使用されます。
  suffix: Option<String>,
 },
}

impl OpenAiChatFinetuning {
 /// (train_path, validation_path, model, suffix)
 pub fn to_tuple_for_input(&self) -> (String, Option<String>, Option<String>, Option<String>) {
  match self {
   OpenAiChatFinetuning::Path(train_path) => (train_path.clone(), None, None, None),
   OpenAiChatFinetuning::Detail {
    train_path,
    validation_path,
    model,
    suffix,
    ..
   } => (train_path.clone(), validation_path.clone(), model.clone(), suffix.clone()),
  }
 }

 /// (train_path, max_lines)
 pub fn train_path(&self) -> String {
  match self {
   OpenAiChatFinetuning::Path(train_path) => train_path.clone(),
   OpenAiChatFinetuning::Detail { train_path, .. } => train_path.clone(),
  }
 }
}

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

 // OpenAI Chat
 pub api_key: Option<String>,
 pub model: Option<String>,
 pub custom_instructions: Option<String>,
 pub max_tokens: Option<u16>,
 pub temperature: Option<f32>,
 pub top_p: Option<f32>,
 pub n: Option<u8>,
 pub presence_penalty: Option<f32>,
 pub frequency_penalty: Option<f32>,
 pub user: Option<String>,
 pub memory_capacity: Option<usize>,
 pub force_activate_regex_pattern: Option<String>,
 pub ignore_regex_pattern: Option<String>,
 pub min_interval_in_secs: Option<u64>,
 pub remove_chars: Option<String>,
 pub fine_tuning: Option<OpenAiChatFinetuning>,

 // gas-translation
 pub script_id: Option<String>,
 pub translate_from: Option<String>,
 pub translate_to: Option<String>,
 pub process_incomplete_input: Option<bool>,

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
}

impl ProcessorConf {
 pub fn as_shared(&self) -> SharedProcessorConf {
  Arc::new(RwLock::new(self.clone()))
 }
}
