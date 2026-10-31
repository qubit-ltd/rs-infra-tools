# rs-infra-tools

[![Rust CI](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml/badge.svg)](https://github.com/qubit-ltd/rs-infra-tools/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/endpoint?url=https://qubit-ltd.github.io/rs-infra-tools/coverage-badge.json)](https://qubit-ltd.github.io/rs-infra-tools/coverage/)
[![Crates.io](https://img.shields.io/crates/v/qubit-infra-tools.svg?color=blue)](https://crates.io/crates/qubit-infra-tools)
[![Rust](https://img.shields.io/badge/rust-1.94+-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![English Document](https://img.shields.io/badge/Document-English-blue.svg)](README.md)

为 Qubit Rust 项目动态解析、编译、缓存并运行基础设施工具，无需 submodule，也无需在每个项目中单独安装工具。

## 安装

```bash
cargo install --git https://github.com/qubit-ltd/rs-infra-tools.git --tag v0.1.0 qubit-infra-tools
```

## 快速开始

在 Rust 项目根目录运行稳定入口：

```bash
./update-infra.sh
```

首次运行需要 Git、Python 3.10 或更高版本、Rust 1.94 或更高版本以及 Cargo。Git 和 Python 会获取 manager 的 `main` 并初始化 `.infra`；随后 Rust/Cargo 从源码构建 manager，并预热已登记工具。之后的默认更新会刷新受管理文件和已登记配置源的 `main` 内容，再预热 manager 和所有已登记工具仓库的最新 `main` 版本。若 `.infra` 已存在，根目录入口会交给其中的 bootstrap 执行这套更新流程。`--dry-run` 只列计划，`--status` 比较本地文件与上游内容；这两种模式都不会预热已登记工具。`--check` 仅检查本地快照且不联网。为兼容旧调用仍接受 `--yes`。执行更新时，manifest 中受管理文件的本地修改会被远端版本覆盖；manifest 之外的项目自有文件保持不变。`rs-infra-tools` 仓库自身也会按完整 manifest 更新。CI 应在运行基础设施命令前执行 `--check`。

默认共享缓存位于 `${XDG_CACHE_HOME:-~/.cache}/qubit/rs-infra`。设置 `RS_INFRA_CACHE_DIR` 可以更换缓存根目录。源码和编译结果按 Git 提交、主机目标及 Rust 编译器版本区分。网络操作默认最多重试 4 次，等待时间按 2、4、8 秒递增；可以用 `RS_INFRA_NETWORK_MAX_ATTEMPTS` 和 `RS_INFRA_NETWORK_RETRY_DELAY_SECONDS` 调整策略。

也可以直接调用命令行入口：

```bash
rs-infra-tools latest --project . --tool rs-infra-style -- check
rs-infra-tools prewarm --project .
```

### 本地平台检查

manager 已登记 `rs-infra-platform`，Cargo 包名为 `qubit-infra-platform`，通过 GitHub 源码分发。在已迁移项目中，先更新受管理的引导文件，再调用平台工具：

```bash
./update-infra.sh
./.infra/bin/infra-tool.sh rs-infra-platform --project . check
```

平台工具目前没有发布到 crates.io。本地开发时，也可以使用 Rust 1.94 或更高版本直接运行源码：

```bash
cargo +1.94.0 run --manifest-path /path/to/rs-infra-platform/Cargo.toml --package qubit-infra-platform --bin rs-infra-platform -- --project . check
```

执行 `check` 前，需要准备项目配置 `.infra/platform/platform.toml` 和本机配置 `~/.config/rs-infra/platform.toml`。项目在前者维护 suite 和目标平台，`update-infra.sh` 会保留这份配置。该入口不会自动在项目 CI 中启用平台任务；可选环境缺失时告警跳过，追加 `--require windows` 可要求实际执行 Windows 测试。默认清理本次运行文件，`--no-cleanup` 或 `RS_INFRA_PLATFORM_CLEANUP=0` 可保留文件，自建环境仍会停止。原有 `rs-infra-verify` 平台 suite 及其配置语义保持不变。

`rs-infra-tools prewarm` 和通过 manager 调用的 `rs-infra-ci` 都会解析全部已登记工具。已登记的 GitHub 仓库必须保持可访问。工具成功解析后，其可执行文件会与其他工具一同放入 `RS_INFRA_BIN_DIR`；Windows 文件名带 `.exe` 后缀。已有按版本划分的工具缓存仍可复用。


## 能力与限制

运行前会解析 `main` 的确切提交；同一全局缓存中的每个工具版本只编译一次，项目直接复用缓存的二进制和上游脚本，不会因更新工具而生成项目文件改动。缓存只在本机共享；独立 CI runner 若要跨任务复用编译结果，需要自行持久化该缓存目录。

## 延伸阅读

查看[引导与更新设计](docs/bootstrap-update-design.md)和命令帮助了解更新约定；源码测试覆盖具体行为。切换到 [English README](README.md)。

## 测试

```bash
# 使用默认 feature 集运行测试
cargo test

# 使用项目声明的全部 feature 运行测试
cargo test --all-features

# 运行项目 CI 检查
./ci-check.sh

# 检查代码覆盖率
./coverage.sh
```

## 许可证

Copyright (c) 2025 - 2026. Haixing Hu. All rights reserved.

本项目基于 Apache License 2.0 授权。完整许可证文本请参阅
[LICENSE](LICENSE)。

## 贡献

欢迎贡献。请遵循 Rust API 指南，及时更新公共 API 文档与测试，并在提交
Pull Request 前运行 `./align-ci.sh`格式化代码，运行`./ci-check.sh`对齐CI要求。

## 作者

**Haixing Hu** - *Qubit Co. Ltd.*

仓库地址：[https://github.com/qubit-ltd/rs-infra-tools](https://github.com/qubit-ltd/rs-infra-tools)
