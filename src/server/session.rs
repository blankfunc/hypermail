use ibig::UBig;
use crate::core::link::InnerChannel;

#[derive(Clone)]
pub struct Session {

}

impl Session {
	pub fn new(id: UBig, channel: InnerChannel) -> Self {
		Self {}
	}
}