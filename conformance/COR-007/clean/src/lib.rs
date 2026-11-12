/// The tolerance of the comparison, in the unit of the ratio.
const EPSILON: f64 = 1e-9;

/// True when the ratio is one half, within `EPSILON`.
pub fn is_half(ratio: f64) -> bool {
    (ratio - 0.5).abs() <= EPSILON
}

/// A test against the exact sentinel zero is not an approximate comparison.
pub fn is_empty(ratio: f64) -> bool {
    ratio == 0.0
}
