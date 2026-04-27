//! Unit string parser: converts strings like "m/s^2", "kg\u{00B7}m/s^2",
//! "\u{03BC}s", "Hz", "\u{0394}K" into a [`Unit`].
//!
//! See `docs/roadmap/phase-ksi-dimensional-quantity-system.md` sections 3.7 and 6.2
//! for the full design rationale.
//!
//! # Grammar (xi-1 baseline)
//!
//! ```text
//! unit_expr := term (separator term)*
//! separator := '/' | U+00B7 | '*' | ' '    // '/' switches following terms to denominator
//! term      := (prefix? atom | derived) exponent?
//! exponent  := '^' signed_int | superscript+
//! ```
//!
//! - Superscripts U+2070..U+2079 and U+207B are normalized to ASCII digits / minus.
//! - Micro prefix accepts three variants: U+03BC (Greek mu), U+00B5 (MICRO SIGN), and ASCII "u".
//! - "kg" is the SI base unit for mass; "g" alone expands to Kilogram x Milli.
//! - U+0394 + "K" (delta-K) and ASCII "dK" both map to `BaseUnitId::KelvinDelta`.
//! - Derived units Hz N Pa J W C V ohm (U+03A9) F T H accept prefixes via
//!   `lookup_prefixable_base`; the unprefixed forms go through `lookup_derived`.
//! - Parentheses are NOT supported in xi-1 (reserved for xi-2+).

use super::prefix::SIPrefix;
use super::unit::Unit;
#[cfg(test)]
use super::unit::BaseUnitId;
#[cfg(test)]
use std::collections::BTreeMap;

#[cfg(test)]
use super::BaseUnitId;
#[cfg(test)]
use std::collections::BTreeMap;

/// Errors produced while parsing a unit string.
#[derive(Debug, Clone, thiserror::Error)]
pub enum UnitParseError {
	#[error("unit string is empty")]
	Empty,
	#[error("unknown unit token: {token:?}")]
	UnknownToken { token: String },
	#[error("invalid exponent: {raw:?}")]
	InvalidExponent { raw: String },
	#[error("parentheses are not supported in xi-1: {input:?}")]
	UnsupportedParenthesis { input: String },
}

/// Parse a unit string into a [`Unit`].
pub fn parse_unit(input: &str) -> Result<Unit, UnitParseError> {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return Err(UnitParseError::Empty);
	}
	if trimmed.contains('(') || trimmed.contains(')') {
		return Err(UnitParseError::UnsupportedParenthesis {
			input: trimmed.to_string(),
		});
	}
	let normalized = normalize_superscripts(trimmed);

	let mut running = Unit::dimensionless();
	let segments = normalized.split('/');
	let mut first = true;
	for seg in segments {
		let is_numerator = first;
		first = false;
		let seg_unit = parse_product_segment(seg)?;
		if is_numerator {
			running = running.mul(&seg_unit);
		} else {
			running = running.mul(&seg_unit.pow_i8(-1));
		}
	}
	Ok(running)
}

fn parse_product_segment(seg: &str) -> Result<Unit, UnitParseError> {
	let trimmed = seg.trim();
	if trimmed.is_empty() {
		return Ok(Unit::dimensionless());
	}
	let mut unit = Unit::dimensionless();
	for token in split_product(trimmed) {
		let term = parse_term(&token)?;
		unit = unit.mul(&term);
	}
	Ok(unit)
}

fn split_product(s: &str) -> Vec<String> {
	s.split(|c: char| c == '\u{00B7}' || c == '*' || c == ' ' || c == '\t')
		.filter(|t| !t.is_empty())
		.map(|t| t.to_string())
		.collect()
}

fn parse_term(token: &str) -> Result<Unit, UnitParseError> {
	if token == "1" {
		return Ok(Unit::dimensionless());
	}
	let (atom_part, exp) = split_exponent(token)?;
	let base_unit = parse_atom_with_prefix(atom_part)?;
	if exp == 1 {
		Ok(base_unit)
	} else {
		Ok(base_unit.pow_i8(exp))
	}
}

