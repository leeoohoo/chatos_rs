# memory_engine_sdk

`memory_engine_sdk` 的实际 Rust crate 已迁移到仓库根目录下的 `crates/memory_engine_sdk`。

这个目录只保留迁移说明，不再包含独立的 Cargo package，也不再维护单独的 `Cargo.lock` 或 `src` 源码树。仓库内服务应统一依赖：

```toml
memory_engine_sdk = { path = "../../crates/memory_engine_sdk" }
```

如果调用方所在目录层级不同，请按实际相对路径指向同一个 `crates/memory_engine_sdk` shared crate。
