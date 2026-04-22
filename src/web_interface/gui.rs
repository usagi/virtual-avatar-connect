//! Phase VI-β-9: `gui/dist` を `/gui/` 配下で配信する。
//!
//! 方針:
//!   - Vite の `base: '/gui/'` 設定に合わせ、actix-files で `/gui` にマウント。
//!   - `index.html` が無いとき（未ビルド時）でも VAC が起動できるよう、ディレクトリ存在を起動時に検査。
//!     無ければ `/gui*` 全体を掴むフォールバックハンドラで親切な HTML を返す。
//!   - SPA のルーティングは現状不要（ルーターを入れていない）ので `index_file("index.html")` は
//!     ルート 1 本だけの挙動でよい。将来 SPA にする場合はフォールバックをそちらに委譲する。

use actix_web::{web, HttpResponse, Responder};
use std::path::PathBuf;

/// actix-web `App` に `/gui/*` を登録する。
///
/// `dist_path` が `Some` かつ `<dist_path>/index.html` が存在すれば静的配信を有効化。
/// それ以外は「未ビルド」案内を返すフォールバックを `/gui` と `/gui/{tail:.*}` に登録する。
pub fn register(cfg: &mut web::ServiceConfig, dist_path: Option<&str>) {
 let Some(dist) = dist_path else {
  log::info!("《GUI》 gui_dist_path が null のため /gui/* は未ビルド案内のみ提供します。");
  register_not_built(cfg, None);
  return;
 };
 let dist_pb = PathBuf::from(dist);
 let index_html = dist_pb.join("index.html");
 if !index_html.is_file() {
  log::warn!(
   "《GUI》 {} が見つかりません。`cd gui && npm install && npm run build` を実行してください。/gui/* はビルド案内を返します。",
   index_html.display()
  );
  register_not_built(cfg, Some(dist_pb));
  return;
 }

 log::info!("《GUI》 {} を /gui/ で配信します。", dist_pb.display());
 cfg.service(
  actix_files::Files::new("/gui", dist_pb)
   .index_file("index.html")
   // dotfiles（.env.* 等）が紛れ込んでいても露出させない。
   .use_hidden_files(),
 );
}

/// 未ビルド時のフォールバック。`/gui`・`/gui/`・`/gui/<任意のパス>` の全てを拾う。
///
/// actix-web の動的パスは `/gui/{tail:.*}` だと **末尾スラッシュ単独** (`/gui/`) が
/// tail="" で一致しないケースがあるため、`/gui{tail:.*}` の形で `/gui` 側から貪欲にマッチさせる。
fn register_not_built(cfg: &mut web::ServiceConfig, dist_pb: Option<PathBuf>) {
 let shown_path = dist_pb
  .map(|p| p.display().to_string())
  .unwrap_or_else(|| "(none; gui_dist_path is null)".to_string());
 log::info!(
  "《GUI》 /gui/* は未ビルド案内 (dist_path={}) を返します。",
  shown_path
 );
 cfg.app_data(web::Data::new(GuiNotBuilt { dist_path: shown_path }));
 cfg.service(web::resource("/gui{tail:.*}").route(web::get().to(not_built_index)));
}

#[derive(Clone)]
struct GuiNotBuilt {
 dist_path: String,
}

async fn not_built_index(ctx: web::Data<GuiNotBuilt>) -> impl Responder {
 let body = NOT_BUILT_HTML.replace("{{DIST_PATH}}", &html_escape(&ctx.dist_path));
 HttpResponse::NotFound()
  .content_type("text/html; charset=utf-8")
  .body(body)
}

fn html_escape(s: &str) -> String {
 s.replace('&', "&amp;")
  .replace('<', "&lt;")
  .replace('>', "&gt;")
  .replace('"', "&quot;")
}

const NOT_BUILT_HTML: &str = r#"<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8" />
<title>VAC GUI — 未ビルド</title>
<meta name="viewport" content="width=device-width,initial-scale=1" />
<style>
 :root { color-scheme: light dark; }
 body { font-family: system-ui, sans-serif; max-width: 640px; margin: 2rem auto; padding: 0 1rem; line-height: 1.6; }
 code, pre { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
 pre { background: #8884; padding: 0.75rem 1rem; border-radius: 6px; overflow-x: auto; }
 h1 { border-bottom: 2px solid #8884; padding-bottom: 0.25rem; }
 .hint { background: #f0f4; border-left: 4px solid #a6f; padding: 0.5rem 1rem; margin: 1rem 0; border-radius: 0 6px 6px 0; }
 .path { background: #8884; padding: 0.1rem 0.4rem; border-radius: 3px; }
</style>
</head>
<body>
<h1>VAC GUI は未ビルドです</h1>
<p>
 この URL（<code>/gui/</code>）は Phase VI-β Control Panel の配信エンドポイントですが、
 配信元ディレクトリに <code>index.html</code> が見つかりませんでした。
</p>
<p>
 配信元（conf の <code>gui_dist_path</code>、既定 <code>gui/dist</code>）:
 <span class="path">{{DIST_PATH}}</span>
</p>

<h2>ビルド手順</h2>
<pre>cd gui
npm install
npm run build</pre>
<p>ビルド後、この URL を再読込すると GUI が表示されます。</p>

<h2>開発中の使い方</h2>
<p>
 ビルドせず Vite dev サーバを使う場合は別ターミナルで:
</p>
<pre>cd gui
npm run dev</pre>
<p>
 Vite が <code>http://localhost:5173/gui/</code> で起動し、<code>/api/*</code> と <code>/ws/*</code> を
 この VAC プロセスへプロキシします。
</p>

<div class="hint">
 <strong>Control API は本ページとは独立に稼働しています。</strong>
 <code>/api/v1/control/ping</code> などは利用可能です（Bearer 認証ポリシーは
 接続元と設定によります）。
</div>

</body>
</html>
"#;
