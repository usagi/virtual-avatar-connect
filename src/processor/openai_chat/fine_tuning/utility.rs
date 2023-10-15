use super::super::ENV_OPENAI_API_KEY;
use super::types::{ApiError, FileDeletionStatus, FileObject, FineTuningJobObject, FineTuningRequest, ListFilesResponse, NamedData};
use crate::conf::{Conf, OpenAiChatFinetuning, ProcessorConf};
use crate::{OpenAiChat, Processor};
use anyhow::{bail, Context, Result};

////////////////////////////////////////////////////////////////////////////////////////////////////

const HEADER_API_KEY_KEY: &str = "Authorization";
const HEADER_API_KEY_VALUE_PREFIX: &str = "Bearer ";

////////////////////////////////////////////////////////////////////////////////////////////////////
// conf

/// -> (api_key, finetune_conf, custom_instructions)
pub fn get(processor_id: Option<&String>, conf: &Conf) -> Result<(String, OpenAiChatFinetuning, Option<String>)> {
 let processor_conf = get_processor_conf(processor_id, conf)?;
 let api_key = get_api_key(processor_conf)?;
 let finetune_conf = get_finetune(processor_conf)?;
 let custom_instructions = processor_conf.custom_instructions.clone();
 Ok((api_key, finetune_conf, custom_instructions))
}

// ProcessorConf を取得
// processor_id が指定されている場合はその ID の ProcessorConf を取得
// processor_id が指定されていない場合は設定ファイルで最初に定義されている OpenAI-Chat プロセッサーの設定を使用
pub fn get_processor_conf<'a>(processor_id: Option<&'a String>, conf: &'a Conf) -> Result<&'a ProcessorConf> {
 if let Some(pid) = processor_id {
   conf
    .processors
    .iter()
    .find(|&pc| pc.id.as_ref() == Some(pid) && pc.feature.as_ref().map(|v| v.to_lowercase()) == Some(OpenAiChat::FEATURE.to_string()))
  } else {
   conf
    .processors
    .iter()
    .find(|&pc| pc.feature.as_ref().map(|v| v.to_lowercase()) == Some(OpenAiChat::FEATURE.to_string()))
  }
  .context("有効な《OpenAI-Chat》プロセッサーの設定が見つかりませんでした。読み込まれたVAC設定ファイルでは《OpenAI-Chat》プロセッサーの定義が検出されないか、明示的に --processor-id 引数を与えている場合は指定されたプロセッサーIDが検出されていない可能性があります。")
}

pub fn get_api_key(processor_conf: &ProcessorConf) -> Result<String> {
 match crate::utility::load_from_env_or_conf(ENV_OPENAI_API_KEY, &processor_conf.api_key) {
  Some(s) => Ok(s),
  None => {
   bail!("OpenAI の API KEY が設定されていません。環境変数 VAC_OPENAI_API_KEY を設定するか、設定ファイルに api_key を設定して下さい。")
  },
 }
}

