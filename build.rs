use std::env;



fn main() {
	capnpc::CompilerCommand::new()
		.src_prefix("model")
		.file("model/packet.capnp")
		.file("model/session.capnp")
		.file("model/stream.capnp")
		.file("model/frame.capnp")
		.file("model/value.capnp")
		.run()
		.expect("Can not compile capnp.");

	let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
	let has_cbind = env::var("CARGO_FEATURE_CBIND").is_ok();
	if target_arch != "wasm32" && has_cbind {
		// 针对非 UniFFI (C/C++) 进行额外编译
		let compile_libuv = env::var("CARGO_FEATURE_LIBUV").is_ok();
		if compile_libuv {
			// 编译 LibUV 作为 Async 支持
			let libuv = cmake::Config::new("libuv")
				.define("BUILD_SHARED_LIBS", "OFF")
				.profile("Release")
				.build();
			
			println!("Built libuv to {}", libuv.display());
			println!("cargo:rustc-link-lib=static=libuv");
			println!("cargo:rustc-link-search=native={}/lib", libuv.display());
		} else {
			// 使用外部 LibUV
			println!("cargo:rustc-link-lib=dylib=uv");
		}

		// 生成 LibUV 绑定
		let libuv_bindings = bindgen::Builder::default()
			.header("libuv/include/uv.h")
			.raw_line("#![allow(warnings)]")
			// .clang_arg(format!("-I{}/include", libuv.display()))
			.generate()
			.expect("Unable to generate bindings for libuv.");
		libuv_bindings
			// .write_to_file("src/ffi/libuv_bindings.rs")
			.write_to_file("src/ffi/libuv.rs")
			.expect("Couldn't write bindings for libuv!");
	}
}