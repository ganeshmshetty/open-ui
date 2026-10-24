//! Number, currency, percent, delta and date formatting (DESIGN §6 rule 12).
//!
//! Phase 1 uses `en-US` rules until host formatters exist (DESIGN §6.12,
//! SPEC §14.2 question 5). Values are wire-encoded IR values (`{"i":2}`,
//! `{"f":2.5}`, strings, booleans and null).

use serde_json::Value;

const MINUS: char = '\u{2212}';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Text,
    Number,
    Compact,
    Currency,
    Percent,
    Delta,
    Date,
}

impl Format {
    pub fn from_name(name: &str) -> Format {
        match name {
            "number" => Format::Number,
            "compact" => Format::Compact,
            "currency" => Format::Currency,
            "percent" => Format::Percent,
            "delta" => Format::Delta,
            "date" => Format::Date,
            _ => Format::Text,
        }
    }
}

/// One scalar decoded from a wire prop. Integers stay integers so large values
/// are never rounded through `f64`.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    Null,
}

impl Scalar {
    pub fn from_wire(value: &Value) -> Scalar {
        match value {
            Value::Number(number) => number
                .as_i64()
                .map(Scalar::Int)
                .or_else(|| number.as_f64().map(Scalar::Float))
                .unwrap_or(Scalar::Null),
            Value::String(text) => Scalar::Text(text.clone()),
            Value::Bool(value) => Scalar::Bool(*value),
            Value::Object(map) => {
                if let Some(value) = map.get("i").and_then(Value::as_i64) {
                    Scalar::Int(value)
                } else if let Some(value) = map.get("f").and_then(Value::as_f64) {
                    Scalar::Float(value)
                } else if let Some(value) = map.get("s").and_then(Value::as_str) {
                    Scalar::Text(value.to_string())
                } else {
                    Scalar::Null
                }
            }
            Value::Array(_) => Scalar::Text("—".into()),
            Value::Null => Scalar::Null,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Scalar::Int(value) => Some(*value as f64),
            Scalar::Float(value) if value.is_finite() => Some(*value),
            _ => None,
        }
    }

    pub fn is_zero(&self) -> bool {
        match self {
            Scalar::Int(value) => *value == 0,
            Scalar::Float(value) => *value == 0.0,
            _ => false,
        }
    }
}

pub fn format_scalar(value: &Scalar, format: Format, currency: &str) -> String {
    if format == Format::Date {
        return match value {
            Scalar::Text(text) => format_date(text),
            _ => plain_number(value),
        };
    }
    match value {
        Scalar::Text(text) => text.clone(),
        Scalar::Bool(value) => value.to_string(),
        Scalar::Null => "—".into(),
        Scalar::Int(_) | Scalar::Float(_) => match format {
            Format::Text => plain_number(value),
            Format::Number => format_number(value),
            Format::Compact => format_compact(value),
            Format::Currency => format_currency(value, currency),
            Format::Percent => format_percent(value),
            Format::Delta => format_delta(value),
            Format::Date => plain_number(value),
        },
    }
}

/// A value rendered as plain text, for Facts values and chart category labels.
pub fn text_of(value: &Value) -> String {
    match Scalar::from_wire(value) {
        Scalar::Int(value) => value.to_string(),
        Scalar::Float(value) => trim_float(value, 2),
        Scalar::Text(text) => text,
        Scalar::Bool(value) => value.to_string(),
        Scalar::Null => String::new(),
    }
}

pub fn numeric(value: &Value) -> Option<f64> {
    Scalar::from_wire(value).as_f64()
}

fn plain_number(value: &Scalar) -> String {
    match value {
        Scalar::Int(value) => value.to_string(),
        Scalar::Float(value) => trim_float(*value, 2),
        _ => format_scalar(value, Format::Number, "USD"),
    }
}

