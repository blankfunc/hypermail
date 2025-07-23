// libuv
pub mod libuv;

pub mod future;

#[test]
fn libuv_v() {
	use crate::ffi::libuv;
	unsafe {
		println!("LibUV Version: {:#?}", libuv::uv_version_string())
	}
}

#[diplomat::bridge]
mod ffi {
	use crate::ffi::libuv;
	pub fn n() -> f64 {
		return unsafe { libuv::y0(0.5) };
	}

	pub fn libuv_v() {
		unsafe {
			println!("LibUV Version: {:#?}", libuv::uv_version_string())
		}
	}
}