# rs-infra-tools

[![Rust CI](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-tools/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-tools/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-tools.svg?color=blue)](https://crates.io/crates/qubit-infra-tools)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Resolve, build, cache, and execute the latest `main` revisions of Qubit Rust infrastructure tools without submodules or repeated per-tool builds.

## Installation

```bash
cargo install --git https://github.com/qubit-ltd/rs-infra-tools.git --tag v0.1.0 qubit-infra-tools
```

## Quick Start

From a Rust project root:

```bash
cargo run --manifest-path /path/to/rs-infra-tools/Cargo.toml -- --help
```

Run the stable root entry point from a Rust project:

```bash
./update-infra.sh
```

The first run requires Git, Python 3.10 or later, Rust 1.94 or later, and Cargo. Git and Python fetch the manager's `main` branch and initialize `.infra`; Rust and Cargo then build the manager from source and prewarm its registered tools. Later default updates refresh managed files and shared configuration from their registered `main` branches, then prewarm the manager and all registered tool repositories at their latest `main` commits. When `.infra` already exists, the stable entry point delegates to its bootstrap to run this update flow. `--dry-run` lists planned changes and `--status` compares installed files with current upstream content; neither prewarms registered tools. `--check` verifies the local snapshot without fetching. `--yes` remains accepted for compatibility. During an update, remote versions overwrite local changes to files listed in the managed manifest. Project-owned files outside the manifest remain untouched. The `rs-infra-tools` repository itself receives the same full manifest update as other projects. CI should run `--check` before infrastructure commands.

The default shared cache is `${XDG_CACHE_HOME:-~/.cache}/qubit/rs-infra`. Set `RS_INFRA_CACHE_DIR` to choose another cache root. Cached sources and binaries are keyed by Git revision, host target, and Rust compiler version. Network operations retry four times by default with exponential delays; `RS_INFRA_NETWORK_MAX_ATTEMPTS` and `RS_INFRA_NETWORK_RETRY_DELAY_SECONDS` change the retry policy.

The command-line entry points are also available directly:

```bash
rs-infra-tools latest --project . --tool rs-infra-style -- check
rs-infra-tools prewarm --project .
```

### Read-only project source analysis

After one normal update, analyze the launcher project from any working directory:

```bash
./update-infra.sh
./.infra/bin/style-analyze.sh
./.infra/bin/style-analyze.sh --format json
```

The updater installs this managed launcher and Python helper, and calls
`prewarm --install-style-analyzer` to publish the resolved native executable and
revision/SHA-256 metadata in ignored `.infra/style/` paths. Ordinary `prewarm`
remains a shared-cache-only operation. Existing projects also receive a usable
installation in their first update, even when their already-running bootstrap
predates the installation flag. That compatibility handoff may perform a second
cache-only prewarm; it does not rebuild an already cached revision.

The entry enumerates tracked and nonignored untracked Rust files, including
nondefault workspace members, standalone fuzz crates, and fixtures. It excludes
generated/vendor directories and build/review internals. Source lists are
created outside the project and removed after invocation. `--files-from` uses an
explicit JSON path list; `--standards`, `--repository-readme`, and `--project`
forward the corresponding native scope. `--project` changes the analyzed tree
while the native installation remains anchored to the launcher project, so a
review worker need not contain the ignored executable. An explicit
`RUST_STYLE_ANALYZER` overrides the native executable.

Analysis does not update infrastructure, build project code, format source, or
run CI. Missing or mismatched installation metadata reports an error and asks
for `update-infra.sh`. Exit codes are `0` for completed mechanical checks without
proved violations, `1` for violations, and `2` for incomplete analysis. Semantic
review is still required. Existing `style-check.sh` and CI gate behavior is unchanged.

### Local platform checks

The manager registers `rs-infra-platform` (Cargo package `qubit-infra-platform`) and distributes it from its GitHub source. In a migrated project, update the managed bootstrap before invoking the platform tool:

```bash
./update-infra.sh
./.infra/bin/infra-tool.sh rs-infra-platform --project . check
```

The platform tool is not published on crates.io. For local source development, Rust 1.94 or later can also build and run a checkout directly:

```bash
cargo +1.94.0 run --manifest-path /path/to/rs-infra-platform/Cargo.toml --package qubit-infra-platform --bin rs-infra-platform -- --project . check
```

The tool reads project configuration from `.infra/platform/platform.toml` and host configuration from `~/.config/rs-infra/platform.toml`. The project owns its platform suites and target-platform lists; `update-infra.sh` preserves that file. Configure these before running `check`; it does not enable platform tasks in project CI automatically. Optional unavailable platforms warn and skip; `--require windows` requires actual Windows execution. Cleanup is enabled by default; `--no-cleanup` or `RS_INFRA_PLATFORM_CLEANUP=0` retains run files while owned environments still stop. The existing `rs-infra-verify` platform suite and its configuration retain their current meaning.

Both `rs-infra-tools prewarm` and manager invocations of `rs-infra-ci` resolve every registered tool. The registered GitHub repositories must remain accessible. Once resolved, the platform executable is included in `RS_INFRA_BIN_DIR` alongside the other tools, with the `.exe` suffix on Windows. Existing revision-specific tool caches remain reusable.

## Capabilities and limitations

The tool resolves exact `main` commits before using cached tools, compiles each revision once per shared cache, and runs cached upstream scripts. Normal updates additionally install the managed, ignored project Analyzer artifact; plain `prewarm` does not write project artifacts. Local caches are host-local; independent CI runners need their own cache persistence to share build results across jobs.

## Learn More

See the [bootstrap update design](docs/bootstrap-update-design.md) and command help for the update contract. Source tests cover its behavior. Switch to [中文文档](README.zh_CN.md).

## Testing

```bash
# Run tests with the default feature set
cargo test

# Run tests with all declared features
cargo test --all-features

# Project CI checks
./ci-check.sh

# Check code coverage
./coverage.sh
```

## License

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for the
full license text.

## Contributing

Contributions are welcome. Please follow the Rust API guidelines, keep public
API documentation and tests current, and run `./align-ci.sh` to format code and
`./ci-check.sh` to satisfy CI requirements before submitting a pull request.

## Author

**Haixing Hu** - *Qubit Co. Ltd.*

Repository: [https://github.com/qubit-ltd/rs-infra-tools](https://github.com/qubit-ltd/rs-infra-tools)
