#[tokio::test]
async fn tmp() {
 use virtual_avatar_connect::web_interface::WsServerPayload;

 let s = r#"
{
 "channel_data":[
  {"channel":"user",
   "content":"a",
   "flags":["is_final"],
   "id":711,
   "datetime":"2023-09-23T13:45:36.105217600Z"
  }
  ],
  "channel_datum":null
}"#;

 assert!(serde_json::from_str::<WsServerPayload>(s).is_ok());

 let s = r#"
 {
  "channel_datum":
   {"channel":"user",
    "content":"a",
    "flags":["is_final"],
    "id":711,
    "datetime":"2023-09-23T13:45:36.105217600Z"
   }
 }"#;

 assert!(serde_json::from_str::<WsServerPayload>(s).is_ok());

 let s = r#"
 {
  "channel_datum": {"content":"nyanko"},
  "flags":["is_final"]
 }
"#;

 assert!(serde_json::from_str::<WsServerPayload>(s).is_err());

 let s = r#"
{
 "channel_datum":
  { "content":"nyanko",
    "channel":"neko" }
}
"#;

 assert!(serde_json::from_str::<WsServerPayload>(s).is_ok());

 let s = r#"
{"channel_datum":
{"channel":
"user",
"content":"hoge",
"flags":["is_final"]}}
"#;

 // assert!(
  serde_json::from_str::<WsServerPayload>(s).unwrap()
 // .is_ok())
 ;
}
