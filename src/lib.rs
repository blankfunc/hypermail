mod packet_capnp {
	include!(concat!(env!("OUT_DIR"), "/packet_capnp.rs"));
}

mod session_capnp {
	include!(concat!(env!("OUT_DIR"), "/session_capnp.rs"));
}

mod stream_capnp {
	include!(concat!(env!("OUT_DIR"), "/stream_capnp.rs"));
}

mod frame_capnp {
	include!(concat!(env!("OUT_DIR"), "/frame_capnp.rs"));
}

mod value_capnp {
	include!(concat!(env!("OUT_DIR"), "/value_capnp.rs"));
}

uniffi::setup_scaffolding!();

pub mod io;
pub mod core;
pub mod client;
pub mod server;

#[cfg(test)]
mod test;

// #[cfg(target_arch = "wasm32")]
// pub mod js; // 针对 Wasm 的绑定