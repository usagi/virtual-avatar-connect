//! Shutdown broker.
//!
//! 実体は `vac-core` crate に分割済み。既存の `crate::shutdown::*` API を保つため再エクスポートする。

pub use vac_core::shutdown::*;
