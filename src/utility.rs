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
