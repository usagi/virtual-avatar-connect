mod types;
mod utility;

use types::*;

use crate::Conf;
use anyhow::Result;

const DEFAULT_BASE_MODEL: &str = "gpt-3.5-turbo";

pub async fn fine_tuning(persona_id: Option<&String>, conf: &Conf) -> Result<()> {
 log::trace!("AI ペルソナの Fine-tune を開始します。");

 let (api_key, finetune_conf, custom_instructions) = utility::get(persona_id, conf)?;
 let client = utility::openai_client(&api_key);
 let (train_path, validation_path, model, suffix) = finetune_conf.to_tuple_for_input();

 let train_file = utility::prepare_train_file(train_path).await?.to_jsonl(custom_instructions.clone()).await?;

 let validation_file = match utility::prepare_validation_file(validation_path).await? {
  Some(validation_file) => Some(validation_file.to_jsonl(custom_instructions).await?),
  None => None,
 };

 let (training_file_id, validation_file_id) = utility::upload_files(&client, train_file, validation_file).await?;
 log::trace!("upload files was succeeded.");

 let job_id = match utility::create_fine_tuning_job(
  &client,
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
   log::error!("AI ペルソナの Fine-tune に失敗しました: {}", e);
   utility::delete_files(&client, training_file_id, validation_file_id).await?;
   std::process::exit(1);
  },
 };

 let wait_result = utility::wait_for_fine_tuning(&client, &job_id).await;
 let delete_result = utility::delete_files(&client, training_file_id, validation_file_id).await;

 wait_result?;
 delete_result?;

 log::info!(
  "AI ペルソナの Fine-tune が完了しました。 OpenAI Playground で確認されることをお勧めします: https://platform.openai.com/playground"
 );
 Ok(())
}

pub async fn delete_file_all(persona_id: Option<&String>, conf: &Conf) -> Result<()> {
 // API Key さえ取れれば ペルソナ固有の設定は不要
 let api_key = if let Ok(p) = utility::get_persona_conf(persona_id, conf) {
  utility::get_api_key(p)?
 } else {
  // 設定にペルソナが無くても、環境変数で渡していれば動かす余地を残す
  match crate::utility::load_from_env_or_conf(super::ENV_OPENAI_API_KEY, &None) {
   Some(s) => s,
   None => {
    anyhow::bail!(
     "AI ペルソナの設定も環境変数 VAC_OPENAI_API_KEY も見つからないため、OpenAI ファイルの全削除ができません。"
    )
   },
  }
 };
 utility::delete_file_all(&api_key).await?;
 Ok(())
}
