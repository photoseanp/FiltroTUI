//! Calculations on cumulative particle counts (particles >= channel size).

use std::f64::consts::PI;

/// Particles in the interval [d_i, d_i+1): N_i - N_i+1. The last value is the open-ended tail (>= d_last).
pub fn interval_counts(cum: &[f64]) -> Vec<f64> {
    (0..cum.len())
        .map(|i| (cum[i] - cum.get(i + 1).copied().unwrap_or(0.0)).max(0.0))
        .collect()
}

/// Representative diameter of interval i: geometric mean of its bounds,
/// the lower bound for the open-ended tail.
pub fn representative_diameter(sizes: &[f64], i: usize) -> f64 {
    match sizes.get(i + 1) {
        Some(hi) => (sizes[i] * hi).sqrt(),
        None => sizes[i],
    }
}

/// Mass of one sphere in micrograms: rho [g/cm3] * pi/6 * d^3 [um^3] (1 um^3 = 1e-12 cm3 = 1e-6 ug / (g/cm3)).
pub fn sphere_mass_ug(d_um: f64, density_g_cm3: f64) -> f64 {
    density_g_cm3 * PI / 6.0 * d_um.powi(3) * 1e-6
}

/// Mass concentration per size interval in mg/mL.
/// `cum_ref` are cumulative counts in the reference volume `ref_volume_ml`.
pub fn mass_per_interval(cum_ref: &[f64], sizes: &[f64], density: f64, ref_volume_ml: f64) -> Vec<f64> {
    let counts = interval_counts(cum_ref);
    let vol = ref_volume_ml.max(1e-9);
    (0..counts.len().min(sizes.len()))
        .map(|i| counts[i] * sphere_mass_ug(representative_diameter(sizes, i), density) / 1000.0 / vol)
        .collect()
}

/// Cumulative count at size `x`, interpolated between channels (log-log when possible).
/// None outside the channel range.
pub fn interp_count(sizes: &[f64], cum: &[f64], x: f64) -> Option<f64> {
    let n = sizes.len().min(cum.len());
    if n == 0 || x < sizes[0] || x > sizes[n - 1] {
        return None;
    }
    for i in 0..n {
        if (x - sizes[i]).abs() < 1e-9 {
            return Some(cum[i]);
        }
        if i + 1 < n && x > sizes[i] && x < sizes[i + 1] {
            let (x0, x1, y0, y1) = (sizes[i], sizes[i + 1], cum[i], cum[i + 1]);
            if y0 > 0.0 && y1 > 0.0 && x0 > 0.0 {
                let t = (x.ln() - x0.ln()) / (x1.ln() - x0.ln());
                return Some((y0.ln() + (y1.ln() - y0.ln()) * t).exp());
            }
            let t = (x - x0) / (x1 - x0);
            return Some(y0 + (y1 - y0) * t);
        }
    }
    None
}

/// Efficiency in percent from cumulative upstream / downstream counts.
pub fn efficiency(up: f64, down: f64) -> Option<f64> {
    if up > 0.0 {
        Some(((1.0 - down / up) * 100.0).clamp(0.0, 100.0))
    } else {
        None
    }
}
