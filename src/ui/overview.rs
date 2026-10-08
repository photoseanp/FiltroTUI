use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Bar, BarChart, BarGroup, Block, Borders, Chart, Dataset, GraphType, List, ListItem,
        ListState, Paragraph,
    },
    Frame,
};

use super::list_offset;
use crate::analysis::{efficiency, interp_count, interval_counts, mass_per_interval};
use crate::app::{App, DistMode, Target, View};

const CUT_COLORS: [Color; 4] = [
    Color::LightMagenta,
    Color::LightGreen,
    Color::LightRed,
    Color::LightBlue,
];

/// Width of the Y axis column left of the distribution bars.
const Y_AXIS_W: u16 = 9;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);

    let ctrl = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);
    draw_pumps(f, ctrl[0], app);
    draw_detection(f, ctrl[1], app);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(rows[1]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[2]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(bottom[0]);

    let panel = |title: &'static str| Block::default().borders(Borders::ALL).title(title);

    f.render_widget(panel(" Flow rate "), top[0]);
    f.render_widget(panel(" Differential pressure "), top[1]);
    f.render_widget(panel(" Circulation pump "), top[2]);

    draw_history(f, left[0], app);
    draw_efficiency(f, left[1], app);
    draw_distribution(f, bottom[1], app);
}

fn pump_span(state: Option<bool>) -> Span<'static> {
    match state {
        Some(true) => Span::styled("ON", Style::default().fg(Color::Green)),
        Some(false) => Span::styled("OFF", Style::default().fg(Color::Red)),
        None => Span::styled("unknown", Style::default().fg(Color::DarkGray)),
    }
}

/// One-row click region inside a bordered panel; `line` counts from the first inner row.
fn line_rect(area: Rect, line: u16, x_off: u16, width: u16) -> Rect {
    Rect {
        x: area.x + 1 + x_off,
        y: area.y + 1 + line,
        width: width.min(area.width.saturating_sub(2 + x_off)),
        height: 1,
    }
}

