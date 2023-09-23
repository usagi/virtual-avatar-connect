#[tokio::test]
async fn conf() -> anyhow::Result<()> {
 use virtual_avatar_connect::Args;
 use virtual_avatar_connect::Conf;

 let mut args = Args::init().await?;
 args.conf = "conf.toml".to_string();
 let conf = Conf::new(&args)?;

 println!("{:#?}", conf);

 Ok(())
}
