# FiltroTUI

Cross-platform TUI for the KZD-3A particle counter / filter test rig.

    cargo run --release

## Global keys
- Tab / Shift+Tab - switch window, q / Ctrl+C - quit

## Overview
- u / d  upstream / downstream sampler pump on/off (WO / WL)
- s start detection (WD), g suspend (WG), p print (WP)
- Up/Down (j/k), PageUp/PageDown, Home/End - scroll the measurement history; the charts show the selected
  measurement (the newest one by default)
- l - log / linear particle-size axis, m - cumulative / differential particle distribution
- c - sync the number of enabled channels with the last frame from the instrument

## Care Center (connection + log)
- Up/Down port, Left/Right baud (default 9600 8N1), Enter connect, d disconnect, F5 refresh
- / raw ASCII command (CR appended), x clear log

## Program Settings
- [ ] page, Up/Down select, Left/Right change, Enter type/cycle, Space toggle channel, r defaults
- w sends the page: Run -> WZ + WH, Channels -> WT, Flush -> WF (press w twice to confirm)
- a - toggle automatic channel-count sync, c - sync now

## Saved settings
All program settings (run/channel/flush setup, channel sizes and enable flags, selected port, baud rate,
chart options, auto-sync) are saved automatically on every change and restored at start-up:
- macOS: ~/Library/Application Support/filtrotui/settings.conf
- Linux: ~/.config/filtrotui/settings.conf
- Windows: %APPDATA%\filtrotui\settings.conf

## RB data frame (observed on a real KZD-3A)
"RB" + N upstream counts + N downstream counts + "WE", every count is 7 digits,
N = number of channels selected on the instrument (e.g. 196 digits = 28 fields = 14 + 14).
The protocol cannot read or set the instrument's channel selection, so N is taken from the frames;
auto-sync makes the number of enabled channels in the program equal to N
(disables the largest / enables the smallest channels). Check which channels are enabled.
