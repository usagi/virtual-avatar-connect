use super::Args;
use crate::Conf;
use anyhow::Result;

impl Args {
 /// conf が必要な特殊モード処理群の実行
 pub async fn execute_special_modes_with_conf(&self, conf: &Conf) -> Result<()> {
  if self.openai_chat_fine_tuning {
   if let Err(e) = crate::processor::OpenAiChat::fine_tuning(self.processor_id.as_ref(), conf).await {
    log::error!("OpenAI Chat の Fine-tune に失敗しました: {}", e);
    std::process::exit(1);
   }
   std::process::exit(0);
  }

  if self.openai_api_clear_files {
   if let Err(e) = crate::processor::OpenAiChat::delete_file_all(self.processor_id.as_ref(), conf).await {
    log::error!(
     "OpenAI API でアップロードされているすべてのファイルを削除する試みに失敗しました: {}",
     e
    );
    std::process::exit(1);
   }
   std::process::exit(0);
  }

  Ok(())
 }
}
