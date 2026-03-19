# Contributing to ArcTerm

ArcTerm is a fork of [WezTerm](https://github.com/wez/wezterm) by Wez Furlong, extended with AI-powered features.

## Upstream Relationship

- **Upstream repository**: https://github.com/wez/wezterm
- **License**: MIT (same as upstream)
- **Remote setup**: `upstream` points to wez/wezterm, `origin` points to lgbarn/arcterm

We periodically merge upstream changes to stay current with WezTerm improvements. ArcTerm-specific code is kept in clearly separated modules/crates to minimize merge conflicts.

### Syncing with upstream

```console
$ git fetch upstream
$ git merge upstream/main
```

## ArcTerm-Specific Features

The following are unique to ArcTerm and not present in upstream WezTerm:

1. **WASM Plugin System** — capability-based sandbox for plugins
2. **AI Integration Layer** — cross-pane context, AI tool detection, Ollama/Claude integration
3. **Structured Output Rendering** — OSC 7770 protocol for rich content

When contributing to these features, please keep them in their dedicated crates/modules.

## Development

### Building

```console
$ cargo build --release
```

### Running in debug mode

```console
$ cargo run --bin wezterm-gui
```

### Running tests

```console
$ cargo test --all
```

### Code formatting

```console
$ rustup component add rustfmt-preview
$ cargo fmt --all
```

## Where to Find Things

- `term/` — Core terminal model (VT parsing, escape sequences)
- `wezterm-gui/` — GUI renderer
- `config/` — Configuration system (Lua plugins)
- `mux/` — Multiplexer
- `arcterm-*` — ArcTerm-specific crates (WASM plugins, AI integration, structured output)

## Submitting a Pull Request

1. Fork the repository
2. Create a feature branch
3. Ensure tests pass: `cargo test --all`
4. Ensure formatting: `cargo fmt --all`
5. Submit your PR with a clear description of the changes
