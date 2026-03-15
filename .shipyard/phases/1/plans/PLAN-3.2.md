---
phase: foundation
plan: "3.2"
wave: 3
dependencies: ["2.1", "2.2"]
must_haves:
  - GitHub Actions CI runs cargo build, cargo test, cargo clippy on every push
  - CI passes on macOS and Linux (Windows best-effort)
  - arcterm-vt and arcterm-pty tests run in CI without GPU
files_touched:
  - .github/workflows/ci.yml
  - .cargo/config.toml
tdd: false
---

# Plan 3.2 -- CI Pipeline

**Wave 3** | Depends on: Plans 2.1, 2.2 (need tests to exist) | Parallel with: Plan 3.1

## Goal

Set up GitHub Actions CI that builds the workspace, runs tests for non-GPU crates, runs clippy, and checks formatting on every push. GPU-dependent rendering tests are gated behind a feature flag and run only on Linux with Mesa.

---

<task id="1" files=".github/workflows/ci.yml" tdd="false">
  <action>
    Create `.github/workflows/ci.yml` with two jobs:

    **Job 1: `check` (matrix: ubuntu-latest, macos-latest, windows-latest)**
    ```yaml
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Check formatting
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Build
        run: cargo build --workspace
      - name: Test (non-GPU crates)
        run: cargo test --package arcterm-core --package arcterm-vt --package arcterm-pty
    ```

    Notes:
    - `arcterm-render` and `arcterm-app` are excluded from the test step because they require a GPU context.
    - `rust-cache` speeds up builds by caching compiled dependencies.
    - Clippy uses `-D warnings` to fail on any warning.
    - Format check uses `--check` flag (no modification, just verification).

    **Job 2: `gpu-test` (ubuntu-latest only)**
    ```yaml
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Mesa software rendering
        run: |
          sudo apt-get update
          sudo apt-get install -y libvulkan1 mesa-vulkan-drivers libegl1-mesa-dev
      - name: Test render crate (software GPU)
        env:
          WGPU_BACKEND: vulkan
        run: cargo test --package arcterm-render --features gpu-tests
        continue-on-error: true  # GPU tests are best-effort in Phase 1
    ```

    Add `[features]` section to `arcterm-render/Cargo.toml`:
    ```toml
    [features]
    default = []
    gpu-tests = []
    ```

    Gate any render tests (if added) with `#[cfg(feature = "gpu-tests")]`.

    **Trigger:** `on: [push, pull_request]` on all branches.

    **Timeout:** Set `timeout-minutes: 30` on each job to prevent hung builds.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cat .github/workflows/ci.yml | head -5</verify>
  <done>`.github/workflows/ci.yml` exists with two jobs. The `check` job runs on three platforms with build, test (non-GPU crates), clippy, and fmt. The `gpu-test` job runs on ubuntu-latest with Mesa. File is valid YAML (parseable by `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`).)</done>
</task>

<task id="2" files=".cargo/config.toml, arcterm-render/Cargo.toml" tdd="false">
  <action>
    Create `.cargo/config.toml` with useful development defaults:

    ```toml
    [build]
    # Use all available CPU cores for compilation
    jobs = 0

    [target.x86_64-apple-darwin]
    rustflags = ["-C", "link-arg=-fuse-ld=lld"]

    [target.aarch64-apple-darwin]
    # Default linker is fine on Apple Silicon

    [alias]
    xt = "test --package arcterm-core --package arcterm-vt --package arcterm-pty"
    xr = "run --package arcterm-app"
    xc = "clippy --workspace --all-targets -- -D warnings"
    ```

    The aliases provide shortcuts:
    - `cargo xt` -- run all non-GPU tests
    - `cargo xr` -- run the terminal app
    - `cargo xc` -- run clippy with deny warnings

    Also add the `gpu-tests` feature to `arcterm-render/Cargo.toml` if not already present.

    Verify the CI workflow is structurally valid by checking:
    - All referenced packages exist in the workspace.
    - Feature flags referenced in CI exist in the crate manifests.
    - The rust-toolchain.toml version is compatible with the dtolnay/rust-toolchain@stable action.
  </action>
  <verify>cd /Users/lgbarn/Personal/myterm && cargo xt --help 2>&1 | head -3 || echo "alias check: cargo help test works"</verify>
  <done>`.cargo/config.toml` exists with aliases. `cargo xt` runs the non-GPU test suite. `cargo xc` runs clippy. CI workflow references valid packages and features. The full Phase 1 success criteria checklist can be verified with `cargo xr` (opens terminal) and `cargo xt` (runs tests).</done>
</task>
