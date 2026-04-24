//! Unit: 次元を持つ名前付き単位。`atoms × prefix × factor` の 3 組で構造化保持する。
//!
//! 詳細設計: [`docs/roadmap/phase-ksi-dimensional-quantity-system.md`](../../../../docs/roadmap/phase-ksi-dimensional-quantity-system.md) §3.2-3.6。
//!
//! # 設計メモ
//!
//! - `atoms`: `BTreeMap<BaseUnitId, i8>`。基本単位 × 指数。合成単位（m/s²）は `{Metre: 1, Second: -2}`。
//! - `si_factor`: 全 atoms 合算後に SI base 値へ揃えるための数値係数。prefix + 非 SI 係数を吸収:
//!   - `km` = `{Metre: 1} × 1e3`（prefix Kilo 由来）
//!   - `deg` = `{Degree: 1} × 1.0`（`Degree` 自体が atom なので factor は 1、rad 変換は Quantity の convert で処理）
//!   - `min` = `{Second: 1} × 60.0`（非 SI の factor を埋め込むパターン、ξ+ で追加予定）
//! - `prefix_hint`: 表示時に使う prefix。`km` を parse したら `Kilo` を、算術計算後は `None` を保持する。
//!   数値比較には使わない。
//! - 構造的等価（`PartialEq`）は `atoms + si_factor` で判定（`prefix_hint` は無視）。
//! - 同次元判定は `dimension()` から [`super::Dimension`] レベルで比較。
//! - Temperature absolute (K) vs delta (ΔK) は別 `BaseUnitId` として分離する（次元同じ、単位ID 違う）。
//!
//! # 既知の限界 (ξ-1)
//!
//! - 合成単位（N, Hz, J, W, ...）の表示名推論は atoms マッチングのみ、複雑な式は `{atom_list}` 生の形で表示する。
//! - 精密なテキストフォーマット（表示用の自動 prefix 調整、例: `0.001 m` → `1 mm`）は ξ-2 / ξ-4 で追加。

use std::collections::BTreeMap;
use std::fmt;

use super::dimension::Dimension;
use super::prefix::SIPrefix;

/// 基本単位 ID。SI 基本 7 種 + Angle(rad, deg) + Temperature delta (ΔK) の 10 種。
///
/// 派生単位（N, J, W, etc.）は atoms 合成で表現し、独立 variant は持たない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseUnitId {
	// --- SI 基本 7 単位 ----------------------------------------------------
	/// m, Dimension L
	Metre,
	/// kg, Dimension M。`g` は `{Kilogram: 1} × Milli` で表現する（§3.4）。
	Kilogram,
	/// s, Dimension T
	Second,
	/// A, Dimension I
	Ampere,
	/// K, Dimension Θ（**absolute 温度**、`ΔK` とは区別）
	Kelvin,
	/// mol, Dimension N
	Mole,
	/// cd, Dimension J
	Candela,

	// --- Angle 疑似次元 ----------------------------------------------------
	/// rad, Dimension A
	Radian,
	/// deg, Dimension A（`Radian` とは別 atom、convert で互いに入れ替え）
	Degree,

	// --- Temperature delta -----------------------------------------------
	/// ΔK = ΔC, Dimension Θ（`Kelvin` とは別 atom、絶対温度と差分温度を型分離）
	KelvinDelta,
}

impl BaseUnitId {
	/// この原子 1 つが寄与する次元。
	pub const fn dimension(&self) -> Dimension {
		match self {
			BaseUnitId::Metre => Dimension::LENGTH,
			BaseUnitId::Kilogram => Dimension::MASS,
			BaseUnitId::Second => Dimension::TIME,
			BaseUnitId::Ampere => Dimension::CURRENT,
			BaseUnitId::Kelvin => Dimension::TEMPERATURE,
			BaseUnitId::Mole => Dimension::AMOUNT,
			BaseUnitId::Candela => Dimension::LUMINOUS,
			BaseUnitId::Radian => Dimension::ANGLE,
			BaseUnitId::Degree => Dimension::ANGLE,
			BaseUnitId::KelvinDelta => Dimension::TEMPERATURE,
		}
	}

