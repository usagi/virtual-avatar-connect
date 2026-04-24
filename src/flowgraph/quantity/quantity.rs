//! Quantity: 数値 + 単位のペア。VAC Flowgraph の単位次元システムの中核。
//!
//! 詳細設計: [`docs/roadmap/phase-ksi-dimensional-quantity-system.md`](../../../../docs/roadmap/phase-ksi-dimensional-quantity-system.md) §3.8 / §3.9 / §6.0 / §6.6。
//!
//! # セマンティクス
//!
//! - `+` / `-`: 両辺 `Dimension` 完全一致必須。異なる prefix でも同次元なら SI base に正規化して計算、
//!   結果 unit は「prefix_hint が大きい方」を継承。
//! - `*` / `/`: atoms の加減算と `si_factor` の積・商で自動組み立て。次元が自動的に合成される。
//! - `pow`: 整数指数のみ、exponent は dimensionless 必須。
//! - 温度特別規則 (§3.6):
//!   - `K + K` → error（絶対温度同士の和は物理的に無意味）
//!   - `K + ΔK` / `ΔK + K` → `K`（絶対 + 差分 = 新しい絶対）
//!   - `K - K` → `ΔK`（絶対 - 絶対 = 差分、F# / Haskell-quantity 流）
//!   - `K - ΔK` → `K`（絶対 - 差分 = 新しい絶対）
//!   - `ΔK - K` → error
//!   - `K × anything` / `K / anything` / `anything × K` / `anything / K` → error（絶対温度は乗除不可）
//!   - `ΔK × Float` / `ΔK × ΔK` → OK（差分はスカラー倍も積も可）

use std::ops::{Add, Div, Mul, Neg, Sub};

use super::dimension::{Dimension, DimensionMismatch};
use super::unit::{BaseUnitId, Unit};

/// 数値 + 単位。VAC Flowgraph が `SocketValue::Float` を置き換える形で ξ-3 で採用する予定の型。
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
	pub value: f64,
	pub unit: Unit,
}

impl Quantity {
	/// 無次元の Quantity を作る。ξ-3 で既存 `SocketValue::Float(v)` の fallback 表現となる。
	pub fn dimensionless(value: f64) -> Self {
		Self {
			value,
			unit: Unit::dimensionless(),
		}
	}

	/// 値 + 単位で Quantity を作る。
	pub fn of(value: f64, unit: Unit) -> Self {
		Self { value, unit }
	}

	/// Dimension を返す。
	pub fn dimension(&self) -> Dimension {
		self.unit.dimension()
	}

	/// 無次元か。
	pub fn is_dimensionless(&self) -> bool {
		self.unit.is_dimensionless()
	}

	/// SI base 値（prefix + factor を掛けた数値）。
	pub fn as_si_base(&self) -> f64 {
		self.unit.to_si_base(self.value)
	}

	/// 指定単位への変換。同次元間のみ許容、次元不一致は [`DimensionMismatch`]。
	///
	/// K ↔ ΔK の変換は semantics が違うため error（同次元だが意味が異なる）。
	pub fn convert_to(&self, target: &Unit) -> Result<Self, DimensionMismatch> {
		// 次元チェック
		if self.unit.dimension() != target.dimension() {
			return Err(DimensionMismatch::with_op(self.unit.dimension(), target.dimension(), "convert"));
		}
		// K ↔ ΔK 特殊ガード: atoms に Kelvin と KelvinDelta のどちらが入っているかで判定
		let self_is_abs = contains_absolute_temp(&self.unit);
		let self_is_delta = contains_temperature_delta(&self.unit);
		let target_is_abs = contains_absolute_temp(target);
		let target_is_delta = contains_temperature_delta(target);
		if (self_is_abs && target_is_delta) || (self_is_delta && target_is_abs) {
			return Err(DimensionMismatch::with_op(
				self.unit.dimension(),
				target.dimension(),
				"convert (K <-> ΔK semantics mismatch)",
			));
		}
		// 値: SI base 経由で換算
		let si = self.as_si_base();
		let new_value = target.from_si_base(si);
		Ok(Self {
			value: new_value,
			unit: target.clone(),
		})
	}

