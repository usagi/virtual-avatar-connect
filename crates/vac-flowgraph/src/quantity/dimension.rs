//! Physical dimension: SI 7 基本次元 + Angle 疑似次元の 8 成分ベクトル。
//!
//! 詳細な設計判断は [`docs/roadmap/phase-ksi-dimensional-quantity-system.md`](../../../../docs/roadmap/phase-ksi-dimensional-quantity-system.md) §3.1 を参照。
//!
//! - 指数は `i8` で保持（実用範囲は -4..+4、`i8` にすることで `Dimension` が 8 bytes で済み packed 比較が可能）。
//! - `Dimensionless` は全成分 0。`is_dimensionless()` で判定する。
//! - Angle を第 8 次元として扱うのは Flowgraph 独自の型安全性ポリシー。SI では `rad` は dimensionless だが、
//!   `sin(45 deg as raw float)` のような事故を型で防ぐためにあえて次元化する。
//! - `+` 演算（加減算）は両辺 `Dimension` が完全一致している必要があり、`DimensionMismatch` エラーで
//!   `Quantity` 側から利用される。
//! - `*` / `/` は成分ごとに加減算される（`add` / `sub`）。`pow_i8` は成分ごとに乗算。
//!
//! `serde` ガードは `Serialize` / `Deserialize` を個別に書かず、人間可読な `canonical()` 文字列を介して
//! 外部化したい場合は [`super::parser`] 経由で行う（本型自体はシリアライズ非対応で良い）。

use std::fmt;

/// SI 7 基本次元 + Angle 疑似次元の 8 成分。各成分は `i8` の指数。
///
/// - `length` (L): metre
/// - `mass` (M): kilogram
/// - `time` (T): second
/// - `current` (I): ampere
/// - `temperature` (Θ): kelvin（absolute + delta 共通）
/// - `amount` (N): mole
/// - `luminous` (J): candela
/// - `angle` (A): 疑似次元、plane angle（rad / deg）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dimension {
	pub length: i8,
	pub mass: i8,
	pub time: i8,
	pub current: i8,
	pub temperature: i8,
	pub amount: i8,
	pub luminous: i8,
	pub angle: i8,
}

impl Dimension {
	/// 無次元（全成分 0）。
	pub const DIMENSIONLESS: Self = Self {
		length: 0,
		mass: 0,
		time: 0,
		current: 0,
		temperature: 0,
		amount: 0,
		luminous: 0,
		angle: 0,
	};

	pub const fn new(length: i8, mass: i8, time: i8, current: i8, temperature: i8, amount: i8, luminous: i8, angle: i8) -> Self {
		Self {
			length,
			mass,
			time,
			current,
			temperature,
			amount,
			luminous,
			angle,
		}
	}

	/// 全成分 0 かどうか。
	pub const fn is_dimensionless(&self) -> bool {
		self.length == 0
			&& self.mass == 0
			&& self.time == 0
			&& self.current == 0
			&& self.temperature == 0
			&& self.amount == 0
			&& self.luminous == 0
			&& self.angle == 0
	}

	/// 成分ごとの加算。`a * b` の次元合成に相当する。overflow は saturating。
	pub fn add(self, rhs: Self) -> Self {
		Self {
			length: self.length.saturating_add(rhs.length),
			mass: self.mass.saturating_add(rhs.mass),
			time: self.time.saturating_add(rhs.time),
			current: self.current.saturating_add(rhs.current),
			temperature: self.temperature.saturating_add(rhs.temperature),
			amount: self.amount.saturating_add(rhs.amount),
			luminous: self.luminous.saturating_add(rhs.luminous),
			angle: self.angle.saturating_add(rhs.angle),
		}
	}

