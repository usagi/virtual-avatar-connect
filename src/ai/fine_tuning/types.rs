// ファインチューニング用のローカル型（アップロード前データ・ジョブ作成パラメータ）。
// 応答型は async_openai::types::finetuning / files をそのまま使う。

use anyhow::{Context, Result};
use async_openai::types::finetuning::{
 BatchSize, CreateFineTuningJobRequest, FineTuneMethod, FineTuneSupervisedHyperparameters, FineTuneSupervisedMethod,
 LearningRateMultiplier, NEpochs,
};
use serde::{Deserialize, Serialize};

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

impl FineTuningRequest {
 pub fn try_into_openai(self) -> Result<CreateFineTuningJobRequest> {
  let method = match self.hyperparameters {
   None => None,
   Some(h) => {
    let n_epochs = match h.n_epochs {
     StringOrInteger::String(ref s) if s.eq_ignore_ascii_case("auto") => NEpochs::Auto,
     StringOrInteger::Integer(n) => NEpochs::NEpochs(
      u8::try_from(n).context("hyperparameters.n_epochs は 0〜255 の整数として解釈して下さい")?,
     ),
     StringOrInteger::String(_) => NEpochs::Auto,
    };
    Some(FineTuneMethod::Supervised {
     supervised: FineTuneSupervisedMethod {
      hyperparameters: FineTuneSupervisedHyperparameters {
       batch_size: BatchSize::Auto,
       learning_rate_multiplier: LearningRateMultiplier::Auto,
       n_epochs,
      },
     },
    })
   },
  };

  Ok(CreateFineTuningJobRequest {
   model: self.model,
   training_file: self.training_file,
   suffix: self.suffix,
   validation_file: self.validation_file,
   method,
   integrations: None,
   seed: None,
   metadata: None,
  })
 }
}

pub struct NamedData {
 pub name: String,
 pub data: Vec<u8>,
}
