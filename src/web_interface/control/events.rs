//! Control API の WebSocket (`/api/v1/control/events`) で使うイベント型の **再エクスポート**。
//!
//! 型の定義本体は [`crate::control_events`]（`state` が `web_interface` に依存しないための配置）。

pub use crate::control_events::{ChannelDatumPhase, ControlEvent, ProcessorInvocationOutcome};