fn get_finetune(processor_conf: &ProcessorConf) -> Result<OpenAiChatFinetuning> {
 processor_conf
  .fine_tuning
  .clone()
  .context("《OpenAI-Chat》プロセッサーの設定に finetune の設定が見つかりませんでした。")
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// local file

impl NamedData {
 /// .csv -> .jsonl (pcのcustom_instructionを考慮しつつ.jsonlに変換)
 /// .jsonl -> .jsonl (そのまま)
 pub async fn to_jsonl(self, custom_instructions: Option<String>) -> Result<Self> {
  if self.name.to_lowercase().ends_with(".csv") {
   use futures::stream::StreamExt;
   use serde::Serialize;

   #[derive(Serialize)]
   struct Line {
    messages: Vec<Message>,
   }
   #[derive(Serialize)]
   struct Message {
    role: String,
    content: String,
   }

   let new_name = format!("{}.jsonl", &self.name);

   let reader = csv_async::AsyncReaderBuilder::new()
    .has_headers(true)
    .create_reader(self.data.as_slice());

   let mut new_data = String::new();
   let mut records = reader.into_records();
   while let Some(r) = records.next().await {
    let record = r?;
    if record.len() != 2 {
     bail!("CSV ファイルのフォーマットが不正です。ヘッダーが1行かつカラムが2列で user, assistant の content を記録したCSVファイルを指定して下さい。");
    }
    let mut messages = vec![];
    if let Some(custom_instructions) = custom_instructions.as_ref() {
     messages.push(Message {
      role: "system".to_string(),
      content: custom_instructions.clone(),
     });
    }
    messages.push(Message {
     role: "user".to_string(),
     content: record[0].to_string(),
    });
    messages.push(Message {
     role: "assistant".to_string(),
     content: record[1].to_string(),
    });
    let line = Line { messages };
    let line = serde_json::to_string(&line)?.replace('\n', "");
    new_data.push_str(&format!("{}\n", line));
   }

   println!("\n\n{}\n\n", &new_data);

   Ok(NamedData {
    name: new_name,
    data: new_data.into_bytes(),
   })
  } else {
   Ok(self)
  }
 }
}

async fn load_file<P: AsRef<str>>(path: P) -> Result<NamedData> {
 let path_buf = std::path::PathBuf::from(path.as_ref());
 let name = path_buf
  .file_name()
  .with_context(|| format!("ファイル名の取得に失敗しました: path={:?}", path.as_ref()))?
  .to_str()
  .with_context(|| format!("ファイル名の文字列処理に失敗しました: path={:?}", path.as_ref()))?
  .to_string();
 let data = tokio::fs::read(path_buf).await?;
 Ok(NamedData { name, data })
}

/// -> (payload, file_name)
pub async fn prepare_train_file<P: AsRef<str>>(path: P) -> Result<NamedData> {
 load_file(path).await
}

/// -> (payload, file_name)
pub async fn prepare_validation_file<P: AsRef<str>>(path: Option<P>) -> Result<Option<NamedData>> {
 match path {
  Some(path) => {
   let named_data = load_file(path).await?;
   Ok(Some(named_data))
  },
  None => Ok(None),
 }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// server file

// -> file_id
async fn upload_file(api_key: &str, file: NamedData) -> Result<String> {
 const URL: &str = "https://api.openai.com/v1/files";
 const PURPOSE: &str = "fine-tune";

 let client = reqwest::Client::new();
 let form = reqwest::multipart::Form::new().text("purpose", PURPOSE).part(
  "file",
  reqwest::multipart::Part::bytes(file.data.clone()).file_name(file.name.clone()),
 );

 let res = client
  .post(URL)
  .header(HEADER_API_KEY_KEY, format!("{}{}", HEADER_API_KEY_VALUE_PREFIX, api_key))
  .multipart(form)
  .send()
  .await?
  .json::<FileObject>()
  .await?;

 log::info!(
  "ファイルのアップロードが完了しました: id={:?} filename={:?} bytes={:?}",
  res.id,
  res.filename,
  res.bytes
 );

 Ok(res.id)
}

// -> (train_file_id, Option<validation_file_id>)
pub async fn upload_files(api_key: &str, train_file: NamedData, validation_file: Option<NamedData>) -> Result<(String, Option<String>)> {
 let train_file_id = upload_file(api_key, train_file).await?;
 let validation_file_id = match validation_file {
  Some(validation_file) => Some(upload_file(api_key, validation_file).await?),
  None => None,
 };
 Ok((train_file_id, validation_file_id))
}

pub async fn file_list(api_key: &str) -> Result<Vec<FileObject>> {
 const URL: &str = "https://api.openai.com/v1/files";

 let client = reqwest::Client::new();

 let res = client
  .get(URL)
  .header(HEADER_API_KEY_KEY, format!("{}{}", HEADER_API_KEY_VALUE_PREFIX, api_key))
  .send()
  .await?
  .json::<ListFilesResponse>()
  .await?;

 Ok(res.data)
}

pub async fn delete_file(api_key: &str, file_id: &str) -> Result<()> {
 const URL_PREFIX: &str = "https://api.openai.com/v1/files/";

 let url = format!("{}{}", URL_PREFIX, file_id);
 let header_api_key_value = format!("{}{}", HEADER_API_KEY_VALUE_PREFIX, api_key);

 // 10回リトライ、3秒おき
 for n in 1..11 {
  let res = reqwest::Client::new()
   .delete(&url)
   .header(HEADER_API_KEY_KEY, &header_api_key_value)
   .send()
   .await?;

  if res.status().is_success() {
   let res = res.json::<FileDeletionStatus>().await?;
   log::info!("アップロードしたファイルの削除が完了しました: file_id={:?}", res.id,);
   break;
  }

  log::warn!(
   "アップロードしたファイルの削除に失敗したため3秒後に再試行します。10回試行しても削除できない場合は諦めます({}/{}): file_id={:?}",
   n,
   10,
   file_id
  );
  tokio::time::sleep(std::time::Duration::from_secs(3)).await;
 }

 Ok(())
}

pub async fn delete_file_all(api_key: &str) -> Result<()> {
 let mut rs = vec![];
 // すべて削除を呼ぶため unwrap は遅延する
 for id in file_list(api_key).await?.into_iter().map(|f| f.id) {
  rs.push(delete_file(api_key, &id).await);
 }

 for r in rs {
  r?;
 }

 Ok(())
}

pub async fn delete_files(api_key: &str, train_file_id: String, validation_file_id: Option<String>) -> Result<()> {
 // 両方削除処理はしたいので unwrap は遅延する
 let train_result = delete_file(api_key, &train_file_id).await;
 if let Some(validation_file_id) = validation_file_id {
  delete_file(api_key, &validation_file_id).await?;
 }
 train_result?;
 Ok(())
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// fine-tune

// -> job_id
pub async fn fine_tuning(api_key: &str, fine_tuning_request: FineTuningRequest) -> Result<String> {
 const URL: &str = "https://api.openai.com/v1/fine_tuning/jobs";
 const HEADER_CONTENT_TYPE_KEY: &str = "Content-Type";
 const HEADER_CONTENT_TYPE_VALUE: &str = "application/json";

 let client = reqwest::Client::new();

 println!("{:?}", serde_json::to_string_pretty(&fine_tuning_request).unwrap());

 let res = client
  .post(URL)
  .header(HEADER_API_KEY_KEY, format!("{}{}", HEADER_API_KEY_VALUE_PREFIX, api_key))
  .header(HEADER_CONTENT_TYPE_KEY, HEADER_CONTENT_TYPE_VALUE)
  .json(&fine_tuning_request)
  .send()
  .await?;

 let status_code = res.status();
 if !status_code.is_success() {
  // エラーの追加情報を取得
  let mut error_detail_string = format!(
   "Fine-tuning のリクエストに失敗しました: status_code={:?}({:?})",
   status_code.as_u16(),
   status_code.as_str()
  );
  if let Ok(api_error) = res.json::<ApiError>().await {
   if let Some(detail) = api_error.error {
    if let Some(message) = detail.message {
     error_detail_string.push_str(&format!(" message={:?} ", message));
    }
    if let Some(r#type) = detail.r#type {
     error_detail_string.push_str(&format!(" type={:?} ", r#type));
    }
    if let Some(param) = detail.param {
     error_detail_string.push_str(&format!(" param={:?} ", param));
    }
    if let Some(code) = detail.code {
     error_detail_string.push_str(&format!(" code={:?} ", code));
    }
   }
  };
  log::error!("{}", &error_detail_string);
  bail!("{}", error_detail_string);
 }

 let res = res.json::<FineTuningJobObject>().await?;

 log::info!("Fine-tuning を開始します: job_id={:?}", res.id);

 Ok(res.id)
}

pub async fn wait_for_fine_tuning(api_key: &str, job_id: &str) -> Result<()> {
 const URL_PREFIX: &str = "https://api.openai.com/v1/fine_tuning/jobs/";

 let url = format!("{}{}", URL_PREFIX, job_id);
 let header_api_key_value = format!("{}{}", HEADER_API_KEY_VALUE_PREFIX, api_key);

 let last_res;

 let t = tokio::time::Instant::now();
 let f = |t: std::time::Duration| -> String {
  let h = t.as_secs() / 3600;
  let m = (t.as_secs() - h * 3600) / 60;
  let s = t.as_secs() - h * 3600 - m * 60;
  format!("{:02}:{:02}:{:02}", h, m, s)
 };

 loop {
  let res = reqwest::Client::new()
   .clone()
   .get(&url)
   .header(HEADER_API_KEY_KEY, &header_api_key_value)
   .send()
   .await?
   .json::<FineTuningJobObject>()
   .await?;

  match res.status.as_ref() {
   "validating_files" => {
    log::info!(
     "Fine-tuning はファイルを検証中です。開始までしばらくお待ち下さい({}経過): job_id={:?}",
     f(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   "queued" => {
    log::info!(
     "Fine-tuning はキューイングされています。開始までしばらくお待ち下さい({}経過): job_id={:?}",
     f(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   "running" => {
    log::info!(
     "Fine-tuning は現在処理中です。完了までしばらくお待ち下さい({}経過): job_id={:?}",
     f(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   "succeeded" => {
    last_res = Some(res);
    break;
   },
   "failed" => {
    let m = format!("Fine-tuning は失敗しました({}経過): job_id={:?}", f(t.elapsed()), job_id);
    if let Some(detail) = res.error {
     if let Some(message) = detail.message {
      log::error!("{} message={:?}", m, message);
     }
     if let Some(r#type) = detail.r#type {
      log::error!("{} type={:?}", m, r#type);
     }
     if let Some(param) = detail.param {
      log::error!("{} param={:?}", m, param);
     }
     if let Some(code) = detail.code {
      log::error!("{} code={:?}", m, code);
     }
    };
    log::error!("{}", m);
    bail!("{}", m);
   },
   "canceled" => {
    let m = format!("Fine-tuning はキャンセルされました({}経過): job_id={:?}", f(t.elapsed()), job_id);
    log::error!("{}", m);
    bail!("{}", m);
   },
   unknown_status => {
    let m = format!(
     "Fine-tuning から不明なステータが返されました({}経過): job_id={:?} status={:?}",
     f(t.elapsed()),
     job_id,
     unknown_status
    );
    log::error!("{}", m);
    bail!("{}", m);
   },
  }
 }

 let last_res = last_res.context("Fine-tuning の結果が取得できませんでした。")?;

 log::info!(
  "Fine-tuning が完了しました({}経過): fine_tuned_model={:?} trained_tokens={:?}",
  f(t.elapsed()),
  last_res.fine_tuned_model,
  last_res.trained_tokens
 );

 Ok(())
}
