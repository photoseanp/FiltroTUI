use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::link::{Link, LinkCmd, LinkEvent};
use crate::persist::{self, Prefs};
use crate::protocol::{self, Counts};
use crate::settings::{Page, PsState, Settings};

const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    ProgramSettings,
    CareCenter,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Overview, Tab::ProgramSettings, Tab::CareCenter];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::ProgramSettings => "Program Settings",
            Tab::CareCenter => "Care Center",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    pub fn next(self) -> Tab {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Tab {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogKind {
    Tx,
    Rx,
    Info,
    Err,
}

pub struct LogLine {
    pub t: f64,
    pub kind: LogKind,
    pub text: String,
}

/// What the particle distribution bar chart shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DistMode {
    /// Cumulative counts (particles >= size).
    Cumulative,
    /// Counts per size interval.
    Interval,
    /// Mass concentration per size interval, spherical particles.
    Mass,
}

impl DistMode {
    pub fn from_u8(n: u8) -> DistMode {
        match n {
            1 => DistMode::Interval,
            2 => DistMode::Mass,
            _ => DistMode::Cumulative,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            DistMode::Cumulative => 0,
            DistMode::Interval => 1,
            DistMode::Mass => 2,
        }
    }

    pub fn next(self) -> DistMode {
        DistMode::from_u8((self.as_u8() + 1) % 3)
    }
}

/// One detection result with the local time it was received.
pub struct Measurement {
    pub at: String,
    /// Raw counts exactly as sent by the instrument.
    pub counts: Counts,
    /// Sample volume behind the raw counts, ml.
    pub volume_ml: f64,
}

/// Data shown by the charts and the extended view: one measurement or the average of the marked ones.
pub struct View {
    pub title: String,
    /// Raw counts (averaged when several measurements are marked).
    pub up: Vec<f64>,
    pub down: Vec<f64>,
    /// The same counts recalculated to the reference volume.
    pub up_n: Vec<f64>,
    pub down_n: Vec<f64>,
    /// Sample volume behind the raw counts, ml.
    pub volume_ml: f64,
    /// Reference volume, ml.
    pub ref_ml: f64,
    /// Number of measurements averaged (1 = a single measurement).
    pub n_meas: usize,
    /// Marked measurements skipped because their channel count differs.
    pub skipped: usize,
    /// Channel sizes in um (ascending) when known.
    pub sizes: Option<Vec<f64>>,
}

/// What a mouse click on a screen region does.
#[derive(Clone, Copy, Debug)]
pub enum Target {
    /// Top-level tab by index.
    Tab(usize),
    /// Program Settings page by index.
    Page(usize),
    /// Program Settings row by index in the field list.
    Field(usize),
    /// History row (0 = newest).
    History(usize),
    /// Port row in Care Center.
    Port(usize),
    /// Same as pressing this key on the Overview tab.
    Key(char),
}

/// A clickable screen region, filled in while drawing.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub target: Target,
}

pub const BAUDS: [u32; 8] = [1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200];

pub struct CareState {
    pub ports: Vec<String>,
    pub selected: usize,
    pub baud_idx: usize,
    pub connected: Option<String>,
    pub raw: Option<String>,
}

