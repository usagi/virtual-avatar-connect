//! SI 接頭辞（20 種）。
//!
//! 詳細設計: [`docs/roadmap/phase-ksi-dimensional-quantity-system.md`](../../../../docs/roadmap/phase-ksi-dimensional-quantity-system.md) §3.4。
//!
//! - Yotta (Y, 10^24) .. Yocto (y, 10^-24) までの 20 種 + `None` (10^0)。
//! - `Unit` は prefix を分離保持し、数値比較時に SI base へ正規化する。
//! - 加減算で両辺の prefix が異なる場合、**より大きい prefix を結果 prefix として継承**
//!   （作者指示: `1 km + 500 m = 1.5 km`、`100 ms + 50 μs = 100.05 ms`）。
//!
//! `kilogram` の特殊事情については [`super::unit`] の `BaseUnitId::Kilogram` 扱いを参照。

use std::fmt;

/// SI 接頭辞。20 倍率 + None。
///
/// `u32` / `i8` で扱える exponent を `exponent()` で返すが、内部計算は主に `factor()` の `f64` 乗数で行う。
/// Atto 以下で `f64` のダイナミックレンジが限界に近づくが、VAC の実用範囲では問題ない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SIPrefix {
	Yotta, // Y   10^24
	Zetta, // Z   10^21
	Exa,   // E   10^18
	Peta,  // P   10^15
	Tera,  // T   10^12
	Giga,  // G   10^9
	Mega,  // M   10^6
	Kilo,  // k   10^3
	Hecto, // h   10^2
	Deca,  // da  10^1
	/// 接頭辞なし（10^0）。多くの単位はこの値を持つ。
	None,
	Deci,  // d   10^-1
	Centi, // c   10^-2
	Milli, // m   10^-3
	Micro, // μ   10^-6
	Nano,  // n   10^-9
	Pico,  // p   10^-12
	Femto, // f   10^-15
	Atto,  // a   10^-18
	Zepto, // z   10^-21
	Yocto, // y   10^-24
}

impl SIPrefix {
	/// 10 の指数。`Yotta` → `24`、`Yocto` → `-24`、`None` → `0`。
	pub const fn exponent(&self) -> i32 {
		match self {
			SIPrefix::Yotta => 24,
			SIPrefix::Zetta => 21,
			SIPrefix::Exa => 18,
			SIPrefix::Peta => 15,
			SIPrefix::Tera => 12,
			SIPrefix::Giga => 9,
			SIPrefix::Mega => 6,
			SIPrefix::Kilo => 3,
			SIPrefix::Hecto => 2,
			SIPrefix::Deca => 1,
			SIPrefix::None => 0,
			SIPrefix::Deci => -1,
			SIPrefix::Centi => -2,
			SIPrefix::Milli => -3,
			SIPrefix::Micro => -6,
			SIPrefix::Nano => -9,
			SIPrefix::Pico => -12,
			SIPrefix::Femto => -15,
			SIPrefix::Atto => -18,
			SIPrefix::Zepto => -21,
			SIPrefix::Yocto => -24,
		}
	}

	/// 乗数。`Kilo` → `1e3`。
	pub fn factor(&self) -> f64 {
		10f64.powi(self.exponent())
	}

