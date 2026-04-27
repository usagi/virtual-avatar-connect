//! VAC の motion 基盤。
//!
//! VMC / OSC のワイヤ表現、OSC decode、生 UDP 転送 helper を持つ。
//! アプリ起動・設定・shutdown との結合は root crate の `motion` module に残す。

pub mod frame;
pub mod osc;
pub mod router;
pub mod vmc;
mod vmc_osc;
pub mod vrchat;

pub use frame::{MotionFrame, OscMessageWire};
pub use vmc_osc::parse_vmc_payload;
