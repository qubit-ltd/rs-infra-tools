# Infrastructure Bootstrap and Update Design

本文记录 `update-infra.sh` 的首次安装与后续更新架构，作为实现和维护约定。

## 入口与流程

项目根目录的 `update-infra.sh` 是稳定入口，按 `.infra` 是否存在分流：

- 首次安装时，入口从 `rs-infra-tools/main` 获取项目模板，使用 Python 同步受管理文件并创建 `.infra`。同步成功后，通过 `cargo run --locked` 从刚获取的 manager 源码构建并运行 `prewarm`。
- 后续更新时，入口调用 `.infra/bootstrap.sh` 的 `bootstrap-update` 路由。bootstrap 按 manager 的最新 `main` 提交准备源码和二进制，再调用该版本的 `sync.py`；成功后由 manager 执行 `prewarm`。
- `prewarm` 解析 manager 与已登记工具仓库的最新 `main` 提交。工具二进制按源码提交、主机目标和 Rust 版本缓存。

首次运行环境需要 Git、Python 3.10 或更高版本、Rust 1.94 或更高版本以及 Cargo。Git/Python 用于首次安装；Cargo 从源码构建 manager 并准备工具。

## 更新模式

| 命令 | 行为 |
| --- | --- |
| `./update-infra.sh` | 无提示同步并预热已登记工具 |
| `./update-infra.sh --dry-run` | 获取上游并列出计划，不修改项目文件、不预热登记工具 |
| `./update-infra.sh --status` | 获取上游并比较快照，不写项目文件、不预热登记工具 |
| `./update-infra.sh --check` | 只检查本地快照，不访问网络 |

`--yes` 为兼容旧调用保留，不改变冲突处理策略。

## 文件所有权与冲突

`assets/project-bootstrap/manifest.json` 声明的目标由上游管理；项目自有文件应放在 manifest 之外。同步器使用 `.infra/bootstrap-source.json` 记录文件 SHA-256 与可执行位。写入前会验证快照、目标路径和本次计划覆盖的文件：

- 已管理文件缺失、内容或可执行位与快照不符时，更新报告冲突并停止。
- 首次安装时，若非同内容文件占用了即将受管理的目标，也会报告冲突；同内容文件可以纳入快照。
- 对已退役目标，只有内容与可执行位都符合旧快照时才会删除；已修改或未登记的文件会保留，已删除的文件会被忽略。退役目标都会从新快照中解除托管。
- `.infra/style/exceptions.toml` 是可选的项目自有配置，不由上游安装或更新。升级时，仍与旧快照一致的旧默认文件会被移除；项目自定义内容会被保留。
- `--yes` 不会绕过冲突。同步器保留临时文件原子替换和过期文件保护。
- `--configs-only` 只更新共享配置，不替换未触及的项目脚本。manager 仓库的稳定入口会自动使用此模式，以保留仓库专用的 bootstrap 行为。

## 扩展工具

新增基础设施工具时，应更新 manager 工具注册表与其调用包装入口。默认更新完成后，`prewarm` 会准备注册表中的工具；CI 和普通工具调用仍通过 manager 解析各仓库的 `main` 提交。