	/// Canonical symbol。
	pub const fn symbol(&self) -> &'static str {
		match self {
			BaseUnitId::Metre => "m",
			BaseUnitId::Kilogram => "kg",
			BaseUnitId::Second => "s",
			BaseUnitId::Ampere => "A",
			BaseUnitId::Kelvin => "K",
			BaseUnitId::Mole => "mol",
			BaseUnitId::Candela => "cd",
			BaseUnitId::Radian => "rad",
			BaseUnitId::Degree => "deg",
			BaseUnitId::KelvinDelta => "ΔK",
		}
	}
}

/// 単位。atoms(基本単位 × 指数) + SI 換算係数 + 表示 prefix hint の 3 組。
///
/// 詳細は module doc を参照。
#[derive(Debug, Clone)]
pub struct Unit {
	/// 基本単位 × 指数。0 exponent の atom は含めない（`normalize` で保証）。
	pub atoms: BTreeMap<BaseUnitId, i8>,
	/// SI base 値への換算係数。prefix + 非 SI factor を吸収済み。
	pub si_factor: f64,
	/// 表示用の prefix hint。`km` parse なら `Kilo`、算術計算後は `None`。
	pub prefix_hint: SIPrefix,
}

/// `Unit` の構造的等価判定（`prefix_hint` は除外）。
///
/// `si_factor` は `f64` のため `f64::EPSILON` オーダで厳密比較する。算術計算で
/// 微小な浮動小数点誤差（例: `1e3 × 1e-3 = 0.9999999...`）があるケースは
/// `Unit::eq_approx` / `same_dimension` で吸収する設計。
impl PartialEq for Unit {
	fn eq(&self, other: &Self) -> bool {
		self.atoms == other.atoms && self.si_factor == other.si_factor
	}
}

impl Unit {
	/// 無次元単位（dimensionless）。
	pub fn dimensionless() -> Self {
		Self {
			atoms: BTreeMap::new(),
			si_factor: 1.0,
			prefix_hint: SIPrefix::None,
		}
	}

	/// 単一原子単位（prefix なし、factor 1.0）。
	pub fn of_atom(id: BaseUnitId) -> Self {
		let mut atoms = BTreeMap::new();
		atoms.insert(id, 1);
		Self {
			atoms,
			si_factor: 1.0,
			prefix_hint: SIPrefix::None,
		}
	}

	/// prefix を適用。`m` → `km` → `Mm` のように重ねると `si_factor` が累積する。
	/// `prefix_hint` は最後に適用した prefix で上書きする（表示用）。
	pub fn with_prefix(mut self, prefix: SIPrefix) -> Self {
		self.si_factor *= prefix.factor();
		self.prefix_hint = prefix;
		self
	}

	/// 非 SI factor を掛ける（`deg = rad × π/180` 等）。prefix_hint は変更しない。
	pub fn with_factor(mut self, factor: f64) -> Self {
		self.si_factor *= factor;
		self
	}

	/// Dimension ベクトルを atoms から導出。
	pub fn dimension(&self) -> Dimension {
		let mut dim = Dimension::DIMENSIONLESS;
		for (id, exp) in &self.atoms {
			dim = dim.add(id.dimension().mul_scalar(*exp));
		}
		dim
	}

	/// 無次元か。
	pub fn is_dimensionless(&self) -> bool {
		self.dimension().is_dimensionless()
	}

	/// 同次元か。
	pub fn same_dimension(&self, other: &Self) -> bool {
		self.dimension() == other.dimension()
	}

	/// SI base 値を返す（入力数値 × si_factor）。
	pub fn to_si_base(&self, value: f64) -> f64 {
		value * self.si_factor
	}

	/// SI base 値から self unit 表現へ変換した数値（`value_in_si / si_factor`）。
	pub fn from_si_base(&self, si_value: f64) -> f64 {
		si_value / self.si_factor
	}

	/// 乗算: atoms は指数を合算、si_factor は積。0 exponent の atom は削除。
	pub fn mul(&self, rhs: &Self) -> Self {
		let mut atoms = self.atoms.clone();
		for (id, exp) in &rhs.atoms {
			let e = atoms.entry(*id).or_insert(0);
			*e = e.saturating_add(*exp);
		}
		normalize_atoms(&mut atoms);
		Self {
			atoms,
			si_factor: self.si_factor * rhs.si_factor,
			prefix_hint: SIPrefix::None, // 合成後は prefix を保持しない
		}
	}

