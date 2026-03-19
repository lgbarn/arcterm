# Data Model: Complete ArcTerm Rebrand

**Date**: 2026-03-18
**Feature**: 001-rebrand-completion

## Overview

This feature is primarily a string-replacement and configuration change across
the codebase. There are no new database entities or data stores. The key
"entities" are configuration values and identity strings.

## Config File Search Order

**Entity: Config Resolution**

The config loader resolves the user's configuration file via a priority chain:

1. `ARCTERM_CONFIG_FILE` environment variable (new, highest priority)
2. `arcterm.lua` in standard config directories:
   - `~/.arcterm.lua` (home directory dotfile)
   - `$XDG_CONFIG_HOME/arcterm/arcterm.lua` (Linux/macOS XDG)
   - `<exe_dir>/arcterm.lua` (Windows portable)
3. `WEZTERM_CONFIG_FILE` environment variable (deprecated fallback)
4. `wezterm.lua` in standard config directories (deprecated fallback)

**State transitions**: None. This is a one-time resolution at startup.

**Validation**: At most one config file is loaded. If deprecated paths are
used, a log warning is emitted.

## SshMultiplexing Enum

**Entity: SshMultiplexing**

```
SshMultiplexing:
  ArcTerm    — new default, preferred variant
  WezTerm    — deprecated alias, serde-compatible
  None       — existing variant (no multiplexing)
```

**Backward compatibility**: Lua configs using `ssh_multiplexing = "WezTerm"`
continue to work. New configs should use `ssh_multiplexing = "ArcTerm"`.

## App Identity Strings

**Entity: Brand Identity**

| Surface | Old Value | New Value |
|---------|-----------|-----------|
| GitHub releases URL | `wezterm/wezterm` | `lgbarn/arcterm` |
| User-Agent | `wezterm/wezterm-{version}` | `arcterm/{version}` |
| macOS bundle ID | `com.github.wez.wezterm` | `com.lgbarn.arcterm` |
| macOS bundle name | `WezTerm` | `ArcTerm` |
| Windows product name | `WezTerm` | `ArcTerm` |
| Linux desktop name | `WezTerm` | `ArcTerm` |
| Linux WM class | `org.wezfurlong.wezterm` | `com.lgbarn.arcterm` |
| Flatpak app-id | `org.wezfurlong.wezterm` | `com.lgbarn.arcterm` |
| Config env var | `WEZTERM_CONFIG_FILE` | `ARCTERM_CONFIG_FILE` |
| Config file name | `wezterm.lua` | `arcterm.lua` |
| RPM/DEB packager | `Wez Furlong` | ArcTerm maintainer |
| Funding links | `wez` / `WezFurlong` | `lgbarn` or removed |
