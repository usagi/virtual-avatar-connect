//! LibreTranslate 連携（ローカル HTTP）。
//!
//! Windows かつ `libretranslate_url` 未指定時は、embeddable Python をキャッシュに展開し
//! `pip install libretranslate` 後に子プロセスでサーバを起動する（試験的）。

use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Windows embeddable の既定バージョン（URL と合わせること）
const EMBED_PYTHON_VERSION: &str = "3.12.7";
const PYTHON_EMBED_ZIP_TEMPLATE: &str = "https://www.python.org/ftp/python/{ver}/python-{ver}-embed-amd64.zip";
const GET_PIP_URL: &str = "https://bootstrap.pypa.io/get-pip.py";

pub type SharedRuntime = Arc<Mutex<Runtime>>;

pub fn runtime_new() -> SharedRuntime {
	Arc::new(Mutex::new(Runtime::default()))
}

#[derive(Default)]
pub struct Runtime {
	/// VAC が起動した子プロセス（embed モード時）
	child: Option<tokio::process::Child>,
	/// 例: `http://127.0.0.1:5000`
	base_url: Option<String>,
}

impl std::fmt::Debug for Runtime {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LibreTranslateRuntime")
			.field("base_url", &self.base_url)
			.field("child_running", &self.child.is_some())
			.finish()
	}
}

impl Runtime {
	/// 翻訳 API のベース URL を返す。embed 未使用時は `configured_base` をそのまま使う。
	pub async fn ensure_server(&mut self, configured_base: Option<&str>, preferred_port: u16) -> Result<String, String> {
		if let Some(u) = configured_base {
			let u = u.trim().trim_end_matches('/').to_string();
			if !u.is_empty() {
				return Ok(u);
			}
		}

		if self.base_url.is_some() && self.child.is_some() {
			return self
				.base_url
				.clone()
				.ok_or_else(|| "LibreTranslate: 内部状態が不正です".to_string());
		}

		let port = pick_free_port(preferred_port);
		let base = format!("http://127.0.0.1:{}", port);

		#[cfg(windows)]
		{
			let embed_dir = prepare_windows_embedded_python().await?;
			start_libretranslate_process(&embed_dir, port, &mut self.child).await?;
			wait_for_http_ready(&base).await?;
			self.base_url = Some(base.clone());
			return Ok(base);
		}

		#[cfg(not(windows))]
		{
			let _ = port;
			Err("LibreTranslate の自動起動（embed）は現在 Windows のみです。\
     手動で LibreTranslate を起動し、設定に libretranslate_url = \"http://127.0.0.1:5000\" のように指定してください。"
				.to_string())
		}
	}

	pub async fn stop(&mut self) {
		if let Some(mut c) = self.child.take() {
			log::info!("《LibreTranslate》: 子プロセスを終了します。");
			let _ = c.kill().await;
			let _ = c.wait().await;
		}
		self.base_url = None;
	}
}

fn pick_free_port(preferred: u16) -> u16 {
	for p in preferred..preferred.saturating_add(32) {
		if TcpListener::bind(("127.0.0.1", p)).is_ok() {
			return p;
		}
	}
	preferred
}

#[cfg(windows)]
async fn prepare_windows_embedded_python() -> Result<PathBuf, String> {
	let root = cache_root();
	let ver = std::env::var("VAC_LIBRETRANSLATE_EMBED_PYTHON_VERSION").unwrap_or_else(|_| EMBED_PYTHON_VERSION.to_string());
	let embed_dir = root.join(format!("python-embed-{}", ver));
	let python_exe = embed_dir.join("python.exe");
	let pip_ok = embed_dir.join(".vac_pip_libretranslate_ok");

	if pip_ok.is_file() && python_exe.is_file() {
		log::info!("《LibreTranslate》: 埋め込み Python を再利用します {:?}", embed_dir);
		return Ok(embed_dir);
	}

	std::fs::create_dir_all(&root).map_err(|e| format!("キャッシュ作成: {e}"))?;

	let zip_url = PYTHON_EMBED_ZIP_TEMPLATE.replace("{ver}", &ver);
	let zip_path = root.join(format!("python-{}-embed-amd64.zip", ver));
	log::info!("《LibreTranslate》: embeddable Python を取得します {}", zip_url);
	download_file(&zip_url, &zip_path).await?;

	let data = tokio::fs::read(&zip_path).await.map_err(|e| format!("zip 読み取り: {e}"))?;
	std::fs::create_dir_all(&embed_dir).map_err(|e| format!("展開先: {e}"))?;
	extract_zip_flat_or_rooted(&data, &embed_dir)?;

	fix_embed_pth(&embed_dir)?;

	let get_pip_path = root.join("get-pip.py");
	if !get_pip_path.is_file() {
		log::info!("《LibreTranslate》: get-pip.py を取得します");
		download_file(GET_PIP_URL, &get_pip_path).await?;
	}

	log::info!("《LibreTranslate》: pip で libretranslate をインストールします（初回は数分かかることがあります）");
	let py = python_exe.clone();
	let gp = get_pip_path.clone();
	let ed = embed_dir.clone();
	tokio::task::spawn_blocking(move || run_pip_bootstrap(&py, &gp, &ed))
		.await
		.map_err(|e| format!("pip タスク: {e}"))??;

	tokio::fs::write(&pip_ok, b"ok")
		.await
		.map_err(|e| format!("マーカー書き込み: {e}"))?;

	let _ = tokio::fs::remove_file(&zip_path).await;

	Ok(embed_dir)
}

