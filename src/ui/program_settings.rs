use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

use super::{list_offset, tab_hits};
use crate::app::{App, Target};
use crate::protocol;
use crate::settings::{AutoManual, Field, Page, RunMode};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let titles: Vec<Line> = Page::ALL.iter().map(|p| Line::from(p.title())).collect();
    let pages = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Program Settings  ( [ ] switch page, saved automatically ) "),
        )
        .select(app.ps.page.index())
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(pages, rows[0]);
    let names: Vec<&str> = Page::ALL.iter().map(|p| p.title()).collect();
    tab_hits(app, rows[0], &names, Target::Page);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[1]);

    draw_form(f, cols[0], app);
    draw_summary(f, cols[1], app);

    let hint = "Up/Down select | Left/Right change | Enter edit | Space toggle | w send | a auto-sync | c sync now | r defaults";
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
}

fn draw_form(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.settings;
    let fields = s.fields(app.ps.page);
    let n = fields.len();
    let sel = app.ps.selected.min(n.saturating_sub(1));
    let visible = area.height.saturating_sub(2) as usize;
    let off = list_offset(sel, visible);

    let items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(i, fld)| {
            let value = match (&app.ps.editing, i == sel) {
                (Some(buf), true) => format!("{}_", buf),
                _ => s.display(*fld),
            };
            let dim = match fld {
                Field::Channel(c) => !s.channels[*c].enabled,
                _ => false,
            };
            let style = if dim {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{:<22}", s.label(*fld))),
                Span::styled(value, style),
            ]))
        })
        .collect();

    let title = if app.ps.editing.is_some() {
        format!(" {} - typing (Enter ok / Esc cancel) ", app.ps.page.title())
    } else {
        format!(" {} ", app.ps.page.title())
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default().with_offset(off).with_selected(Some(sel));
    f.render_stateful_widget(list, area, &mut state);

    for r in 0..visible.min(n.saturating_sub(off)) {
        app.add_hit(
            Rect {
                x: area.x + 1,
                y: area.y + 1 + r as u16,
                width: area.width.saturating_sub(2),
                height: 1,
            },
            Target::Field(off + r),
        );
    }
}

fn draw_summary(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.settings;
    let r = &s.run;
    let mut lines: Vec<Line> = Vec::new();

    match app.ps.page {
        Page::Run => {
            lines.push(Line::from(format!("Mode:             {}", r.mode.label())));
            lines.push(Line::from(format!("Sampler flow:     {} ml/min", s.flow_ml_min())));
            lines.push(Line::from(format!("Flow x counting:  {:.2} ml", s.sample_volume_ml())));
            let cycle = (r.counting_time_s + r.interval_s).max(1) as f64;
            match r.mode {
                RunMode::Initial => lines.push(Line::from(format!(
                    "Counts in 60 min: ~{}",
                    (3600.0 / cycle).floor()
                ))),
                RunMode::Formal => lines.push(Line::from(format!(
                    "Counts in test:   ~{}",
                    (r.test_time_min as f64 * 60.0 / cycle).floor()
                ))),
                RunMode::Single => {
                    let n = if r.sample_mode == AutoManual::Auto { r.detections } else { 1 };
                    lines.push(Line::from(format!("Counts per start: {}", n)));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Sent by w: {} then {}",
                protocol::wz(s),
                protocol::wh(r.print_mode)
            )));
            lines.push(Line::from("Sampler speed is not settable over RS232 - set it on the instrument."));
        }
        Page::Channels => {
            let sizes = s.enabled_sizes();
            lines.push(Line::from(format!("Enabled: {} / 16", sizes.len())));
            let txt: Vec<String> = sizes.iter().map(|v| format!("{}", v)).collect();
            lines.push(Line::from(format!("Sorted sizes (um): {}", txt.join(", "))));
            lines.push(Line::from(""));
            let (text, color) = match app.instrument_channels {
                Some(n) if n == sizes.len() => {
                    (format!("Instrument sends {} channels - matches.", n), Color::Green)
                }
                Some(n) => (
                    format!(
                        "Instrument sends {} channels, {} enabled here - press c to sync.",
                        n,
                        sizes.len()
                    ),
                    Color::Yellow,
                ),
                None => (
                    "Instrument channel count unknown until the first detection.".to_string(),
                    Color::Gray,
                ),
            };
            lines.push(Line::from(Span::styled(text, Style::default().fg(color))));
            lines.push(Line::from(format!(
                "Auto-sync (a): {}",
                if app.auto_sync { "ON" } else { "OFF" }
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(format!("Sent by w: {}", protocol::wt(s))));
            lines.push(Line::from(Span::styled(
                "WT always sends all 16 sizes (um x 10, 4 digits each); the protocol cannot read or set which channels are selected on the instrument. Sync only matches the COUNT - verify which channels are enabled.",
                Style::default().fg(Color::Yellow),
            )));
        }
        Page::Flush => {
            lines.push(Line::from(format!("Cleaning time: {} s", s.flush.time_s)));
            lines.push(Line::from(format!("Direction: {}", s.flush.direction.label())));
            lines.push(Line::from(format!("Sensor: {}", s.flush.sensor.label())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "WF runs the flush using the settings stored ON THE INSTRUMENT. The protocol cannot set time, direction or sensor - check them on the instrument screen.",
                Style::default().fg(Color::Yellow),
            )));
            lines.push(Line::from(Span::styled(
                "Never flush while the samplers are connected to the test system! A wrong direction can damage the metering pump.",
                Style::default().fg(Color::Red),
            )));
            if app.flush_confirm {
                lines.push(Line::from(Span::styled(
                    "Press w again to send WF, any other key cancels.",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )));
            }
        }
        Page::Analysis => {
            let a = &s.analysis;
            let eff_vol = s.effective_volume_ml();
            lines.push(Line::from("Local analysis settings - not sent to the instrument."));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Raw counts come from a {:.1} ml sample. Everything else is recalculated to {:.0} ml: N_ref = N_raw x {:.0} / {:.1} = N_raw x {:.3}.",
                eff_vol,
                a.ref_volume_ml,
                a.ref_volume_ml,
                eff_vol,
                a.ref_volume_ml / eff_vol.max(1e-9)
            )));
            lines.push(Line::from(
                "Sample volume 0 = flow x counting time from Run Setup. Beta and efficiency are ratios and do not depend on the volume.",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Density {:.2} g/cm3: used for the mass distribution (m on Overview), concentrations in mg/mL.",
                a.density
            )));
            lines.push(Line::from(
                "Mass = counts per interval x rho x pi/6 x d^3, d = geometric mean of the interval bounds; the last channel is an open tail counted with its lower bound.",
            ));
            lines.push(Line::from(""));
            let cuts: Vec<String> = a
                .cutoffs
                .iter()
                .filter(|c| **c > 0.0)
                .map(|c| format!("{:.1}", c))
                .collect();
            lines.push(Line::from(format!(
                "Cut-offs (um): {}",
                if cuts.is_empty() { "none".to_string() } else { cuts.join(", ") }
            )));
            lines.push(Line::from(
                "Vertical lines on the efficiency chart, coloured group labels on the distribution bars. 0 = off.",
            ));
        }
    }

    lines.push(Line::from(""));
    let warns = s.warnings();
    if warns.is_empty() {
        lines.push(Line::from(Span::styled("Settings OK", Style::default().fg(Color::Green))));
    } else {
        for w in warns {
            lines.push(Line::from(Span::styled(
                format!("! {}", w),
                Style::default().fg(Color::Red),
            )));
        }
    }

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Summary "))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}
