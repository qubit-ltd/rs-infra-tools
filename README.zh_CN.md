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

在已迁移的项目根目录运行固定入口：

```bash
./update-infra.sh
```

入口会先检查 `rs-infra-tools/main` 的最新提交；本地缓存没有对应版本时，再下载并编译。随后它直接运行共享缓存中的上游脚本。普通项目命令按需解析和运行工具。各项目仓库有一个由上游管理的根目录 `update-infra.sh`，用于手动更新相对稳定的 bootstrap 脚本和包装脚本；工具仓库使用只同步公共配置的模式，保留其现有工具脚本。默认会列出覆盖文件并提示一次；传入 `--yes` 可跳过提示。更新器会拉取已登记工具仓库的 `main` 分支，把各自 `conf/manifest.json` 声明的文件安装到项目对应的 `.infra` 目录，包括公共 CI 工具版本、rustfmt 配置和当前依赖策略 baseline；项目不能自行选择 baseline 版本。`./update-infra.sh --check` 检查已安装快照，`--status` 则与上游最新公共包及配置内容比较。CI 应在运行基础设施命令前执行 `--check`。项目专属的 CI 任务、Pages 和风格例外配置仍保存在项目自己的 `.infra` 目录中。

默认共享缓存位于 `${XDG_CACHE_HOME:-~/.cache}/qubit/rs-infra`。设置 `RS_INFRA_CACHE_DIR` 可以更换缓存根目录。源码和编译结果按 Git 提交、主机目标及 Rust 编译器版本区分。网络操作默认最多重试 4 次，等待时间按 2、4、8 秒递增；可以用 `RS_INFRA_NETWORK_MAX_ATTEMPTS` 和 `RS_INFRA_NETWORK_RETRY_DELAY_SECONDS` 调整策略。

也可以直接调用命令行入口：

```bash
rs-infra-tools latest --project . --tool rs-infra-style -- check
rs-infra-tools prewarm --project .
```


## 能力与限制

运行前会解析 `main` 的确切提交；同一全局缓存中的每个工具版本只编译一次，项目直接复用缓存的二进制和上游脚本，不会因更新工具而生成项目文件改动。缓存只在本机共享；独立 CI runner 若要跨任务复用编译结果，需要自行持久化该缓存目录。

## 延伸阅读

可通过命令帮助和源码测试了解实际接口。切换到 [English README](README.md)。

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
