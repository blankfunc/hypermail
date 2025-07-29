mod protocol {
	include!(concat!(env!("OUT_DIR"), "/flatbuffers/mod.rs"));
}

uniffi::setup_scaffolding!();

pub mod io;
pub mod core;