pub struct App {
    pub tab: Tab,
    pub should_quit: bool,
    pub settings: Settings,
    pub ps: PsState,
    pub care: CareState,
    pub log: Vec<LogLine>,
    pub status: String,
    /// Sampler pump state as last confirmed by the instrument (None = unknown).
    pub pump_up: Option<bool>,
    pub pump_down: Option<bool>,
    pub detecting: bool,
    pub flush_confirm: bool,
    pub results: Vec<Measurement>,
    /// History cursor, 0 = newest measurement.
    pub hist_sel: usize,
    /// Marked measurements (chronological indices) for averaging.
    pub marks: BTreeSet<usize>,
    /// Extended view of the selected / averaged measurement.
    pub detail_open: bool,
    pub detail_scroll: u16,
    /// Logarithmic particle-size axis on the efficiency chart.
    pub log_x: bool,
    /// Logarithmic bar height on the distribution chart.
    pub log_y: bool,
    pub dist_mode: DistMode,
    /// Number of channels in the last data frame from the instrument.
    pub instrument_channels: Option<usize>,
    /// Keep the number of enabled channels equal to the number the instrument sends.
    pub auto_sync: bool,
    /// Clickable regions of the last drawn frame.
    pub hits: RefCell<Vec<Hit>>,
    pref_port: Option<String>,
    saved_blob: String,
    outbox: VecDeque<String>,
    awaiting: Option<(String, Instant)>,
    link: Link,
    started: Instant,
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

impl App {
    pub fn new() -> Self {
        let link = Link::spawn();
        link.send(LinkCmd::ListPorts);

        let mut settings = Settings::default();
        let (prefs, saved_blob) = match persist::load_text() {
            Some(text) => {
                let p = persist::decode(&text, &mut settings);
                (p, text)
            }
            None => (Prefs::default(), String::new()),
        };
        let baud_idx = prefs
            .baud
            .and_then(|b| BAUDS.iter().position(|x| *x == b))
            .unwrap_or(3);

        Self {
            tab: Tab::Overview,
            should_quit: false,
            settings,
            ps: PsState::new(),
            care: CareState {
                ports: Vec::new(),
                selected: 0,
                baud_idx,
                connected: None,
                raw: None,
            },
            log: Vec::new(),
            status: String::new(),
            pump_up: None,
            pump_down: None,
            detecting: false,
            flush_confirm: false,
            results: Vec::new(),
            hist_sel: 0,
            marks: BTreeSet::new(),
            detail_open: false,
            detail_scroll: 0,
            log_x: prefs.log_x,
            log_y: prefs.log_y,
            dist_mode: DistMode::from_u8(prefs.dist_mode),
            instrument_channels: None,
            auto_sync: prefs.auto_sync,
            hits: RefCell::new(Vec::new()),
            pref_port: prefs.port,
            saved_blob,
            outbox: VecDeque::new(),
            awaiting: None,
            link,
            started: Instant::now(),
        }
    }

    pub fn baud(&self) -> u32 {
        BAUDS[self.care.baud_idx]
    }

    /// Registers a clickable region (called while drawing).
    pub fn add_hit(&self, rect: Rect, target: Target) {
        self.hits.borrow_mut().push(Hit { rect, target });
    }

    /// Chronological index of the history cursor.
    pub fn cursor_index(&self) -> Option<usize> {
        let n = self.results.len();
        if n == 0 {
            None
        } else {
            Some(n - 1 - self.hist_sel.min(n - 1))
        }
    }

    /// Factor that recalculates counts of `m` to the reference volume.
    pub fn scale_of(&self, m: &Measurement) -> f64 {
        if m.volume_ml > 0.0 {
            self.settings.analysis.ref_volume_ml / m.volume_ml
        } else {
            1.0
        }
    }

    /// Channel sizes (um, ascending) for `n` channels, when they can be assigned reliably:
    /// the number of enabled channels must equal `n` (or all 16 channels are reported).
    pub fn sizes_for(&self, n: usize) -> Option<Vec<f64>> {
        let enabled = self.settings.enabled_sizes();
        if enabled.len() == n {
            Some(enabled)
        } else if n == 16 {
            Some(self.settings.all_sizes_sorted())
        } else {
            None
        }
    }

