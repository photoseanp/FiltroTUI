//! Saving selected measurements (raw txt, optionally a PNG chart) and loading them back into the history.
//!
//! The PNG is drawn here with a tiny software rasteriser and a built-in 5x7 font, so no system
//! fonts or GUI libraries are needed (works the same on macOS, Windows and Linux).

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::analysis::{efficiency, interp_count, interval_counts, mass_per_interval};
use crate::app::{DistMode, Measurement, View};

/// Analysis / chart settings that are active at the moment of saving.
pub struct Ctx {
    pub ref_volume_ml: f64,
    pub density: f64,
    pub cutoffs: [f64; 4],
    pub dist_mode: DistMode,
    pub log_x: bool,
    pub log_y: bool,
}

/// One measurement read back from a saved file.
pub struct LoadedItem {
    pub at: String,
    pub volume_ml: f64,
    pub sizes: Option<Vec<f64>>,
    pub up: Vec<u64>,
    pub down: Vec<u64>,
}

pub struct Loaded {
    pub name: String,
    pub notes: String,
    pub items: Vec<LoadedItem>,
}

const MAGIC: &str = "FILTROTUI-DATA";

// ------------------------------------------------------------------ paths

/// Turns what the user typed into a folder path (`~`, quotes and `\ ` from drag-and-drop are handled).
pub fn expand_dir(s: &str) -> PathBuf {
    let mut s = s.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    if !cfg!(windows) {
        s = s.replace("\\ ", " ");
    }
    if s == "~" {
        if let Some(h) = dirs::home_dir() {
            return h;
        }
    }
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    }
    PathBuf::from(s)
}

/// File name without extension that is safe on every OS.
pub fn stem_of(name: &str) -> String {
    let mut n = name.trim().to_string();
    let low = n.to_lowercase();
    if low.ends_with(".txt") || low.ends_with(".png") {
        n.truncate(n.len() - 4);
    }
    let cleaned: String = n
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == '_').to_string();
    if cleaned.is_empty() {
        "measurement".to_string()
    } else {
        cleaned
    }
}

fn mode_name(m: DistMode) -> &'static str {
    match m {
        DistMode::Cumulative => "cumulative",
        DistMode::Interval => "interval",
        DistMode::Mass => "mass",
    }
}

fn time_of(at: &str) -> &str {
    at.split(" [").next().unwrap_or(at).trim()
}

fn clean(s: &str) -> String {
    s.replace(|c: char| c == '\n' || c == '\r' || c == '|', " ")
        .trim()
        .to_string()
}