	/// Canonical シンボル。`Micro` → `"μ"`。
	pub const fn symbol(&self) -> &'static str {
		match self {
			SIPrefix::Yotta => "Y",
			SIPrefix::Zetta => "Z",
			SIPrefix::Exa => "E",
			SIPrefix::Peta => "P",
			SIPrefix::Tera => "T",
			SIPrefix::Giga => "G",
			SIPrefix::Mega => "M",
			SIPrefix::Kilo => "k",
			SIPrefix::Hecto => "h",
			SIPrefix::Deca => "da",
			SIPrefix::None => "",
			SIPrefix::Deci => "d",
			SIPrefix::Centi => "c",
			SIPrefix::Milli => "m",
			SIPrefix::Micro => "μ",
			SIPrefix::Nano => "n",
			SIPrefix::Pico => "p",
			SIPrefix::Femto => "f",
			SIPrefix::Atto => "a",
			SIPrefix::Zepto => "z",
			SIPrefix::Yocto => "y",
		}
	}

	/// シンボル文字列から prefix を引く。U+00B5 (MICRO SIGN) と U+03BC (GREEK SMALL LETTER MU) を両方受理。
	///
	/// `None`（空文字列）も受理する。未知の prefix は `None` (Option) を返す（呼び出し側で base unit 側とあわせて再解釈する）。
	pub fn from_symbol(sym: &str) -> Option<Self> {
		match sym {
			"" => Some(SIPrefix::None),
			"Y" => Some(SIPrefix::Yotta),
			"Z" => Some(SIPrefix::Zetta),
			"E" => Some(SIPrefix::Exa),
			"P" => Some(SIPrefix::Peta),
			"T" => Some(SIPrefix::Tera),
			"G" => Some(SIPrefix::Giga),
			"M" => Some(SIPrefix::Mega),
			"k" => Some(SIPrefix::Kilo),
			"h" => Some(SIPrefix::Hecto),
			"da" => Some(SIPrefix::Deca),
			"d" => Some(SIPrefix::Deci),
			"c" => Some(SIPrefix::Centi),
			"m" => Some(SIPrefix::Milli),
			"μ" | "µ" | "u" => Some(SIPrefix::Micro), // U+03BC, U+00B5, ASCII fallback
			"n" => Some(SIPrefix::Nano),
			"p" => Some(SIPrefix::Pico),
			"f" => Some(SIPrefix::Femto),
			"a" => Some(SIPrefix::Atto),
			"z" => Some(SIPrefix::Zepto),
			"y" => Some(SIPrefix::Yocto),
			_ => None,
		}
	}

	/// 加減算時の「より大きい方」継承規則。
	///
	/// 絶対値が大きい exponent を採るのではなく、**exponent が大きい方**（= 物理量として大きい prefix）を採る。
	/// 例: `Kilo (3)` vs `Milli (-3)` → `Kilo`。`Milli` vs `Micro` → `Milli`。
	pub fn max_of(a: SIPrefix, b: SIPrefix) -> SIPrefix {
		if a.exponent() >= b.exponent() {
			a
		} else {
			b
		}
	}

	/// 全 prefix バリアントの列挙順リスト（exponent 降順 = `Yotta` → `Yocto`）。
	/// Parser が最長一致を取る際に使う。
	pub const ALL_DESC: [SIPrefix; 21] = [
		SIPrefix::Yotta,
		SIPrefix::Zetta,
		SIPrefix::Exa,
		SIPrefix::Peta,
		SIPrefix::Tera,
		SIPrefix::Giga,
		SIPrefix::Mega,
		SIPrefix::Kilo,
		SIPrefix::Hecto,
		SIPrefix::Deca,
		SIPrefix::None,
		SIPrefix::Deci,
		SIPrefix::Centi,
		SIPrefix::Milli,
		SIPrefix::Micro,
		SIPrefix::Nano,
		SIPrefix::Pico,
		SIPrefix::Femto,
		SIPrefix::Atto,
		SIPrefix::Zepto,
		SIPrefix::Yocto,
	];
}

impl fmt::Display for SIPrefix {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.symbol())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn exponent_matches_sign_and_magnitude() {
		assert_eq!(SIPrefix::Yotta.exponent(), 24);
		assert_eq!(SIPrefix::Kilo.exponent(), 3);
		assert_eq!(SIPrefix::None.exponent(), 0);
		assert_eq!(SIPrefix::Milli.exponent(), -3);
		assert_eq!(SIPrefix::Yocto.exponent(), -24);
	}

	#[test]
	fn factor_is_ten_to_exponent() {
		assert_eq!(SIPrefix::Kilo.factor(), 1_000.0);
		assert_eq!(SIPrefix::None.factor(), 1.0);
		assert_eq!(SIPrefix::Milli.factor(), 0.001);
		assert!((SIPrefix::Micro.factor() - 1e-6).abs() < 1e-12);
	}

	#[test]
	fn symbol_uses_canonical_micro() {
		assert_eq!(SIPrefix::Micro.symbol(), "μ"); // U+03BC
		assert_eq!(SIPrefix::Kilo.symbol(), "k");
		assert_eq!(SIPrefix::None.symbol(), "");
	}

	#[test]
	fn from_symbol_accepts_both_micro_variants() {
		// U+03BC (Greek small letter mu)
		assert_eq!(SIPrefix::from_symbol("μ"), Some(SIPrefix::Micro));
		// U+00B5 (Micro sign)
		assert_eq!(SIPrefix::from_symbol("µ"), Some(SIPrefix::Micro));
		// ASCII fallback
		assert_eq!(SIPrefix::from_symbol("u"), Some(SIPrefix::Micro));
	}

	#[test]
	fn from_symbol_empty_is_none_prefix() {
		assert_eq!(SIPrefix::from_symbol(""), Some(SIPrefix::None));
	}

	#[test]
	fn from_symbol_unknown_returns_none_option() {
		assert_eq!(SIPrefix::from_symbol("xyz"), None);
	}

	#[test]
	fn max_of_prefers_larger_exponent() {
		assert_eq!(SIPrefix::max_of(SIPrefix::Kilo, SIPrefix::Milli), SIPrefix::Kilo);
		assert_eq!(SIPrefix::max_of(SIPrefix::Milli, SIPrefix::Micro), SIPrefix::Milli);
		assert_eq!(SIPrefix::max_of(SIPrefix::None, SIPrefix::None), SIPrefix::None);
		assert_eq!(SIPrefix::max_of(SIPrefix::Mega, SIPrefix::Kilo), SIPrefix::Mega);
	}

	#[test]
	fn all_desc_covers_every_variant() {
		assert_eq!(SIPrefix::ALL_DESC.len(), 21);
		let exps: Vec<i32> = SIPrefix::ALL_DESC.iter().map(|p| p.exponent()).collect();
		let mut sorted = exps.clone();
		sorted.sort_by(|a, b| b.cmp(a));
		assert_eq!(exps, sorted, "ALL_DESC must be in descending exponent order");
	}
}