fn draw_pumps(f: &mut Frame, area: Rect, app: &App) {
    let lines = vec![
        Line::from(vec![Span::raw("[u] Upstream sampler pump:   "), pump_span(app.pump_up)]),
        Line::from(vec![Span::raw("[d] Downstream sampler pump: "), pump_span(app.pump_down)]),
        Line::from(format!(
            "Speed: F{} ({} ml/min) - set on the instrument",
            app.settings.run.flow_level,
            app.settings.flow_ml_min()
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Sampler pumps ")),
        area,
    );
    let w = area.width.saturating_sub(2);
    app.add_hit(line_rect(area, 0, 0, w), Target::Key('u'));
    app.add_hit(line_rect(area, 1, 0, w), Target::Key('d'));
}

fn draw_detection(f: &mut Frame, area: Rect, app: &App) {
    let state = if app.detecting {
        Span::styled("RUNNING", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("idle", Style::default().fg(Color::Gray))
    };
    let enabled = app.settings.enabled_count();
    let sync_line = match app.instrument_channels {
        Some(n) => {
            let color = if n == enabled { Color::Green } else { Color::Yellow };
            Line::from(Span::styled(
                format!(
                    "Instrument sends {} ch | enabled here {} | auto-sync {} | [c] sync now",
                    n,
                    enabled,
                    if app.auto_sync { "ON" } else { "OFF" }
                ),
                Style::default().fg(color),
            ))
        }
        None => Line::from(format!(
            "Channels enabled here: {} | instrument count unknown until first detection",
            enabled
        )),
    };
    let lines = vec![
        Line::from(vec![Span::raw("State: "), state]),
        Line::from(format!(
            "Mode: {} | measurements: {} | sample {:.1} ml -> {:.0} ml",
            app.settings.run.mode.label(),
            app.results.len(),
            app.settings.effective_volume_ml(),
            app.settings.analysis.ref_volume_ml
        )),
        Line::from("[s] start (WD)  [g] suspend (WG)  [p] print (WP)"),
        sync_line,
    ];
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Detection ")),
        area,
    );
    // "[s] start (WD)" = 14 chars, "[g] suspend (WG)" = 16, "[p] print (WP)" = 14, two spaces between.
    app.add_hit(line_rect(area, 2, 0, 14), Target::Key('s'));
    app.add_hit(line_rect(area, 2, 16, 16), Target::Key('g'));
    app.add_hit(line_rect(area, 2, 34, 14), Target::Key('p'));
    app.add_hit(line_rect(area, 3, 0, area.width.saturating_sub(2)), Target::Key('c'));
}

fn fmt_size(s: f64) -> String {
    if s < 10.0 {
        format!("{:.1}", s)
    } else if (s - s.round()).abs() < 0.05 {
        format!("{:.0}", s)
    } else {
        format!("{:.1}", s)
    }
}

/// X axis of the efficiency chart: channel sizes in um when known, channel numbers otherwise.
struct XAxis {
    xs: Vec<f64>,
    real: bool,
    log: bool,
}

impl XAxis {
    fn new(v: &View, log_x: bool) -> Self {
        match &v.sizes {
            Some(s) => XAxis { xs: s.clone(), real: true, log: log_x },
            None => XAxis {
                xs: (1..=v.up.len()).map(|i| i as f64).collect(),
                real: false,
                log: false,
            },
        }
    }

    fn map(&self, x: f64) -> f64 {
        if self.log {
            x.max(0.01).log10()
        } else {
            x
        }
    }

    fn bounds(&self) -> [f64; 2] {
        let lo = self.map(*self.xs.first().unwrap_or(&0.0));
        let mut hi = self.map(*self.xs.last().unwrap_or(&1.0));
        if (hi - lo).abs() < 1e-9 {
            hi = lo + 1.0;
        }
        let pad = (hi - lo) * 0.03;
        [lo - pad, hi + pad]
    }

    /// Evenly spaced labels along the axis; the number grows with the chart width.
    fn labels(&self, width: u16) -> Vec<Span<'static>> {
        let [lo, hi] = self.bounds();
        let k = ((width.saturating_sub(10) as usize) / 7).clamp(3, 14);
        (0..k)
            .map(|i| {
                let t = lo + (hi - lo) * i as f64 / (k - 1) as f64;
                let real = if self.log { 10f64.powf(t) } else { t };
                Span::raw(fmt_size(real))
            })
            .collect()
    }

    fn title(&self) -> &'static str {
        if self.real {
            "particle size, um"
        } else {
            "channel # (enable the same number of channels: c)"
        }
    }

    /// Cut-off sizes that lie inside the measured size range (only with real sizes).
    fn cuts(&self, app: &App) -> Vec<(usize, f64)> {
        if !self.real || self.xs.is_empty() {
            return Vec::new();
        }
        let lo = self.xs[0];
        let hi = self.xs[self.xs.len() - 1];
        app.settings
            .analysis
            .cutoffs
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, c)| *c > 0.0 && *c >= lo && *c <= hi)
            .collect()
    }
}

fn draw_history(f: &mut Frame, area: Rect, app: &App) {
    let n = app.results.len();
    let ref_ml = app.settings.analysis.ref_volume_ml;
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .rev()
        .map(|(i, m)| {
            let c = &m.counts;
            let u = c.up.first().copied().unwrap_or(0);
            let d = c.down.first().copied().unwrap_or(0);
            let scale = app.scale_of(m);
            let beta = if d == 0 {
                "inf".to_string()
            } else {
                format!("{:.2}", u as f64 / d as f64)
            };
            let eff = if u == 0 {
                "-".to_string()
            } else {
                format!("{:.2}", (1.0 - d as f64 / u as f64) * 100.0)
            };
            ListItem::new(format!(
                "[{}] #{:<3} {} ch{:<2} raw {:>6}/{:<6} {:.0}ml {:>8.0}/{:<8.0} beta {:>6} eff {:>5}%",
                if app.marks.contains(&i) { "x" } else { " " },
                i + 1,
                m.at,
                c.up.len(),
                u,
                d,
                ref_ml,
                u as f64 * scale,
                d as f64 * scale,
                beta,
                eff
            ))
        })
        .collect();
    let title = format!(
        " History, smallest channel up/down ({} marked): Space/a/n mark, Enter details ",
        app.marks.len()
    );
    let visible = area.height.saturating_sub(2) as usize;
    let sel = app.hist_sel.min(n.saturating_sub(1));
    let off = list_offset(sel, visible);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default().with_offset(off);
    if n > 0 {
        state.select(Some(sel));
    }
    f.render_stateful_widget(list, area, &mut state);

    for r in 0..visible.min(n.saturating_sub(off)) {
        app.add_hit(
            Rect {
                x: area.x + 1,
                y: area.y + 1 + r as u16,
                width: area.width.saturating_sub(2),
                height: 1,
            },
            Target::History(off + r),
        );
    }
}

