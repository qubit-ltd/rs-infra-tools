# rs-infra-tools

[![Rust CI](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-tools/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-tools/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-tools.svg?color=blue)](https://crates.io/crates/qubit-infra-tools)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![中文文档](https://img.shields.io/badge/文档-中文版-blue.svg)](README.zh_CN.md)

Install and execute revision- and digest-pinned infrastructure binaries without submodules.

## Installation

```bash
cargo install --git https://github.com/qubit-ltd/rs-infra-tools.git --tag v0.1.0 qubit-infra-tools
```

## Quick Start

From a Rust project root:

```bash
cargo run --manifest-path /path/to/rs-infra-tools/Cargo.toml -- --help
```

The project's `.infra` configuration remains the source of truth; this tool does not copy project configuration into the tool repository.

Projects can also synchronize the standard scripts and tool metadata bundled with the pinned `rs-infra-tools` revision:

```bash
rs-infra-tools sync-scripts --project /path/to/project
```

This refreshes managed scripts under `.infra/bin` and `.infra/lib`, plus standard fields in enabled `.infra/*/tool.toml` files. It preserves selected revisions, extra tool metadata, and project-specific CI, dependency, Pages, and style configuration.

## Capabilities and limitations

The tool provides locked binary installation and execution, plus revision-pinned synchronization of shared `.infra` shell scripts. Project-specific policy remains in the consuming project's `.infra` configuration.

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
