# FiltroTUI

Cross-platform TUI for the KZD-3A particle counter / filter test rig.

    cargo run --release

## Global keys
- Tab / Shift+Tab - switch window, q / Ctrl+C - quit

## Overview
- u / d  upstream / downstream sampler pump on/off (WO / WL)
- s start detection (WD), g suspend (WG), p print (WP), c sync channel count with the instrument
- History: Up/Down (j/k), PageUp/PageDown, Home/End move the cursor; Space marks / unmarks a measurement,
  a marks all, n clears marks; Enter opens the extended view of the selected (or averaged) measurement
- With marked measurements the charts and the extended view show the AVERAGE counts of the marked
  measurements (counts are averaged first, beta / efficiency are calculated from the averages, as ISO 16889 requires)
- Efficiency chart: intermediate labels on the particle-size axis; l - log / linear size axis
- Particle distribution: grouped bar chart (cyan = upstream, yellow = downstream, group label = channel size).
  m - mode: cumulative counts / counts per interval / mass (spherical particles); y - log / linear bar height
- Vertical cut-off lines (Program Settings -> Analysis) on the efficiency chart, coloured group labels on the bars

## Volumes and units
The counter reports raw counts for the sample that passed the sensor (sample volume, default 20 ml).
Raw counts are kept and shown as "raw" in the history and in the extended view. Everything else
(counts per interval, cumulative counts on the charts, cut-off counts, masses) is recalculated to the reference
volume (default 100 ml): N_ref = N_raw * V_ref / V_sample. Beta and efficiency are ratios and do not depend on the
volume. Mass concentrations are in mg/mL. Both volumes are set in Program Settings -> Analysis
(sample volume 0 = calculated from sampler flow x counting time).

## Mass distribution (spherical approximation)
Counts are cumulative (particles >= channel size). Particles in the interval [d_i, d_i+1) = N_i - N_i+1,
each with the diameter sqrt(d_i * d_i+1) and mass rho * pi/6 * d^3. The last channel is an open-ended tail and is
counted with its lower bound (minimum estimate). Mass concentration = mass of the particles / reference volume,
in mg/mL. Particles smaller than the first channel are not counted. Density: Program Settings -> Analysis.

## Care Center (connection + log)
- Up/Down port, Left/Right baud (default 9600 8N1), Enter connect, d disconnect, F5 refresh
- / raw ASCII command (CR appended), x clear log

## Program Settings
- [ ] page (Run / Channel / Flush / Analysis), Up/Down select, Left/Right change, Enter type/cycle,
  Space toggle channel, r defaults
- w sends the page: Run -> WZ + WH, Channels -> WT, Flush -> WF (press w twice to confirm)
- a - toggle automatic channel-count sync, c - sync now

## Saved settings
Everything in Program Settings, selected port, baud rate, chart options and auto-sync are saved
automatically and restored at start-up:
- macOS: ~/Library/Application Support/filtrotui/settings.conf
- Linux: ~/.config/filtrotui/settings.conf
- Windows: %APPDATA%\filtrotui\settings.conf

## RB data frame (observed on a real KZD-3A)
"RB" + N upstream counts + N downstream counts + "WE", every count is 7 digits,
N = number of channels selected on the instrument; counts are cumulative (particles >= channel size).
