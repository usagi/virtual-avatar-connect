//! OpenAI Responses API の DTO 群。
//!
//! 型対応表は `docs/roadmap/phase-chi-openai-responses.md` §4 を参照。

pub mod input;
pub mod request;
pub mod response;
pub mod stream;

// 以下の `pub use` は他モジュールからの短縮アクセスを提供する公開 API。χ-5 時点では
// 内部呼び出しが `types::input::*` などのフルパスを利用しているため `unused_imports`
// が出るが、公開表面として維持したいので allow で抑制しておく。
#[allow(unused_imports)]
pub use input::{InputContent, InputContentPart, InputItem};
#[allow(unused_imports)]
pub use request::{
 CreateResponseRequest, NamedToolChoice, Reasoning, ReasoningEffort, TextConfig, TextFormat, Tool,
 ToolChoice, ToolChoiceMode,
};
#[allow(unused_imports)]
pub use response::{
 ErrorObject, FunctionCallView, IncompleteDetails, MessageContent, OutputItem, Response,
 ResponseStatus, Usage, UsageInputDetails, UsageOutputDetails,
};
#[allow(unused_imports)]
pub use stream::StreamEvent;
