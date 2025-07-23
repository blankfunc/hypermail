<h1 align="center">
	HyperMail
</h1>

<p align="center">
	A simple protocol encapsulation
	<br/>
	<i>designed to provide lightweight, fast, and high-performance connection flow</i>
</p1>

## Why HyperMail

> This was originally part of the SkiProc protocol encapsulation, but I thought it might be useful elsewhere, so I picked it up separately.

This protocol achieves the most fundamental and critical connectivity concept with minimal data size expansion.

By utilizing this protocol and **reasonable program design**, the following functions can theoretically be achieved:

+ 🔃 **Synchronization.** Whether it's cross device, cross network reconnection, or backend load balancing, session synchronization can always be ensured.
+ 🚀 **High-Performance.** Regardless of the underlying protocol, it can always maximize the performance and characteristics of the protocol.
+ 🧷 **Integrity.** Complete data will not be lost due to packet loss caused by disconnection, reconnection, or network turbulence.
+ 💻️ **Cross-Platforms.** The rust + cbindgen approach, combined with a super simple API, makes it easy to integrate with any program.

## Bindgen
We have used `UniFFI`, `Diplomat`, and `Wasm-Bindgen` to perform FFI bridging binding on multiple languages.

+ [Kotlin][UniFFI]
+ [Swift][UniFFI]
+ [Python][UniFFI]
+ [Wasm (JS/TS)](https://github.com/rustwasm/wasm-bindgen)
+ [C/C++](https://github.com/rust-diplomat/diplomat)

> For `C/C++`, you should enable the feature `cbind` or `libuv` (this will compile [libuv](https://github.com/libuv/libuv)).

[UniFFI]: https://github.com/mozilla/uniffi-rs