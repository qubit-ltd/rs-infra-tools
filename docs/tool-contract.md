# rs-infra 工具契约

所有 `rs-infra-*` 工具都提供 `--project`、`--config`、`--format json` 和
`--version`。`check`、`plan`、`report` 不修改项目受版本控制文件；`fix` 与
`sync` 必须显式调用并支持 `--dry-run`。

退出码约定：0 表示通过，1 表示规则或验证失败，2 表示配置、安装或执行错误。

JSON 输出的顶层字段固定为 `schema_version`、`tool`、`status`、`diagnostics`
和 `artifacts`。工具必须保留自己的错误代码，并把子进程的标准错误输出到标准错误。

## tools.lock

锁文件使用便于 bootstrap 解析的空白分隔格式：

```text
# name source revision target sha256
rs-infra-style https://example.invalid/rs-infra-style 0123456789abcdef0123456789abcdef01234567 x86_64-unknown-linux-gnu 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

每条记录必须有五列：工具名、直接可执行 artifact 来源、40 位 Git revision、
目标 triple 和 64 位 SHA-256。工具名不能包含路径分隔符。来源可以是 HTTPS URL 或
本地开发用的 `file://` URL。
