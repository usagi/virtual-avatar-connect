use super::super::ENV_OPENAI_API_KEY;
use super::types::{FineTuningRequest, NamedData};
use crate::ai::config::{AiPersonaConf, OpenAiChatFinetuning};
use crate::Conf;
use anyhow::{bail, Context, Result};
use async_openai::config::OpenAIConfig;
use async_openai::types::files::{CreateFileRequestArgs, FileInput, FilePurpose};
use async_openai::types::finetuning::{CreateFineTuningJobRequest, FineTuningJobStatus};
use async_openai::types::InputSource;
use async_openai::Client;

////////////////////////////////////////////////////////////////////////////////////////////////////
// conf

/// -> (api_key, finetune_conf, custom_instructions)
pub fn get(persona_id: Option<&String>, conf: &Conf) -> Result<(String, OpenAiChatFinetuning, Option<String>)> {
 let persona = get_persona_conf(persona_id, conf)?;
 let api_key = get_api_key(persona)?;
 let finetune_conf = get_finetune(persona)?;
 let custom_instructions = persona.custom_instructions.clone();
 Ok((api_key, finetune_conf, custom_instructions))
}

pub fn get_persona_conf<'a>(persona_id: Option<&'a String>, conf: &'a Conf) -> Result<&'a AiPersonaConf> {
 if let Some(pid) = persona_id {
  conf.ai.personas.iter().find(|&p| p.id.as_ref() == Some(pid))
 } else {
  conf.ai.personas.first()
 }
 .context(
  "有効な AI ペルソナの設定が見つかりませんでした。`[[ai.personas]]` が 1 件も定義されていないか、\
  明示的に --persona-id 引数を与えている場合は指定された id が検出されていない可能性があります。",
 )
}

pub fn get_api_key(persona: &AiPersonaConf) -> Result<String> {
 match crate::utility::load_from_env_or_conf(ENV_OPENAI_API_KEY, &persona.api_key) {
  Some(s) => Ok(s),
  None => {
   bail!("OpenAI の API KEY が設定されていません。環境変数 VAC_OPENAI_API_KEY を設定するか、設定ファイルに api_key を設定して下さい。")
  },
 }
}

fn get_finetune(persona: &AiPersonaConf) -> Result<OpenAiChatFinetuning> {
 persona
  .fine_tuning
  .clone()
  .context("AI ペルソナの設定に fine_tuning の設定が見つかりませんでした。")
}