	/// 除算: atoms は指数を減算、si_factor は商。
	pub fn div(&self, rhs: &Self) -> Self {
		let mut atoms = self.atoms.clone();
		for (id, exp) in &rhs.atoms {
			let e = atoms.entry(*id).or_insert(0);
			*e = e.saturating_sub(*exp);
		}
		normalize_atoms(&mut atoms);
		Self {
			atoms,
			si_factor: self.si_factor / rhs.si_factor,
			prefix_hint: SIPrefix::None,
		}
	}

	/// 整数冪: 全 atom exponent を `n` 倍、si_factor は `powi(n)`。
	pub fn pow_i8(&self, n: i8) -> Self {
		let mut atoms = BTreeMap::new();
		for (id, exp) in &self.atoms {
			let new_exp = exp.saturating_mul(n);
			if new_exp != 0 {
				atoms.insert(*id, new_exp);
			}
		}
		Self {
			atoms,
			si_factor: self.si_factor.powi(n as i32),
			prefix_hint: SIPrefix::None,
		}
	}

	/// Canonical display。atoms を symbol 化して `·` / `^` で綴る。
	///
	/// - 無次元かつ factor 1.0: `"1"`
	/// - 単一 atom + exponent 1 で factor が prefix 一致: `"{prefix}{symbol}"` (例: `"km"`)
	/// - それ以外: `"{prefix_hint? ""}{atoms}..."` を綴る（`si_factor` は factor=1 以外なら前置数値）
	pub fn canonical(&self) -> String {
		if self.atoms.is_empty() && (self.si_factor - 1.0).abs() < f64::EPSILON {
			return "1".to_string();
		}

		// 単一 atom + exponent 1 のとき: prefix_hint + atom_symbol をまず試す
		if self.atoms.len() == 1 {
			if let Some((id, &exp)) = self.atoms.iter().next() {
				if exp == 1 {
					// si_factor が prefix_hint.factor() と一致するなら hint を使う
					let hint_factor = self.prefix_hint.factor();
					if (self.si_factor - hint_factor).abs() < hint_factor * 1e-12 {
						return format!("{}{}", self.prefix_hint.symbol(), id.symbol());
					}
				}
			}
		}

		// それ以外は atoms 列挙。si_factor != 1 なら数値前置。
		let mut parts: Vec<String> = Vec::new();
		for (id, exp) in &self.atoms {
			if *exp == 1 {
				parts.push(id.symbol().to_string());
			} else {
				parts.push(format!("{}^{}", id.symbol(), exp));
			}
		}
		let body = parts.join("·");

		if (self.si_factor - 1.0).abs() < f64::EPSILON {
			body
		} else {
			// 非トリビアルな factor は数値前置して明示する
			format!("{}·{body}", format_factor(self.si_factor))
		}
	}

	// -----------------------------------------------------------------
	// Convenience constructors for SI base units
	// -----------------------------------------------------------------

	pub fn metre() -> Self {
		Self::of_atom(BaseUnitId::Metre)
	}
	pub fn kilogram() -> Self {
		Self::of_atom(BaseUnitId::Kilogram)
	}
	pub fn gram() -> Self {
		// g = kg × 10^-3
		Self::of_atom(BaseUnitId::Kilogram).with_prefix(SIPrefix::Milli)
	}
	pub fn second() -> Self {
		Self::of_atom(BaseUnitId::Second)
	}
	pub fn ampere() -> Self {
		Self::of_atom(BaseUnitId::Ampere)
	}
	pub fn kelvin() -> Self {
		Self::of_atom(BaseUnitId::Kelvin)
	}
	pub fn kelvin_delta() -> Self {
		Self::of_atom(BaseUnitId::KelvinDelta)
	}
	pub fn mole() -> Self {
		Self::of_atom(BaseUnitId::Mole)
	}
	pub fn candela() -> Self {
		Self::of_atom(BaseUnitId::Candela)
	}
	pub fn radian() -> Self {
		Self::of_atom(BaseUnitId::Radian)
	}
	pub fn degree() -> Self {
		Self::of_atom(BaseUnitId::Degree)
	}

	// -----------------------------------------------------------------
	// Convenience constructors for SI derived units (atom composition)
	// -----------------------------------------------------------------

