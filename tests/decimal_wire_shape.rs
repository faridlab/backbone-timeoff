//! Decimals travel as exact strings; a field that wants a JSON number asks for one itself.
//!
//! The crate-wide `rust_decimal/serde-float` feature changes how EVERY decimal in the composed
//! service serializes, because Cargo unifies features across the build: money, rates and
//! quantities all became floats once this crate turned it on. The per-field float module needs
//! only `serde-with-float`, which leaves the default alone.

use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Serialize)]
struct Cap {
    #[serde(with = "rust_decimal::serde::float_option")]
    max_days_per_request: Option<Decimal>,
}

#[test]
fn a_decimal_serializes_as_an_exact_string() {
    assert_eq!(serde_json::to_string(&Decimal::new(30000000, 2)).unwrap(), "\"300000.00\"");
}

#[test]
fn a_field_that_asks_for_a_number_still_gets_one() {
    let cap = Cap { max_days_per_request: Some(Decimal::new(300, 2)) };
    assert_eq!(serde_json::to_string(&cap).unwrap(), r#"{"max_days_per_request":3.0}"#);
}
