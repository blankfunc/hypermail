pub mod channel {
	use ibig::UBig;
	use derive_builder::Builder;

	// 对于 Link 的包
	pub mod link {
		#[derive(Clone)]
		pub enum Health {
			Ping,
			Pong
		}

		#[derive(Clone)]
		pub enum Event {
			SessionAck(super::session::Event),
			StreamAck(super::stream::Event),

			Health(Health)
		}
	}

	// 对于 Session 的包
	pub mod session {
		use crate::core::{packet::Reason, strategy::Acceptable};
		use derive_builder::Builder;

		#[derive(Clone)]
		pub enum Ways {
			OnlyRead,
			OnlyWrite,
			TwoWays
		}

		#[derive(Clone, Builder)]
		pub struct OpenOptions {
			#[builder(default = Ways::TwoWays)]
			pub way: Ways,
			#[builder(default = true)]
			pub allow_reconnect: bool,
		}

		#[derive(Clone)]
		pub enum Event {
			Open(OpenOptions),
			OpenAck(Acceptable),
			Reopen,
			ReopenAck(Acceptable),
			Close,
			CloseAck(Acceptable),
			Death(Reason)
		}
	}

	// 对于 Stream 的包
	pub mod stream {
		use crate::core::{packet::Reason, strategy::Acceptable};
		use ibig::UBig;
		use derive_builder::Builder;
		use bytes::Bytes;

		#[derive(Clone, Builder)]
		pub struct Block {
			#[builder(default = true)]
			pub ask_response: bool,
			pub data: Bytes
		}

		#[derive(Clone, Builder)]
		pub struct OpenOptions {
			#[builder(default = true)]
			pub allow_reconnect: bool,
			#[builder(default = false)]
			pub enforce_orderliness: bool,
			#[builder(default = true)]
			pub enforce_integrity: bool,
		}

		#[derive(Clone, Builder)]
		pub struct Chunk {
			pub order: UBig,
			pub data: Bytes,
		}

		#[derive(Clone)]
		pub struct ChunkAck {
			pub order: UBig,
		}

		#[derive(Clone)]
		pub struct Flush {
			pub length: UBig,
		}

		#[derive(Clone)]
		pub struct Lack {
			pub orders: Vec<UBig>
		}

		#[derive(Clone)]
		pub enum Event {
			Block(Block),
			BlockAck,
			Open { options: OpenOptions, length: Option<UBig> },
			OpenAck(Acceptable),
			Reopen,
			ReopenAck(Acceptable),
			Chunk(Chunk),
			ChunkAck(ChunkAck),
			Flush(Flush),
			FlushAck,
			Lack(Lack),
			Later,
			Go,
			Clear(Reason)
		}
	}

	// 所有包
	#[derive(Clone)]
	pub enum Event {
		Link(link::Event),
		Session(session::Event),
		Stream(stream::Event)
	}

	// 包内所包含的所有 ID
	#[derive(Clone, Builder)]
	pub struct IdSet {
		pub link: u64,
		pub event: Option<UBig>,
		pub session: Option<UBig>,
		pub stream: Option<UBig>,
	}

	// 交互的真正内容
	#[derive(Clone)]
	pub struct Datagram {
		pub id: IdSet,
		pub event: Event,
	}
}

#[derive(Clone)]
pub struct Reason {
	pub code: u64,
}

/// How to send data packets.
pub enum TransWays {
	/// Send a whole piece of data and complete it all at once.
	Block,
	/// Send a predictable total quantity of data.
	Buffer,
	/// Sending an unpredictable total amount of data.
	Stream
}