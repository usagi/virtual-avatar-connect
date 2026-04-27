//! VAC runtime core primitives.
//!
//! 第一段では shutdown broker と runtime path primitives を持つ。conf / shared state primitives は後続で移す。

pub mod control_events;
pub mod datetime;
pub mod resource;
pub mod runtime;
pub mod shutdown;
pub mod twitch_oauth_sessions;
pub mod utility;
