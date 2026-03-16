//! Build script for arcterm-app.
//!
//! Generates `arcterm.1` man page into `$OUT_DIR` using `clap_mangen`.
//! The man page mirrors the CLI definition in `src/main.rs` without importing
//! the binary's runtime dependencies.
//!
//! The generated file is placed at `$OUT_DIR/arcterm.1`.  To install it:
//!   cp $(cargo metadata --format-version 1 | jq -r '.target_directory')/build/arcterm-app-*/out/arcterm.1 /usr/local/share/man/man1/

use std::path::PathBuf;

fn build_cli() -> clap::Command {
    use clap::{Arg, Command};

    Command::new("arcterm")
        .version(env!("CARGO_PKG_VERSION"))
        .about("GPU-rendered AI terminal emulator")
        .long_about(
            "Arcterm is a GPU-accelerated terminal emulator with built-in AI agent \
             support, multi-pane multiplexing, workspace persistence, and a plugin \
             system for extending functionality via WebAssembly.",
        )
        .subcommand(
            Command::new("open")
                .about("Open a named workspace")
                .arg(
                    Arg::new("name")
                        .help("Workspace name (without .toml extension)")
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("save")
                .about("Save current session as a workspace")
                .arg(
                    Arg::new("name")
                        .help("Workspace name")
                        .required(true),
                ),
        )
        .subcommand(Command::new("list").about("List available workspaces"))
        .subcommand(
            Command::new("plugin")
                .about("Manage plugins")
                .subcommand(
                    Command::new("install")
                        .about("Install a plugin from a directory containing plugin.toml")
                        .arg(
                            Arg::new("path")
                                .help("Path to the plugin directory")
                                .required(true),
                        ),
                )
                .subcommand(Command::new("list").about("List installed plugins"))
                .subcommand(
                    Command::new("remove")
                        .about("Remove an installed plugin by name")
                        .arg(
                            Arg::new("name")
                                .help("Plugin name (as declared in plugin.toml)")
                                .required(true),
                        ),
                )
                .subcommand(
                    Command::new("dev")
                        .about("Load a plugin directly from a directory (for development, no copy)")
                        .arg(
                            Arg::new("path")
                                .help("Path to the plugin directory")
                                .required(true),
                        ),
                ),
        )
        .subcommand(
            Command::new("config")
                .about("Manage configuration")
                .subcommand(
                    Command::new("flatten")
                        .about("Print the fully resolved configuration (base + accepted overlays) as TOML"),
                ),
        )
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let man_dir = out_dir.join("man");
    std::fs::create_dir_all(&man_dir).expect("failed to create man output directory");

    let cmd = build_cli();
    let man = clap_mangen::Man::new(cmd);

    let mut buf = Vec::new();
    man.render(&mut buf).expect("failed to render man page");

    let man_path = man_dir.join("arcterm.1");
    std::fs::write(&man_path, &buf).expect("failed to write arcterm.1");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/main.rs");
}
