#[tokio::test]
async fn conf() -> anyhow::Result<()> {
 use virtual_avatar_connect::Args;
 use virtual_avatar_connect::Conf;

 let (_stream, stream_handle) = rodio::OutputStream::try_default()?;
 let audio_sink = rodio::Sink::try_new(&stream_handle)?;
 let audio_sink = tokio::sync::Arc::new(tokio::sync::Mutex::new(AudioSink(audio_sink)));

 let mut args = Args::init(audio_sink.clone()).await?;
 args.conf = "conf.toml".to_string();
 let conf = Conf::new(&args)?;

 println!("{:#?}", conf);

 audio_sink.lock().await.0.sleep_until_end();

 Ok(())
}
