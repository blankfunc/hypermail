use ibig::UBig;
use once_cell::sync::Lazy;
use forever_safer::atomic_poll::AtomicPoll;

use crate::{core::packet::channel::session, protocol};

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

pub fn handle_flatbuffer<'a>(packet: crate::protocol::packet::Packet<'a>, link_id: u64) -> Option<self::channel::Datagram> {
	use bytes::Bytes;
	use super::strategy::Acceptable;
	use ibig::UBig;
	use crate::protocol;
	use self::channel;

	// 快速获取 UBig
	fn quickly_ubig<'a>(try_value: Option<protocol::value::UBig<'a>>) -> Option<UBig> {
		if let Some(value) = try_value {
			if let Some(uint64) = value.from_as_uint_64() {
				// UInt64 模式
				return Some(UBig::from(uint64.uint()))
			} else if let Some(ubytes_obj) = value.from_as_bytes() {
				// Bytes 模式 UBig BE 表达
				if let Some(ubytes) = ubytes_obj.bytes()
				&& ubytes.len() > 0 {
					return Some(UBig::from_be_bytes(ubytes.bytes()))
				} else {
					return None
				}
			}
		}

		return None;
	}

	// 设置 ID
	let id_set = {
		let packet_id = packet.id();
		let event_id = quickly_ubig(packet_id.event_id_as_value_ubig());
		let session_id = quickly_ubig(packet_id.session_id_as_value_ubig());
		let stream_id = quickly_ubig(packet_id.stream_id_as_value_ubig());

		let try_id_set = channel::IdSetBuilder::default()
			.link(link_id)
			.event(event_id)
			.session(session_id)
			.stream(stream_id)
			.build();

		match try_id_set {
			Ok(id) => id,
			// 搞不了就拉倒
			Err(_) => return None,
		}
	};

	// 简单校验 ID
	if [
		// 没有 event_id
		id_set.event.is_none(),
		// 有 stream_id 但是没有 session_id
		id_set.stream.is_some() && id_set.session.is_none()
	].iter().any(|v| *v) {
		return None;
	}
					
	let head = packet.head();

	// 快速封装数据
	macro_rules! impl_data {
		($event:expr) => {
			channel::Datagram {
				id: id_set.clone(),
				event: $event
			}
		}
	}

	// 快速处理 None Payload
	macro_rules! quickly_none {
		($event:expr) => {
			if let Some(_) = packet.payload_as_none() {
				return Some(impl_data!($event));
			}
		};
	}

	// 快速处理纯 Response Payload
	macro_rules! quickly_response {
		($payload:ident, $builder:expr) => {
			{
				let response = if let Some(_) = $payload.response_as_accept() {
					// 接受
					Acceptable::Accept
				} else if let Some(response) = $payload.response_as_reject()
				&& let Some(reason) = response.reason() {
					// 拒绝
					Acceptable::Reject(Reason { code: reason.code() })
				} else {
					// 无效包
					return None;
				};

				let event = $builder(response);
				return Some(impl_data!(event));
			}
		};
	}

	use channel::{
		Event as WrapEvent,
		session::Event as SessionEvent,
		stream::Event as StreamEvent,
		link::{Event as LinkEvent, Health}
	};

	use protocol::packet::Head;
	match head {
		// 请求开启会话
		Head::SessionOpen		=> if let Some(payload) = packet.payload_as_session_open() {
			// 获取配置
			let options = {
				use channel::session::{OpenOptionsBuilder, Ways};

				let mut builder = OpenOptionsBuilder::default();
				if let Some(options) = payload.options() {
					use protocol::session::SessionWays;
					let way = match options.way() {
						SessionWays::OnlyRead => Ways::OnlyRead,
						SessionWays::OnlyWrite => Ways::OnlyWrite,
						SessionWays::TwoWays => Ways::TwoWays,
						_ => return None
					};

					builder
						.allow_reconnect(options.allow_reconnect())
						.way(way);
				}

				let try_options = builder.build();

				match try_options {
					Ok(options) => options,
					// 不行拉倒
					Err(_) => return None
				}
			};

			let data = impl_data!(WrapEvent::Session(SessionEvent::Open(options)));
			return Some(data);
		},
		// 响应开启会话
		Head::SessionOpenAck	=>	if let Some(payload) = packet.payload_as_session_open_ack() {
			quickly_response!(payload, |response| WrapEvent::Session(SessionEvent::OpenAck(response)));
		},
		// 请求重连会话
		Head::SessionReopen		=> quickly_none!(WrapEvent::Session(SessionEvent::Reopen)),
		// 响应重连会话
		Head::SessionReopenAck	=> if let Some(payload) = packet.payload_as_session_reopen_ack() {
			quickly_response!(payload, |response| WrapEvent::Session(SessionEvent::ReopenAck(response)));
		},
		// 请求关闭会话
		Head::SessionClose		=> quickly_none!(WrapEvent::Session(SessionEvent::Close)),
		// 响应关闭会话
		Head::SessionCloseAck	=> if let Some(payload) = packet.payload_as_session_close_ack() {
			quickly_response!(payload, |response| WrapEvent::Session(SessionEvent::CloseAck(response)));
		},
		// 强制关闭
		Head::SessionDeath		=> if let Some(payload) = packet.payload_as_session_death() {
			let data = impl_data!(WrapEvent::Session(SessionEvent::Death(Reason { code: payload.reason().code() })));
			return Some(data);
		},
		// 整包数据
		Head::StreamBlock		=> if let Some(payload) = packet.payload_as_stream_block() {
			let bytes = if let Some(the_bytes) = payload.data() {
				Bytes::copy_from_slice(the_bytes.bytes())
			} else {
				Bytes::new()
			};
					
			let block = channel::stream::Block {
				ask_response: payload.ask_response(),
				data: bytes
			};
			let data = impl_data!(WrapEvent::Stream(StreamEvent::Block(block)));
			return Some(data);
		},
		// 响应收到整包
		Head::StreamBlockAck	=> quickly_none!(WrapEvent::Stream(StreamEvent::BlockAck)),
		// 请求传输流
		Head::StreamOpen		=> if let Some(payload) = packet.payload_as_stream_open() {
			let options = {
				use channel::stream::OpenOptionsBuilder;

				let mut builder = OpenOptionsBuilder::default();

				if let Some(options) = payload.options() {
					builder
						.allow_reconnect(options.allow_reconnect())
						.enforce_integrity(options.enforce_integrity())
						.enforce_orderliness(options.enforce_orderliness());
				}

				let try_options = builder.build();
				match try_options {
					Ok(options) => options,
					Err(_) => return None
				}
			};

			let length = quickly_ubig(payload.length());

			let data = impl_data!(WrapEvent::Stream(StreamEvent::Open { options, length }));
			return Some(data);
		},
		// 响应传输流
		Head::StreamOpenAck		=> if let Some(payload) = packet.payload_as_stream_open_ack() {
			quickly_response!(payload, |response| WrapEvent::Stream(StreamEvent::OpenAck(response)));
		},
		// 请求重连流
		Head::StreamReopen		=> quickly_none!(WrapEvent::Stream(StreamEvent::Reopen)),
		// 响应重连流
		Head::StreamReopenAck	=> if let Some(payload) = packet.payload_as_session_close_ack() {
			quickly_response!(payload, |response| WrapEvent::Stream(StreamEvent::ReopenAck(response)));
		},
		// 传输分块
		Head::StreamChunk		=> if let Some(payload) = packet.payload_as_stream_chunk() {
			let bytes = if let Some(the_bytes) = payload.data() {
				Bytes::copy_from_slice(the_bytes.bytes())
			} else {
				Bytes::new()
			};

			let order = match quickly_ubig(Some(payload.order())) {
				Some(order) => order,
				None => return None
			};

			let chunk = channel::stream::Chunk {
				data: bytes,
				order
			};

			let data = impl_data!(WrapEvent::Stream(StreamEvent::Chunk(chunk)));
			return Some(data);
		},
		// 响应接收分块
		Head::StreamChunkAck	=> if let Some(payload) = packet.payload_as_stream_chunk_ack() {
			let order = match quickly_ubig(Some(payload.order())) {
				Some(order) => order,
				None => return None
			};

			let chunk_ack = channel::stream::ChunkAck {
				order
			};

			let data = impl_data!(WrapEvent::Stream(StreamEvent::ChunkAck(chunk_ack)));
			return Some(data);
		},
		Head::StreamFlush		=> if let Some(payload) = packet.payload_as_stream_flush() {
			let length = match quickly_ubig(Some(payload.length())) {
				Some(order) => order,
				None => return None
			};

			let flush = channel::stream::Flush {
				length
			};

			let data = impl_data!(WrapEvent::Stream(StreamEvent::Flush(flush)));
			return Some(data);
		},
		Head::StreamFlushAck	=> quickly_none!(WrapEvent::Stream(StreamEvent::FlushAck)),
		Head::StreamLack		=> if let Some(payload) = packet.payload_as_stream_lack() {
			if let Some(the_orders) = payload.orders() {
				let mut orders = vec![];
				let mut errored = false;
				for the_order in the_orders.iter() {
					let order = match quickly_ubig(Some(the_order)) {
						Some(order) => order,
						None => {
							errored = true;
							break;
						}
					};

					orders.push(order);
				}

				if errored {
					return None;
				}

				let lack = channel::stream::Lack {
					orders
				};

				let data = impl_data!(WrapEvent::Stream(StreamEvent::Lack(lack)));
				return Some(data);
			}
		},
		Head::StreamLater		=> quickly_none!(WrapEvent::Stream(StreamEvent::Later)),
		Head::StreamGo			=> quickly_none!(WrapEvent::Stream(StreamEvent::Go)),
		Head::StreamClear		=> if let Some(payload) = packet.payload_as_stream_clear() {
			let data = impl_data!(WrapEvent::Stream(StreamEvent::Clear(Reason { code: payload.reason().code() })));
			return Some(data);
		},
		Head::HealthPing		=> quickly_none!(WrapEvent::Link(LinkEvent::Health(Health::Ping))),
		Head::HealthPong		=> quickly_none!(WrapEvent::Link(LinkEvent::Health(Health::Pong))),
		// 不兼容？？？
		_ => {}
	}

	None
}


