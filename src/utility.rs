use anyhow::Context;
use anyhow::Result;

/// ISO639 3文字言語コードを2文字言語コードに変換する
pub fn iso_639_lang_code_3_to_2(l3: &str) -> Result<String> {
 let l2 = isolang::Language::from_639_3(&l3).context("ISO639の3文字言語コードの識別に失敗しました。")?;
 let l2 = l2
  .to_639_1()
  .context("ISO639の3文字言語コードから2文字言語コードへの変換に失敗しました。")?;
 Ok(l2.to_string().to_lowercase())
}

pub fn iso_3166_to_lang_code(iso_3166: &str) -> Result<String> {
 let lang = iso_3166.replace('_', "-");
 let lang = lang.split('-').next().context("ISO3166から言語コードの抽出に失敗しました。")?;
 Ok(lang.to_lowercase())
}

pub fn bool_true() -> bool {
 true
}

/// 環境変数 env_var または or_else から値を読み込む、どちらにもなかったら None が返る
pub fn load_from_env_or_conf<A: AsRef<str>>(env_var: A, or_else: &Option<String>) -> Option<String> {
 match std::env::var(env_var.as_ref()) {
  Ok(s) => Some(s),
  Err(_) => or_else.clone(),
 }
}
