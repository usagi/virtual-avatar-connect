mod types;
mod utility;

use types::*;

use super::OpenAiChat;
use crate::Conf;
use anyhow::Result;

const DEFAULT_BASE_MODEL: &str = "gpt-3.5-turbo";

impl OpenAiChat {
 /// note: async-openai は現在の OpenAI API に対応していないため使用できない
 /// ref: https://github.com/64bit/async-openai/issues/119
 pub async fn fine_tuning(processor_id: Option<&String>, conf: &Conf) -> Result<()> {
  log::trace!("OpenAI Chat の Fine-tune を開始します。");

  let (api_key, finetune_conf, custom_instructions) = utility::get(processor_id, conf)?;
  let (train_path, validation_path, model, suffix) = finetune_conf.to_tuple_for_input();

  // train_file preparing
  let train_file = utility::prepare_train_file(train_path).await?.to_jsonl(custom_instructions.clone()).await?;

  // validation_file preparing
  let validation_file = match utility::prepare_validation_file(validation_path).await? {
   Some(validation_file) => Some(validation_file.to_jsonl(custom_instructions).await?),
   None => None,
  };

  // upload files
  let (training_file_id, validation_file_id) = utility::upload_files(&api_key, train_file, validation_file).await?;
  log::trace!("upload files was succeeded.");

  // create fine-tune job
  let job_id = match utility::fine_tuning(
   &api_key,
   FineTuningRequest {
    model: model.unwrap_or_else(|| DEFAULT_BASE_MODEL.to_string()),
    training_file: training_file_id.clone(),
    suffix,
    validation_file: validation_file_id.clone(),
    ..Default::default()
   },
  )
  .await
  {
   Ok(job_id) => job_id,
   Err(e) => {
    log::error!("OpenAI Chat の Fine-tune に失敗しました: {}", e);
    utility::delete_files(&api_key, training_file_id, validation_file_id).await?;
    std::process::exit(1);
   },
  };

  // wait for fine-tune job
  // どうあれファイルは削除したいので unwrap を遅延
  let wait_result = utility::wait_for_fine_tuning(&api_key, &job_id).await;
  // 先に wait_result を unwrap したいのでさらに遅延
  let delete_result = utility::delete_files(&api_key, training_file_id, validation_file_id).await;

  wait_result?;
  delete_result?;

  log::info!(
   "OpenAI Chat の Fine-tune が完了しました。 OpenAI Playground で確認されることをお勧めします: https://platform.openai.com/playground"
  );
  Ok(())
 }

 pub async fn delete_file_all(processor_id: Option<&String>, conf: &Conf) -> Result<()> {
  // この機能は ProcessorConf は必須ではないので、取得できなくてもエラーにしない
  let processor_conf = match utility::get_processor_conf(processor_id, conf) {
   Ok(processor_conf) => processor_conf.clone(),
   _ => crate::ProcessorConf::default(),
  };
  // API Key さえ取れれば ProcessorConf は不要
  let api_key = utility::get_api_key(&processor_conf)?;
  utility::delete_file_all(&api_key).await?;
  Ok(())
 }
}