#[cfg(windows)]
fn run_pip_bootstrap(python_exe: &Path, get_pip: &Path, work_dir: &Path) -> Result<(), String> {
	use std::process::Command;
	let o = Command::new(python_exe)
		.current_dir(work_dir)
		.arg(get_pip)
		.arg("--no-warn-script-location")
		.output()
		.map_err(|e| format!("get-pip 実行: {e}"))?;
	if !o.status.success() {
		return Err(format!(
			"get-pip 失敗: {}\n{}",
			String::from_utf8_lossy(&o.stderr),
			String::from_utf8_lossy(&o.stdout)
		));
	}
	let o = Command::new(python_exe)
		.current_dir(work_dir)
		.args(["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel"])
		.output()
		.map_err(|e| format!("pip upgrade: {e}"))?;
	if !o.status.success() {
		log::warn!("《LibreTranslate》: pip upgrade 警告: {}", String::from_utf8_lossy(&o.stderr));
	}
	let o = Command::new(python_exe)
		.current_dir(work_dir)
		.args(["-m", "pip", "install", "libretranslate"])
		.output()
		.map_err(|e| format!("pip install libretranslate: {e}"))?;
	if !o.status.success() {
		return Err(format!(
			"pip install libretranslate 失敗: {}\n{}",
			String::from_utf8_lossy(&o.stderr),
			String::from_utf8_lossy(&o.stdout)
		));
	}
	Ok(())
}

#[cfg(windows)]
fn fix_embed_pth(embed_dir: &Path) -> Result<(), String> {
	let ver = std::env::var("VAC_LIBRETRANSLATE_EMBED_PYTHON_VERSION").unwrap_or_else(|_| EMBED_PYTHON_VERSION.to_string());
	let name = python_pth_filename(&ver);
	let pth = embed_dir.join(&name);
	let mut s = std::fs::read_to_string(&pth).map_err(|e| format!("{} 読み取り: {}", pth.display(), e))?;
	if !s.contains("import site") {
		s = s.replace("#import site", "import site");
		if !s.contains("import site") {
			s.push_str("\r\nimport site\r\n");
		}
	}
	// site-packages を明示（pip が作ったパス）
	let lib_site = format!("Lib\\site-packages");
	if !s.contains(&lib_site) {
		s.push_str(&format!("{}\r\n", lib_site));
	}
	std::fs::write(&pth, s).map_err(|e| format!("{} 書き込み: {}", pth.display(), e))?;
	Ok(())
}

#[cfg(windows)]
fn python_pth_filename(ver: &str) -> String {
	let p: Vec<&str> = ver.split('.').take(2).collect();
	if p.len() == 2 {
		format!("python{}{}._pth", p[0], p[1])
	} else {
		format!("python{}._pth", ver.replace('.', ""))
	}
}

#[cfg(windows)]
async fn start_libretranslate_process(embed_dir: &Path, port: u16, child_slot: &mut Option<tokio::process::Child>) -> Result<(), String> {
	let python = embed_dir.join("python.exe");
	let script_exe = embed_dir.join("Scripts").join("libretranslate.exe");
	let mut cmd = if script_exe.is_file() {
		log::debug!("《LibreTranslate》: {:?}", script_exe);
		let mut c = tokio::process::Command::new(&script_exe);
		c.current_dir(embed_dir);
		c
	} else {
		if !python.is_file() {
			return Err(format!("python.exe が見つかりません: {:?}", python));
		}
		let mut c = tokio::process::Command::new(&python);
		c.current_dir(embed_dir);
		c.args(["-m", "libretranslate"]);
		c
	};

	cmd.stdin(Stdio::null());
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());
	cmd.args(["--host", "127.0.0.1", "--port", &port.to_string(), "--disable-web-ui"]);

	log::info!(
		"《LibreTranslate》: サーバを起動します (port={})。初回はモデル取得で時間がかかることがあります。",
		port
	);

	let mut child = cmd.spawn().map_err(|e| format!("LibreTranslate 起動: {e}"))?;

	if let Some(mut err) = child.stderr.take() {
		tokio::spawn(async move {
			use tokio::io::AsyncReadExt;
			let mut buf = Vec::new();
			let _ = err.read_to_end(&mut buf).await;
			if !buf.is_empty() {
				log::debug!("《LibreTranslate》 stderr: {}", String::from_utf8_lossy(&buf));
			}
		});
	}

	*child_slot = Some(child);
	Ok(())
}

