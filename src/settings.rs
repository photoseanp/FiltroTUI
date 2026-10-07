//! KZD-3A program settings model (Run / Channel / Flush setup).
//! Ranges follow the KZD-3A instruction manual (sections 5.3, 5.4, 5.8, Appendix IV).

use crossterm::event::{KeyCode, KeyEvent};

pub fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, dir: i32) -> T {
    let n = all.len() as i32;
    let i = all.iter().position(|x| *x == cur).unwrap_or(0) as i32;
    all[(i + dir).rem_euclid(n) as usize]
}

pub fn index_of<T: PartialEq>(all: &[T], v: T) -> usize {
    all.iter().position(|x| *x == v).unwrap_or(0)
}

pub fn at<T: Copy>(all: &[T], i: usize, default: T) -> T {
    all.get(i).copied().unwrap_or(default)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunMode {
    Initial,
    Formal,
    Single,
}
impl RunMode {
    pub const ALL: [RunMode; 3] = [RunMode::Initial, RunMode::Formal, RunMode::Single];
    pub fn label(self) -> &'static str {
        match self {
            RunMode::Initial => "INITIAL (upstream baseline)",
            RunMode::Formal => "FORMAL (upstream + downstream)",
            RunMode::Single => "SINGLE (particle counter)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sensor {
    Upstream,
    Downstream,
    Both,
}
impl Sensor {
    pub const ALL: [Sensor; 3] = [Sensor::Upstream, Sensor::Downstream, Sensor::Both];
    pub fn label(self) -> &'static str {
        match self {
            Sensor::Upstream => "Upstream",
            Sensor::Downstream => "Downstream",
            Sensor::Both => "Both",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AutoManual {
    Manual,
    Auto,
}
impl AutoManual {
    pub const ALL: [AutoManual; 2] = [AutoManual::Manual, AutoManual::Auto];
    pub fn label(self) -> &'static str {
        match self {
            AutoManual::Manual => "Manual",
            AutoManual::Auto => "Automatic",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Forward,
    Reverse,
}
impl Direction {
    pub const ALL: [Direction; 2] = [Direction::Forward, Direction::Reverse];
    pub fn label(self) -> &'static str {
        match self {
            Direction::Forward => "Forward",
            Direction::Reverse => "Reverse",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunSetup {
    pub mode: RunMode,
    pub counting_time_s: u32,
    pub interval_s: u32,
    pub test_time_min: u32,
    pub dilution: u32,
    pub sensor: Sensor,
    pub sample_mode: AutoManual,
    pub detections: u32,
    pub print_mode: AutoManual,
    pub flow_level: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub enabled: bool,
    pub size_um: f64,
}

#[derive(Clone, Debug)]
pub struct FlushSetup {
    pub time_s: u32,
    pub direction: Direction,
    pub sensor: Sensor,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub run: RunSetup,
    pub channels: [Channel; 16],
    pub flush: FlushSetup,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Run,
    Channels,
    Flush,
}
impl Page {
    pub const ALL: [Page; 3] = [Page::Run, Page::Channels, Page::Flush];
    pub fn title(self) -> &'static str {
        match self {
            Page::Run => "Run Setup",
            Page::Channels => "Channel Setup",
            Page::Flush => "Flush Setup",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
    pub fn step(self, dir: i32) -> Page {
        cycle(&Self::ALL, self, dir)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    RunMode,
    CountTime,
    Interval,
    TestTime,
    Dilution,
    Sensor,
    SampleMode,
    Detections,
    PrintMode,
    FlowLevel,
    FlushTime,
    FlushDir,
    FlushSensor,
    Channel(usize),
}

pub enum Kind {
    Num { min: f64, max: f64, step: f64 },
    Choice,
}

impl Default for Settings {
    fn default() -> Self {
        // Channel sizes are placeholders - edit them to match your calibration.
        let sizes = [
            4.0, 5.0, 6.0, 7.0, 10.0, 14.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0,
            100.0,
        ];
        let mut channels = [Channel { enabled: false, size_um: 0.0 }; 16];
        for (i, c) in channels.iter_mut().enumerate() {
            c.size_um = sizes[i];
            c.enabled = i < 8;
        }
        Self {
            run: RunSetup {
                mode: RunMode::Formal,
                counting_time_s: 20,
                interval_s: 10,
                test_time_min: 60,
                dilution: 1,
                sensor: Sensor::Both,
                sample_mode: AutoManual::Manual,
                detections: 1,
                print_mode: AutoManual::Manual,
                flow_level: 2,
            },
            channels,
            flush: FlushSetup {
                time_s: 30,
                direction: Direction::Forward,
                sensor: Sensor::Both,
            },
        }
    }
}

impl Settings {
    pub fn fields(&self, page: Page) -> Vec<Field> {
        match page {
            Page::Run => {
                let mut v = vec![Field::RunMode];
                match self.run.mode {
                    RunMode::Initial => {
                        v.extend([Field::CountTime, Field::Interval, Field::PrintMode]);
                    }
                    RunMode::Formal => {
                        v.extend([
                            Field::CountTime,
                            Field::Interval,
                            Field::TestTime,
                            Field::Dilution,
                            Field::PrintMode,
                        ]);
                    }
                    RunMode::Single => {
                        v.extend([
                            Field::CountTime,
                            Field::Sensor,
                            Field::SampleMode,
                            Field::Detections,
                            Field::PrintMode,
                        ]);
                    }
                }
                v.push(Field::FlowLevel);
                v
            }
            Page::Channels => (0..16).map(Field::Channel).collect(),
            Page::Flush => vec![Field::FlushTime, Field::FlushDir, Field::FlushSensor],
        }
    }

    pub fn label(&self, f: Field) -> String {
        match f {
            Field::RunMode => "Run mode".into(),
            Field::CountTime => "Counting time".into(),
            Field::Interval => match self.run.mode {
                RunMode::Initial => "Initial interval".into(),
                _ => "Interval".into(),
            },
            Field::TestTime => "Test time".into(),
            Field::Dilution => "Dilution ratio".into(),
            Field::Sensor => "Sensor".into(),
            Field::SampleMode => "Sample mode".into(),
            Field::Detections => "Detection frequency".into(),
            Field::PrintMode => "Print mode".into(),
            Field::FlowLevel => "Sampler speed".into(),
            Field::FlushTime => "Cleaning time".into(),
            Field::FlushDir => "Running direction".into(),
            Field::FlushSensor => "Sensor".into(),
            Field::Channel(i) => format!("Channel {:02}", i + 1),
        }
    }

    pub fn kind(&self, f: Field) -> Kind {
        match f {
            Field::CountTime => Kind::Num { min: 5.0, max: 60.0, step: 1.0 },
            Field::Interval => Kind::Num { min: 1.0, max: 500.0, step: 1.0 },
            Field::TestTime => Kind::Num { min: 5.0, max: 240.0, step: 5.0 },
            Field::Dilution => Kind::Num { min: 1.0, max: 10.0, step: 1.0 },
            Field::Detections => Kind::Num { min: 1.0, max: 5.0, step: 1.0 },
            Field::FlowLevel => Kind::Num { min: 0.0, max: 5.0, step: 1.0 },
            Field::FlushTime => Kind::Num { min: 1.0, max: 132.0, step: 1.0 },
            Field::Channel(_) => Kind::Num { min: 0.1, max: 999.9, step: 1.0 },
            _ => Kind::Choice,
        }
    }

    pub fn flow_ml_min(&self) -> f64 {
        10.0 + 5.0 * self.run.flow_level as f64
    }

    fn get_num(&self, f: Field) -> f64 {
        match f {
            Field::CountTime => self.run.counting_time_s as f64,
            Field::Interval => self.run.interval_s as f64,
            Field::TestTime => self.run.test_time_min as f64,
            Field::Dilution => self.run.dilution as f64,
            Field::Detections => self.run.detections as f64,
            Field::FlowLevel => self.run.flow_level as f64,
            Field::FlushTime => self.flush.time_s as f64,
            Field::Channel(i) => self.channels[i].size_um,
            _ => 0.0,
        }
    }

    pub fn set_num(&mut self, f: Field, v: f64) {
        if let Kind::Num { min, max, .. } = self.kind(f) {
            let v = v.clamp(min, max);
            let u = v.round() as u32;
            match f {
                Field::CountTime => self.run.counting_time_s = u,
                Field::Interval => self.run.interval_s = u,
                Field::TestTime => self.run.test_time_min = u,
                Field::Dilution => self.run.dilution = u,
                Field::Detections => self.run.detections = u,
                Field::FlowLevel => self.run.flow_level = u,
                Field::FlushTime => self.flush.time_s = u,
                Field::Channel(i) => self.channels[i].size_um = (v * 10.0).round() / 10.0,
                _ => {}
            }
        }
    }

    pub fn adjust(&mut self, f: Field, dir: i32) {
        match f {
            Field::RunMode => self.run.mode = cycle(&RunMode::ALL, self.run.mode, dir),
            Field::Sensor => self.run.sensor = cycle(&Sensor::ALL, self.run.sensor, dir),
            Field::SampleMode => {
                self.run.sample_mode = cycle(&AutoManual::ALL, self.run.sample_mode, dir)
            }
            Field::PrintMode => {
                self.run.print_mode = cycle(&AutoManual::ALL, self.run.print_mode, dir)
            }
            Field::FlushDir => {
                self.flush.direction = cycle(&Direction::ALL, self.flush.direction, dir)
            }
            Field::FlushSensor => self.flush.sensor = cycle(&Sensor::ALL, self.flush.sensor, dir),
            _ => {
                if let Kind::Num { step, .. } = self.kind(f) {
                    let v = self.get_num(f) + step * dir as f64;
                    self.set_num(f, v);
                }
            }
        }
    }

    pub fn display(&self, f: Field) -> String {
        match f {
            Field::RunMode => self.run.mode.label().into(),
            Field::CountTime => format!("{} s", self.run.counting_time_s),
            Field::Interval => format!("{} s", self.run.interval_s),
            Field::TestTime => format!("{} min", self.run.test_time_min),
            Field::Dilution => format!("{}", self.run.dilution),
            Field::Sensor => self.run.sensor.label().into(),
            Field::SampleMode => self.run.sample_mode.label().into(),
            Field::Detections => format!("{}", self.run.detections),
            Field::PrintMode => self.run.print_mode.label().into(),
            Field::FlowLevel => format!("F{} ({} ml/min)", self.run.flow_level, self.flow_ml_min()),
            Field::FlushTime => format!("{} s", self.flush.time_s),
            Field::FlushDir => self.flush.direction.label().into(),
            Field::FlushSensor => self.flush.sensor.label().into(),
            Field::Channel(i) => {
                let c = self.channels[i];
                format!("[{}] {:.1} um", if c.enabled { "x" } else { " " }, c.size_um)
            }
        }
    }

    pub fn enabled_count(&self) -> usize {
        self.channels.iter().filter(|c| c.enabled).count()
    }

    /// Enabled channel sizes sorted ascending (the instrument sorts them itself).
    pub fn enabled_sizes(&self) -> Vec<f64> {
        let mut v: Vec<f64> = self
            .channels
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.size_um)
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// All 16 channel sizes sorted ascending.
    pub fn all_sizes_sorted(&self) -> Vec<f64> {
        let mut v: Vec<f64> = self.channels.iter().map(|c| c.size_um).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Makes the number of enabled channels equal to `n` (the number the instrument sends).
    /// Too many enabled: the largest sizes are disabled; too few: the smallest disabled sizes
    /// are enabled. Returns Some((old, new)) when something changed.
    pub fn sync_enabled_count(&mut self, n: usize) -> Option<(usize, usize)> {
        let n = n.min(16);
        let cur = self.enabled_count();
        if cur == n {
            return None;
        }
        let mut order: Vec<usize> = (0..16).collect();
        order.sort_by(|&a, &b| {
            self.channels[a]
                .size_um
                .partial_cmp(&self.channels[b].size_um)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        if cur > n {
            let mut extra = cur - n;
            for &i in order.iter().rev() {
                if extra == 0 {
                    break;
                }
                if self.channels[i].enabled {
                    self.channels[i].enabled = false;
                    extra -= 1;
                }
            }
        } else {
            let mut need = n - cur;
            for &i in order.iter() {
                if need == 0 {
                    break;
                }
                if !self.channels[i].enabled {
                    self.channels[i].enabled = true;
                    need -= 1;
                }
            }
        }
        Some((cur, n))
    }

    pub fn sample_volume_ml(&self) -> f64 {
        self.flow_ml_min() * self.run.counting_time_s as f64 / 60.0
    }

    pub fn warnings(&self) -> Vec<String> {
        let r = &self.run;
        let mut w = Vec::new();
        if r.mode == RunMode::Initial {
            if r.interval_s >= 30 {
                w.push("INITIAL: interval should be < 30 s (GB/T 18853, 8.1.6)".to_string());
            }
            if r.counting_time_s + r.interval_s >= 60 {
                w.push("INITIAL: counting time + interval should be < 60 s".to_string());
            }
        }
        if r.mode == RunMode::Single && r.counting_time_s > 50 {
            w.push("SINGLE: RS232 protocol lists counting time 5-50 s".to_string());
        }
        let sizes = self.enabled_sizes();
        if sizes.is_empty() {
            w.push("No channels enabled".to_string());
        }
        if sizes.windows(2).any(|p| (p[0] - p[1]).abs() < 1e-9) {
            w.push("Duplicate sizes among enabled channels".to_string());
        }
        w
    }
}

/// UI state of the Program Settings window.
pub struct PsState {
    pub page: Page,
    pub selected: usize,
    pub editing: Option<String>,
}

impl PsState {
    pub fn new() -> Self {
        Self { page: Page::Run, selected: 0, editing: None }
    }

    pub fn handle_key(&mut self, s: &mut Settings, key: KeyEvent) {
        let fields = s.fields(self.page);
        if fields.is_empty() {
            return;
        }
        if self.selected >= fields.len() {
            self.selected = fields.len() - 1;
        }
        let f = fields[self.selected];

        if self.editing.is_some() {
            match key.code {
                KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                    if let Some(b) = self.editing.as_mut() {
                        if b.len() < 8 {
                            b.push(c);
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let Some(b) = self.editing.as_mut() {
                        b.pop();
                    }
                }
                KeyCode::Enter => {
                    let parsed = self.editing.as_ref().and_then(|b| b.parse::<f64>().ok());
                    if let Some(v) = parsed {
                        s.set_num(f, v);
                    }
                    self.editing = None;
                }
                KeyCode::Esc => self.editing = None,
                _ => {}
            }
            return;
        }

        let n = fields.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.selected = (self.selected + n - 1) % n,
            KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1) % n,
            KeyCode::Left | KeyCode::Char('h') => s.adjust(f, -1),
            KeyCode::Right | KeyCode::Char('l') => s.adjust(f, 1),
            KeyCode::Enter => match s.kind(f) {
                Kind::Num { .. } => self.editing = Some(String::new()),
                Kind::Choice => s.adjust(f, 1),
            },
            KeyCode::Char(' ') => {
                if let Field::Channel(i) = f {
                    s.channels[i].enabled = !s.channels[i].enabled;
                } else {
                    s.adjust(f, 1);
                }
            }
            KeyCode::Char('[') | KeyCode::PageUp => {
                self.page = self.page.step(-1);
                self.selected = 0;
            }
            KeyCode::Char(']') | KeyCode::PageDown => {
                self.page = self.page.step(1);
                self.selected = 0;
            }
            KeyCode::Char('r') => *s = Settings::default(),
            _ => {}
        }
    }
}
