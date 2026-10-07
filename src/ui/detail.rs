use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::analysis::{
    efficiency, interp_count, interval_counts, mass_per_interval, representative_diameter,
    sphere_mass_ug,
};
use crate::app::App;

fn centered(area: Rect, pw: u16, ph: u16) -> Rect {
    let w = area.width * pw / 100;
    let h = area.height * ph / 100;
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn beta_text(u: f64, d: f64) -> String {
    if d <= 0.0 {
        if u > 0.0 {
            "inf".to_string()
        } else {
            "-".to_string()
        }
    } else {
        format!("{:.2}", u / d)
    }
}

/// Extended view: raw instrument data and everything recalculated to the reference volume.
pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(v) = app.view() else { return };
    let rect = centered(area, 98, 92);
    f.render_widget(Clear, rect);

    let n = v.up.len();
    let avg = v.n_meas > 1;
    // Raw counts are integers for a single measurement; averages and recalculated values need decimals.
    let fr = |x: f64| if avg { format!("{:.1}", x) } else { format!("{:.0}", x) };
    let fv = |x: f64| format!("{:.1}", x);
    let rho = app.settings.analysis.density;
    let scale = if v.volume_ml > 0.0 { v.ref_ml / v.volume_ml } else { 1.0 };

    let sizes = v.sizes.clone();
    let masses = sizes.as_ref().map(|s| {
        (
            mass_per_interval(&v.up_n, s, rho, v.ref_ml),
            mass_per_interval(&v.down_n, s, rho, v.ref_ml),
        )
    });
    let int_up = interval_counts(&v.up_n);
    let int_down = interval_counts(&v.down_n);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(v.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!(
            "   sample {:.1} ml -> {:.0} ml (x{:.3})   density {:.2} g/cm3   channels {}",
            v.volume_ml, v.ref_ml, scale, rho, n
        )),
    ]));
    lines.push(Line::from(
        "raw = counts from the instrument; per-ref columns, intervals and masses are recalculated to the reference volume; beta and efficiency do not depend on the volume",
    ));
    if v.skipped > 0 {
        lines.push(Line::from(Span::styled(
            format!("{} marked measurement(s) skipped: different channel count", v.skipped),
            Style::default().fg(Color::Yellow),
        )));
    }
    if sizes.is_none() {
        lines.push(Line::from(Span::styled(
            "Sizes unknown: enable the same number of channels as the instrument sends (key c) to get sizes and mass.",
            Style::default().fg(Color::Yellow),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "{:>3} {:>6} {:>8} {:>8} {:>10} {:>10} {:>8} {:>7} {:>10} {:>10} {:>10} {:>10}",
            "ch", "um", "up raw", "down raw", "up >=/ref", "down >=/ref", "beta", "eff %",
            "up int/ref", "dn int/ref", "up mg/mL", "dn mg/mL"
        ),
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for i in 0..n {
        let size = sizes
            .as_ref()
            .map(|s| format!("{:.1}", s[i]))
            .unwrap_or_else(|| "-".to_string());
        let eff = efficiency(v.up[i], v.down[i])
            .map(|e| format!("{:.2}", e))
            .unwrap_or_else(|| "-".to_string());
        let (mu, md) = match &masses {
            Some((a, b)) => (format!("{:.3e}", a[i]), format!("{:.3e}", b[i])),
            None => ("-".to_string(), "-".to_string()),
        };
        lines.push(Line::from(format!(
            "{:>3} {:>6} {:>8} {:>8} {:>10} {:>10} {:>8} {:>7} {:>10} {:>10} {:>10} {:>10}",
            i + 1,
            size,
            fr(v.up[i]),
            fr(v.down[i]),
            fv(v.up_n[i]),
            fv(v.down_n[i]),
            beta_text(v.up[i], v.down[i]),
            eff,
            fv(int_up[i]),
            fv(int_down[i]),
            mu,
            md
        )));
    }

    lines.push(Line::from(""));
    if let (Some((mu, md)), Some(s)) = (&masses, &sizes) {
        let su: f64 = mu.iter().sum();
        let sd: f64 = md.iter().sum();
        let eff = if su > 0.0 {
            format!("{:.2}", (1.0 - sd / su) * 100.0)
        } else {
            "-".to_string()
        };
        lines.push(Line::from(Span::styled(
            "Mass (spherical particles, from the smallest channel up)",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!(
            "concentration: upstream {:.3e} mg/mL | downstream {:.3e} mg/mL | retained {:.3e} mg/mL | mass efficiency {} %",
            su,
            sd,
            su - sd,
            eff
        )));
        lines.push(Line::from(format!(
            "in {:.0} ml: upstream {:.3e} mg | downstream {:.3e} mg | retained {:.3e} mg",
            v.ref_ml,
            su * v.ref_ml,
            sd * v.ref_ml,
            (su - sd) * v.ref_ml
        )));
        let tail = n - 1;
        lines.push(Line::from(format!(
            "last channel (>= {:.1} um) is an open-ended tail counted with its lower bound; one such particle = {:.3e} ug (interval diameter = geometric mean of the bounds, e.g. {:.2} um for channel 1)",
            s[tail],
            sphere_mass_ug(representative_diameter(s, tail), rho),
            representative_diameter(s, 0)
        )));
        lines.push(Line::from(""));
    }

    if let Some(s) = &sizes {
        let cuts: Vec<f64> = app
            .settings
            .analysis
            .cutoffs
            .iter()
            .copied()
            .filter(|c| *c > 0.0)
            .collect();
        if !cuts.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(
                    "Cut-off sizes (counts per {:.0} ml, interpolated log-log between channels)",
                    v.ref_ml
                ),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!(
                "{:>8} {:>10} {:>10} {:>9} {:>8}",
                "um", "up >=", "down >=", "beta", "eff %"
            )));
            for c in cuts {
                match (interp_count(s, &v.up_n, c), interp_count(s, &v.down_n, c)) {
                    (Some(u), Some(d)) => {
                        let eff = efficiency(u, d)
                            .map(|e| format!("{:.2}", e))
                            .unwrap_or_else(|| "-".to_string());
                        lines.push(Line::from(format!(
                            "{:>8.1} {:>10.1} {:>10.1} {:>9} {:>8}",
                            c,
                            u,
                            d,
                            beta_text(u, d),
                            eff
                        )));
                    }
                    _ => lines.push(Line::from(format!(
                        "{:>8.1} outside the channel range {:.1}..{:.1} um",
                        c,
                        s[0],
                        s[n - 1]
                    ))),
                }
            }
        }
    }

    let inner_h = rect.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(inner_h) as u16;
    let scroll = app.detail_scroll.min(max_scroll);
    let title = if avg {
        " Averaged measurements - Up/Down/PgUp/PgDn scroll, Esc/Enter close "
    } else {
        " Measurement details - Up/Down/PgUp/PgDn scroll, Esc/Enter close "
    };
    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .block(Block::default().borders(Borders::ALL).title(title)),
        rect,
    );
}