fn split_exponent(token: &str) -> Result<(&str, i8), UnitParseError> {
	if let Some(idx) = token.find('^') {
		let atom = &token[..idx];
		let exp_str = &token[idx + 1..];
		let exp = exp_str
			.parse::<i8>()
			.map_err(|_| UnitParseError::InvalidExponent { raw: exp_str.to_string() })?;
		Ok((atom, exp))
	} else {
		Ok((token, 1))
	}
}

fn parse_atom_with_prefix(s: &str) -> Result<Unit, UnitParseError> {
	if let Some(u) = lookup_bare_atom(s) {
		return Ok(u);
	}
	if let Some(u) = lookup_derived(s) {
		return Ok(u);
	}
	for prefix in SIPrefix::ALL_DESC.iter() {
		let sym = prefix.symbol();
		if sym.is_empty() {
			continue;
		}
		if let Some(rest) = s.strip_prefix(sym) {
			if rest.is_empty() {
				continue;
			}
			if let Some(base) = lookup_prefixable_base(rest) {
				return Ok(base.with_prefix(*prefix));
			}
		}
	}
	// Accept U+00B5 (MICRO SIGN) and ASCII "u" as alternate micro prefix spellings.
	for &alt in &["\u{00B5}", "u"] {
		if let Some(rest) = s.strip_prefix(alt) {
			if !rest.is_empty() {
				if let Some(base) = lookup_prefixable_base(rest) {
					return Ok(base.with_prefix(SIPrefix::Micro));
				}
			}
		}
	}
	Err(UnitParseError::UnknownToken { token: s.to_string() })
}

fn lookup_bare_atom(s: &str) -> Option<Unit> {
	match s {
		"m" => Some(Unit::metre()),
		"kg" => Some(Unit::kilogram()),
		"g" => Some(Unit::gram()),
		"s" => Some(Unit::second()),
		"A" => Some(Unit::ampere()),
		"K" => Some(Unit::kelvin()),
		"mol" => Some(Unit::mole()),
		"cd" => Some(Unit::candela()),
		"rad" => Some(Unit::radian()),
		"deg" => Some(Unit::degree()),
		"\u{0394}K" | "dK" => Some(Unit::kelvin_delta()),
		_ => None,
	}
}

fn lookup_prefixable_base(s: &str) -> Option<Unit> {
	match s {
		"m" => Some(Unit::metre()),
		"g" => Some(Unit::gram()),
		"s" => Some(Unit::second()),
		"A" => Some(Unit::ampere()),
		"K" => Some(Unit::kelvin()),
		"mol" => Some(Unit::mole()),
		"cd" => Some(Unit::candela()),
		"rad" => Some(Unit::radian()),
		"Hz" => Some(Unit::hertz()),
		"N" => Some(Unit::newton()),
		"Pa" => Some(Unit::pascal()),
		"J" => Some(Unit::joule()),
		"W" => Some(Unit::watt()),
		"C" => Some(Unit::coulomb()),
		"V" => Some(Unit::volt()),
		"\u{03A9}" => Some(Unit::ohm()),
		"F" => Some(Unit::farad()),
		"H" => Some(Unit::henry()),
		_ => None,
	}
}

fn lookup_derived(s: &str) -> Option<Unit> {
	match s {
		"Hz" => Some(Unit::hertz()),
		"N" => Some(Unit::newton()),
		"Pa" => Some(Unit::pascal()),
		"J" => Some(Unit::joule()),
		"W" => Some(Unit::watt()),
		"C" => Some(Unit::coulomb()),
		"V" => Some(Unit::volt()),
		"\u{03A9}" | "ohm" => Some(Unit::ohm()),
		"F" => Some(Unit::farad()),
		"T" => Some(Unit::tesla()),
		"H" => Some(Unit::henry()),
		_ => None,
	}
}

fn normalize_superscripts(s: &str) -> String {
	let mut out = String::with_capacity(s.len());
	let mut in_superscript = false;
	for ch in s.chars() {
		if let Some(mapped) = map_superscript(ch) {
			if !in_superscript {
				out.push('^');
				in_superscript = true;
			}
			out.push(mapped);
		} else {
			in_superscript = false;
			out.push(ch);
		}
	}
	out
}