fn draw_efficiency(f: &mut Frame, area: Rect, app: &App) {
    let Some(v) = app.view() else {
        f.render_widget(
            Paragraph::new("No data yet - start a detection").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Separation efficiency "),
            ),
            area,
        );
        return;
    };
    let n = v.up.len();
    let ax = XAxis::new(&v, app.log_x);

    // Efficiency is a ratio of counts, so it does not depend on the volume.
    let pts: Vec<(f64, f64)> = (0..n)
        .filter_map(|i| efficiency(v.up[i], v.down[i]).map(|e| (ax.map(ax.xs[i]), e)))
        .collect();

    let cuts = ax.cuts(app);
    let cut_pts: Vec<[(f64, f64); 2]> = cuts
        .iter()
        .map(|(_, c)| {
            let x = ax.map(*c);
            [(x, 0.0), (x, 100.0)]
        })
        .collect();
    let cut_names: Vec<String> = cuts
        .iter()
        .map(|(_, c)| {
            let e = v.sizes.as_ref().and_then(|s| {
                let u = interp_count(s, &v.up_n, *c)?;
                let d = interp_count(s, &v.down_n, *c)?;
                efficiency(u, d)
            });
            match e {
                Some(e) => format!("{} um E={:.1}%", c, e),
                None => format!("{} um", c),
            }
        })
        .collect();

    let title = format!(
        " Efficiency {} (click: {} axis) ",
        v.title,
        if app.log_x { "log" } else { "linear" }
    );
    let mut datasets = vec![
        Dataset::default()
            .name("efficiency")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&pts),
        Dataset::default()
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Scatter)
            .style(Style::default().fg(Color::White))
            .data(&pts),
    ];
    for (k, (idx, _)) in cuts.iter().enumerate() {
        datasets.push(
            Dataset::default()
                .name(cut_names[k].clone())
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(CUT_COLORS[*idx % CUT_COLORS.len()]))
                .data(&cut_pts[k]),
        );
    }
    let chart = Chart::new(datasets)
        .block(Block::default().borders(Borders::ALL).title(title))
        .x_axis(
            Axis::default()
                .title(ax.title())
                .style(Style::default().fg(Color::Gray))
                .bounds(ax.bounds())
                .labels(ax.labels(area.width)),
        )
        .y_axis(
            Axis::default()
                .title("efficiency, %")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 100.0])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw("25"),
                    Span::raw("50"),
                    Span::raw("75"),
                    Span::raw("100"),
                ]),
        );
    f.render_widget(chart, area);
    // Click on the title row toggles the size axis (key l).
    app.add_hit(
        Rect { x: area.x, y: area.y, width: area.width, height: 1 },
        Target::Key('l'),
    );
}

