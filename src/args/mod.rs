mod sm_with_conf;
mod sm_without_conf;

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

 /// AivisSpeech Engine の `/speakers` を表示します（既定 URL: `http://127.0.0.1:10101`）。エンジンを起動した状態で実行してください。
 #[arg(long)]
 pub aivisspeech_speakers: bool,

 /// VOICEVOX ENGINE の `/speakers` を表示します（既定 URL: `http://127.0.0.1:50021`）。エンジンを起動した状態で実行してください。
 #[arg(long)]
 pub voicevox_speakers: bool,

 /// OS-TTSのテストを実行します。使用可能なOS-TTSの一覧を確認する用途でも使用できます。
 #[arg(long)]
 pub test_os_tts: bool,

 /// AI ペルソナ（`[[ai.personas]]`）のファインチューニングを実行します。
 /// --persona-id で persona を指定でき、未指定時は設定ファイルで最初に定義されている AI persona が使用されます。
 #[arg(long)]
 pub openai_chat_fine_tuning: bool,

 /// 特定の AI persona（`[[ai.personas]]`）を指定したい場合に使用します。主に `--openai-chat-fine-tuning` と併用します。
 #[arg(long)]
 pub persona_id: Option<String>,

 /// OpenAI API でアップロードされている全てのファイルを削除します。
 /// 処理の失敗などで既に不要なファイルが API サービス側に残ってしまっている場合など、それらを削除したい場合に使用できます。
 #[arg(long)]
 pub openai_api_clear_files: bool,

 /// 実験的な機能を有効にします。主に開発用で、動作内容は何かと開発者の都合にあわせて変化します。
 #[arg(long)]
 pub experimental: bool,

 /// v1 `conf.toml` を best-effort で v2（Flowgraph 中心）に変換して書き出します。
 /// 既存ファイルは上書きせず、`--migrate-out-dir` 配下にすべて出力されます。
 #[arg(long)]
 pub migrate: bool,

 /// `--migrate` の出力先ディレクトリ（省略時は `./migrated/`）。
 #[arg(long, default_value = "migrated")]
 pub migrate_out_dir: String,

 /// `--migrate` で warning も error 扱いとし、1 件でも出たら exit 1 する。
 #[arg(long)]
 pub migrate_strict: bool,
}

// note: Args の impl 群はサブモジュールに分離されています。
impl Args {
 pub fn new() -> Args {
  log::info!("コマンドライン引数をパースします。");
  Args::parse()
 }
}
