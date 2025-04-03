// ref: https://platform.openai.com/docs/api-reference/fine-tuning

use serde::{Deserialize, Serialize};

/// The fine-tuning job object
/// ref: https://platform.openai.com/docs/api-reference/fine-tuning/object
#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
pub struct FineTuningJobObject {
 pub id: String,
 pub created_at: usize,
 pub error: Option<ApiErrorDetail>,
 pub fine_tuned_model: Option<String>,
 pub hyperparameters: Hyperparameters,
 pub model: String,
 pub object: String,
 pub organization_id: String,
 pub result_files: Vec<String>,
 pub status: String,
 pub trained_tokens: Option<usize>,
 pub training_file: String,
 pub validation_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Hyperparameters {
 pub n_epochs: StringOrInteger,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum StringOrInteger {
 String(String),
 Integer(usize),
}

impl Default for StringOrInteger {
 fn default() -> Self {
  Self::String("auto".to_string())
 }
}

/// Create fine-tuning job
/// ref: https://platform.openai.com/docs/api-reference/fine-tuning/create
/// response: FineTuningJobObject
#[derive(Serialize, Debug, Default)]
pub struct FineTuningRequest {
 pub model: String,
 pub training_file: String,
 #[serde(skip_serializing_if = "Option::is_none")]
 pub hyperparameters: Option<Hyperparameters>,
 #[serde(skip_serializing_if = "Option::is_none")]
 pub suffix: Option<String>,
 #[serde(skip_serializing_if = "Option::is_none")]
 pub validation_file: Option<String>,
}

// Retrieve fine-tuning job
// request: GET https://api.openai.com/v1/fine_tuning/jobs/{fine_tuning_job_id}
// response: FineTuningJobObject
// (request object is not available)

/// The file object
/// ref: https://platform.openai.com/docs/api-reference/files/object
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct FileObject {
 pub id: String,
 pub bytes: usize,
 pub created_at: usize,
 pub filename: String,
 pub object: String,
 pub purpose: String,
 pub status: String,
 pub status_details: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
pub struct ListFilesResponse {
 pub data: Vec<FileObject>,
 pub object: String,
}

// Upload file
// ref: https://platform.openai.com/docs/api-reference/files/create
// request: POST https://api.openai.com/v1/files
//  note: multipart/form-data
//   file   (required): @file (つまり file object)
//   purpose(required): "fine-tune" などの文字列値
// response: FileObject

#[derive(Deserialize, Debug, Default)]
pub struct ApiError {
 pub error: Option<ApiErrorDetail>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct ApiErrorDetail {
 pub r#type: Option<String>,
 pub param: Option<String>,
 pub message: Option<String>,
 pub code: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Default)]
pub struct FileDeletionStatus {
 pub id: String,
 pub object: String,
 pub deleted: bool,
}

/// note: これはローカル側でファイルを扱うために使用
pub struct NamedData {
 pub name: String,
 pub data: Vec<u8>,
}