	/// unit を捨てて dimensionless にする（明示 escape hatch, `flowgraph.unit.strip` の実装本体）。
	pub fn strip_unit(&self) -> Self {
		Self::dimensionless(self.value)
	}
}

// ---------------------------------------------------------------------------
// Helper predicates for temperature semantics
// ---------------------------------------------------------------------------

fn contains_absolute_temp(u: &Unit) -> bool {
	u.atoms.get(&BaseUnitId::Kelvin).copied().unwrap_or(0) != 0
}

fn contains_temperature_delta(u: &Unit) -> bool {
	u.atoms.get(&BaseUnitId::KelvinDelta).copied().unwrap_or(0) != 0
}

fn is_pure_absolute_temp(u: &Unit) -> bool {
	u.atoms.len() == 1 && u.atoms.get(&BaseUnitId::Kelvin).copied() == Some(1)
}

fn is_pure_temperature_delta(u: &Unit) -> bool {
	u.atoms.len() == 1 && u.atoms.get(&BaseUnitId::KelvinDelta).copied() == Some(1)
}

// ---------------------------------------------------------------------------
// Arithmetic errors
// ---------------------------------------------------------------------------

/// Quantity 演算時のエラー。次元不一致 / 温度セマンティクス違反を区別して持つ。
#[derive(Debug, Clone, thiserror::Error)]
pub enum QuantityArithError {
	#[error(transparent)]
	DimensionMismatch(#[from] DimensionMismatch),
	#[error("absolute temperature arithmetic not allowed: {context}")]
	AbsoluteTemperature { context: &'static str },
	#[error("division by zero")]
	DivisionByZero,
}

// ---------------------------------------------------------------------------
// Add / Sub
// ---------------------------------------------------------------------------

impl Quantity {
	/// 加算。両辺 Dimension 完全一致必須。温度特殊規則あり。
	pub fn try_add(&self, rhs: &Self) -> Result<Self, QuantityArithError> {
		// 温度特殊規則
		let lhs_abs = is_pure_absolute_temp(&self.unit);
		let rhs_abs = is_pure_absolute_temp(&rhs.unit);
		let lhs_delta = is_pure_temperature_delta(&self.unit);
		let rhs_delta = is_pure_temperature_delta(&rhs.unit);

		if lhs_abs && rhs_abs {
			return Err(QuantityArithError::AbsoluteTemperature {
				context: "K + K: absolute + absolute is not physically meaningful",
			});
		}
		if lhs_abs && rhs_delta {
			// K + ΔK → K
			let sum_si = self.as_si_base() + rhs.as_si_base();
			return Ok(Self {
				value: self.unit.from_si_base(sum_si),
				unit: self.unit.clone(),
			});
		}
		if lhs_delta && rhs_abs {
			// ΔK + K → K (commutative)
			let sum_si = self.as_si_base() + rhs.as_si_base();
			return Ok(Self {
				value: rhs.unit.from_si_base(sum_si),
				unit: rhs.unit.clone(),
			});
		}
		// その他に absolute temperature が絡むケース（例: Kelvin atom を持つ compound）
		if contains_absolute_temp(&self.unit) || contains_absolute_temp(&rhs.unit) {
			// ここは次元チェックに任せる。atoms が完全一致していれば通常 add、違えば DimensionMismatch
		}

		// 通常の add: 次元一致必須
		if self.unit.dimension() != rhs.unit.dimension() {
			return Err(QuantityArithError::DimensionMismatch(DimensionMismatch::with_op(
				self.unit.dimension(),
				rhs.unit.dimension(),
				"+",
			)));
		}
		// SI base で合算、結果 unit は「prefix_hint が大きい方」
		let sum_si = self.as_si_base() + rhs.as_si_base();
		let result_unit = pick_larger_prefix_unit(&self.unit, &rhs.unit);
		Ok(Self {
			value: result_unit.from_si_base(sum_si),
			unit: result_unit,
		})
	}

	/// 減算。両辺 Dimension 完全一致必須。温度特殊規則あり。
	pub fn try_sub(&self, rhs: &Self) -> Result<Self, QuantityArithError> {
		let lhs_abs = is_pure_absolute_temp(&self.unit);
		let rhs_abs = is_pure_absolute_temp(&rhs.unit);
		let lhs_delta = is_pure_temperature_delta(&self.unit);
		let rhs_delta = is_pure_temperature_delta(&rhs.unit);

		if lhs_abs && rhs_abs {
			// K - K → ΔK
			let diff_si = self.as_si_base() - rhs.as_si_base();
			return Ok(Self {
				value: diff_si, // ΔK の si_factor は 1.0
				unit: Unit::kelvin_delta(),
			});
		}
		if lhs_abs && rhs_delta {
			// K - ΔK → K
			let diff_si = self.as_si_base() - rhs.as_si_base();
			return Ok(Self {
				value: self.unit.from_si_base(diff_si),
				unit: self.unit.clone(),
			});
		}
		if lhs_delta && rhs_abs {
			return Err(QuantityArithError::AbsoluteTemperature {
				context: "ΔK - K: delta minus absolute is not physically meaningful",
			});
		}

		// 通常の sub: 次元一致必須
		if self.unit.dimension() != rhs.unit.dimension() {
			return Err(QuantityArithError::DimensionMismatch(DimensionMismatch::with_op(
				self.unit.dimension(),
				rhs.unit.dimension(),
				"-",
			)));
		}
		let diff_si = self.as_si_base() - rhs.as_si_base();
		let result_unit = pick_larger_prefix_unit(&self.unit, &rhs.unit);
		Ok(Self {
			value: result_unit.from_si_base(diff_si),
			unit: result_unit,
		})
	}

	/// 乗算。atoms 合成。absolute temperature 側が絡めば error。
	pub fn try_mul(&self, rhs: &Self) -> Result<Self, QuantityArithError> {
		if contains_absolute_temp(&self.unit) || contains_absolute_temp(&rhs.unit) {
			return Err(QuantityArithError::AbsoluteTemperature {
				context: "* involving absolute temperature (K) is not allowed; use ΔK or unit.strip",
			});
		}
		Ok(Self {
			value: self.value * rhs.value,
			unit: self.unit.mul(&rhs.unit),
		})
	}

	/// 除算。atoms 合成。absolute temperature 側が絡めば error。rhs.value == 0 は NaN/Inf のまま通す。
	pub fn try_div(&self, rhs: &Self) -> Result<Self, QuantityArithError> {
		if contains_absolute_temp(&self.unit) || contains_absolute_temp(&rhs.unit) {
			return Err(QuantityArithError::AbsoluteTemperature {
				context: "/ involving absolute temperature (K) is not allowed; use ΔK or unit.strip",
			});
		}
		Ok(Self {
			value: self.value / rhs.value,
			unit: self.unit.div(&rhs.unit),
		})
	}

	/// 整数冪。atoms exponent を n 倍。exponent は dimensionless 扱いで int のみ受ける設計。
	pub fn pow_i8(&self, n: i8) -> Self {
		Self {
			value: self.value.powi(n as i32),
			unit: self.unit.pow_i8(n),
		}
	}

	/// 平方根。全 atom exponent が 2 で割り切れる必要がある。
	pub fn try_sqrt(&self) -> Result<Self, QuantityArithError> {
		let dim = self.unit.dimension();
		if !dim.all_divisible_by(2) {
			return Err(QuantityArithError::DimensionMismatch(DimensionMismatch::with_op(
				dim,
				dim,
				"sqrt (non-integer resulting exponents)",
			)));
		}
		if contains_absolute_temp(&self.unit) {
			return Err(QuantityArithError::AbsoluteTemperature {
				context: "sqrt of absolute temperature (K) is not allowed",
			});
		}
		// atoms 側も全 exponent が偶数のはず
		let mut new_atoms = std::collections::BTreeMap::new();
		for (id, exp) in &self.unit.atoms {
			if exp % 2 != 0 {
				return Err(QuantityArithError::DimensionMismatch(DimensionMismatch::with_op(
					dim,
					dim,
					"sqrt (atom-level non-even exponent)",
				)));
			}
			if *exp != 0 {
				new_atoms.insert(*id, exp / 2);
			}
		}
		Ok(Self {
			value: self.value.sqrt(),
			unit: Unit {
				atoms: new_atoms,
				si_factor: self.unit.si_factor.sqrt(),
				prefix_hint: super::prefix::SIPrefix::None,
			},
		})
	}

	/// スカラー倍（右辺が dimensionless Float の特殊ケース、`flowgraph.math.*_mul` で使う）。
	/// `try_mul` の糖衣だが、dimensionless でない場合も unit を維持した mul として動作。
	pub fn scale(&self, factor: f64) -> Result<Self, QuantityArithError> {
		self.try_mul(&Self::dimensionless(factor))
	}

	/// 符号反転。unit 不変。
	pub fn negate(&self) -> Self {
		Self {
			value: -self.value,
			unit: self.unit.clone(),
		}
	}
}

// ---------------------------------------------------------------------------
// Display: "{value} {unit}" 形式で出力、dimensionless は数値のみ
// ---------------------------------------------------------------------------

impl std::fmt::Display for Quantity {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if self.is_dimensionless() {
			write!(f, "{}", self.value)
		} else {
			write!(f, "{} {}", self.value, self.unit.canonical())
		}
	}
}

/// 加減算時の「prefix_hint が大きい方」継承。
/// atoms が異なる場合は self の unit を優先（表現上の多様性を維持）。
fn pick_larger_prefix_unit(a: &Unit, b: &Unit) -> Unit {
	if a.atoms != b.atoms {
		// 次元は同じだが atom 配置が違う（例: N vs kg·m/s^2）: self を優先
		return a.clone();
	}
	if super::prefix::SIPrefix::max_of(a.prefix_hint, b.prefix_hint) == a.prefix_hint {
		a.clone()
	} else {
		b.clone()
	}
}

// ---------------------------------------------------------------------------
// std::ops impls (panicking on error, convenient for pure arithmetic contexts)
// ---------------------------------------------------------------------------

impl Neg for Quantity {
	type Output = Quantity;
	fn neg(self) -> Self::Output {
		self.negate()
	}
}

/// `+` は panic on error. ノード実装では `try_add` を使い、graph error に合流する前提。
impl Add for Quantity {
	type Output = Result<Quantity, QuantityArithError>;
	fn add(self, rhs: Self) -> Self::Output {
		self.try_add(&rhs)
	}
}

impl Sub for Quantity {
	type Output = Result<Quantity, QuantityArithError>;
	fn sub(self, rhs: Self) -> Self::Output {
		self.try_sub(&rhs)
	}
}

impl Mul for Quantity {
	type Output = Result<Quantity, QuantityArithError>;
	fn mul(self, rhs: Self) -> Self::Output {
		self.try_mul(&rhs)
	}
}

impl Div for Quantity {
	type Output = Result<Quantity, QuantityArithError>;
	fn div(self, rhs: Self) -> Self::Output {
		self.try_div(&rhs)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::quantity::prefix::SIPrefix;

	// ---- dimensionless --------------------------------------------------

	#[test]
	fn dimensionless_quantity_is_dimensionless() {
		let q = Quantity::dimensionless(42.0);
		assert!(q.is_dimensionless());
		assert_eq!(q.value, 42.0);
		assert_eq!(q.dimension(), Dimension::DIMENSIONLESS);
	}

	// ---- as_si_base -----------------------------------------------------

	#[test]
	fn as_si_base_applies_prefix_factor() {
		let q = Quantity::of(5.0, Unit::metre().with_prefix(SIPrefix::Kilo));
		assert_eq!(q.as_si_base(), 5000.0);
	}

	// ---- convert_to -----------------------------------------------------

	#[test]
	fn convert_km_to_m() {
		let km = Quantity::of(1.0, Unit::metre().with_prefix(SIPrefix::Kilo));
		let m = km.convert_to(&Unit::metre()).unwrap();
		assert_eq!(m.value, 1000.0);
	}

	#[test]
	fn convert_s_to_ms() {
		let s = Quantity::of(1.0, Unit::second());
		let ms = s.convert_to(&Unit::second().with_prefix(SIPrefix::Milli)).unwrap();
		assert_eq!(ms.value, 1000.0);
	}

	#[test]
	fn convert_between_different_dimensions_errors() {
		let m = Quantity::of(5.0, Unit::metre());
		let err = m.convert_to(&Unit::second());
		assert!(err.is_err());
	}

	#[test]
	fn convert_absolute_to_delta_is_rejected() {
		// K → ΔK は同次元だが semantics が違うので error
		let k = Quantity::of(300.0, Unit::kelvin());
		let err = k.convert_to(&Unit::kelvin_delta());
		assert!(err.is_err());
	}

	// ---- add ------------------------------------------------------------

	#[test]
	fn add_same_unit_values() {
		let a = Quantity::of(3.0, Unit::metre());
		let b = Quantity::of(2.0, Unit::metre());
		let sum = a.try_add(&b).unwrap();
		assert_eq!(sum.value, 5.0);
		assert_eq!(sum.unit, Unit::metre());
	}

	#[test]
	fn add_different_prefix_normalizes_and_keeps_larger() {
		// 1 km + 500 m → 1500 m in SI, result unit km (larger prefix)
		let km = Quantity::of(1.0, Unit::metre().with_prefix(SIPrefix::Kilo));
		let m = Quantity::of(500.0, Unit::metre());
		let sum = km.try_add(&m).unwrap();
		// result unit should be km (prefix_hint Kilo > None)
		assert_eq!(sum.unit.prefix_hint, SIPrefix::Kilo);
		// 1.5 km
		assert!((sum.value - 1.5).abs() < 1e-12);
	}

	#[test]
	fn add_dimension_mismatch_errors() {
		let m = Quantity::of(5.0, Unit::metre());
		let s = Quantity::of(3.0, Unit::second());
		let err = m.try_add(&s);
		assert!(matches!(err, Err(QuantityArithError::DimensionMismatch(_))));
	}

	// ---- sub ------------------------------------------------------------

	#[test]
	fn sub_same_unit_values() {
		let a = Quantity::of(5.0, Unit::metre());
		let b = Quantity::of(3.0, Unit::metre());
		let diff = a.try_sub(&b).unwrap();
		assert_eq!(diff.value, 2.0);
	}

	#[test]
	fn sub_dimension_mismatch_errors() {
		let m = Quantity::of(5.0, Unit::metre());
		let s = Quantity::of(3.0, Unit::second());
		assert!(m.try_sub(&s).is_err());
	}

	// ---- mul / div ------------------------------------------------------

	#[test]
	fn mul_composes_dimensions() {
		let m = Quantity::of(3.0, Unit::metre());
		let s = Quantity::of(2.0, Unit::second());
		let product = m.try_mul(&s).unwrap();
		assert_eq!(product.value, 6.0);
		assert_eq!(product.dimension(), Dimension::LENGTH.add(Dimension::TIME));
	}

	#[test]
	fn div_composes_dimensions() {
		let m = Quantity::of(100.0, Unit::metre());
		let s = Quantity::of(4.0, Unit::second());
		let velocity = m.try_div(&s).unwrap();
		assert_eq!(velocity.value, 25.0);
		assert_eq!(velocity.dimension(), Dimension::VELOCITY);
	}

	// ---- pow / sqrt -----------------------------------------------------

	#[test]
	fn pow_i8_squares_value_and_dimension() {
		let m = Quantity::of(3.0, Unit::metre());
		let area = m.pow_i8(2);
		assert_eq!(area.value, 9.0);
		assert_eq!(area.dimension(), Dimension::LENGTH.mul_scalar(2));
	}

	#[test]
	fn sqrt_halves_dimension() {
		let area = Quantity::of(9.0, Unit::metre().pow_i8(2));
		let side = area.try_sqrt().unwrap();
		assert_eq!(side.value, 3.0);
		assert_eq!(side.dimension(), Dimension::LENGTH);
	}

	#[test]
	fn sqrt_rejects_odd_dimension() {
		let volume = Quantity::of(8.0, Unit::metre().pow_i8(3));
		assert!(volume.try_sqrt().is_err());
	}

	// ---- temperature special rules --------------------------------------

	#[test]
	fn k_plus_k_is_error() {
		let a = Quantity::of(300.0, Unit::kelvin());
		let b = Quantity::of(10.0, Unit::kelvin());
		assert!(matches!(a.try_add(&b), Err(QuantityArithError::AbsoluteTemperature { .. })));
	}

	#[test]
	fn k_plus_dk_is_k() {
		let a = Quantity::of(300.0, Unit::kelvin());
		let dk = Quantity::of(10.0, Unit::kelvin_delta());
		let result = a.try_add(&dk).unwrap();
		assert_eq!(result.value, 310.0);
		assert_eq!(result.unit, Unit::kelvin());
	}

	#[test]
	fn dk_plus_k_is_k_commutative() {
		let dk = Quantity::of(10.0, Unit::kelvin_delta());
		let k = Quantity::of(300.0, Unit::kelvin());
		let result = dk.try_add(&k).unwrap();
		assert_eq!(result.value, 310.0);
		assert_eq!(result.unit, Unit::kelvin());
	}

	#[test]
	fn k_minus_k_is_delta() {
		let a = Quantity::of(310.0, Unit::kelvin());
		let b = Quantity::of(300.0, Unit::kelvin());
		let diff = a.try_sub(&b).unwrap();
		assert_eq!(diff.value, 10.0);
		assert_eq!(diff.unit, Unit::kelvin_delta());
	}

	#[test]
	fn k_minus_dk_is_k() {
		let k = Quantity::of(300.0, Unit::kelvin());
		let dk = Quantity::of(10.0, Unit::kelvin_delta());
		let result = k.try_sub(&dk).unwrap();
		assert_eq!(result.value, 290.0);
		assert_eq!(result.unit, Unit::kelvin());
	}

	#[test]
	fn dk_minus_k_is_error() {
		let dk = Quantity::of(10.0, Unit::kelvin_delta());
		let k = Quantity::of(300.0, Unit::kelvin());
		assert!(matches!(
			dk.try_sub(&k),
			Err(QuantityArithError::AbsoluteTemperature { .. })
		));
	}

	#[test]
	fn dk_plus_dk_is_dk() {
		let a = Quantity::of(5.0, Unit::kelvin_delta());
		let b = Quantity::of(3.0, Unit::kelvin_delta());
		let sum = a.try_add(&b).unwrap();
		assert_eq!(sum.value, 8.0);
		assert_eq!(sum.unit, Unit::kelvin_delta());
	}

	#[test]
	fn dk_minus_dk_is_dk() {
		let a = Quantity::of(5.0, Unit::kelvin_delta());
		let b = Quantity::of(3.0, Unit::kelvin_delta());
		let diff = a.try_sub(&b).unwrap();
		assert_eq!(diff.value, 2.0);
	}

	#[test]
	fn k_times_anything_is_error() {
		let k = Quantity::of(300.0, Unit::kelvin());
		let two = Quantity::dimensionless(2.0);
		assert!(matches!(
			k.try_mul(&two),
			Err(QuantityArithError::AbsoluteTemperature { .. })
		));
		assert!(matches!(
			two.try_mul(&k),
			Err(QuantityArithError::AbsoluteTemperature { .. })
		));
	}

	#[test]
	fn k_divided_by_anything_is_error() {
		let k = Quantity::of(300.0, Unit::kelvin());
		let s = Quantity::of(2.0, Unit::second());
		assert!(matches!(
			k.try_div(&s),
			Err(QuantityArithError::AbsoluteTemperature { .. })
		));
	}

	#[test]
	fn dk_times_float_is_dk() {
		let dk = Quantity::of(5.0, Unit::kelvin_delta());
		let two = Quantity::dimensionless(2.0);
		let result = dk.try_mul(&two).unwrap();
		assert_eq!(result.value, 10.0);
		// atoms: {KelvinDelta: 1}
		assert_eq!(result.unit.atoms.get(&BaseUnitId::KelvinDelta).copied(), Some(1));
	}

	// ---- ops overload ---------------------------------------------------

	#[test]
	fn std_ops_add_returns_result() {
		let a = Quantity::of(1.0, Unit::metre());
		let b = Quantity::of(2.0, Unit::metre());
		let sum: Result<Quantity, QuantityArithError> = a + b;
		assert_eq!(sum.unwrap().value, 3.0);
	}

	#[test]
	fn neg_inverts_value_keeps_unit() {
		let q = Quantity::of(5.0, Unit::metre());
		let n = -q.clone();
		assert_eq!(n.value, -5.0);
		assert_eq!(n.unit, q.unit);
	}

	#[test]
	fn strip_unit_returns_dimensionless() {
		let q = Quantity::of(9.8, Unit::metre().div(&Unit::second().pow_i8(2)));
		let stripped = q.strip_unit();
		assert!(stripped.is_dimensionless());
		assert_eq!(stripped.value, 9.8);
	}
}