fn join_u(v: &[u64]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

fn join_f(v: &[f64], prec: usize) -> String {
    v.iter()
        .map(|x| format!("{:.*}", prec, x))
        .collect::<Vec<_>>()
        .join(",")
}

// ------------------------------------------------------------------ save

/// Writes `<name>.txt` (and `<name>.png` when `with_png`) into `dir`; existing files are never
/// overwritten (`_1`, `_2`, ... is appended instead). Returns the written paths.
pub fn save_all(
    dir: &str,
    name: &str,
    notes: &str,
    ctx: &Ctx,
    ms: &[&Measurement],
    sizes: &[Option<Vec<f64>>],
    view: &View,
    with_png: bool,
) -> Result<(PathBuf, Option<PathBuf>)> {
    if ms.is_empty() {
        return Err(anyhow!("no measurements selected"));
    }
    let folder = expand_dir(dir);
    fs::create_dir_all(&folder).map_err(|e| anyhow!("cannot create {}: {}", folder.display(), e))?;

    let base = stem_of(name);
    let mut stem = base.clone();
    let mut k = 0;
    loop {
        let t = folder.join(format!("{}.txt", stem));
        let p = folder.join(format!("{}.png", stem));
        if !t.exists() && !(with_png && p.exists()) {
            break;
        }
        k += 1;
        stem = format!("{}_{}", base, k);
        if k > 9999 {
            return Err(anyhow!("too many files with this name"));
        }
    }
    let txt = folder.join(format!("{}.txt", stem));
    let png = folder.join(format!("{}.png", stem));

    let text = build_txt(&stem, notes, ctx, ms, sizes, view);
    fs::write(&txt, text).map_err(|e| anyhow!("cannot write {}: {}", txt.display(), e))?;
    if with_png {
        render_png(&png, &stem, notes, ctx, view)
            .map_err(|e| anyhow!("cannot write {}: {}", png.display(), e))?;
        Ok((txt, Some(png)))
    } else {
        Ok((txt, None))
    }
}

fn build_txt(
    name: &str,
    notes: &str,
    ctx: &Ctx,
    ms: &[&Measurement],
    sizes: &[Option<Vec<f64>>],
    view: &View,
) -> String {
    let mut o = String::new();
    o.push_str(&format!("{} 1\n\n", MAGIC));
    o.push_str("[meta]\n");
    o.push_str(&format!("name={}\n", clean(name)));
    o.push_str(&format!("notes={}\n", clean(notes)));
    o.push_str(&format!(
        "saved={}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    o.push_str(&format!("measurements={}\n", ms.len()));
    o.push_str(&format!("view={}\n", clean(&view.title)));
    let combine = if view.n_meas < 2 {
        "single"
    } else if view.summed {
        "sum"
    } else {
        "average"
    };
    o.push_str(&format!("combine={}\n", combine));
    o.push_str(&format!("ref_volume_ml={:.1}\n", ctx.ref_volume_ml));
    o.push_str(&format!("density_g_cm3={:.2}\n", ctx.density));
    o.push_str(&format!("cutoffs_um={}\n", join_f(&ctx.cutoffs, 1)));
    o.push_str(&format!("dist_mode={}\n", mode_name(ctx.dist_mode)));
    o.push_str(&format!("log_x={}\n", ctx.log_x as u8));
    o.push_str(&format!("log_y={}\n", ctx.log_y as u8));

    o.push_str("\n[measurements]\n");
    o.push_str("# M|time|sample_volume_ml|sizes_um or -|upstream counts (cumulative, >= size)|downstream counts\n");
    for (i, m) in ms.iter().enumerate() {
        let sz = match sizes.get(i).and_then(|s| s.as_ref()) {
            Some(s) => join_f(s, 1),
            None => "-".to_string(),
        };
        o.push_str(&format!(
            "M|{}|{:.2}|{}|{}|{}\n",
            clean(time_of(&m.at)),
            m.volume_ml,
            sz,
            join_u(&m.counts.up),
            join_u(&m.counts.down)
        ));
    }

    o.push_str("\n[chart]\n");
    o.push_str(&format!(
        "# combined data of the selection: {} (raw = counts of the {} {:.1} ml, ref = recalculated to {:.0} ml)\n",
        clean(&view.title),
        if view.summed { "total sample volume" } else { "mean sample volume" },
        view.volume_ml,
        view.ref_ml
    ));
    o.push_str("ch;size_um;up_raw;down_raw;up_ref;down_ref;efficiency_pct\n");
    for i in 0..view.up.len() {
        let size = view
            .sizes
            .as_ref()
            .and_then(|s| s.get(i))
            .map(|x| format!("{:.1}", x))
            .unwrap_or_else(|| "-".to_string());
        let eff = efficiency(view.up[i], view.down[i])
            .map(|e| format!("{:.3}", e))
            .unwrap_or_else(|| "-".to_string());
        o.push_str(&format!(
            "{};{};{:.2};{:.2};{:.2};{:.2};{}\n",
            i + 1,
            size,
            view.up[i],
            view.down[i],
            view.up_n[i],
            view.down_n[i],
            eff
        ));
    }
    o
}

// ------------------------------------------------------------------ load

fn parse_u64s(s: &str) -> Option<Vec<u64>> {
    s.split(',')
        .map(|x| x.trim().parse::<u64>().ok())
        .collect::<Option<Vec<u64>>>()
}

fn parse_f64s(s: &str) -> Option<Vec<f64>> {
    s.split(',')
        .map(|x| x.trim().parse::<f64>().ok())
        .collect::<Option<Vec<f64>>>()
}

/// Reads a file written by `save_all`.
pub fn parse_txt(text: &str) -> std::result::Result<Loaded, String> {
    let mut section = String::new();
    let mut seen_magic = false;
    let mut name = String::new();
    let mut notes = String::new();
    let mut items: Vec<LoadedItem> = Vec::new();

    for (ln, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !seen_magic {
            if line.starts_with(MAGIC) {
                seen_magic = true;
                continue;
            }
            return Err("not a FiltroTUI data file".to_string());
        }
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        match section.as_str() {
            "meta" => {
                if let Some((k, v)) = line.split_once('=') {
                    match k.trim() {
                        "name" => name = v.trim().to_string(),
                        "notes" => notes = v.trim().to_string(),
                        _ => {}
                    }
                }
            }
            "measurements" => {
                if !line.starts_with("M|") {
                    continue;
                }
                let p: Vec<&str> = line.split('|').collect();
                if p.len() != 6 {
                    return Err(format!("line {}: expected 6 fields", ln + 1));
                }
                let volume_ml = p[2]
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("line {}: bad volume", ln + 1))?;
                let up = parse_u64s(p[4]).ok_or(format!("line {}: bad upstream counts", ln + 1))?;
                let down =
                    parse_u64s(p[5]).ok_or(format!("line {}: bad downstream counts", ln + 1))?;
                if up.is_empty() || up.len() != down.len() {
                    return Err(format!("line {}: upstream/downstream length mismatch", ln + 1));
                }
                let sizes = if p[3].trim() == "-" {
                    None
                } else {
                    parse_f64s(p[3]).filter(|s| s.len() == up.len())
                };
                items.push(LoadedItem {
                    at: p[1].trim().to_string(),
                    volume_ml,
                    sizes,
                    up,
                    down,
                });
            }
            _ => {}
        }
    }
    if !seen_magic {
        return Err("not a FiltroTUI data file".to_string());
    }
    if items.is_empty() {
        return Err("no measurements in the file".to_string());
    }
    Ok(Loaded { name, notes, items })
}

/// Valid FiltroTUI data files in `dir` as (path, list label), newest name first.
pub fn list_files(dir: &str) -> std::result::Result<Vec<(PathBuf, String)>, String> {
    let d = expand_dir(dir);
    let rd = fs::read_dir(&d).map_err(|e| format!("{}: {}", d.display(), e))?;
    let mut v: Vec<(PathBuf, String)> = Vec::new();
    for ent in rd.flatten() {
        let p = ent.path();
        let is_txt = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("txt"))
            .unwrap_or(false);
        if !is_txt {
            continue;
        }
        let Ok(text) = fs::read_to_string(&p) else { continue };
        let Ok(l) = parse_txt(&text) else { continue };
        let fname = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let label = format!(
            "{}  ({} meas, {} ch)  {}",
            fname,
            l.items.len(),
            l.items[0].up.len(),
            l.name
        );
        v.push((p, label));
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(v)
}

// ------------------------------------------------------------------ PNG

type Rgb = [u8; 3];

const W: usize = 1500;
const H: usize = 1260;
const BLACK: Rgb = [30, 30, 30];
const GRID: Rgb = [222, 222, 222];
const UP: Rgb = [31, 119, 180];
const DOWN: Rgb = [255, 127, 14];
const CUTS: [Rgb; 4] = [[188, 70, 200], [40, 160, 70], [210, 50, 50], [50, 90, 200]];

struct Canvas {
    w: usize,
    h: usize,
    buf: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Canvas { w, h, buf: vec![255u8; w * h * 3] }
    }

    fn px(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 3;
        self.buf[i] = c[0];
        self.buf[i + 1] = c[1];
        self.buf[i + 2] = c[2];
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    fn frame(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: Rgb) {
        self.line(x0, y0, x1, y0, 2, c);
        self.line(x0, y1, x1, y1, 2, c);
        self.line(x0, y0, x0, y1, 2, c);
        self.line(x1, y0, x1, y1, 2, c);
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, th: i32, c: Rgb) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let x = (x0 + dx * t).round() as i32;
            let y = (y0 + dy * t).round() as i32;
            self.rect(x - th / 2, y - th / 2, th, th, c);
        }
    }

    fn text_w(s: &str, scale: i32) -> i32 {
        s.chars().filter(|c| c.is_ascii()).count() as i32 * 6 * scale
    }

    fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Rgb) {
        let mut cx = x;
        for ch in s.chars().filter(|c| c.is_ascii()) {
            let rows = glyph(ch);
            for (r, row) in rows.iter().enumerate() {
                for col in 0..5 {
                    if row & (1 << (4 - col)) != 0 {
                        self.rect(cx + col * scale, y + r as i32 * scale, scale, scale, c);
                    }
                }
            }
            cx += 6 * scale;
        }
    }

    fn text_center(&mut self, cx: i32, y: i32, s: &str, scale: i32, c: Rgb) {
        let w = Self::text_w(s, scale);
        self.text(cx - w / 2, y, s, scale, c);
    }

    fn text_right(&mut self, rx: i32, y: i32, s: &str, scale: i32, c: Rgb) {
        let w = Self::text_w(s, scale);
        self.text(rx - w, y, s, scale, c);
    }

    fn dashed_v(&mut self, x: f64, y0: f64, y1: f64, c: Rgb) {
        let mut y = y0;
        while y < y1 {
            self.line(x, y, x, (y + 7.0).min(y1), 2, c);
            y += 14.0;
        }
    }
}