/// `number`: thousands separators, at most 2 decimals, trailing zeros trimmed.
pub fn format_number(value: &Scalar) -> String {
    match value {
        Scalar::Int(value) => group_i64(*value),
        Scalar::Float(value) if value.is_finite() => {
            let rounded = trim_float(*value, 2);
            if rounded == "0" {
                "0".into()
            } else {
                group_signed_text(&rounded)
            }
        }
        Scalar::Float(_) => "—".into(),
        _ => plain_number(value),
    }
}

/// `compact`: 1.2K, 3.4M, 12B, at most 1 decimal.
pub fn format_compact(value: &Scalar) -> String {
    let Some(number) = value.as_f64() else {
        return plain_number(value);
    };
    if !number.is_finite() {
        return "—".into();
    }
    let sign = if number < 0.0 {
        MINUS.to_string()
    } else {
        String::new()
    };
    let abs = number.abs();
    let (scaled, suffix) = if abs >= 1e12 {
        (abs / 1e12, "T")
    } else if abs >= 1e9 {
        (abs / 1e9, "B")
    } else if abs >= 1e6 {
        (abs / 1e6, "M")
    } else if abs >= 1e3 {
        (abs / 1e3, "K")
    } else {
        (abs, "")
    };
    if suffix.is_empty() {
        return format!(
            "{sign}{}",
            trim_float(abs, 1).replace('-', &MINUS.to_string())
        );
    }
    format!("{sign}{}{suffix}", trim_float(scaled, 1))
}

/// `currency`: locale currency format with the currency's minor units. Values
/// of 1,000,000 or more use compact form.
pub fn format_currency(value: &Scalar, code: &str) -> String {
    let Some(number) = value.as_f64() else {
        return plain_number(value);
    };
    if !number.is_finite() {
        return "—".into();
    }
    let (symbol, minor) = currency_parts(code);
    let sign = if number < 0.0 {
        MINUS.to_string()
    } else {
        String::new()
    };
    if number.abs() >= 1e6 {
        return format!(
            "{sign}{symbol}{}",
            format_compact(&Scalar::Float(number.abs()))
        );
    }
    let fixed = format!("{:.*}", minor, number.abs());
    let (whole, fraction) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let whole = whole.parse::<u64>().unwrap_or(0);
    let whole = group_u64(whole);
    if fraction.is_empty() {
        format!("{sign}{symbol}{whole}")
    } else {
        format!("{sign}{symbol}{whole}.{fraction}")
    }
}

/// `percent`: the value is already a percentage, at most 1 decimal.
pub fn format_percent(value: &Scalar) -> String {
    let Some(number) = value.as_f64() else {
        return plain_number(value);
    };
    if !number.is_finite() {
        return "—".into();
    }
    format!(
        "{}%",
        trim_float(number, 1).replace('-', &MINUS.to_string())
    )
}

/// `delta`: like number but always signed; zero has no sign. Negative numbers
/// use U+2212.
pub fn format_delta(value: &Scalar) -> String {
    let Some(number) = value.as_f64() else {
        return plain_number(value);
    };
    if !number.is_finite() {
        return "—".into();
    }
    if number > 0.0 {
        format!(
            "+{}",
            format_number(&Scalar::Float(number)).trim_start_matches('+')
        )
    } else {
        format_number(&Scalar::Float(number))
    }
}

/// `date`: ISO 8601 date in the locale's medium date format.
pub fn format_date(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let Some((year, month, day)) = parse_iso_date(value) else {
        return value.to_string();
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return value.to_string();
    }
    format!("{} {day}, {year}", MONTHS[(month - 1) as usize])
}

fn parse_iso_date(value: &str) -> Option<(i32, u32, u32)> {
    let date = value.split('T').next().unwrap_or(value);
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((year, month, day))
}

fn currency_parts(code: &str) -> (&'static str, usize) {
    match code.to_ascii_uppercase().as_str() {
        "USD" => ("$", 2),
        "EUR" => ("€", 2),
        "GBP" => ("£", 2),
        "JPY" => ("¥", 0),
        "CNY" => ("¥", 2),
        "INR" => ("₹", 2),
        _ => ("", 2),
    }
}

