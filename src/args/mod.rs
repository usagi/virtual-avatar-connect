mod sm_with_conf;
mod sm_without_conf;

pub use sm_with_conf::*;
pub use sm_without_conf::*;

use clap::Parser;

const DEFAULT_CONF_PATH: &str = "conf.toml";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
 /// 個別の設定ファイルのパスを指定することもできます。
 #[arg(name = "CONF", default_value = DEFAULT_CONF_PATH)]
 pub conf: String,

 /// デバッグモードを有効にします。ログの出力レベルが Trace に設定されログが滝のように流れ出します。
 #[arg(short = 'D', long)]
 pub debug: bool,

 /// CoeiroInk の Speakers を表示します。使用可能な Speakers の一覧を確認する用途で使用できます。
 #[arg(long)]
 pub coeiroink_speakers: bool,

 /// OS-TTSのテストを実行します。使用可能なOS-TTSの一覧を確認する用途でも使用できます。
 #[arg(long)]
 pub test_os_tts: bool,

 /// OpenAI-Chat のファインチューニングを実行します。
 /// --processor-id でプロセッサーIDを指定できます。
 /// プロセッサーIDを指定しない場合は設定ファイルで最初に定義されている OpenAI-Chat プロセッサーの設定が使用されます。
 #[arg(long)]
 pub openai_chat_fine_tuning: bool,

 /// この引数は他の引数と併用する引数です。(--openai-chat-finetune など。)
 /// 特定のプロセッサーを指定したい場合に使用できます。
 #[arg(long)]
 pub processor_id: Option<String>,

 /// OpenAI API でアップロードされている全てのファイルを削除します。
 /// 処理の失敗などで既に不要なファイルが API サービス側に残ってしまっている場合など、それらを削除したい場合に使用できます。
 #[arg(long)]
 pub openai_api_clear_files: bool,

 /// 実験的な機能を有効にします。主に開発用で、動作内容は何かと開発者の都合にあわせて変化します。
 #[arg(long)]
 pub experimental: bool,
}

// note: Args の impl 群はサブモジュールに分離されています。
impl Args {
 pub fn new() -> Args {
  log::info!("コマンドライン引数をパースします。");
  Args::parse()
 }
}
