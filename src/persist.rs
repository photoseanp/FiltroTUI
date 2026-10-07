//! Saving / restoring program settings (plain `key=value` text, no extra dependencies).

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::settings::{
    at, index_of, AutoManual, Direction, Field, RunMode, Sensor, Settings,
};

/// Non-`Settings` values that are remembered between runs.
pub struct Prefs {
    pub port: Option<String>,
    pub baud: Option<u32>,
    pub log_x: bool,
    pub log_y: bool,
    /// 0 = cumulative, 1 = per interval, 2 = mass.
    pub dist_mode: u8,
    pub auto_sync: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { port: None, baud: None, log_x: false, log_y: true, dist_mode: 0, auto_sync: true }
    }
}

pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("filtrotui").join("settings.conf"))
}

pub fn load_text() -> Option<String> {
    fs::read_to_string(path()?).ok()
}

pub fn save(text: &str) -> io::Result<()> {
    let p = path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory"))?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(p, text)
}

pub fn encode(s: &Settings, prefs: &Prefs) -> String {
    let r = &s.run;
    let mut o = String::new();
    let mut put = |k: &str, v: String| {
        o.push_str(k);
        o.push('=');
        o.push_str(&v);
        o.push('\n');
    };
    put("mode", index_of(&RunMode::ALL, r.mode).to_string());
    put("count_time", r.counting_time_s.to_string());
    put("interval", r.interval_s.to_string());
    put("test_time", r.test_time_min.to_string());
    put("dilution", r.dilution.to_string());
    put("sensor", index_of(&Sensor::ALL, r.sensor).to_string());
    put("sample_mode", index_of(&AutoManual::ALL, r.sample_mode).to_string());
    put("detections", r.detections.to_string());
    put("print_mode", index_of(&AutoManual::ALL, r.print_mode).to_string());
    put("flow_level", r.flow_level.to_string());
    put("flush_time", s.flush.time_s.to_string());
    put("flush_dir", index_of(&Direction::ALL, s.flush.direction).to_string());
    put("flush_sensor", index_of(&Sensor::ALL, s.flush.sensor).to_string());
    for (i, c) in s.channels.iter().enumerate() {
        put(
            &format!("ch{}", i + 1),
            format!("{},{:.1}", if c.enabled { 1 } else { 0 }, c.size_um),
        );
    }
    put("sample_volume", format!("{:.1}", s.analysis.sample_volume_ml));
    put("ref_volume", format!("{:.1}", s.analysis.ref_volume_ml));
    put("density", format!("{:.2}", s.analysis.density));
    for (i, c) in s.analysis.cutoffs.iter().enumerate() {
        put(&format!("cutoff{}", i + 1), format!("{:.1}", c));
    }
    if let Some(p) = &prefs.port {
        put("port", p.clone());
    }
    if let Some(b) = prefs.baud {
        put("baud", b.to_string());
    }
    put("log_x", (prefs.log_x as u8).to_string());
    put("log_y", (prefs.log_y as u8).to_string());
    put("dist_mode", prefs.dist_mode.to_string());
    put("auto_sync", (prefs.auto_sync as u8).to_string());
    o
}

/// Applies saved values on top of `s`; unknown or malformed lines are ignored.
pub fn decode(text: &str, s: &mut Settings) -> Prefs {
    let mut p = Prefs::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        let num = v.parse::<u32>().ok();
        match k {
            "mode" => {
                if let Some(n) = num {
                    s.run.mode = at(&RunMode::ALL, n as usize, s.run.mode);
                }
            }
            "count_time" => set(s, Field::CountTime, num),
            "interval" => set(s, Field::Interval, num),
            "test_time" => set(s, Field::TestTime, num),
            "dilution" => set(s, Field::Dilution, num),
            "sensor" => {
                if let Some(n) = num {
                    s.run.sensor = at(&Sensor::ALL, n as usize, s.run.sensor);
                }
            }
            "sample_mode" => {
                if let Some(n) = num {
                    s.run.sample_mode = at(&AutoManual::ALL, n as usize, s.run.sample_mode);
                }
            }
            "detections" => set(s, Field::Detections, num),
            "print_mode" => {
                if let Some(n) = num {
                    s.run.print_mode = at(&AutoManual::ALL, n as usize, s.run.print_mode);
                }
            }
            "flow_level" => set(s, Field::FlowLevel, num),
            "flush_time" => set(s, Field::FlushTime, num),
            "flush_dir" => {
                if let Some(n) = num {
                    s.flush.direction = at(&Direction::ALL, n as usize, s.flush.direction);
                }
            }
            "flush_sensor" => {
                if let Some(n) = num {
                    s.flush.sensor = at(&Sensor::ALL, n as usize, s.flush.sensor);
                }
            }
            "sample_volume" => setf(s, Field::SampleVolume, v),
            "ref_volume" => setf(s, Field::RefVolume, v),
            "density" => setf(s, Field::Density, v),
            "port" => p.port = Some(v.to_string()),
            "baud" => p.baud = num,
            "log_x" => p.log_x = v == "1",
            "log_y" => p.log_y = v != "0",
            "dist_mode" => p.dist_mode = num.unwrap_or(0).min(2) as u8,
            "auto_sync" => p.auto_sync = v != "0",
            _ => {
                if let Some(rest) = k.strip_prefix("ch") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if (1..=16).contains(&i) {
                            if let Some((e, sz)) = v.split_once(',') {
                                s.channels[i - 1].enabled = e.trim() == "1";
                                if let Ok(x) = sz.trim().parse::<f64>() {
                                    s.set_num(Field::Channel(i - 1), x);
                                }
                            }
                        }
                    }
                } else if let Some(rest) = k.strip_prefix("cutoff") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if (1..=4).contains(&i) {
                            setf(s, Field::Cutoff(i - 1), v);
                        }
                    }
                }
            }
        }
    }
    p
}

fn set(s: &mut Settings, f: Field, v: Option<u32>) {
    if let Some(n) = v {
        s.set_num(f, n as f64);
    }
}

fn setf(s: &mut Settings, f: Field, v: &str) {
    if let Ok(x) = v.parse::<f64>() {
        s.set_num(f, x);
    }
}