const EVENT_ID: Lazy<AtomicPoll> = Lazy::new(|| AtomicPoll::new());
const U64_MAX: Lazy<UBig> = Lazy::new(|| UBig::from(u64::MAX));

pub fn serialize_datagram<'a>(builder: &mut flatbuffers::FlatBufferBuilder<'a>, data: self::channel::Datagram) -> flatbuffers::WIPOffset<protocol::packet::Packet<'a>> {
	use flatbuffers::{FlatBufferBuilder, WIPOffset, UnionWIPOffset, Vector};

	let id = {
		let link_id = data.id.link;
		let event_id = match data.id.event {
			Some(id) => id,
			None => EVENT_ID.get_and_increase()
		};
		let session_id = data.id.session;
		let stream_id = data.id.stream;

		self::channel::IdSet {
			link: link_id,
			event: Some(event_id),
			session: session_id,
			stream: stream_id
		}
	};

	// 快速创建空 Payload
	fn quickly_none_payload<'a>(builder: &mut FlatBufferBuilder<'a>) -> (protocol::packet::Payload, WIPOffset<UnionWIPOffset>) {
		use protocol::packet::{Payload, NoneBuilder};
		(Payload::None, NoneBuilder::new(builder).finish().as_union_value())
	}

	// 处理 Reason
	fn handle_reason<'a>(builder: &mut FlatBufferBuilder<'a>, my_reason: self::Reason) -> WIPOffset<protocol::value::Reason<'a>> {
		use crate::protocol::value::ReasonBuilder;
		let mut reason_builder = ReasonBuilder::new(builder);
		reason_builder.add_code(my_reason.code);
		let reason = reason_builder.finish();

		return reason;
	}
	
	// 快速生成 Response
	fn handle_response<'a>(builder: &mut FlatBufferBuilder<'a>, acceptable: super::strategy::Acceptable) -> (protocol::value::Response, Option<WIPOffset<UnionWIPOffset>>) {
		use super::strategy::Acceptable;
		use crate::protocol::value::{AcceptBuilder, RejectBuilder, Response};
		match acceptable {
			Acceptable::Accept => {
				let accept = AcceptBuilder::new(builder).finish();

				(Response::Accept, Some(accept.as_union_value()))
			},
			Acceptable::Reject(my_reason) => {
				let reason = handle_reason(builder, my_reason);
				let mut reject_builder = RejectBuilder::new(builder);
				reject_builder.add_reason(reason);
				let reject = reject_builder.finish();

				(Response::Reject, Some(reject.as_union_value()))
			},
		}
	}

	// 生成 UBig
	fn handle_ubig<'a>(builder: &mut FlatBufferBuilder<'a>, value: UBig) -> WIPOffset<UnionWIPOffset> {
		use protocol::value::{UBigBuilder, UBigUnion, UInt64Builder, BytesBuilder};
		let (from_type, from) = if value > U64_MAX.clone() {
			let the_bytes = builder.create_vector(&value.to_be_bytes());
			let mut bytes_builder = BytesBuilder::new(builder);
			bytes_builder.add_bytes(the_bytes);
			let bytes = bytes_builder.finish();

			(UBigUnion::Bytes, bytes.as_union_value())
		} else {
			let number = u64::try_from(value).unwrap();
			let mut uint64_builder = UInt64Builder::new(builder);
			uint64_builder.add_uint(number);
			let uint64 = uint64_builder.finish();

			(UBigUnion::UInt64, uint64.as_union_value())
		};

		let mut ubig_builder = UBigBuilder::new(builder);
		ubig_builder.add_from_type(from_type);
		ubig_builder.add_from(from);

		ubig_builder.finish().as_union_value()
	}
	
	// 生成 UBytes
	fn handle_ubytes<'a>(builder: &mut FlatBufferBuilder<'a>, bytes: bytes::Bytes) -> WIPOffset<Vector<'a, u8>> {
		builder.create_vector(&bytes.to_vec())
	}

	use self::channel::Event as WrapEvent;
	use protocol::packet::{Head, Payload};

	fn serialize_event<'a>(builder: &mut flatbuffers::FlatBufferBuilder<'a>, event: WrapEvent) -> (Head, (Payload, WIPOffset<UnionWIPOffset>)) {
		match event {
			WrapEvent::Link(event) => {
				use self::channel::link::{Event, Health};

				match event {
					Event::SessionAck(the_event) => {
						return serialize_event(builder, WrapEvent::Session(the_event));
					},
					Event::StreamAck(the_event) => {
						return serialize_event(builder, WrapEvent::Stream(the_event));
					},
					Event::Health(health) => todo!(),
				}
			},
			WrapEvent::Session(event) => {
				use self::channel::session::Event;

				match event {
					Event::Open(open_options) => todo!(),
					Event::OpenAck(acceptable) => {
					},
					Event::Reopen => {
					},
					Event::ReopenAck(acceptable) => todo!(),
					Event::Close => todo!(),
					Event::CloseAck(acceptable) => todo!(),
					Event::Death(reason) => todo!(),
				}
			},
			WrapEvent::Stream(event) => {
				use self::channel::stream::Event;
				
				match event {
					Event::Block(block) => {
						use protocol::stream::BlockBuilder;

						let data = handle_ubytes(builder, block.data);

						let mut builder = BlockBuilder::new(builder);
						builder.add_ask_response(block.ask_response);
						builder.add_data(data);
						let payload = builder.finish().as_union_value();

						
						return (Head::StreamBlock, (Payload::Stream_Block, payload));
					},
					Event::BlockAck => todo!(),
					Event::Open { options, length } => todo!(),
					Event::OpenAck(acceptable) => todo!(),
					Event::Reopen => todo!(),
					Event::ReopenAck(acceptable) => todo!(),
					Event::Chunk(chunk) => todo!(),
					Event::ChunkAck(chunk_ack) => todo!(),
					Event::Flush(flush) => todo!(),
					Event::FlushAck => todo!(),
					Event::Lack(lack) => todo!(),
					Event::Later => todo!(),
					Event::Go => todo!(),
					Event::Clear(reason) => todo!(),
				}
			}
		}

		Ok(())
	}

	let (head, (payload_type, payload)) = serialize_event(builder, data.event);

	let packet_id = {
		use protocol::packet::{PacketIdBuilder, Id};
		fn impl_id<'a>(builder: &mut FlatBufferBuilder<'a>, my_id: Option<UBig>) -> (Id, WIPOffset<UnionWIPOffset>) {
			use protocol::packet::NoneBuilder;
			if let Some(the_id) = my_id {
				(Id::Value_UBig, handle_ubig(builder, the_id))
			} else {
				(Id::None, NoneBuilder::new(builder).finish().as_union_value())
			}
		}

		let (event_id_type, event_id) = impl_id(builder, id.event);
		let (session_id_type, session_id) = impl_id(builder, id.session);
		let (stream_id_type, stream_id) = impl_id(builder, id.stream);

		let mut packet_id_builder = PacketIdBuilder::new(builder);
		packet_id_builder.add_link_id(id.link);
		
		macro_rules! impl_id {
			($name:ident) => {
				paste::paste! {
					packet_id_builder.[<add_ $name _id_type>]([<$name _id_type>]);
					packet_id_builder.[<add_ $name _id>]([<$name _id>]);
				}
			};
		}

		impl_id!(event);
		impl_id!(session);
		impl_id!(stream);

		packet_id_builder.finish()
	};

	let packet = {
		use protocol::packet::PacketBuilder;

		let mut packet_builder = PacketBuilder::new(builder);
		packet_builder.add_head(head);
		packet_builder.add_id(packet_id);
		packet_builder.add_payload_type(payload_type);
		packet_builder.add_payload(payload);

		packet_builder.finish()
	};

	packet
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