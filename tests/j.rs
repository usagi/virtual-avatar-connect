#[tokio::test]
async fn test() -> anyhow::Result<()> {
 let json_data = r#"{
 "array": [1, 2, 3],
 "object": {
     "key1": "value1",
     "key2": "value2"
 },
 "number": 42.0,
 "string": "Hello, World!"
}"#;

 use serde_json::{Map, Value};

 let parsed_data: Value = serde_json::from_str(json_data)?;

 // データの型に応じて抽出します
 let array = parsed_data["array"].as_array();
 let object = parsed_data["object"].as_object();
 let number = parsed_data["number"].as_f64();
 let string = parsed_data["string"].as_str();

 let array_vec = array.unwrap_or(&vec![]).to_vec();
 let object_map = object.unwrap_or(&Map::new()).clone();
 let number_f64 = number.unwrap_or(0.0);
 let string_str = string.unwrap_or("");
}