/// Value text printed on a bar / axis tick.
fn short(v: f64) -> String {
    if v <= 0.0 {
        "0".to_string()
    } else if (v - v.round()).abs() < 1e-9 && v < 1e5 {
        format!("{}", v as u64)
    } else if !(0.01..1e5).contains(&v) {
        format!("{:.1e}", v)
    } else if v >= 100.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

fn count_label(k: i32) -> String {
    match k {
        i32::MIN..=2 => format!("{}", 10u64.pow(k.max(0) as u32)),
        3..=5 => format!("{}k", 10u64.pow((k - 3) as u32)),
        _ => format!("{}M", 10u64.pow((k - 6) as u32)),
    }
}

/// Bar height (integer units). Log: counts use log10(c + 1), masses are measured from `lo` decades.
fn bar_height(c: f64, is_mass: bool, log: bool, lo: i32, maxv: f64) -> u64 {
    if c <= 0.0 {
        return 0;
    }
    if log {
        if is_mass {
            ((c.log10() - lo as f64) * 100.0).max(0.0) as u64 + 1
        } else {
            ((c + 1.0).log10() * 100.0) as u64 + 1
        }
    } else if maxv > 0.0 {
        ((c / maxv) * 1000.0).round().max(1.0) as u64
    } else {
        0
    }
}

/// Y axis labels (value in bar-height units + text) for the distribution bars.
fn y_ticks(is_mass: bool, log: bool, lo: i32, maxh: u64, maxv: f64) -> Vec<(u64, String)> {
    let mut t: Vec<(u64, String)> = Vec::new();
    if log {
        if is_mass {
            for k in lo..=(lo + (maxh / 100) as i32 + 1) {
                let h = ((k - lo) * 100 + 1) as u64;
                if h <= maxh {
                    t.push((h, format!("1e{}", k)));
                }
            }
        } else {
            for k in 0..=((maxh / 100) as i32 + 1) {
                let h = ((10f64.powi(k) + 1.0).log10() * 100.0) as u64 + 1;
                if h <= maxh {
                    t.push((h, count_label(k)));
                }
            }
        }
    } else {
        for q in 0..=4u64 {
            t.push((maxh * q / 4, short(maxv * q as f64 / 4.0)));
        }
    }
    t
}

fn draw_distribution(f: &mut Frame, area: Rect, app: &App) {
    let Some(v) = app.view() else {
        f.render_widget(
            Paragraph::new("No data yet").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Particle distribution "),
            ),
            area,
        );
        return;
    };
    let n = v.up.len();
    let rho = app.settings.analysis.density;

    // Everything is shown for the reference volume (counts) or per mL (mass).
    let (up_raw, down_raw, is_mass, mode_text, unit) = match app.dist_mode {
        DistMode::Cumulative => (
            v.up_n.clone(),
            v.down_n.clone(),
            false,
            "cumulative >= d".to_string(),
            format!("N/{:.0}ml", v.ref_ml),
        ),
        DistMode::Interval => (
            interval_counts(&v.up_n),
            interval_counts(&v.down_n),
            false,
            "per interval".to_string(),
            format!("N/{:.0}ml", v.ref_ml),
        ),
        DistMode::Mass => match &v.sizes {
            Some(s) => (
                mass_per_interval(&v.up_n, s, rho, v.ref_ml),
                mass_per_interval(&v.down_n, s, rho, v.ref_ml),
                true,
                format!("mass, rho {:.2}", rho),
                "mg/mL".to_string(),
            ),
            None => {
                f.render_widget(
                    Paragraph::new(
                        "Mass distribution needs particle sizes: enable the same number of channels as the instrument sends (key c). m: other mode",
                    )
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Particle distribution, mass "),
                    ),
                    area,
                );
                return;
            }
        },
    };

    let all: Vec<f64> = up_raw.iter().chain(down_raw.iter()).copied().collect();
    let maxv = all.iter().copied().fold(0.0_f64, f64::max);
    let lo: i32 = {
        let logs: Vec<f64> = all.iter().copied().filter(|x| *x > 0.0).map(|x| x.log10()).collect();
        if logs.is_empty() {
            0
        } else {
            logs.iter().copied().fold(f64::MAX, f64::min).floor() as i32
        }
    };
    let hu: Vec<u64> = up_raw
        .iter()
        .map(|c| bar_height(*c, is_mass, app.log_y, lo, maxv))
        .collect();
    let hd: Vec<u64> = down_raw
        .iter()
        .map(|c| bar_height(*c, is_mass, app.log_y, lo, maxv))
        .collect();
    let maxh = hu.iter().chain(hd.iter()).copied().max().unwrap_or(1).max(1);

    // Y axis column on the left, bars on the right.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(Y_AXIS_W), Constraint::Min(8)])
        .split(area);
    let axis_area = cols[0];
    let chart_area = cols[1];

    let inner_w = chart_area.width.saturating_sub(2) as usize;
    let per_group = (inner_w / n.max(1)).max(3);
    let bw = (per_group.saturating_sub(1) / 2).clamp(1, 7) as u16;
    let group_gap: u16 = if per_group >= 2 * bw as usize + 1 { 1 } else { 0 };

    // Rows available for the bars: chart height minus the borders and the label row.
    let plot_h = chart_area.height.saturating_sub(3) as usize;
    let lw = (Y_AXIS_W - 1) as usize;
    let mut tick_rows: Vec<Option<String>> = vec![None; plot_h];
    if plot_h > 0 {
        for (h, text) in y_ticks(is_mass, app.log_y, lo, maxh, maxv) {
            let r = ((h as f64 / maxh as f64) * plot_h as f64).round() as usize;
            let r = r.min(plot_h);
            // The row whose top edge corresponds to the value; zero sits on the bottom row.
            let t = if r == 0 { plot_h - 1 } else { plot_h - r };
            if tick_rows[t].is_none() {
                tick_rows[t] = Some(text);
            }
        }
    }
    let gray = Style::default().fg(Color::Gray);
    let mut axis_lines: Vec<Line> = Vec::new();
    axis_lines.push(Line::from(Span::styled(
        format!("{:>w$} ", unit.chars().take(lw).collect::<String>(), w = lw),
        gray,
    )));
    for t in 0..plot_h {
        let text = match &tick_rows[t] {
            Some(s) => format!("{:>w$}┤", s.chars().take(lw).collect::<String>(), w = lw),
            None => format!("{:>w$}│", "", w = lw),
        };
        axis_lines.push(Line::from(Span::styled(text, gray)));
    }
    let x_unit = if v.sizes.is_some() { "um" } else { "ch#" };
    axis_lines.push(Line::from(Span::styled(format!("{:>w$} ", x_unit, w = lw), gray)));
    f.render_widget(Paragraph::new(axis_lines), axis_area);
    // Click on the axis toggles log / linear bar height (key y).
    app.add_hit(axis_area, Target::Key('y'));

    let title = Line::from(vec![
        Span::raw(format!(" {} {} ", mode_text, v.title)),
        Span::styled("\u{25a0} up ", Style::default().fg(Color::Cyan)),
        Span::styled("\u{25a0} down ", Style::default().fg(Color::Yellow)),
    ]);
    let mut chart = BarChart::default()
        .block(Block::default().borders(Borders::ALL).title(title))
        .bar_width(bw)
        .bar_gap(0)
        .group_gap(group_gap)
        .max(maxh);

    let cutoffs = app.settings.analysis.cutoffs;
    for i in 0..n {
        let label_text = match &v.sizes {
            Some(s) => fmt_size(s[i]),
            None => format!("{}", i + 1),
        };
        // Colour the label of the interval that contains a cut-off size.
        let mut label_style = Style::default().fg(Color::Gray);
        if let Some(s) = &v.sizes {
            let lo_d = s[i];
            let hi_d = s.get(i + 1).copied().unwrap_or(f64::MAX);
            for (k, c) in cutoffs.iter().enumerate() {
                if *c > 0.0 && *c >= lo_d && *c < hi_d {
                    label_style = Style::default()
                        .fg(CUT_COLORS[k % CUT_COLORS.len()])
                        .add_modifier(Modifier::BOLD);
                }
            }
        }
        let bars = [
            Bar::default()
                .value(hu[i])
                .text_value(short(up_raw[i]))
                .style(Style::default().fg(Color::Cyan)),
            Bar::default()
                .value(hd.get(i).copied().unwrap_or(0))
                .text_value(short(down_raw.get(i).copied().unwrap_or(0.0)))
                .style(Style::default().fg(Color::Yellow)),
        ];
        chart = chart.data(
            BarGroup::default()
                .label(Line::from(Span::styled(label_text, label_style)))
                .bars(&bars),
        );
    }
    f.render_widget(chart, chart_area);
    // Click on the chart title row cycles the distribution mode (key m).
    app.add_hit(
        Rect { x: chart_area.x, y: chart_area.y, width: chart_area.width, height: 1 },
        Target::Key('m'),
    );
}
