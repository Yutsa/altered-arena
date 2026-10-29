//! Win rates with Wilson 95 % intervals.

/// Wilson score interval (95 %) of `wins` out of `n`
pub fn wilson(wins: f64, n: f64) -> (f64, f64) {
    if n <= 0.0 {
        return (0.0, 1.0);
    }
    let z = 1.96f64;
    let p = wins / n;
    let den = 1.0 + z * z / n;
    let centre = p + z * z / (2.0 * n);
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt();
    (((centre - half) / den).max(0.0), ((centre + half) / den).min(1.0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn wilson_half() {
        let (lo, hi) = super::wilson(50.0, 100.0);
        assert!((lo - 0.404).abs() < 0.01 && (hi - 0.596).abs() < 0.01);
    }
}
