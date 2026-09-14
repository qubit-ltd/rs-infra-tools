# tools.lock 格式

`tools.lock` 同时固定工具源码身份和可执行 artifact 身份。工具安装器只使用
revision 作为缓存隔离的一部分，最终仍必须校验 artifact 的 SHA-256。

缓存路径为：

```text
<cache-root>/rs-infra/<name>/<revision>/<target>/<name>
```

默认缓存根目录依次使用 `RS_INFRA_CACHE_DIR`、`XDG_CACHE_HOME/qubit` 或
`HOME/.cache/qubit`。下载先写入同一缓存目录下的临时文件，校验成功后再原子改名。
校验失败的临时文件必须删除。
