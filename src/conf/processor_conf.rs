use crate::{utility::bool_true, Arc, RwLock};
use serde::{Deserialize, Serialize};

pub type SharedProcessorConf = Arc<RwLock<ProcessorConf>>;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ProcessorConf {
 // Common
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