/// 5x7 bitmap font (upper case only); each row uses the lowest 5 bits.
fn glyph(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        ',' => [0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b00100, 0b01000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '+' => [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        '/' => [0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000],
        '=' => [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000],
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        '>' => [0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000],
        '<' => [0b00001, 0b00010, 0b00100, 0b01000, 0b00100, 0b00010, 0b00001],
        '(' => [0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010],
        ')' => [0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000],
        ':' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        _ => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100],
    }
}


fn ascii(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).collect()
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
        3..=5 => format!("{}K", 10u64.pow((k - 3) as u32)),
        _ => format!("{}M", 10u64.pow((k - 6) as u32)),
    }
}

pub fn render_png(path: &Path, name: &str, notes: &str, ctx: &Ctx, v: &View) -> Result<()> {
    let mut c = Canvas::new(W, H);

    c.text(40, 20, &format!("FILTROTUI  {}", ascii(name)), 3, BLACK);
    c.text(40, 56, &ascii(&v.title), 2, BLACK);
    let info = format!(
        "{} {:.1} ML -> {:.0} ML   DENSITY {:.2} G/CM3   CHANNELS {}   MEASUREMENTS {}   X AXIS {}   Y AXIS {}",
        if v.summed { "TOTAL SAMPLE" } else { "SAMPLE" },
        v.volume_ml,
        v.ref_ml,
        ctx.density,
        v.up.len(),
        v.n_meas,
        if ctx.log_x { "LOG" } else { "LINEAR" },
        if ctx.log_y { "LOG" } else { "LINEAR" },
    );
    c.text(40, 84, &info, 2, BLACK);
    let n = ascii(notes);
    if !n.trim().is_empty() {
        c.text(40, 112, &format!("NOTES: {}", n), 2, BLACK);
    }

    draw_eff(&mut c, v, ctx, 120.0, 190.0, (W - 60) as f64, 560.0);
    draw_dist(&mut c, v, ctx, 120.0, 730.0, (W - 60) as f64, 1120.0);

    let file = fs::File::create(path)?;
    let mut enc = png::Encoder::new(BufWriter::new(file), W as u32, H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header()?;
    w.write_image_data(&c.buf)?;
    drop(w);
    Ok(())
}

fn draw_eff(c: &mut Canvas, v: &View, ctx: &Ctx, x0: f64, y0: f64, x1: f64, y1: f64) {
    let n = v.up.len();
    if n == 0 {
        return;
    }
    let sizes: Option<&Vec<f64>> = v.sizes.as_ref().filter(|s| s.len() == n);
    let real = sizes.is_some();
    let xs: Vec<f64> = match sizes {
        Some(s) => s.clone(),
        None => (1..=n).map(|i| i as f64).collect(),
    };
    let use_log = real && ctx.log_x;
    let mx = move |x: f64| if use_log { x.max(0.01).log10() } else { x };
    let mut lo = mx(xs[0]);
    let mut hi = mx(xs[n - 1]);
    if (hi - lo).abs() < 1e-9 {
        hi = lo + 1.0;
    }
    let pad = (hi - lo) * 0.03;
    lo -= pad;
    hi += pad;
    let px = move |x: f64| x0 + (mx(x) - lo) / (hi - lo) * (x1 - x0);
    let py = move |e: f64| y1 - e / 100.0 * (y1 - y0);

    c.text(
        x0 as i32,
        (y0 as i32) - 34,
        &format!(
            "SEPARATION EFFICIENCY, %  ({} SIZE AXIS)",
            if use_log { "LOG" } else { "LINEAR" }
        ),
        2,
        BLACK,
    );

    for q in 0..=4 {
        let e = 25.0 * q as f64;
        let y = py(e);
        c.line(x0, y, x1, y, 1, GRID);
        c.text_right(x0 as i32 - 10, y as i32 - 7, &format!("{}", e as i32), 2, BLACK);
    }

    if real {
        let k = 9;
        for i in 0..k {
            let t = lo + (hi - lo) * i as f64 / (k - 1) as f64;
            let val = if use_log { 10f64.powf(t) } else { t };
            let x = x0 + (x1 - x0) * i as f64 / (k - 1) as f64;
            c.line(x, y0, x, y1, 1, GRID);
            c.text_center(x as i32, y1 as i32 + 10, &fmt_size(val), 2, BLACK);
        }
    } else {
        for (i, xv) in xs.iter().enumerate() {
            let x = px(*xv);
            c.line(x, y0, x, y1, 1, GRID);
            c.text_center(x as i32, y1 as i32 + 10, &format!("{}", i + 1), 2, BLACK);
        }
    }
    c.text_center(
        ((x0 + x1) / 2.0) as i32,
        y1 as i32 + 36,
        if real { "PARTICLE SIZE, UM" } else { "CHANNEL #" },
        2,
        BLACK,
    );

    let pts: Vec<(f64, f64)> = (0..n)
        .filter_map(|i| efficiency(v.up[i], v.down[i]).map(|e| (px(xs[i]), py(e))))
        .collect();
    for w in pts.windows(2) {
        c.line(w[0].0, w[0].1, w[1].0, w[1].1, 3, UP);
    }
    for p in &pts {
        c.rect(p.0 as i32 - 4, p.1 as i32 - 4, 9, 9, BLACK);
    }

    if let Some(s) = sizes {
        let mut row = 0;
        for (idx, cut) in ctx.cutoffs.iter().enumerate() {
            if *cut > 0.0 && *cut >= s[0] && *cut <= s[n - 1] {
                let col = CUTS[idx % CUTS.len()];
                let x = px(*cut);
                c.dashed_v(x, y0, y1, col);
                let e = match (interp_count(s, &v.up_n, *cut), interp_count(s, &v.down_n, *cut)) {
                    (Some(u), Some(d)) => efficiency(u, d),
                    _ => None,
                };
                let label = match e {
                    Some(e) => format!("{} UM  E={:.1}%", fmt_size(*cut), e),
                    None => format!("{} UM", fmt_size(*cut)),
                };
                c.text(x as i32 + 6, y0 as i32 + 6 + row * 20, &label, 2, col);
                row += 1;
            }
        }
    }
    c.frame(x0, y0, x1, y1, BLACK);
}

fn draw_dist(c: &mut Canvas, v: &View, ctx: &Ctx, x0: f64, y0: f64, x1: f64, y1: f64) {
    let n = v.up.len();
    if n == 0 {
        return;
    }
    let sizes: Option<&Vec<f64>> = v.sizes.as_ref().filter(|s| s.len() == n);
    let rho = ctx.density;

    let mode_text = match ctx.dist_mode {
        DistMode::Cumulative => "CUMULATIVE (>= SIZE)",
        DistMode::Interval => "PER SIZE INTERVAL",
        DistMode::Mass => "MASS CONCENTRATION PER INTERVAL",
    };
    c.text(
        x0 as i32,
        (y0 as i32) - 34,
        &format!("PARTICLE DISTRIBUTION, {}", mode_text),
        2,
        BLACK,
    );
    let lx = x1 as i32 - 330;
    c.rect(lx, y0 as i32 - 32, 16, 16, UP);
    c.text(lx + 24, y0 as i32 - 33, "UPSTREAM", 2, BLACK);
    c.rect(lx + 170, y0 as i32 - 32, 16, 16, DOWN);
    c.text(lx + 194, y0 as i32 - 33, "DOWNSTREAM", 2, BLACK);

    let unit_ref = format!("N/{:.0}ML", v.ref_ml);
    let (up_raw, down_raw, is_mass, unit): (Vec<f64>, Vec<f64>, bool, String) = match ctx.dist_mode {
        DistMode::Cumulative => (v.up_n.clone(), v.down_n.clone(), false, unit_ref),
        DistMode::Interval => (
            interval_counts(&v.up_n),
            interval_counts(&v.down_n),
            false,
            unit_ref,
        ),
        DistMode::Mass => match sizes {
            Some(s) => (
                mass_per_interval(&v.up_n, s, rho, v.ref_ml),
                mass_per_interval(&v.down_n, s, rho, v.ref_ml),
                true,
                "MG/ML".to_string(),
            ),
            None => {
                c.frame(x0, y0, x1, y1, BLACK);
                c.text_center(
                    ((x0 + x1) / 2.0) as i32,
                    ((y0 + y1) / 2.0) as i32,
                    "MASS DISTRIBUTION NEEDS PARTICLE SIZES (CHANNEL SIZES UNKNOWN)",
                    2,
                    BLACK,
                );
                return;
            }
        },
    };
    let get = |a: &Vec<f64>, i: usize| a.get(i).copied().unwrap_or(0.0);

    let mut maxv = 0.0f64;
    let mut minpos = f64::MAX;
    for i in 0..n {
        for val in [get(&up_raw, i), get(&down_raw, i)] {
            maxv = maxv.max(val);
            if val > 0.0 {
                minpos = minpos.min(val);
            }
        }
    }
    let lo: i32 = if minpos == f64::MAX { 0 } else { minpos.log10().floor() as i32 };
    let log = ctx.log_y;
    let top: i32 = if is_mass {
        if maxv > 0.0 { (maxv.log10().ceil() as i32).max(lo + 1) } else { lo + 1 }
    } else {
        (((maxv + 1.0).log10()).ceil() as i32).max(1)
    };

    let frac = |val: f64| -> f64 {
        if val <= 0.0 {
            return 0.0;
        }
        if log {
            if is_mass {
                ((val.log10() - lo as f64) / (top - lo) as f64).clamp(0.0, 1.0).max(0.01)
            } else {
                (((val + 1.0).log10()) / top as f64).clamp(0.0, 1.0).max(0.01)
            }
        } else if maxv > 0.0 {
            (val / maxv).clamp(0.0, 1.0).max(0.01)
        } else {
            0.0
        }
    };
    let py = |f: f64| y1 - f * (y1 - y0);

    c.text(x0 as i32 - 100, y0 as i32 - 12, &unit, 2, BLACK);
    let mut ticks: Vec<(f64, String)> = Vec::new();
    if log {
        if is_mass {
            for k in lo..=top {
                ticks.push(((k - lo) as f64 / (top - lo) as f64, format!("1E{}", k)));
            }
        } else {
            for k in 0..=top {
                ticks.push((frac(10f64.powi(k)), count_label(k)));
            }
        }
    } else {
        for q in 0..=4 {
            ticks.push((q as f64 / 4.0, short(maxv * q as f64 / 4.0)));
        }
    }
    for (f, label) in &ticks {
        let y = py(*f);
        c.line(x0, y, x1, y, 1, GRID);
        c.text_right(x0 as i32 - 10, y as i32 - 7, label, 2, BLACK);
    }

    let gw = (x1 - x0) / n as f64;
    let bw = (gw * 0.4).floor().max(2.0);
    let scale = if gw >= 60.0 { 2 } else { 1 };
    for i in 0..n {
        let gx = x0 + gw * i as f64;
        let ux = gx + gw * 0.1;
        let dx = ux + bw;
        let fu = frac(get(&up_raw, i));
        let fd = frac(get(&down_raw, i));
        c.rect(ux as i32, py(fu) as i32, bw as i32, (y1 - py(fu)).round() as i32, UP);
        c.rect(dx as i32, py(fd) as i32, bw as i32, (y1 - py(fd)).round() as i32, DOWN);

        let (label, col) = match sizes {
            Some(s) => {
                let hi_d = s.get(i + 1).copied().unwrap_or(f64::MAX);
                let mut col = BLACK;
                for (k, cut) in ctx.cutoffs.iter().enumerate() {
                    if *cut > 0.0 && *cut >= s[i] && *cut < hi_d {
                        col = CUTS[k % CUTS.len()];
                    }
                }
                (fmt_size(s[i]), col)
            }
            None => (format!("{}", i + 1), BLACK),
        };
        c.text_center((gx + gw / 2.0) as i32, y1 as i32 + 10, &label, scale, col);
    }
    c.text_center(
        ((x0 + x1) / 2.0) as i32,
        y1 as i32 + 36,
        if sizes.is_some() { "CHANNEL SIZE (LOWER BOUND), UM" } else { "CHANNEL #" },
        2,
        BLACK,
    );
    c.frame(x0, y0, x1, y1, BLACK);
}
