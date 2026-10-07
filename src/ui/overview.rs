use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, ListState, Paragraph,
    },
    Frame,
};

use crate::app::App;

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
            "Mode: {} | measurements: {}",
            app.settings.run.mode.label(),
            app.results.len()
        )),
        Line::from("[s] start (WD)  [g] suspend (WG)  [p] print (WP)"),
        sync_line,
    ];
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Detection ")),
        area,
    );
}

/// X axis of the charts: channel sizes in um when known, channel numbers otherwise.
struct XAxis {
    xs: Vec<f64>,
    real: bool,
    log: bool,
}

impl XAxis {
    /// The instrument reports only its selected channels, so sizes are exact only when
    /// the same number of channels is enabled in Channel Setup.
    fn new(app: &App, n: usize) -> Self {
        let enabled = app.settings.enabled_sizes();
        let sizes = if enabled.len() == n {
            Some(enabled)
        } else if n == 16 {
            Some(app.settings.all_sizes_sorted())
        } else {
            None
        };
        match sizes {
            Some(s) => XAxis { xs: s, real: true, log: app.log_x },
            None => XAxis { xs: (1..=n).map(|i| i as f64).collect(), real: false, log: false },
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

    fn labels(&self) -> Vec<Span<'static>> {
        let [lo, hi] = self.bounds();
        [lo, (lo + hi) / 2.0, hi]
            .iter()
            .map(|v| {
                let real = if self.log { 10f64.powf(*v) } else { *v };
                Span::raw(if real < 10.0 {
                    format!("{:.1}", real)
                } else {
                    format!("{:.0}", real)
                })
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
}

fn draw_history(f: &mut Frame, area: Rect, app: &App) {
    let n = app.results.len();
    let items: Vec<ListItem> = app
        .results
        .iter()
        .enumerate()
        .rev()
        .map(|(i, m)| {
            let c = &m.counts;
            let u = c.up.first().copied().unwrap_or(0);
            let d = c.down.first().copied().unwrap_or(0);
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
                "#{:<3} {}  ch {:<2}  up {:>7}  down {:>7}  beta {:>7}  eff {:>6}%",
                i + 1,
                m.at,
                c.up.len(),
                u,
                d,
                beta,
                eff
            ))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Measurement history (Up/Down/PgUp/PgDn/Home/End; smallest channel) "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if n > 0 {
        state.select(Some(app.hist_sel.min(n - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_efficiency(f: &mut Frame, area: Rect, app: &App) {
    let Some(m) = app.selected() else {
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
    let n_all = app.results.len();
    let number = n_all - app.hist_sel.min(n_all - 1);
    let c = &m.counts;
    let n = c.up.len();
    let ax = XAxis::new(app, n);

    let pts: Vec<(f64, f64)> = (0..n)
        .filter(|&i| c.up[i] > 0)
        .map(|i| {
            let d = c.down.get(i).copied().unwrap_or(0) as f64;
            let e = ((1.0 - d / c.up[i] as f64) * 100.0).clamp(0.0, 100.0);
            (ax.map(ax.xs[i]), e)
        })
        .collect();

    let title = format!(
        " Separation efficiency #{} {} (l: {} size axis) ",
        number,
        m.at,
        if app.log_x { "log" } else { "linear" }
    );
    let line = Dataset::default()
        .name("efficiency")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&pts);
    let dots = Dataset::default()
        .marker(symbols::Marker::Dot)
        .graph_type(GraphType::Scatter)
        .style(Style::default().fg(Color::White))
        .data(&pts);
    let chart = Chart::new(vec![line, dots])
        .block(Block::default().borders(Borders::ALL).title(title))
        .x_axis(
            Axis::default()
                .title(ax.title())
                .style(Style::default().fg(Color::Gray))
                .bounds(ax.bounds())
                .labels(ax.labels()),
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
}

/// Cumulative counts as sent by the instrument, or counts per size interval.
fn series(app: &App, v: &[u64]) -> Vec<f64> {
    (0..v.len())
        .map(|i| {
            let x = if app.diff_mode {
                v[i].saturating_sub(v.get(i + 1).copied().unwrap_or(0))
            } else {
                v[i]
            };
            x as f64
        })
        .collect()
}

fn log_count(c: f64) -> f64 {
    if c <= 1.0 {
        0.0
    } else {
        c.log10()
    }
}

fn count_label(k: u32) -> String {
    match k {
        0..=2 => format!("{}", 10u64.pow(k)),
        3..=5 => format!("{}k", 10u64.pow(k - 3)),
        _ => format!("{}M", 10u64.pow(k - 6)),
    }
}

fn draw_distribution(f: &mut Frame, area: Rect, app: &App) {
    let mode = if app.diff_mode {
        "per size interval"
    } else {
        "cumulative, >= size"
    };
    let Some(m) = app.selected() else {
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
    let c = &m.counts;
    let n = c.up.len();
    let ax = XAxis::new(app, n);
    let up = series(app, &c.up);
    let down = series(app, &c.down);
    let pu: Vec<(f64, f64)> = (0..n).map(|i| (ax.map(ax.xs[i]), log_count(up[i]))).collect();
    let pd: Vec<(f64, f64)> = (0..n.min(down.len()))
        .map(|i| (ax.map(ax.xs[i]), log_count(down[i])))
        .collect();

    let top = pu
        .iter()
        .chain(pd.iter())
        .map(|p| p.1)
        .fold(0.0_f64, f64::max)
        .ceil()
        .max(1.0) as usize;
    let step = (top / 4).max(1);
    let ymax = ((top + step - 1) / step * step) as f64;
    let ylabels: Vec<Span> = (0..=(ymax as usize))
        .step_by(step)
        .map(|k| Span::raw(count_label(k as u32)))
        .collect();

    let title = format!(" Particle distribution, {} (m: toggle) ", mode);
    let d_up = Dataset::default()
        .name("upstream")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&pu);
    let d_down = Dataset::default()
        .name("downstream")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Yellow))
        .data(&pd);
    let chart = Chart::new(vec![d_up, d_down])
        .block(Block::default().borders(Borders::ALL).title(title))
        .x_axis(
            Axis::default()
                .title(ax.title())
                .style(Style::default().fg(Color::Gray))
                .bounds(ax.bounds())
                .labels(ax.labels()),
        )
        .y_axis(
            Axis::default()
                .title("counts (log)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, ymax])
                .labels(ylabels),
        );
    f.render_widget(chart, area);
}
