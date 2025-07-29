fn main() {
	// println!("cargo:rerun-if-changed=libuv/include/uv.h");
	println!("cargo:rerun-if-changed=build.rs");
	
	flatbuffers_build::BuilderOptions::new_with_files([
		"models/value.fbs",
		"models/session.fbs",
		"models/stream.fbs",
		"models/health.fbs",
		"models/packet.fbs",
	])
		.compile()
		.expect("Can not compile flatbuffers models.")
}