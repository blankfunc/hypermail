fn main() {
	// println!("cargo:rerun-if-changed=libuv/include/uv.h");
	println!("cargo:rerun-if-changed=build.rs");
	
	capnpc::CompilerCommand::new()
		.src_prefix("model")
		.file("model/packet.capnp")
		.file("model/session.capnp")
		.file("model/stream.capnp")
		.file("model/frame.capnp")
		.file("model/value.capnp")
		.run()
		.expect("Can not compile capnp.");
}