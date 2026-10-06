# rs-infra-tools

[![Rust CI](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-tools/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-tools/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-tools.svg?color=blue)](https://crates.io/crates/qubit-infra-tools)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Resolve, build, cache, and execute the latest `main` revisions of Qubit Rust infrastructure tools without submodules or per-project tool installations.

## Installation

```bash
cargo install --git https://github.com/qubit-ltd/rs-infra-tools.git --tag v0.1.0 qubit-infra-tools
```

## Quick Start

From a Rust project root:

```bash
cargo run --manifest-path /path/to/rs-infra-tools/Cargo.toml -- --help
```

Run the fixed bootstrap entry point from any migrated project:

```bash
./update-infra.sh
```

The bootstrap checks `rs-infra-tools/main`, downloads and builds that revision when needed, then runs the upstream script directly from the shared cache. `update-infra.sh` in this repository prewarms the latest tool binaries; regular project commands resolve the required tools on demand. Project repositories also carry an upstream-managed root `update-infra.sh` for refreshing their stable bootstrap scripts and wrappers. Run it manually when desired; it lists the files it will overwrite and asks once for confirmation. Pass `--yes` to skip the prompt. `./update-infra.sh --check` verifies the installed snapshot offline, while `--status` compares it with the latest upstream package. CI should run `--check` before infrastructure commands. Project-specific CI, dependency, Pages, and style settings remain under the project's `.infra` directory.

The default shared cache is `${XDG_CACHE_HOME:-~/.cache}/qubit/rs-infra`. Set `RS_INFRA_CACHE_DIR` to choose another cache root. Cached sources and binaries are keyed by Git revision, host target, and Rust compiler version. Network operations retry four times by default with exponential delays; `RS_INFRA_NETWORK_MAX_ATTEMPTS` and `RS_INFRA_NETWORK_RETRY_DELAY_SECONDS` change the retry policy.

The command-line entry points are also available directly:

```bash
rs-infra-tools latest --project . --tool rs-infra-style -- check
rs-infra-tools prewarm --project .
```

## Capabilities and limitations

The tool resolves exact `main` commits before using cached tools, compiles each revision once per shared cache, and runs cached upstream scripts without writing generated files into consuming projects. Local caches are host-local; independent CI runners need their own cache persistence to share build results across jobs.

## Learn More

See the command help and source tests for the supported interface. Switch to [中文文档](README.zh_CN.md).

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