    /// Data for the charts: the average of the marked measurements, or the one under the cursor.
    /// Counts are averaged first; beta and efficiency are calculated from the averages (ISO 16889).
    pub fn view(&self) -> Option<View> {
        let idx = self.cursor_index()?;
        let ref_ml = self.settings.analysis.ref_volume_ml;

        let marked: Vec<usize> = self
            .marks
            .iter()
            .copied()
            .filter(|i| *i < self.results.len())
            .collect();
        let (title, sel, skipped): (String, Vec<&Measurement>, usize) = if marked.is_empty() {
            let m = &self.results[idx];
            (format!("#{} {}", idx + 1, m.at), vec![m], 0)
        } else {
            let last = *marked.last()?;
            let len = self.results[last].counts.up.len();
            let sel: Vec<&Measurement> = marked
                .iter()
                .map(|i| &self.results[*i])
                .filter(|m| m.counts.up.len() == len && m.counts.down.len() == len)
                .collect();
            let skipped = marked.len() - sel.len();
            let title = if sel.len() == 1 {
                format!("#{} {}", last + 1, self.results[last].at)
            } else {
                format!("average of {} measurements", sel.len())
            };
            (title, sel, skipped)
        };

        let len = sel[0].counts.up.len();
        let k = sel.len() as f64;
        let mut up = vec![0.0; len];
        let mut down = vec![0.0; len];
        let mut up_n = vec![0.0; len];
        let mut down_n = vec![0.0; len];
        let mut vol = 0.0;
        for m in &sel {
            let scale = self.scale_of(m);
            for i in 0..len {
                let (u, d) = (m.counts.up[i] as f64, m.counts.down[i] as f64);
                up[i] += u;
                down[i] += d;
                up_n[i] += u * scale;
                down_n[i] += d * scale;
            }
            vol += m.volume_ml;
        }
        for i in 0..len {
            up[i] /= k;
            down[i] /= k;
            up_n[i] /= k;
            down_n[i] /= k;
        }
        Some(View {
            title,
            up,
            down,
            up_n,
            down_n,
            volume_ml: vol / k,
            ref_ml,
            n_meas: sel.len(),
            skipped,
            sizes: self.sizes_for(len),
        })
    }

    fn prefs(&self) -> Prefs {
        Prefs {
            port: self.pref_port.clone(),
            baud: Some(self.baud()),
            log_x: self.log_x,
            log_y: self.log_y,
            dist_mode: self.dist_mode.as_u8(),
            auto_sync: self.auto_sync,
        }
    }

    /// Writes the settings file when anything changed.
    fn persist(&mut self) {
        let text = persist::encode(&self.settings, &self.prefs());
        if text != self.saved_blob {
            if let Err(e) = persist::save(&text) {
                self.status = format!("Cannot save settings: {}", e);
            }
            self.saved_blob = text;
        }
    }

    fn push_log(&mut self, kind: LogKind, text: String) {
        let t = self.started.elapsed().as_secs_f64();
        self.log.push(LogLine { t, kind, text });
        if self.log.len() > 1000 {
            self.log.drain(..200);
        }
    }

    /// Queues one ASCII command (CR is appended on send).
    pub fn send_cmd(&mut self, cmd: &str) {
        if self.care.connected.is_none() {
            self.push_log(LogKind::Err, "Not connected - open a port in Care Center".into());
            self.status = "Not connected".into();
            return;
        }
        self.outbox.push_back(cmd.to_string());
    }

    fn pump_outbox(&mut self) {
        let timed_out = match &self.awaiting {
            Some((c, t)) if t.elapsed() > REPLY_TIMEOUT => Some(c.clone()),
            Some(_) => return,
            None => None,
        };
        if let Some(c) = timed_out {
            self.awaiting = None;
            self.push_log(LogKind::Err, format!("No reply to {}", c));
        }
        if let Some(cmd) = self.outbox.pop_front() {
            let long_running = cmd == protocol::START_DETECTION
                || cmd == protocol::FLUSH
                || cmd == protocol::PRINT;
            if !long_running {
                self.awaiting = Some((cmd.clone(), Instant::now()));
            }
            if cmd == protocol::START_DETECTION {
                self.detecting = true;
            }
            self.link.send(LinkCmd::Send(protocol::frame(&cmd)));
        }
    }

