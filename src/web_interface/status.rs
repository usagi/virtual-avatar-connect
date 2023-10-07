use crate::{resource::CONTENT_TYPE_TEXT_HTML, Result};
use actix_web::{get, HttpResponse, Responder};

const CONTENT_HEAD: &str = r#"<!DOCTYPE html>
<meta charset="utf-8">
<title>VAC/status</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Kiwi+Maru:wght@500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="/resources/css/common/dark-mode.css">
"#;

#[get("/status")]
async fn get() -> Result<impl Responder> {
 log::trace!("/status");

 let mut content = CONTENT_HEAD.to_string();

 {
  content.push_str("<h1>Virtual Avatar Connect</h1>\n<ul>\n");
  let vac_version = env!("CARGO_PKG_VERSION");
  content.push_str(&format!("<li>VAC Version: {}</li>\n", vac_version));
  // 実行環境のOS情報を取得
  let os_info = os_info::get();
  content.push_str(&format!("<li>OS: {}</li>\n", os_info));
  content.push_str("</ul>\n<hr>\n");
 }

 match make_os_tts_section().await {
  Ok(section) => content.push_str(&section),
  Err(e) => {
   log::error!("OS-TTS の情報を取得できませんでした。: {}", e);
   content.push_str("<section><h2>OS-TTS</h2><p>OS-TTS の情報を取得できませんでした。</p></section>");
  },
 }

 match make_coeiroink_section().await {
  Ok(section) => content.push_str(&section),
  Err(e) => {
   log::error!("CoeiroInk の情報を取得できませんでした。: {}", e);
   content.push_str("<section><h2>OS-TTS</h2><p>CoriroInk の情報を取得できませんでした。</p></section>");
  },
 }

 Ok(HttpResponse::Ok().content_type(CONTENT_TYPE_TEXT_HTML).body(content))
}

fn make_section(title: &str, content: &str) -> String {
 let mut section = String::new();
 section.push_str("<section>\n");
 section.push_str(&format!("<h2>{}</h2>\n", title));
 section.push_str(content);
 section.push_str("</section>\n<hr>\n");
 section
}

fn make_tr_td(vs: Vec<String>) -> String {
 let mut tr = String::new();
 tr.push_str("<tr>");
 for v in vs {
  tr.push_str(&format!("<td>{}</td>", v));
 }
 tr.push_str("</tr>");
 tr
}

fn make_tr_th(vs: Vec<String>) -> String {
 let mut tr = String::new();
 tr.push_str("<tr>");
 for v in vs {
  tr.push_str(&format!("<th style=\"padding-right: 1em; text-align: left\">{}</th>", v));
 }
 tr.push_str("</tr>");
 tr
}

async fn make_os_tts_section() -> Result<String> {
 let mut section_content = "".to_string();

 let tts = tts::Tts::default()?;
 let voices = tts.voices()?;
 let trs = voices
  .into_iter()
  .map(|v| {
   let lang = v.language().to_string();
   let name = v.name().to_string();
   let id = v.id().to_string();
   make_tr_td(vec![lang, name, id])
  })
  .collect::<Vec<_>>()
  .join("\n");

 section_content.push_str("<table>\n");
 section_content.push_str(make_tr_th(vec!["Lang".to_string(), "Name".to_string(), "ID".to_string()]).as_str());
 section_content.push_str(trs.as_str());
 section_content.push_str("</table>\n");

 Ok(make_section("《OS-TTS》", section_content.as_str()))
}

async fn make_coeiroink_section() -> Result<String> {
 let mut section_content = "".to_string();

 let mut trs = vec![];
 let speakers = crate::processor::CoeiroInk::get_speakers().await?;
 for speaker in speakers {
  let img = if let Some(v) = speaker.base64Portrait {
   format!("<img src=\"data:image/png;base64,{}\" style=\"max-height:8em\">", v)
  } else {
   "".to_string()
  };
  let name = speaker.speakerName;
  let uuid = speaker.speakerUuid;
  for style in speaker.styles {
   let style_name = style.styleName;
   let style_id = style.styleId.to_string();
   let text = format!(
    "バーチャルアバターコネクトからこんにちは！コエイロインク{}の{}スタイルのテストです。",
    &name, &style_name
   )
   .to_string();
   let play_button = format!(
    "<div style=\"width: 10em\"><button onclick=\"test_coeiroink(this, '{}',{},'{}')\">Play</button></div>",
    uuid, style_id, text
   );
   trs.push(make_tr_td(vec![
    img.clone(),
    name.clone(),
    uuid.clone(),
    style_name,
    style_id,
    play_button,
   ]));
  }
 }

 section_content.push_str(
  r#"<script>
async function test_coeiroink(element, uuid, style_id, text) {
 try
 {
  element.innerText = 'Loading...'
  element.disabled = true
  let method = 'POST'
  let headers = { 'Content-Type': 'application/json' }
  let body = JSON.stringify({ speakerUuid: uuid, styleId: style_id, text, speedScale: 1 })
  let url = 'http://127.0.0.1:50032/v1/predict'
  let wav = await fetch(url, { method, headers, body })
  let data = await wav.arrayBuffer()
  let ac = new AudioContext()
  ac.decodeAudioData(data, buffer => {
   let source = ac.createBufferSource()
   source.buffer = buffer
   source.connect(ac.destination)
   source.onended = () => {
    element.innerText = 'Play'
    element.disabled = false
   }
   source.start()
  })
 }
 catch (e)
 {
  console.error(e)
  element.innerText = 'Play'
  element.disabled = false
 }
}
</script>"#,
 );
 section_content.push_str("<table>\n");
 const THS: [&str; 6] = ["Portrait", "Name", "UUID", "Style Name", "Style ID", "Test"];
 section_content.push_str(make_tr_th(THS.iter().map(|s| s.to_string()).collect()).as_str());
 section_content.push_str(trs.join("\n").as_str());
 section_content.push_str("</table>\n");

 Ok(make_section("《CoeiroInk》", section_content.as_str()))
}