	/// 成分ごとの減算。`a / b` の次元合成に相当する。overflow は saturating。
	pub fn sub(self, rhs: Self) -> Self {
		Self {
			length: self.length.saturating_sub(rhs.length),
			mass: self.mass.saturating_sub(rhs.mass),
			time: self.time.saturating_sub(rhs.time),
			current: self.current.saturating_sub(rhs.current),
			temperature: self.temperature.saturating_sub(rhs.temperature),
			amount: self.amount.saturating_sub(rhs.amount),
			luminous: self.luminous.saturating_sub(rhs.luminous),
			angle: self.angle.saturating_sub(rhs.angle),
		}
	}

	/// 成分ごとのスカラー乗算。`pow(a, n)` の次元合成に相当する。overflow は saturating。
	pub fn mul_scalar(self, n: i8) -> Self {
		Self {
			length: self.length.saturating_mul(n),
			mass: self.mass.saturating_mul(n),
			time: self.time.saturating_mul(n),
			current: self.current.saturating_mul(n),
			temperature: self.temperature.saturating_mul(n),
			amount: self.amount.saturating_mul(n),
			luminous: self.luminous.saturating_mul(n),
			angle: self.angle.saturating_mul(n),
		}
	}

	/// 符号反転。`1 / a` の次元合成に相当する。
	pub fn invert(self) -> Self {
		Self::DIMENSIONLESS.sub(self)
	}

	/// 全成分が `divisor` で割り切れるか。`sqrt` / `cbrt` が許容されるかの判定用。
	///
	/// 例: `Dimension::LENGTH_SQUARED.all_divisible_by(2)` → `true`（各成分 `[2,0,0,...] / 2 = [1,0,0,...]`）。
	pub fn all_divisible_by(&self, divisor: i8) -> bool {
		if divisor == 0 {
			return false;
		}
		self.length % divisor == 0
			&& self.mass % divisor == 0
			&& self.time % divisor == 0
			&& self.current % divisor == 0
			&& self.temperature % divisor == 0
			&& self.amount % divisor == 0
			&& self.luminous % divisor == 0
			&& self.angle % divisor == 0
	}

	/// 成分の整数スカラー除算（`all_divisible_by` で保護された上で使う）。
	pub fn div_scalar(self, divisor: i8) -> Self {
		debug_assert!(divisor != 0);
		Self {
			length: self.length / divisor,
			mass: self.mass / divisor,
			time: self.time / divisor,
			current: self.current / divisor,
			temperature: self.temperature / divisor,
			amount: self.amount / divisor,
			luminous: self.luminous / divisor,
			angle: self.angle / divisor,
		}
	}

	// --- 代表的 Dimension 定数（テスト・unit.rs 側で頻用） ---------------------

	pub const LENGTH: Self = Self::new(1, 0, 0, 0, 0, 0, 0, 0);
	pub const MASS: Self = Self::new(0, 1, 0, 0, 0, 0, 0, 0);
	pub const TIME: Self = Self::new(0, 0, 1, 0, 0, 0, 0, 0);
	pub const CURRENT: Self = Self::new(0, 0, 0, 1, 0, 0, 0, 0);
	pub const TEMPERATURE: Self = Self::new(0, 0, 0, 0, 1, 0, 0, 0);
	pub const AMOUNT: Self = Self::new(0, 0, 0, 0, 0, 1, 0, 0);
	pub const LUMINOUS: Self = Self::new(0, 0, 0, 0, 0, 0, 1, 0);
	pub const ANGLE: Self = Self::new(0, 0, 0, 0, 0, 0, 0, 1);

