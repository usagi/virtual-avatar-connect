//! Dimensional Quantity System (Phase ξ)。
//!
//! VAC Flowgraph の数値系の核。F# / Haskell-quantity 風の SI 単位次元をランタイム dynamic typing 上で再現する。
//!
//! # モジュール
//!
//! - [`dimension`]: 7 SI 基本次元 + Angle 疑似次元の 8 成分ベクトル [`Dimension`]。
//! - [`prefix`]: SI 接頭辞 20 種 [`SIPrefix`]。
//! - [`unit`]: [`Unit`] = atoms + si_factor + prefix_hint。派生単位コンストラクタ（Hz, N, J, ...）。
//! - [`quantity`]: [`Quantity`] = 値 + Unit。加減乗除、`convert_to`, 温度特殊規則。
//! - [`parser`]: 文字列 `"m/s^2"`, `"kg·m/s^2"`, `"μs"` 等を [`Unit`] にパース。
//!
//! # 設計判断
//!
//! Phase ξ の全体設計は `docs/roadmap/phase-ksi-dimensional-quantity-system.md` に集約されている。
//! ξ-1 ではコア型のみを提供し、`SocketValue::Float` の置換や `flowgraph.unit.*` ノード
//! は ξ-2 以降で追加する。

pub mod dimension;
pub mod parser;
pub mod prefix;
pub mod quantity;
pub mod unit;

pub use dimension::{Dimension, DimensionMismatch};
pub use parser::{parse_unit, UnitParseError};
pub use prefix::SIPrefix;
pub use quantity::{Quantity, QuantityArithError};
pub use unit::{BaseUnitId, Unit};