pub fn openai_client(api_key: &str) -> Client<OpenAIConfig> {
 Client::with_config(OpenAIConfig::default().with_api_key(api_key.to_string()))
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// local file

impl NamedData {
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

pub async fn prepare_train_file<P: AsRef<str>>(path: P) -> Result<NamedData> {
 load_file(path).await
}

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
// OpenAI Files / Fine-tuning（async-openai）

async fn upload_file(client: &Client<OpenAIConfig>, file: NamedData) -> Result<String> {
 let req = CreateFileRequestArgs::default()
  .file(FileInput {
   source: InputSource::VecU8 {
    filename: file.name.clone(),
    vec: file.data.clone(),
   },
  })
  .purpose(FilePurpose::FineTune)
  .build()
  .map_err(|e| anyhow::anyhow!("ファイルアップロードリクエストの構築に失敗: {e}"))?;

 let res = client.files().create(req).await?;

 log::info!(
  "ファイルのアップロードが完了しました: id={:?} filename={:?} bytes={:?}",
  res.id,
  res.filename,
  res.bytes
 );

 Ok(res.id)
}

pub async fn upload_files(
 client: &Client<OpenAIConfig>,
 train_file: NamedData,
 validation_file: Option<NamedData>,
) -> Result<(String, Option<String>)> {
 let train_file_id = upload_file(client, train_file).await?;
 let validation_file_id = match validation_file {
  Some(validation_file) => Some(upload_file(client, validation_file).await?),
  None => None,
 };
 Ok((train_file_id, validation_file_id))
}

async fn file_list(client: &Client<OpenAIConfig>) -> Result<Vec<async_openai::types::files::OpenAIFile>> {
 Ok(client.files().list().await?.data)
}

pub async fn delete_file(client: &Client<OpenAIConfig>, file_id: &str) -> Result<()> {
 for n in 1..11 {
  match client.files().delete(file_id).await {
   Ok(res) => {
    log::info!("アップロードしたファイルの削除が完了しました: file_id={:?} deleted={}", res.id, res.deleted);
    return Ok(());
   },
   Err(e) => {
    log::warn!(
     "アップロードしたファイルの削除に失敗したため3秒後に再試行します。10回試行しても削除できない場合は諦めます({}/{}): file_id={:?} err={}",
     n,
     10,
     file_id,
     e
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
   },
  }
 }
 Ok(())
}

pub async fn delete_file_all(api_key: &str) -> Result<()> {
 let client = openai_client(api_key);
 let mut rs = vec![];
 for id in file_list(&client).await?.into_iter().map(|f| f.id) {
  rs.push(delete_file(&client, &id).await);
 }
 for r in rs {
  r?;
 }
 Ok(())
}

pub async fn delete_files(
 client: &Client<OpenAIConfig>,
 train_file_id: String,
 validation_file_id: Option<String>,
) -> Result<()> {
 let train_result = delete_file(client, &train_file_id).await;
 if let Some(validation_file_id) = validation_file_id {
  delete_file(client, &validation_file_id).await?;
 }
 train_result?;
 Ok(())
}

pub async fn create_fine_tuning_job(client: &Client<OpenAIConfig>, request: FineTuningRequest) -> Result<String> {
 let body: CreateFineTuningJobRequest = request.try_into_openai()?;
 log::debug!(
  "Fine-tuning リクエスト: {}",
  serde_json::to_string_pretty(&body).unwrap_or_else(|_| "<serialize error>".into())
 );

 let res = client
  .fine_tuning()
  .create(body)
  .await
  .map_err(|e| anyhow::anyhow!("Fine-tuning ジョブ作成に失敗: {e}"))?;

 log::info!("Fine-tuning を開始します: job_id={:?}", res.id);

 Ok(res.id)
}

pub async fn wait_for_fine_tuning(client: &Client<OpenAIConfig>, job_id: &str) -> Result<()> {
 let t = tokio::time::Instant::now();
 let fmt_elapsed = |d: std::time::Duration| -> String {
  let h = d.as_secs() / 3600;
  let m = (d.as_secs() - h * 3600) / 60;
  let s = d.as_secs() - h * 3600 - m * 60;
  format!("{:02}:{:02}:{:02}", h, m, s)
 };

 loop {
  let res = client
   .fine_tuning()
   .retrieve(job_id)
   .await
   .map_err(|e| anyhow::anyhow!("Fine-tuning ジョブ取得に失敗: {e}"))?;

  match res.status {
   FineTuningJobStatus::ValidatingFiles => {
    log::info!(
     "Fine-tuning はファイルを検証中です。開始までしばらくお待ち下さい({}経過): job_id={:?}",
     fmt_elapsed(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   FineTuningJobStatus::Queued => {
    log::info!(
     "Fine-tuning はキューイングされています。開始までしばらくお待ち下さい({}経過): job_id={:?}",
     fmt_elapsed(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   FineTuningJobStatus::Running => {
    log::info!(
     "Fine-tuning は現在処理中です。完了までしばらくお待ち下さい({}経過): job_id={:?}",
     fmt_elapsed(t.elapsed()),
     job_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
   },
   FineTuningJobStatus::Succeeded => {
    log::info!(
     "Fine-tuning が完了しました({}経過): fine_tuned_model={:?} trained_tokens={:?}",
     fmt_elapsed(t.elapsed()),
     res.fine_tuned_model,
     res.trained_tokens
    );
    return Ok(());
   },
   FineTuningJobStatus::Failed => {
    let m = format!("Fine-tuning は失敗しました({}経過): job_id={:?}", fmt_elapsed(t.elapsed()), job_id);
    if let Some(detail) = res.error {
     log::error!("{} code={} message={}", m, detail.code, detail.message);
     if let Some(param) = detail.param {
      log::error!("{} param={:?}", m, param);
     }
    } else {
     log::error!("{}", m);
    }
    bail!("{}", m);
   },
   FineTuningJobStatus::Cancelled => {
    let m = format!("Fine-tuning はキャンセルされました({}経過): job_id={:?}", fmt_elapsed(t.elapsed()), job_id);
    log::error!("{}", m);
    bail!("{}", m);
   },
  }
 }
}