fn trim_float(value: f64, max_decimals: usize) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let mut text = format!("{value:.max_decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text.remove(0);
    }
    text.replace('-', &MINUS.to_string())
}

fn group_i64(value: i64) -> String {
    if value < 0 {
        format!("{MINUS}{}", group_u64(value.unsigned_abs()))
    } else {
        group_u64(value as u64)
    }
}

fn group_u64(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Groups the integer part of an already-rendered signed decimal string.
fn group_signed_text(text: &str) -> String {
    let (sign, body) = if let Some(rest) = text.strip_prefix(MINUS) {
        (MINUS.to_string(), rest)
    } else {
        (String::new(), text)
    };
    let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
    let whole = whole.parse::<u64>().unwrap_or(0);
    let grouped = group_u64(whole);
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{fraction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn int(value: i64) -> Scalar {
        Scalar::Int(value)
    }

    fn float(value: f64) -> Scalar {
        Scalar::Float(value)
    }

    #[test]
    fn number_uses_grouping_two_decimals_and_a_true_minus() {
        assert_eq!(format_number(&int(1234)), "1,234");
        assert_eq!(format_number(&float(1234.5)), "1,234.5");
        assert_eq!(format_number(&float(1234.567)), "1,234.57");
        assert_eq!(format_number(&int(-1234)), "−1,234");
        assert_eq!(format_number(&float(-0.0)), "0");
        assert_eq!(format_number(&int(0)), "0");
        assert_eq!(format_number(&float(f64::NAN)), "—");
    }

    #[test]
    fn compact_scales_from_k_to_t_with_one_trimmed_decimal() {
        assert_eq!(format_compact(&int(999)), "999");
        assert_eq!(format_compact(&int(1000)), "1K");
        assert_eq!(format_compact(&float(1234.)), "1.2K");
        assert_eq!(format_compact(&float(3_400_000.)), "3.4M");
        assert_eq!(format_compact(&float(12_000_000_000.)), "12B");
        assert_eq!(format_compact(&float(-1_500_000.)), "−1.5M");
    }

    #[test]
    fn currency_uses_minor_units_and_compact_large_values() {
        assert_eq!(format_currency(&float(1234.5), "USD"), "$1,234.50");
        assert_eq!(format_currency(&int(1235), "JPY"), "¥1,235");
        assert_eq!(format_currency(&float(1_200_000.), "USD"), "$1.2M");
        assert_eq!(format_currency(&float(-2.5), "EUR"), "−€2.50");
        assert_eq!(format_currency(&float(10.), "CAD"), "10.00");
    }

    #[test]
    fn percent_and_delta_keep_semantics() {
        assert_eq!(format_percent(&float(12.3)), "12.3%");
        assert_eq!(format_percent(&float(0.032)), "0%");
        assert_eq!(format_delta(&float(4.2)), "+4.2");
        assert_eq!(format_delta(&float(-1.3)), "−1.3");
        assert_eq!(format_delta(&int(0)), "0");
    }

    #[test]
    fn date_parses_iso_dates_and_preserves_invalid_text() {
        assert_eq!(format_date("2026-10-24"), "Oct 24, 2026");
        assert_eq!(format_date("2026-01-02T10:30:00Z"), "Jan 2, 2026");
        assert_eq!(format_date("tomorrow"), "tomorrow");
        assert_eq!(format_date("2026-13-01"), "2026-13-01");
    }

    #[test]
    fn stat_text_values_and_wire_scalars() {
        assert_eq!(
            format_scalar(&Scalar::Text("4.8 stars".into()), Format::Number, "USD"),
            "4.8 stars"
        );
        assert_eq!(Scalar::from_wire(&json!({"i": 7})), Scalar::Int(7));
        assert_eq!(Scalar::from_wire(&json!({"f": 2.5})), Scalar::Float(2.5));
        assert_eq!(text_of(&json!({"f": 2.5})), "2.5");
        assert_eq!(text_of(&json!("Ada")), "Ada");
    }
}