	pub const FREQUENCY: Self = Self::new(0, 0, -1, 0, 0, 0, 0, 0); // Hz = 1/s
	pub const VELOCITY: Self = Self::new(1, 0, -1, 0, 0, 0, 0, 0); // m/s
	pub const ACCELERATION: Self = Self::new(1, 0, -2, 0, 0, 0, 0, 0); // m/s^2
	pub const FORCE: Self = Self::new(1, 1, -2, 0, 0, 0, 0, 0); // N
	pub const PRESSURE: Self = Self::new(-1, 1, -2, 0, 0, 0, 0, 0); // Pa
	pub const ENERGY: Self = Self::new(2, 1, -2, 0, 0, 0, 0, 0); // J
	pub const POWER: Self = Self::new(2, 1, -3, 0, 0, 0, 0, 0); // W
	pub const CHARGE: Self = Self::new(0, 0, 1, 1, 0, 0, 0, 0); // C = A·s
	pub const VOLTAGE: Self = Self::new(2, 1, -3, -1, 0, 0, 0, 0); // V
	pub const RESISTANCE: Self = Self::new(2, 1, -3, -2, 0, 0, 0, 0); // Ω
	pub const CAPACITANCE: Self = Self::new(-2, -1, 4, 2, 0, 0, 0, 0); // F
	pub const MAGNETIC_FLUX_DENSITY: Self = Self::new(0, 1, -2, -1, 0, 0, 0, 0); // T
	pub const INDUCTANCE: Self = Self::new(2, 1, -2, -2, 0, 0, 0, 0); // H
	pub const ILLUMINANCE: Self = Self::new(-2, 0, 0, 0, 0, 0, 1, 0); // lx

	/// Canonical 文字列表記（`"L·T^-2"` 形式）。`is_dimensionless()` のときは `"1"`。
	///
	/// ログ・デバッグ用。厳密なパースラウンドトリップは保証しない（パーサは [`super::parser`] 側）。
	pub fn canonical(&self) -> String {
		if self.is_dimensionless() {
			return "1".to_string();
		}
		let parts = [
			("L", self.length),
			("M", self.mass),
			("T", self.time),
			("I", self.current),
			("Θ", self.temperature),
			("N", self.amount),
			("J", self.luminous),
			("A", self.angle),
		];
		let mut out = String::new();
		let mut first = true;
		for (sym, exp) in parts {
			if exp == 0 {
				continue;
			}
			if !first {
				out.push('·');
			}
			first = false;
			out.push_str(sym);
			if exp != 1 {
				out.push_str(&format!("^{exp}"));
			}
		}
		out
	}
}

impl fmt::Display for Dimension {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.canonical())
	}
}

/// 次元不一致エラー。加減算・convert で発生する。`Quantity` 側と `flowgraph.unit.convert`
/// ノードから `NodeExecError` に包まれて流れる前提。
#[derive(Debug, Clone, thiserror::Error)]
#[error("Dimension mismatch: lhs={lhs}, rhs={rhs}{}",
	match op {
		Some(op) => format!(" (op: {op})"),
		None => String::new(),
	})]
pub struct DimensionMismatch {
	pub lhs: Dimension,
	pub rhs: Dimension,
	pub op: Option<&'static str>,
}