fn map_superscript(c: char) -> Option<char> {
	match c {
		'\u{2070}' => Some('0'),
		'\u{00B9}' => Some('1'),
		'\u{00B2}' => Some('2'),
		'\u{00B3}' => Some('3'),
		'\u{2074}' => Some('4'),
		'\u{2075}' => Some('5'),
		'\u{2076}' => Some('6'),
		'\u{2077}' => Some('7'),
		'\u{2078}' => Some('8'),
		'\u{2079}' => Some('9'),
		'\u{207B}' => Some('-'),
		'\u{207A}' => Some('+'),
		_ => None,
	}
}

#[cfg(test)]
fn atoms_vec(u: &Unit) -> Vec<(BaseUnitId, i8)> {
	let mut v: Vec<(BaseUnitId, i8)> = u.atoms.iter().map(|(k, v)| (*k, *v)).collect();
	v.sort();
	v
}

#[cfg(test)]
fn atoms_map(pairs: &[(BaseUnitId, i8)]) -> BTreeMap<BaseUnitId, i8> {
	pairs.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
	use super::super::dimension::Dimension;
	use super::*;

	#[test]
	fn parses_dimensionless_one() {
		let u = parse_unit("1").unwrap();
		assert!(u.is_dimensionless());
	}

	#[test]
	fn parses_metre() {
		let u = parse_unit("m").unwrap();
		assert_eq!(u, Unit::metre());
	}

	#[test]
	fn parses_kilometre() {
		let u = parse_unit("km").unwrap();
		assert_eq!(u, Unit::metre().with_prefix(SIPrefix::Kilo));
	}

	#[test]
	fn parses_millisecond() {
		let u = parse_unit("ms").unwrap();
		assert_eq!(u, Unit::second().with_prefix(SIPrefix::Milli));
	}

	#[test]
	fn parses_microsecond_with_greek_mu() {
		let u = parse_unit("\u{03BC}s").unwrap();
		assert_eq!(u, Unit::second().with_prefix(SIPrefix::Micro));
	}

	#[test]
	fn parses_microsecond_with_micro_sign() {
		let u = parse_unit("\u{00B5}s").unwrap();
		assert_eq!(u, Unit::second().with_prefix(SIPrefix::Micro));
	}

	#[test]
	fn parses_microsecond_with_ascii_u() {
		let u = parse_unit("us").unwrap();
		assert_eq!(u, Unit::second().with_prefix(SIPrefix::Micro));
	}

	#[test]
	fn parses_kilogram_as_base() {
		let u = parse_unit("kg").unwrap();
		assert_eq!(u, Unit::kilogram());
		assert_eq!(u.si_factor, 1.0);
	}

	#[test]
	fn parses_gram_as_milli_kilogram() {
		let u = parse_unit("g").unwrap();
		assert_eq!(u, Unit::gram());
		assert!((u.si_factor - 0.001).abs() < 1e-12);
	}

	#[test]
	fn parses_milligram() {
		let u = parse_unit("mg").unwrap();
		assert_eq!(u.dimension(), Dimension::MASS);
		assert!((u.si_factor - 1e-6).abs() < 1e-18);
	}

	#[test]
	fn parses_division_m_over_s() {
		let u = parse_unit("m/s").unwrap();
		assert_eq!(u.dimension(), Dimension::VELOCITY);
	}

	#[test]
	fn parses_acceleration_with_caret_exponent() {
		let u = parse_unit("m/s^2").unwrap();
		assert_eq!(u.dimension(), Dimension::ACCELERATION);
	}

	#[test]
	fn parses_acceleration_with_superscript() {
		let u = parse_unit("m/s\u{00B2}").unwrap();
		assert_eq!(u.dimension(), Dimension::ACCELERATION);
	}

	#[test]
	fn parses_product_with_middle_dot() {
		let u = parse_unit("kg\u{00B7}m/s^2").unwrap();
		assert_eq!(u.dimension(), Dimension::FORCE);
	}

	#[test]
	fn parses_product_with_asterisk() {
		let u = parse_unit("kg*m/s^2").unwrap();
		assert_eq!(u.dimension(), Dimension::FORCE);
	}

	#[test]
	fn parses_product_with_space() {
		let u = parse_unit("kg m/s^2").unwrap();
		assert_eq!(u.dimension(), Dimension::FORCE);
	}

	#[test]
	fn parses_hertz_as_derived() {
		let u = parse_unit("Hz").unwrap();
		assert_eq!(u.dimension(), Dimension::FREQUENCY);
	}

	#[test]
	fn parses_newton_as_derived() {
		let u = parse_unit("N").unwrap();
		assert_eq!(u.dimension(), Dimension::FORCE);
	}

	#[test]
	fn parses_ohm_both_symbols() {
		assert_eq!(parse_unit("\u{03A9}").unwrap().dimension(), Dimension::RESISTANCE);
		assert_eq!(parse_unit("ohm").unwrap().dimension(), Dimension::RESISTANCE);
	}

	#[test]
	fn parses_degree() {
		let u = parse_unit("deg").unwrap();
		assert_eq!(u, Unit::degree());
	}

	#[test]
	fn parses_radian() {
		let u = parse_unit("rad").unwrap();
		assert_eq!(u, Unit::radian());
	}

	#[test]
	fn parses_kelvin_delta_with_greek_delta() {
		let u = parse_unit("\u{0394}K").unwrap();
		assert_eq!(u, Unit::kelvin_delta());
	}

	#[test]
	fn parses_kelvin_delta_with_ascii_d() {
		let u = parse_unit("dK").unwrap();
		assert_eq!(u, Unit::kelvin_delta());
	}

	#[test]
	fn parses_one_over_s() {
		let u = parse_unit("1/s").unwrap();
		assert_eq!(u.dimension(), Dimension::FREQUENCY);
	}

	#[test]
	fn parses_negative_exponent() {
		let u = parse_unit("s^-1").unwrap();
		assert_eq!(u.dimension(), Dimension::FREQUENCY);
	}

	#[test]
	fn parses_superscript_minus() {
		let u = parse_unit("s\u{207B}\u{00B9}").unwrap();
		assert_eq!(u.dimension(), Dimension::FREQUENCY);
	}

	#[test]
	fn rejects_empty() {
		assert!(matches!(parse_unit(""), Err(UnitParseError::Empty)));
		assert!(matches!(parse_unit("   "), Err(UnitParseError::Empty)));
	}

	#[test]
	fn rejects_unknown_token() {
		let err = parse_unit("nonsense").unwrap_err();
		assert!(matches!(err, UnitParseError::UnknownToken { .. }));
	}

	#[test]
	fn rejects_parenthesis() {
		let err = parse_unit("m/(s*s)").unwrap_err();
		assert!(matches!(err, UnitParseError::UnsupportedParenthesis { .. }));
	}

	#[test]
	fn repeated_division_is_denominator_stack() {
		let u = parse_unit("m/s/s").unwrap();
		assert_eq!(u.dimension(), Dimension::ACCELERATION);
	}

	#[test]
	fn normalize_superscripts_inserts_caret() {
		assert_eq!(normalize_superscripts("s\u{00B2}"), "s^2");
		assert_eq!(normalize_superscripts("s\u{207B}\u{00B9}"), "s^-1");
		assert_eq!(normalize_superscripts("m/s\u{00B2}"), "m/s^2");
		assert_eq!(normalize_superscripts("m"), "m");
	}

	#[test]
	fn atoms_layout_matches_for_force() {
		let u = parse_unit("kg\u{00B7}m/s^2").unwrap();
		let expected = atoms_map(&[(BaseUnitId::Kilogram, 1), (BaseUnitId::Metre, 1), (BaseUnitId::Second, -2)]);
		assert_eq!(u.atoms, expected);
		let _ = atoms_vec(&u);
	}
}