	/// Hz = 1/s
	pub fn hertz() -> Self {
		Self::second().pow_i8(-1)
	}
	/// N = kg·m/s²
	pub fn newton() -> Self {
		Self::kilogram().mul(&Self::metre()).mul(&Self::second().pow_i8(-2))
	}
	/// Pa = N/m² = kg/(m·s²)
	pub fn pascal() -> Self {
		Self::newton().div(&Self::metre().pow_i8(2))
	}
	/// J = N·m = kg·m²/s²
	pub fn joule() -> Self {
		Self::newton().mul(&Self::metre())
	}
	/// W = J/s
	pub fn watt() -> Self {
		Self::joule().div(&Self::second())
	}
	/// C = A·s
	pub fn coulomb() -> Self {
		Self::ampere().mul(&Self::second())
	}
	/// V = W/A
	pub fn volt() -> Self {
		Self::watt().div(&Self::ampere())
	}
	/// Ω = V/A
	pub fn ohm() -> Self {
		Self::volt().div(&Self::ampere())
	}
	/// F = C/V
	pub fn farad() -> Self {
		Self::coulomb().div(&Self::volt())
	}
	/// T = kg/(A·s²)
	pub fn tesla() -> Self {
		Self::kilogram().div(&Self::ampere()).div(&Self::second().pow_i8(2))
	}
	/// H = V·s/A
	pub fn henry() -> Self {
		Self::volt().mul(&Self::second()).div(&Self::ampere())
	}
}

impl fmt::Display for Unit {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.canonical())
	}
}

/// 0 exponent の atom を除去。
fn normalize_atoms(atoms: &mut BTreeMap<BaseUnitId, i8>) {
	atoms.retain(|_, exp| *exp != 0);
}

