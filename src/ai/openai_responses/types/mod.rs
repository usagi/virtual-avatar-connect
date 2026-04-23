//! OpenAI Responses API の DTO 群。
//!
//! 型対応表は `docs/roadmap/phase-chi-openai-responses.md` §4 を参照。

pub mod input;
pub mod request;
pub mod response;
pub mod stream;

pub use input::{InputContent, InputContentPart, InputItem};
pub use request::{
 CreateResponseRequest, NamedToolChoice, Reasoning, ReasoningEffort, TextConfig, TextFormat, Tool,
 ToolChoice, ToolChoiceMode,
};
pub use response::{
 ErrorObject, IncompleteDetails, MessageContent, OutputItem, Response, ResponseStatus, Usage,
 UsageInputDetails, UsageOutputDetails,
};
