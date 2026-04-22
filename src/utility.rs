use anyhow::Context;
use anyhow::Result;

/// Vosk 日本語などが単語境界に入れる **ASCII 空白・和字間スペース** を、
/// 前後がいずれも日本語系文字（ひら・カタ・漢字・半角カナ）のときだけ詰める。
/// 英数字どうしのスペースや、日本語と英語の境はそのまま残す。
pub fn normalize_interstitial_japanese_spaces(s: &str) -> String {
 if !s.contains(' ') && !s.contains('\u{3000}') {
  return s.to_string();
 }
 let chars: Vec<char> = s.chars().collect();
 let mut out = String::with_capacity(s.len());
 let mut i = 0;
 while i < chars.len() {
  let c = chars[i];
  if (c == ' ' || c == '\u{3000}') && i > 0 {
   let mut j = i;
   while j < chars.len() && (chars[j] == ' ' || chars[j] == '\u{3000}') {
    j += 1;
   }
   if j < chars.len() && is_japanese_script_for_spacing(chars[i - 1]) && is_japanese_script_for_spacing(chars[j]) {
    i = j;
    continue;
   }
  }
  out.push(c);
  i += 1;
 }
 out
}

#[inline]
fn is_japanese_script_for_spacing(c: char) -> bool {
 matches!(
  c,
  '\u{3040}'..='\u{309F}' // Hiragana
   | '\u{30A0}'..='\u{30FF}' // Katakana
   | '\u{3400}'..='\u{9FFF}' // CJK …
   | '\u{FF66}'..='\u{FF9F}' // Halfwidth Katakana
 )
}

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

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn collapse_vosk_like_word_spaces() {
  let s = "実 は 今 誤 認識 に つい て";
  assert_eq!(normalize_interstitial_japanese_spaces(s), "実は今誤認識について");
 }

 #[test]
 fn keeps_space_between_latin_and_japanese() {
  let s = "hello 世界";
  assert_eq!(normalize_interstitial_japanese_spaces(s), "hello 世界");
 }
}
