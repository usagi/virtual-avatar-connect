#[test]
fn conf() -> anyhow::Result<()> {
 use virtual_avatar_connect::Conf;

 let conf_path = "conf.toml";
 let conf = Conf::load(conf_path)?;

 println!("{:#?}", conf);

 Ok(())
}