impl DimensionMismatch {
	pub fn new(lhs: Dimension, rhs: Dimension) -> Self {
		Self { lhs, rhs, op: None }
	}
	pub fn with_op(lhs: Dimension, rhs: Dimension, op: &'static str) -> Self {
		Self { lhs, rhs, op: Some(op) }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dimensionless_is_all_zero() {
		assert!(Dimension::DIMENSIONLESS.is_dimensionless());
		assert_eq!(Dimension::DIMENSIONLESS.canonical(), "1");
	}

	#[test]
	fn length_has_only_l_component() {
		assert_eq!(Dimension::LENGTH.length, 1);
		assert_eq!(Dimension::LENGTH.canonical(), "L");
		assert!(!Dimension::LENGTH.is_dimensionless());
	}

	#[test]
	fn acceleration_composes_from_velocity_over_time() {
		// m/s^2 == (m/s) / s == velocity - time
		let computed = Dimension::VELOCITY.sub(Dimension::TIME);
		assert_eq!(computed, Dimension::ACCELERATION);
	}

	#[test]
	fn force_composes_from_mass_times_acceleration() {
		// N == kg · m/s^2 == MASS + ACCELERATION
		let computed = Dimension::MASS.add(Dimension::ACCELERATION);
		assert_eq!(computed, Dimension::FORCE);
	}

	#[test]
	fn energy_composes_from_force_times_length() {
		// J == N · m == FORCE + LENGTH
		let computed = Dimension::FORCE.add(Dimension::LENGTH);
		assert_eq!(computed, Dimension::ENERGY);
	}

	#[test]
	fn power_composes_from_energy_over_time() {
		// W == J/s == ENERGY - TIME
		let computed = Dimension::ENERGY.sub(Dimension::TIME);
		assert_eq!(computed, Dimension::POWER);
	}

	#[test]
	fn voltage_composes_from_power_over_current() {
		// V == W/A == POWER - CURRENT
		let computed = Dimension::POWER.sub(Dimension::CURRENT);
		assert_eq!(computed, Dimension::VOLTAGE);
	}

	#[test]
	fn resistance_composes_from_voltage_over_current() {
		// Ω == V/A
		let computed = Dimension::VOLTAGE.sub(Dimension::CURRENT);
		assert_eq!(computed, Dimension::RESISTANCE);
	}

	#[test]
	fn frequency_is_inverse_time() {
		// Hz == 1/s == invert(T)
		assert_eq!(Dimension::TIME.invert(), Dimension::FREQUENCY);
	}

	#[test]
	fn mul_scalar_squares_dimension() {
		// area == length^2 == LENGTH * 2
		let area = Dimension::LENGTH.mul_scalar(2);
		assert_eq!(area.length, 2);
		assert_eq!(area.canonical(), "L^2");
	}

	#[test]
	fn add_then_sub_returns_identity() {
		let roundtrip = Dimension::VELOCITY.add(Dimension::ACCELERATION).sub(Dimension::ACCELERATION);
		assert_eq!(roundtrip, Dimension::VELOCITY);
	}

	#[test]
	fn all_divisible_by_detects_sqrt_eligibility() {
		// area = L^2 can take sqrt (divide all components by 2)
		let area = Dimension::LENGTH.mul_scalar(2);
		assert!(area.all_divisible_by(2));
		// L^3 cannot take square root (non-integer exponent)
		let volume = Dimension::LENGTH.mul_scalar(3);
		assert!(!volume.all_divisible_by(2));
		// dimensionless is always divisible
		assert!(Dimension::DIMENSIONLESS.all_divisible_by(2));
	}

	#[test]
	fn div_scalar_is_sqrt_of_squared() {
		let area = Dimension::LENGTH.mul_scalar(2);
		assert_eq!(area.div_scalar(2), Dimension::LENGTH);
	}

	#[test]
	fn canonical_composite_dimension() {
		// acceleration = L·T^-2
		assert_eq!(Dimension::ACCELERATION.canonical(), "L·T^-2");
		// force = L·M·T^-2
		assert_eq!(Dimension::FORCE.canonical(), "L·M·T^-2");
	}

	#[test]
	fn angle_is_independent_pseudo_dimension() {
		// Angle は他の次元と直交
		assert!(!Dimension::ANGLE.is_dimensionless());
		assert_ne!(Dimension::ANGLE, Dimension::DIMENSIONLESS);
		assert_ne!(Dimension::ANGLE, Dimension::LENGTH);
	}

	#[test]
	fn saturating_overflow_on_mul_scalar() {
		// very large exponent saturates at i8::MAX
		let huge = Dimension::LENGTH.mul_scalar(i8::MAX);
		assert_eq!(huge.length, i8::MAX);
	}

	#[test]
	fn dimension_mismatch_error_display() {
		let err = DimensionMismatch::with_op(Dimension::LENGTH, Dimension::TIME, "+");
		let msg = err.to_string();
		assert!(msg.contains("L"));
		assert!(msg.contains("T"));
		assert!(msg.contains("+"));
	}
}