    fn send_current_page(&mut self) {
        match self.ps.page {
            Page::Run => {
                let wz = protocol::wz(&self.settings);
                let wh = protocol::wh(self.settings.run.print_mode);
                self.send_cmd(&wz);
                self.send_cmd(&wh);
            }
            Page::Channels => {
                let wt = protocol::wt(&self.settings);
                self.send_cmd(&wt);
            }
            Page::Flush => {
                if self.flush_confirm {
                    self.flush_confirm = false;
                    self.send_cmd(protocol::FLUSH);
                } else {
                    self.flush_confirm = true;
                    self.status =
                        "FLUSH: samplers must be disconnected from the system. Press w again to start".into();
                }
            }
            Page::Analysis => {
                self.status = "Analysis settings are used locally and are not sent to the instrument".into();
            }
        }
    }

    /// Makes the number of enabled channels equal to `n`, the count the instrument sends.
    fn sync_channels(&mut self, n: usize, auto: bool) {
        if n > 16 {
            self.push_log(LogKind::Err, format!("Instrument sent {} channels (max 16)", n));
            return;
        }
        match self.settings.sync_enabled_count(n) {
            Some((a, b)) => {
                let msg = format!(
                    "Channels synced with instrument: {} -> {} enabled (check which ones in Channel Setup)",
                    a, b
                );
                self.status = msg.clone();
                self.push_log(LogKind::Info, msg);
            }
            None => {
                if !auto {
                    self.status = format!("Channel count already matches ({})", n);
                }
            }
        }
    }

    fn sync_now(&mut self) {
        match self.instrument_channels {
            Some(n) => self.sync_channels(n, false),
            None => {
                self.status =
                    "No data frame yet - the channel count is known after the first detection".into();
            }
        }
    }

    fn detail_max_scroll(&self) -> u16 {
        self.view().map(|v| v.up.len() as u16 + 18).unwrap_or(0)
    }

    pub fn on_key(&mut self, ev: KeyEvent) {
        self.handle_key(ev);
        self.persist();
    }

