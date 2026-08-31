# OrbitRelay Kernel

OrbitRelay Kernel is the open-source Rust foundation for event-driven realtime
collaboration. It owns the shared protocol implementation, Action/Event
execution model, synchronization and replay ports, collaboration domains, and
reusable transport/application adapters.

The repository is deployment-neutral. It does not contain a single-node
executable, SQLite composition, enterprise clustering, tenancy, billing, or
product-specific policy.

## Workspace

- Foundation: `orbitrelay-core`, `orbitrelay-protocol`, `orbitrelay-node`
- Execution: `orbitrelay-runtime`, `orbitrelay-sync`, `orbitrelay-storage`,
  `orbitrelay-query`
- Domains: `orbitrelay-asset`, `orbitrelay-canvas`, `orbitrelay-document`
- Application: `orbitrelay-asset-runtime`, `orbitrelay-canvas-runtime`,
  `orbitrelay-document-runtime`, `orbitrelay-pdf`
- Transport: `orbitrelay-transport`

Dependencies point from adapters and application layers toward lower-level
ports and domains. The kernel never depends on `orbitrelay-server`, Flutter,
or any commercial repository.

## Verification

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Protocol fixtures and language-neutral wire contracts are maintained in
[`orbitrelay-spec`](https://github.com/orbitrelay/orbitrelay-spec).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
