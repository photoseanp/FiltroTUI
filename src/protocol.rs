//! KZD-3A serial protocol (manual, Appendix IV).
//! Frame: leading letter (R/W/A) + code + data + CR (0x0D). Error reply: "WM".

use crate::settings::{AutoManual, RunMode, Sensor, Settings};

pub const START_DETECTION: &str = "WD";
pub const SUSPEND_DETECTION: &str = "WG";
pub const PRINT: &str = "WP";
pub const FLUSH: &str = "WF";

/// Width of one count in an RB frame (observed on a real instrument).
const RB_FIELD_WIDTH: usize = 7;

pub fn frame(cmd: &str) -> Vec<u8> {
    let mut v = cmd.as_bytes().to_vec();
    v.push(0x0D);
    v
}

fn sensor_code(s: Sensor) -> u8 {
    match s {
        Sensor::Upstream => 0,
        Sensor::Downstream => 1,
        Sensor::Both => 2,
    }
}

/// WZ - set detection mode. Manual examples: WZ030050 / WZ13006020001 / WZ230203
pub fn wz(s: &Settings) -> String {
    let r = &s.run;
    match r.mode {
        RunMode::Initial => format!("WZ0{:02}{:03}", r.counting_time_s, r.interval_s),
        RunMode::Formal => format!(
            "WZ1{:02}{:03}{:03}{:02}",
            r.counting_time_s, r.interval_s, r.test_time_min, r.dilution
        ),
        RunMode::Single => format!(
            "WZ2{:02}{}{}{}",
            r.counting_time_s,
            sensor_code(r.sensor),
            if r.sample_mode == AutoManual::Auto { 1 } else { 0 },
            r.detections
        ),
    }
}

/// WT - set 16 channel sizes: 16 groups x 4 digits, value = size in um x 10
/// (manual: "00101000" = 1.0 um and 100.0 um).
pub fn wt(s: &Settings) -> String {
    let mut out = String::from("WT");
    for c in s.channels.iter() {
        let v = (c.size_um * 10.0).round().clamp(0.0, 9999.0) as u32;
        out.push_str(&format!("{:04}", v));
    }
    out
}

/// WH - print option: 0 manual, 1 automatic.
pub fn wh(m: AutoManual) -> String {
    format!("WH{}", if m == AutoManual::Auto { 1 } else { 0 })
}

/// WO - sampler pump start: 0 upstream, 1 downstream.
pub fn pump_on(downstream: bool) -> String {
    format!("WO{}", if downstream { 1 } else { 0 })
}

/// WL - sampler pump stop: 0 upstream, 1 downstream.
pub fn pump_off(downstream: bool) -> String {
    format!("WL{}", if downstream { 1 } else { 0 })
}

/// Raw cumulative counts of one detection (particles >= channel size), per selected channel.
#[derive(Clone, Debug)]
pub struct Counts {
    pub up: Vec<u64>,
    pub down: Vec<u64>,
}

/// RB data frame: "RB" + N upstream counts + N downstream counts + "WE".
/// Every count is 7 digits; N is the number of channels selected on the instrument
/// (e.g. 196 digits = 28 fields = 14 + 14). Numbers separated by non-digits are also accepted.
pub fn parse_rb(line: &str) -> Result<Counts, String> {
    let body = line.trim().strip_prefix("RB").ok_or("no RB prefix")?;
    let body = body.strip_suffix("WE").unwrap_or(body);

    let nums: Vec<u64> = if !body.is_empty() && body.chars().all(|c| c.is_ascii_digit()) {
        let n = body.len();
        if n % (2 * RB_FIELD_WIDTH) != 0 {
            return Err(format!(
                "{} digits is not a multiple of {} (2 x {}-digit fields)",
                n,
                2 * RB_FIELD_WIDTH,
                RB_FIELD_WIDTH
            ));
        }
        body.as_bytes()
            .chunks(RB_FIELD_WIDTH)
            .map(|c| std::str::from_utf8(c).ok()?.parse::<u64>().ok())
            .collect::<Option<Vec<u64>>>()
            .ok_or("bad digits")?
    } else {
        body.split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u64>().ok())
            .collect::<Option<Vec<u64>>>()
            .ok_or("bad number")?
    };

    if nums.is_empty() || nums.len() % 2 != 0 {
        return Err(format!("{} fields (need an even number)", nums.len()));
    }
    let half = nums.len() / 2;
    Ok(Counts { up: nums[..half].to_vec(), down: nums[half..].to_vec() })
}