    fn handle_key(&mut self, ev: KeyEvent) {
        if ev.modifiers.contains(KeyModifiers::CONTROL) && ev.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.tab == Tab::ProgramSettings && self.ps.editing.is_some() {
            self.ps.handle_key(&mut self.settings, ev);
            return;
        }
        if self.tab == Tab::CareCenter && self.care.raw.is_some() {
            self.raw_key(ev);
            return;
        }
        if self.tab == Tab::Overview && self.detail_open {
            let max = self.detail_max_scroll();
            match ev.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('v') => {
                    self.detail_open = false;
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.detail_scroll = (self.detail_scroll + 1).min(max);
                    return;
                }
                KeyCode::PageUp => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(10);
                    return;
                }
                KeyCode::PageDown => {
                    self.detail_scroll = (self.detail_scroll + 10).min(max);
                    return;
                }
                KeyCode::Tab | KeyCode::BackTab | KeyCode::Char('q') => {
                    self.detail_open = false;
                }
                _ => return,
            }
        }
        match ev.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                return;
            }
            KeyCode::Tab => {
                self.flush_confirm = false;
                self.tab = self.tab.next();
                return;
            }
            KeyCode::BackTab => {
                self.flush_confirm = false;
                self.tab = self.tab.prev();
                return;
            }
            _ => {}
        }

        match self.tab {
            Tab::Overview => self.overview_key(ev),
            Tab::ProgramSettings => match ev.code {
                KeyCode::Char('w') => self.send_current_page(),
                KeyCode::Char('a') => {
                    self.flush_confirm = false;
                    self.auto_sync = !self.auto_sync;
                    self.status = format!(
                        "Automatic channel-count sync {}",
                        if self.auto_sync { "ON" } else { "OFF" }
                    );
                    if self.auto_sync {
                        self.sync_now();
                    }
                }
                KeyCode::Char('c') => {
                    self.flush_confirm = false;
                    self.sync_now();
                }
                _ => {
                    self.flush_confirm = false;
                    self.ps.handle_key(&mut self.settings, ev);
                }
            },
            Tab::CareCenter => self.care_key(ev),
        }
    }

    fn overview_key(&mut self, ev: KeyEvent) {
        let n = self.results.len();
        match ev.code {
            KeyCode::Char('u') => {
                let cmd = if self.pump_up == Some(true) {
                    protocol::pump_off(false)
                } else {
                    protocol::pump_on(false)
                };
                self.send_cmd(&cmd);
            }
            KeyCode::Char('d') => {
                let cmd = if self.pump_down == Some(true) {
                    protocol::pump_off(true)
                } else {
                    protocol::pump_on(true)
                };
                self.send_cmd(&cmd);
            }
            KeyCode::Char('s') => self.send_cmd(protocol::START_DETECTION),
            KeyCode::Char('g') => self.send_cmd(protocol::SUSPEND_DETECTION),
            KeyCode::Char('p') => self.send_cmd(protocol::PRINT),
            KeyCode::Char('c') => self.sync_now(),
            KeyCode::Char('l') => self.log_x = !self.log_x,
            KeyCode::Char('y') => self.log_y = !self.log_y,
            KeyCode::Char('m') => self.dist_mode = self.dist_mode.next(),
            KeyCode::Char(' ') => {
                if let Some(i) = self.cursor_index() {
                    if !self.marks.remove(&i) {
                        self.marks.insert(i);
                    }
                }
            }
            KeyCode::Char('a') => self.marks = (0..n).collect(),
            KeyCode::Char('n') => self.marks.clear(),
            KeyCode::Enter | KeyCode::Char('v') => {
                if n > 0 {
                    self.detail_open = true;
                    self.detail_scroll = 0;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.hist_sel + 1 < n {
                    self.hist_sel += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.hist_sel = self.hist_sel.saturating_sub(1),
            KeyCode::PageDown => self.hist_sel = (self.hist_sel + 5).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.hist_sel = self.hist_sel.saturating_sub(5),
            KeyCode::Home => self.hist_sel = 0,
            KeyCode::End => self.hist_sel = n.saturating_sub(1),
            _ => {}
        }
    }

    fn raw_key(&mut self, ev: KeyEvent) {
        match ev.code {
            KeyCode::Esc => self.care.raw = None,
            KeyCode::Backspace => {
                if let Some(b) = self.care.raw.as_mut() {
                    b.pop();
                }
            }
            KeyCode::Enter => {
                let text = self.care.raw.take().unwrap_or_default();
                if !text.is_empty() {
                    self.send_cmd(&text);
                }
            }
            KeyCode::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                if let Some(b) = self.care.raw.as_mut() {
                    if b.len() < 200 {
                        b.push(c);
                    }
                }
            }
            _ => {}
        }
    }

    fn care_key(&mut self, ev: KeyEvent) {
        let n = self.care.ports.len();
        match ev.code {
            KeyCode::Up if n > 0 => self.care.selected = (self.care.selected + n - 1) % n,
            KeyCode::Down if n > 0 => self.care.selected = (self.care.selected + 1) % n,
            KeyCode::Left => {
                self.care.baud_idx = (self.care.baud_idx + BAUDS.len() - 1) % BAUDS.len()
            }
            KeyCode::Right => self.care.baud_idx = (self.care.baud_idx + 1) % BAUDS.len(),
            KeyCode::F(5) => self.link.send(LinkCmd::ListPorts),
            KeyCode::Enter => {
                if let Some(name) = self.care.ports.get(self.care.selected).cloned() {
                    if self.care.connected.is_some() {
                        self.link.send(LinkCmd::Close);
                    }
                    let baud = self.baud();
                    self.link.send(LinkCmd::Open { port: name, baud });
                }
            }
            KeyCode::Char('d') => self.link.send(LinkCmd::Close),
            KeyCode::Char('x') => self.log.clear(),
            KeyCode::Char('/') => self.care.raw = Some(String::new()),
            _ => {}
        }
        if matches!(ev.code, KeyCode::Up | KeyCode::Down | KeyCode::Enter) {
            if let Some(p) = self.care.ports.get(self.care.selected).cloned() {
                self.pref_port = Some(p);
            }
        }
    }

    // ---------------------------------------------------------------- mouse

    pub fn on_mouse(&mut self, m: MouseEvent) {
        self.handle_mouse(m);
        self.persist();
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        // While a value is being typed the mouse is ignored.
        if (self.tab == Tab::ProgramSettings && self.ps.editing.is_some())
            || (self.tab == Tab::CareCenter && self.care.raw.is_some())
        {
            return;
        }
        match m.kind {
            MouseEventKind::Down(btn) if btn == MouseButton::Left || btn == MouseButton::Right => {
                if self.tab == Tab::Overview && self.detail_open {
                    self.detail_open = false;
                    return;
                }
                let target = self
                    .hits
                    .borrow()
                    .iter()
                    .rev()
                    .find(|h| rect_contains(h.rect, m.column, m.row))
                    .map(|h| h.target);
                if let Some(t) = target {
                    self.click(t, btn == MouseButton::Right);
                }
            }
            MouseEventKind::ScrollUp => self.wheel(-1),
            MouseEventKind::ScrollDown => self.wheel(1),
            MouseEventKind::ScrollLeft => self.hscroll(-1),
            MouseEventKind::ScrollRight => self.hscroll(1),
            _ => {}
        }
    }

    fn click(&mut self, t: Target, right: bool) {
        match t {
            Target::Tab(i) => {
                self.flush_confirm = false;
                self.tab = Tab::ALL[i % Tab::ALL.len()];
            }
            Target::Page(i) => {
                if self.tab == Tab::ProgramSettings {
                    self.flush_confirm = false;
                    self.ps.page = Page::ALL[i % Page::ALL.len()];
                    self.ps.selected = 0;
                }
            }
            Target::Field(i) => {
                let nf = self.settings.fields(self.ps.page).len();
                if i >= nf {
                    return;
                }
                self.flush_confirm = false;
                if right {
                    self.ps.selected = i;
                    self.ps.handle_key(&mut self.settings, key(KeyCode::Char(' ')));
                } else if self.ps.selected == i {
                    self.ps.handle_key(&mut self.settings, key(KeyCode::Enter));
                } else {
                    self.ps.selected = i;
                }
            }
            Target::History(i) => {
                let n = self.results.len();
                if i >= n {
                    return;
                }
                if right {
                    self.hist_sel = i;
                    self.overview_key(key(KeyCode::Char(' ')));
                } else if self.hist_sel == i {
                    self.overview_key(key(KeyCode::Enter));
                } else {
                    self.hist_sel = i;
                }
            }
            Target::Port(i) => {
                if i >= self.care.ports.len() {
                    return;
                }
                if self.care.selected == i && !right {
                    self.care_key(key(KeyCode::Enter));
                } else {
                    self.care.selected = i;
                    self.pref_port = self.care.ports.get(i).cloned();
                }
            }
            Target::Key(c) => {
                if self.tab == Tab::Overview {
                    self.overview_key(key(KeyCode::Char(c)));
                }
            }
        }
    }

    fn wheel(&mut self, dir: i32) {
        match self.tab {
            Tab::Overview => {
                if self.detail_open {
                    let max = self.detail_max_scroll();
                    self.detail_scroll = if dir > 0 {
                        (self.detail_scroll + 3).min(max)
                    } else {
                        self.detail_scroll.saturating_sub(3)
                    };
                } else {
                    let n = self.results.len();
                    if dir > 0 {
                        if self.hist_sel + 1 < n {
                            self.hist_sel += 1;
                        }
                    } else {
                        self.hist_sel = self.hist_sel.saturating_sub(1);
                    }
                }
            }
            Tab::ProgramSettings => {
                let n = self.settings.fields(self.ps.page).len();
                if n > 0 {
                    let cur = self.ps.selected.min(n - 1) as i32;
                    self.ps.selected = (cur + dir).clamp(0, n as i32 - 1) as usize;
                }
            }
            Tab::CareCenter => {
                let n = self.care.ports.len();
                if n > 0 {
                    let cur = self.care.selected.min(n - 1) as i32;
                    self.care.selected = (cur + dir).clamp(0, n as i32 - 1) as usize;
                    self.pref_port = self.care.ports.get(self.care.selected).cloned();
                }
            }
        }
    }

    fn hscroll(&mut self, dir: i32) {
        match self.tab {
            Tab::ProgramSettings => {
                let code = if dir < 0 { KeyCode::Left } else { KeyCode::Right };
                self.ps.handle_key(&mut self.settings, key(code));
            }
            Tab::CareCenter => {
                let code = if dir < 0 { KeyCode::Left } else { KeyCode::Right };
                self.care_key(key(code));
            }
            Tab::Overview => {}
        }
    }

    // ------------------------------------------------------------ instrument

    fn reset_session(&mut self) {
        self.outbox.clear();
        self.awaiting = None;
        self.pump_up = None;
        self.pump_down = None;
        self.detecting = false;
        self.flush_confirm = false;
    }

    fn handle_reply(&mut self, text: &str) {
        let waiting = self.awaiting.as_ref().map(|(c, _)| c.clone());
        match text {
            "WM" => {
                self.status = "Instrument replied WM (error)".to_string();
                self.awaiting = None;
                self.outbox.clear();
            }
            "WD" | "WG" => {
                self.detecting = false;
                self.awaiting = None;
                self.status = format!("{} done", text);
            }
            other => {
                if let Some(c) = waiting {
                    if c.starts_with(other) {
                        let on = other == "WO";
                        if other == "WO" || other == "WL" {
                            if c.ends_with('1') {
                                self.pump_down = Some(on);
                            } else {
                                self.pump_up = Some(on);
                            }
                        }
                        self.awaiting = None;
                        self.status = format!("{} OK", c);
                    }
                }
            }
        }
    }

    pub fn on_tick(&mut self) {
        let events: Vec<LinkEvent> = self.link.rx.try_iter().collect();
        for ev in events {
            match ev {
                LinkEvent::Ports(p) => {
                    self.care.ports = p;
                    if self.care.selected >= self.care.ports.len() {
                        self.care.selected = 0;
                    }
                    if let Some(pp) = &self.pref_port {
                        if let Some(i) = self.care.ports.iter().position(|x| x == pp) {
                            self.care.selected = i;
                        }
                    }
                }
                LinkEvent::Opened(name) => {
                    self.reset_session();
                    let baud = self.baud();
                    self.push_log(LogKind::Info, format!("Opened {} @ {} 8N1", name, baud));
                    self.care.connected = Some(name);
                }
                LinkEvent::Closed => {
                    self.reset_session();
                    self.push_log(LogKind::Info, "Port closed".into());
                    self.care.connected = None;
                }
                LinkEvent::Sent(b) => {
                    let text = String::from_utf8_lossy(&b).trim_end_matches('\r').to_string();
                    self.push_log(LogKind::Tx, text);
                }
                LinkEvent::Received(b) => {
                    let text = String::from_utf8_lossy(&b).trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    self.push_log(LogKind::Rx, text.clone());
                    if text.starts_with("RB") {
                        match protocol::parse_rb(&text) {
                            Ok(c) => {
                                let n = c.up.len();
                                let at = chrono::Local::now().format("%H:%M:%S").to_string();
                                let volume_ml = self.settings.effective_volume_ml();
                                self.results.push(Measurement { at, counts: c, volume_ml });
                                self.hist_sel = 0;
                                self.status = format!("Data frame #{} received", self.results.len());
                                self.instrument_channels = Some(n);
                                if self.auto_sync {
                                    self.sync_channels(n, true);
                                }
                            }
                            Err(e) => {
                                self.push_log(
                                    LogKind::Err,
                                    format!("RB frame not parsed: {} (frame length {})", e, text.len()),
                                );
                            }
                        }
                    } else {
                        self.handle_reply(&text);
                    }
                }
                LinkEvent::Error(e) => {
                    self.status = format!("Error: {}", e);
                    self.push_log(LogKind::Err, e);
                }
            }
        }
        self.pump_outbox();
        self.persist();
    }
}