async fn wait_for_http_ready(base: &str) -> Result<(), String> {
	let url = format!("{}/languages", base);
	let client = reqwest::Client::builder()
		.timeout(std::time::Duration::from_secs(5))
		.build()
		.map_err(|e| e.to_string())?;

	for attempt in 0..120u32 {
		match client.get(&url).send().await {
			Ok(r) if r.status().is_success() => {
				log::info!("《LibreTranslate》: HTTP 応答を確認しました ({})", url);
				return Ok(());
			}
			Ok(r) => {
				log::trace!("《LibreTranslate》: 待機中... HTTP {}", r.status());
			}
			Err(e) => {
				log::trace!("《LibreTranslate》: 待機中... {}", e);
			}
		}
		tokio::time::sleep(std::time::Duration::from_secs(1)).await;
		if attempt > 0 && attempt % 15 == 0 {
			log::info!(
				"《LibreTranslate》: まだ起動待ちです（{} 秒経過）。初回はモデルダウンロードで長引くことがあります。",
				attempt
			);
		}
	}

	Err(
		"LibreTranslate が起動しませんでした（タイムアウト）。ファイアウォール・ポート競合・初回モデル取得失敗などを確認してください。"
			.into(),
	)
}

fn cache_root() -> PathBuf {
	dirs::cache_dir()
		.unwrap_or_else(std::env::temp_dir)
		.join("virtual-avatar-connect")
		.join("libretranslate")
}

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
	if dest.is_file() {
		return Ok(());
	}
	let resp = reqwest::get(url).await.map_err(|e| format!("GET {url}: {e}"))?;
	if !resp.status().is_success() {
		return Err(format!("GET {url} -> HTTP {}", resp.status()));
	}
	let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
	tokio::fs::write(dest, &bytes)
		.await
		.map_err(|e| format!("保存 {}: {e}", dest.display()))?;
	Ok(())
}

fn extract_zip_flat_or_rooted(data: &[u8], dest: &Path) -> Result<(), String> {
	let reader = Cursor::new(data);
	let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("zip: {e}"))?;
	for i in 0..archive.len() {
		let mut file = archive.by_index(i).map_err(|e| format!("zip {i}: {e}"))?;
		let name = file.name();
		let base = Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or("");
		if base.is_empty() {
			continue;
		}
		let out = dest.join(base);
		if name.ends_with('/') {
			std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
			continue;
		}
		if let Some(p) = out.parent() {
			std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
		}
		let mut out_f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
		std::io::copy(&mut file, &mut out_f).map_err(|e| e.to_string())?;
	}
	Ok(())
}

#[derive(Serialize)]
pub struct TranslateRequest<'a> {
	pub q: &'a str,
	pub source: &'a str,
	pub target: &'a str,
	pub format: &'a str,
}

#[derive(Deserialize)]
pub struct TranslateResponse {
	#[serde(rename = "translatedText")]
	pub translated_text: String,
}

pub async fn translate(base: &str, source: &str, target: &str, text: &str) -> Result<String, String> {
	let url = format!("{}/translate", base.trim_end_matches('/'));
	let client = reqwest::Client::builder()
		.timeout(std::time::Duration::from_secs(120))
		.build()
		.map_err(|e| e.to_string())?;
	let body = TranslateRequest {
		q: text,
		source,
		target,
		format: "text",
	};
	let resp = client.post(&url).json(&body).send().await.map_err(|e| format!("POST {url}: {e}"))?;
	let status = resp.status();
	if !status.is_success() {
		let t = resp.text().await.unwrap_or_default();
		return Err(format!("LibreTranslate HTTP {}: {}", status, t));
	}
	let tr: TranslateResponse = resp.json().await.map_err(|e| e.to_string())?;
	Ok(tr.translated_text)
}