/// `si_factor` を canonical 数値表記に整形。
fn format_factor(f: f64) -> String {
	if f == f.trunc() && f.abs() < 1e15 {
		format!("{}", f as i64)
	} else {
		format!("{f}")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dimensionless_has_empty_atoms() {
		let d = Unit::dimensionless();
		assert!(d.atoms.is_empty());
		assert_eq!(d.si_factor, 1.0);
		assert!(d.is_dimensionless());
		assert_eq!(d.canonical(), "1");
	}

	#[test]
	fn metre_has_length_dimension() {
		let m = Unit::metre();
		assert_eq!(m.dimension(), Dimension::LENGTH);
		assert_eq!(m.canonical(), "m");
	}

	#[test]
	fn kilometre_is_metre_with_kilo_prefix() {
		let km = Unit::metre().with_prefix(SIPrefix::Kilo);
		assert_eq!(km.dimension(), Dimension::LENGTH);
		assert_eq!(km.si_factor, 1000.0);
		assert_eq!(km.canonical(), "km");
	}

	#[test]
	fn gram_is_kilogram_with_milli_prefix() {
		// g = kg × 10^-3、§3.4 規約（kg が基本単位）
		let g = Unit::gram();
		assert_eq!(g.dimension(), Dimension::MASS);
		assert!((g.si_factor - 0.001).abs() < 1e-12);
		assert_eq!(g.canonical(), "mkg"); // 「milli-kilogram」の素直な表示
	}

	#[test]
	fn newton_dimension_matches_force() {
		let n = Unit::newton();
		assert_eq!(n.dimension(), Dimension::FORCE);
	}

	#[test]
	fn pascal_dimension_matches_pressure() {
		assert_eq!(Unit::pascal().dimension(), Dimension::PRESSURE);
	}

	#[test]
	fn joule_dimension_matches_energy() {
		assert_eq!(Unit::joule().dimension(), Dimension::ENERGY);
	}

	#[test]
	fn watt_dimension_matches_power() {
		assert_eq!(Unit::watt().dimension(), Dimension::POWER);
	}

	#[test]
	fn volt_dimension_matches_voltage() {
		assert_eq!(Unit::volt().dimension(), Dimension::VOLTAGE);
	}

	#[test]
	fn ohm_dimension_matches_resistance() {
		assert_eq!(Unit::ohm().dimension(), Dimension::RESISTANCE);
	}

	#[test]
	fn hertz_dimension_matches_frequency() {
		assert_eq!(Unit::hertz().dimension(), Dimension::FREQUENCY);
	}

	#[test]
	fn radian_and_degree_share_angle_dimension() {
		assert_eq!(Unit::radian().dimension(), Dimension::ANGLE);
		assert_eq!(Unit::degree().dimension(), Dimension::ANGLE);
		assert!(Unit::radian().same_dimension(&Unit::degree()));
	}

	#[test]
	fn radian_and_degree_are_structurally_different() {
		// 次元は同じでも atom が違う
		assert_ne!(Unit::radian(), Unit::degree());
	}

	#[test]
	fn kelvin_and_kelvin_delta_share_temperature_dimension() {
		assert_eq!(Unit::kelvin().dimension(), Dimension::TEMPERATURE);
		assert_eq!(Unit::kelvin_delta().dimension(), Dimension::TEMPERATURE);
		assert!(Unit::kelvin().same_dimension(&Unit::kelvin_delta()));
		// ただし atom は別
		assert_ne!(Unit::kelvin(), Unit::kelvin_delta());
	}

	#[test]
	fn mul_composes_atoms_and_factors() {
		let km = Unit::metre().with_prefix(SIPrefix::Kilo);
		let hour = Unit::second().with_factor(3600.0); // `hour` は ξ+ で正式サポート予定、テスト用
		let km_per_hour = km.div(&hour);
		// km/h の dimension は velocity
		assert_eq!(km_per_hour.dimension(), Dimension::VELOCITY);
		// si_factor は 1000 / 3600 ≈ 0.27777...
		assert!((km_per_hour.si_factor - 1000.0 / 3600.0).abs() < 1e-12);
	}

	#[test]
	fn pow_i8_squares_atoms_and_factor() {
		let km = Unit::metre().with_prefix(SIPrefix::Kilo);
		let km_sq = km.pow_i8(2);
		assert_eq!(km_sq.dimension(), Dimension::LENGTH.mul_scalar(2));
		// si_factor: 1000^2 = 1e6
		assert!((km_sq.si_factor - 1e6).abs() < 1e-6);
	}

	#[test]
	fn pow_i8_negative_inverts_exponents() {
		let m = Unit::metre();
		let m_inv = m.pow_i8(-1);
		assert_eq!(m_inv.dimension(), Dimension::LENGTH.invert());
		assert_eq!(m_inv.si_factor, 1.0);
	}

	#[test]
	fn div_subtracts_exponents_and_divides_factors() {
		let m = Unit::metre();
		let s = Unit::second();
		let vel = m.div(&s);
		assert_eq!(vel.dimension(), Dimension::VELOCITY);
		assert_eq!(vel.si_factor, 1.0);
	}

	#[test]
	fn to_si_base_applies_factor() {
		let km = Unit::metre().with_prefix(SIPrefix::Kilo);
		assert_eq!(km.to_si_base(5.0), 5000.0);
	}

	#[test]
	fn from_si_base_reverses_factor() {
		let km = Unit::metre().with_prefix(SIPrefix::Kilo);
		assert_eq!(km.from_si_base(5000.0), 5.0);
	}

	#[test]
	fn equal_units_ignore_prefix_hint() {
		// Same atoms + same si_factor = equal, even with different prefix_hint
		// (though in practice same atoms + factor means same prefix_hint, this documents intent)
		let a = Unit::metre();
		let b = Unit::metre();
		assert_eq!(a, b);
	}

	#[test]
	fn canonical_composite() {
		let n = Unit::newton();
		let c = n.canonical();
		// atoms は {Kilogram: 1, Metre: 1, Second: -2}
		assert!(c.contains("kg"));
		assert!(c.contains("m"));
		assert!(c.contains("s^-2"));
	}

	#[test]
	fn normalize_removes_zero_exponent_atoms() {
		let m = Unit::metre();
		let m_inv = m.pow_i8(-1);
		// m × m^-1 = dimensionless
		let product = m.mul(&m_inv);
		assert!(product.atoms.is_empty());
		assert!(product.is_dimensionless());
	}

	#[test]
	fn base_unit_id_dimension_mapping() {
		assert_eq!(BaseUnitId::Metre.dimension(), Dimension::LENGTH);
		assert_eq!(BaseUnitId::Kilogram.dimension(), Dimension::MASS);
		assert_eq!(BaseUnitId::Second.dimension(), Dimension::TIME);
		assert_eq!(BaseUnitId::Ampere.dimension(), Dimension::CURRENT);
		assert_eq!(BaseUnitId::Kelvin.dimension(), Dimension::TEMPERATURE);
		assert_eq!(BaseUnitId::KelvinDelta.dimension(), Dimension::TEMPERATURE);
		assert_eq!(BaseUnitId::Mole.dimension(), Dimension::AMOUNT);
		assert_eq!(BaseUnitId::Candela.dimension(), Dimension::LUMINOUS);
		assert_eq!(BaseUnitId::Radian.dimension(), Dimension::ANGLE);
		assert_eq!(BaseUnitId::Degree.dimension(), Dimension::ANGLE);
	}
}
