# memory-socket

[![memory-socket on crates.io](https://img.shields.io/crates/v/memory-socket)](https://crates.io/crates/memory-socket)
[![Documentation (latest release)](https://docs.rs/memory-socket/badge.svg)](https://docs.rs/memory-socket/)
[![License](https://img.shields.io/badge/license-Apache-green.svg)](LICENSE-APACHE)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE-MIT)

Provides an in-memory socket abstraction.

The `memory-socket` crate provides the [`MemoryListener`] and [`MemorySocket`] types which can
be thought of as in-memory versions of the standard library `TcpListener` and `TcpStream`
types.

## Feature flags

- `async`: Adds async support for [`MemorySocket`] and [`MemoryListener`]

[`MemoryListener`]: https://docs.rs/memory-socket/latest/memory-socket/struct.MemoryListener.html
[`MemorySocket`]: https://docs.rs/memory-socket/latest/memory-socket/struct.MemorySocket.html

## License

This project is available under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT
license](LICENSE-MIT).
