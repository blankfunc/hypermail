use std::{fmt::Debug, sync::Arc};
use crate::packet_capnp as packet;
use crate::frame_capnp as frame;
use crate::value_capnp as value;
use thiserror::Error;
use ibig::UBig;

pub mod proc_convert {
	use crate::value_capnp as value;
	use ibig::UBig;

	#[derive(Debug, Clone, thiserror::Error)]
	pub enum ConvertError {
		#[error(transparent)]
		CapnpError(#[from] capnp::Error),

		#[error(transparent)]
		NotInSchema(#[from] capnp::NotInSchema)
	}

	/// 从 Value.BigUInt (be_bytes) 中读取 UBig
	pub fn get_ubig<'a>(id: value::big_u_int::Reader<'a>) -> Result<Option<UBig>, ConvertError> {
		let value = id.get_value().map_err(|err| ConvertError::CapnpError(err))?;

		return Ok(value.as_slice().map(|bytes| UBig::from_be_bytes(bytes)));
	}

	/// 从 Value.Id 中获取 ID
	pub fn get_id<'a>(id: value::id::Reader<'a>) -> Result<Option<UBig>, ConvertError> {
		if let value::id::Which::Value(ubig_result) =
			id.which().map_err(|err| ConvertError::NotInSchema(err))? {
			let ubig = ubig_result.map_err(|err| ConvertError::CapnpError(err))?;

			return get_ubig(ubig);
		}

		return Ok(None);
	}

	#[derive(Clone)]
	pub struct Reason {
		reason_code: u64
	}

	/// 从 Value.EventReason 中获取原因
	pub fn get_reason<'a>(reader: value::event_reason::Reader<'a>) -> Result<Reason, ConvertError> {
		let reason_code = reader.get_reason_code();

		Ok(Reason {
			reason_code
		})
	}
}

/// 内部传输待处理消息
pub mod channel_data {
	use derive_builder::Builder;
	use super::proc_convert::Reason;
	use ibig::UBig;

	/// ID
	#[derive(Clone, Builder)]
	pub struct PacketId {
		pub(crate) order_id: UBig,
		pub(crate) session_id: Option<UBig>,
		pub(crate) stream_id: Option<UBig>,
		pub(crate) frame_id: Option<UBig>,
		pub(crate) block_order_id: Option<UBig>
	}

	// ReqOptions
	#[derive(Clone, Builder)]
	pub struct FrameReqOptions {
		pub(crate) strong_orderliness: bool,
		pub(crate) strong_integrity: bool,
		pub(crate) disposable: bool
	}

	#[derive(Clone)]
	pub struct FrameReq {
		pub(crate) options: FrameReqOptions,
		pub(crate) length: Option<UBig>
	}

	/// 外部包裹结构
	#[derive(Clone)]
	pub struct WrapData<D> {
		pub(crate) id: PacketId,
		pub(crate) data: D
	}

	#[derive(Debug, Clone, thiserror::Error)]
	pub enum ThreadError {
		#[error(transparent)]
		IO(#[from] crate::io::IOError)
	}

	#[derive(Debug, Clone, thiserror::Error)]
	pub enum WhereError {
		#[error(transparent)]
		Thread(#[from] ThreadError)
	}

	// 传输的数据
	#[derive(Clone)]
	pub enum Data {
		// 错误
		Error(WhereError),

		SessionAccept(WrapData<()>),
		SessionReject(WrapData<Reason>),
		SessionClose(WrapData<()>),
		SessionCloseAck(WrapData<()>),
		SessionDeath(WrapData<Reason>),
		StreamAccept(WrapData<()>),
		StreamReject(WrapData<Reason>),
		StreamClose(WrapData<()>),
		StreamCloseAck(WrapData<()>),
		StreamDeath(WrapData<Reason>),
		FramePacket(WrapData<Vec<u8>>),
		FramePacketAck(WrapData<()>),
		FrameReq(WrapData<FrameReq>),
		FrameReqAccept(WrapData<()>),
		FrameReqReject(WrapData<Reason>),
		FrameBlock(WrapData<Vec<u8>>),
		FrameBlockAck(WrapData<()>),
		FrameFlush(WrapData<()>),
		FrameFlushAck(WrapData<()>),
		FrameClear(WrapData<Reason>),
		FrameBlockLater(WrapData<()>),
		FrameBlockGo(WrapData<()>),
		FrameBlockLack(WrapData<Vec<UBig>>),
		FrameReqRetry(WrapData<()>),
		FrameReqRetryAck(WrapData<Option<Reason>>),
		HealthPong(WrapData<()>),

		SessionOpen(WrapData<()>),
		SessionReopen(WrapData<()>),
		StreamOpen(WrapData<()>),
		StreamReopen(WrapData<()>),
		HealthPing(WrapData<()>),
	}

	// 封装 channel 消息
	pub fn wrap_channel_data<D>(packet_id: self::PacketId, data: D) -> self::WrapData<D> {
		self::WrapData {
			id: packet_id,
			data
		}
	}
}

use self::proc_convert::ConvertError;

#[derive(Debug, Error, Clone)]
pub enum SerializeError {
	#[error(transparent)]
	ConvertError(ConvertError),
	#[error("Unable to resolve to `PacketId`. {}", .0)]
	InvalidPacketId(capnp::Error),
	#[error("Unable to resolve to `OrderId` for `Packet`. ({:?})", .0)]
	InvalidPacketOrderId(Option<capnp::Error>),
	#[error("Cannot resolve to 'SessionId' and the current `Head` requires it. ({:?})", .0)]
	InvalidSessionId(Option<capnp::Error>),
	#[error("Cannot resolve to 'StreamId' and the current `Head` requires it. ({:?})", .0)]
	InvalidStreamId(Option<capnp::Error>),
	#[error("Cannot resolve to 'FrameId' and the current `Head` requires it. ({:?})", .0)]
	InvalidFrameId(Option<capnp::Error>),
	#[error("Unable to resolve to `Head` or no corresponding `Head`. {}", .0)]
	InvalidHead(capnp::NotInSchema),
	#[error("The `Head` ({head}) should not appear here.")]
	UnexpectedHead { head: String },
	#[error("Cannot resolve to 'Payload' and the current `Head` requires it. {}", .0)]
	InvalidPayload(capnp::Error),
	#[error("The received `Payload` and do not match the requirements for `Head`.")]
	UnexpectedPayload,
	#[error("Unable to parse `Reason` from `Payload`.")]
	InvalidReason(ConvertError),
	#[error("Unable to parse `Data`/`Text` from `Payload`.")]
	InvalidBlob(capnp::Error),
	#[error("Unable to resolve to `OrderId` for `Packet.Block`. ({:?})", .0)]
	InvalidBlockOrderId(Option<capnp::Error>),
	#[error("Unable to resolve to `Options` for `Packet.Req`. ({:?})", .0)]
	InvalidReqOptions(capnp::Error),
	#[error("Unable to resolve to `Length` for `Packet.Req`. ({:?})", .0)]
	InvalidReqLength(capnp::Error),
	#[error("Unable to resolve to list of `OrderId` for `Packet.Lack`. ({:?})", .0)]
	InvalidLackOrderIds(capnp::Error),
	#[error("Unable to resolve to a `OrderId` for `Packet.Lack`. ({:?})", .0)]
	InvalidLackOrderId(Option<ConvertError>),
	#[error("Cannot resolve to 'Reject' pf `ReqRetryAck`. ({:?})", .0)]
	InvalidFrameReqRetryAckReject(Option<capnp::Error>)
}

#[derive(Debug, Error, Clone)]
pub enum PacketError {
	#[error("An error occurred while creating the thread: {}", .0)]
	RuntimeError(#[from] Arc<std::io::Error>),
	#[error(transparent)]
	SerializeError(SerializeError),
	#[error("An error occurred while transmitting messages through the internal channel: {}", .0)]
	ChannelError(flume::SendError<self::channel_data::Data>),
	#[error("Error occurred while encapsulating internal channel message.")]
	WrapError,
}

// 实现快速绑定 SerializeError
impl From<SerializeError> for PacketError {
	fn from(value: SerializeError) -> Self {
		PacketError::SerializeError(value)
	}
}

// 实现快速绑定 ChannelError
impl From<flume::SendError<channel_data::Data>> for PacketError {
	fn from(value: flume::SendError<channel_data::Data>) -> Self {
		PacketError::ChannelError(value)
	}
}

pub fn handle_packet<'a>(sender: flume::Sender<channel_data::Data>, reader: packet::packet::Reader<'a>) -> Result<(), PacketError> {
	let head = reader.get_head()
		.map_err(|err| SerializeError::InvalidHead(err))?;

	// 获取各种 ID
	let (order_id, try_session_id, try_stream_id, try_frame_id) = {
		// 封装 get_id 的错误
		fn get_id<'a>(id: value::id::Reader<'a>) -> Result<Option<UBig>, PacketError> {
			let result = self::proc_convert::get_id(id)
			.map_err(|err| SerializeError::ConvertError(err));

			Ok(result?)
		}

		// 再次封装 get_id，方便后续判断是否需要对应 id
		fn try_get_id<'a>(id: value::id::Reader<'a>, err: SerializeError) -> Result<Result<UBig, PacketError>, PacketError> {
			let result =  self::proc_convert::get_id(id)
			.map_err(|err| SerializeError::ConvertError(err))?
			.ok_or(PacketError::SerializeError(err));

			Ok(result)
		}

		let packet_id = reader.reborrow().get_id()
			.map_err(|err| SerializeError::InvalidPacketId(err))?;

		let order_id = {
			let id_reader = packet_id.get_order_id()
				.map_err(|err| SerializeError::InvalidPacketOrderId(Some(err)))?;
			
			get_id(id_reader)?
				// Packet 一定要有 OrderID
				.ok_or(SerializeError::InvalidPacketOrderId(None))?
		};

		let try_session_id = {
			let id_reader = packet_id.get_session_id()
				.map_err(|err| SerializeError::InvalidSessionId(Some(err)))?;

			try_get_id(id_reader, SerializeError::InvalidSessionId(None))?
		};

		let try_stream_id = {
			let id_reader = packet_id.get_stream_id()
				.map_err(|err| SerializeError::InvalidStreamId(Some(err)))?;

			try_get_id(id_reader, SerializeError::InvalidStreamId(None))?
		};

		let try_frame_id = {
			let id_reader = packet_id.get_frame_id()
				.map_err(|err| SerializeError::InvalidFrameId(Some(err)))?;

			try_get_id(id_reader, SerializeError::InvalidFrameId(None))?
		};

		(order_id, try_session_id, try_stream_id, try_frame_id)
	};

	// 包装一下 ID
	let mut packet_id_builder = {
		let mut builder = channel_data::PacketIdBuilder::default();
		builder.order_id(order_id.clone())
			.session_id(try_session_id.clone().ok())
			.stream_id(try_stream_id.clone().ok())
			.frame_id(try_frame_id.clone().ok());
		builder
	};
		

	// 简单做个 ID 判断
	{
		if try_stream_id.is_ok() && try_session_id.is_err() {
			// 如果有 StreamId 就一定有 SessionId
			return Err(PacketError::SerializeError(SerializeError::InvalidSessionId(None)));
		}

		if try_frame_id.is_ok() && try_stream_id.is_err() {
			// 如果有 FrameId 就一定有 StreamId
			return Err(PacketError::SerializeError(SerializeError::InvalidStreamId(None)));
		}
	}

	// 尝试获取 Payload
	let try_payload = reader.get_payload()
		.map_err(|err| SerializeError::InvalidPayload(err));

	use packet::Head;
	use packet::packet_payload::*;
	use self::proc_convert::Reason;
	use self::channel_data::{self, Data, wrap_channel_data};

	// 封装 get_reason
	fn get_reason<'a>(reader: value::event_reason::Reader<'a>) -> Result<Reason, PacketError> {
		let result =  self::proc_convert::get_reason(reader)
			.map_err(|err| SerializeError::InvalidReason(err));

		Ok(result?)
	}

	// 封装发送
	let boradcast = |data: Data| -> Result<(), PacketError> {
		sender.send(data).map_err(|err| PacketError::ChannelError(err))
	};

	// 懒得写 build
	macro_rules! packet_id {
		() => {
			packet_id_builder.build().map_err(|_| PacketError::WrapError)?
		};
	}

	/// 对没有 payload 的 Packet 进行快速处理
	macro_rules! non_payload {
		// 需要验证 ID
		($payload_name:ident) => {
			let data = Data::$payload_name(wrap_channel_data(packet_id!(), ()));
			boradcast(data)?;
		};
	}

	/// 快速获取 payload（没有则直接抛出错误）
	macro_rules! with_payload {
		($payload_name:ident, |$val:ident| $body:block) => {
			if let Ok($payload_name(Ok(the_reader))) = try_payload?.which() {
				let $val = the_reader;
				$body
			} else {
				return Err(PacketError::SerializeError(SerializeError::UnexpectedPayload));
			}
		};
	}

	/// match 匹配的 reason 进行统一处理
	macro_rules! payload_reason {
		($payload_name:ident) => {
			with_payload!($payload_name, |reader| {
				// 获取对应的 Payload
				let data = Data::$payload_name(wrap_channel_data(packet_id!(), get_reason(reader)?));
				boradcast(data)?;
			});
		}
	}

	// 快速拿到 Payload 中的 data
	macro_rules! payload_data {
		($name:ident) => {
			$name
				.get_data()
				.map_err(|err| SerializeError::InvalidBlob(err))?
				.to_vec()
		};
	}

	// 快速拿到 Block 的 OrderId
	macro_rules! block_order_id {
		($name:ident) => {{
			use self::proc_convert::get_ubig;
			let order_reader = $name.get_order().map_err(|err| SerializeError::InvalidBlockOrderId(Some(err)))?;
			let order = get_ubig(order_reader).map_err(|err| SerializeError::ConvertError(err))?;
			order.ok_or(SerializeError::InvalidBlockOrderId(None))?
		}}
	}

	match head {
		Head::SessionAccept => {
			// 对方接受了创建/重连 Session
			let _ = try_session_id?;
			non_payload!(SessionAccept);
		},
		Head::SessionReject => {
			// 对方拒绝创建/重连 Session
			payload_reason!(SessionReject);
		},
		Head::SessionClose => {
			// 对方请求关闭会话
			let _ = try_session_id?;
			non_payload!(SessionClose);
		},
		Head::SessionCloseAck => {
			// 对方收到了关闭请求
			let _ = try_session_id?;
			non_payload!(SessionCloseAck);
		},
		Head::SessionDeath => {
			// 对方强制关闭了连接
			let _ = try_session_id?;
			payload_reason!(SessionDeath);
		},
		Head::StreamAccept => {
			// 对方接受了创建/重连 Stream
			let _ = try_stream_id?;
			non_payload!(StreamAccept);
		},
		Head::StreamReject => {
			// 对方拒绝创建/重连 Stream
			payload_reason!(StreamReject);
		},
		Head::StreamClose => {
			// 对方请求关闭 Stream
			let _ = try_stream_id?;
			non_payload!(StreamClose);
		},
		Head::StreamCloseAck => {
			// 对方收到了关闭请求
			let _ = try_stream_id?;
			non_payload!(StreamCloseAck);
		},
		Head::StreamDeath => {
			// 对方强制关闭了 Stream
			let _ = try_stream_id?;
			payload_reason!(StreamDeath);
		},
		Head::FramePacket => {
			// 整包数据
			let _ = try_stream_id?;
			with_payload!(FramePacket, |reader| {
				let bytes = payload_data!(reader);
				let data = Data::FramePacket(wrap_channel_data(packet_id!(), bytes));
				boradcast(data)?;
			});
		},
		Head::FramePacketAck => {
			// 对方收到了整包数据
			let _ = try_stream_id?;
			non_payload!(FramePacketAck);
		},
		Head::FrameReq => {
			// 对方请求创建写入流
			let _ = try_stream_id?;
			with_payload!(FrameReq, |reader| {
				use self::channel_data::{FrameReqOptionsBuilder, FrameReq};

				let options_reader = reader.get_options().map_err(|err| SerializeError::InvalidReqOptions(err))?;
				let mut options_builder = FrameReqOptionsBuilder::default();
				let options = options_builder
					.strong_orderliness(options_reader.get_strong_orderliness())
					.strong_integrity(options_reader.get_strong_integrity())
					.disposable(options_reader.get_disposable())
					.build()
					.map_err(|_| PacketError::WrapError)?;

				let length_reader = reader.get_length().map_err(|err| SerializeError::InvalidReqLength(err))?;
				let length = length_reader.get_value()
					.map_err(|err| SerializeError::InvalidReqLength(err))?
					.as_slice()
					.map(|bytes| UBig::from_be_bytes(bytes));

				let req = FrameReq {
					options,
					length
				};

				let data = Data::FrameReq(wrap_channel_data(packet_id!(), req));
				boradcast(data)?;
			});
		},
		Head::FrameReqAccept => {
			// 允许发送连续块
			let _ = try_frame_id?;
			non_payload!(FrameReqAccept);
		},
		Head::FrameReqReject => {
			// 拒绝发送连续块
			let _ = try_stream_id?;
			payload_reason!(FrameReqReject);
		},
		Head::FrameBlock => {
			// 分块的其中一个块
			let _ = try_frame_id?;
			with_payload!(FrameBlock, |reader| {
				let order_id = block_order_id!(reader);
				packet_id_builder.order_id(order_id);

				let bytes = payload_data!(reader);
				let data = Data::FramePacket(wrap_channel_data(packet_id!(), bytes));
				
				boradcast(data)?;
			});
		},
		Head::FrameBlockAck => {
			// 对方收到了分块的其中一个块
			let _ = try_frame_id?;
			with_payload!(FrameBlockAck, |reader| {
				let order_id = block_order_id!(reader);
				packet_id_builder.order_id(order_id);
				
				let data = Data::FrameBlockAck(wrap_channel_data(packet_id!(), ()));
				boradcast(data)?;
			});
		},
		Head::FrameFlush => {
			// 发送连续块的结束
			let _ = try_frame_id?;
			non_payload!(FrameFlush);
		},
		Head::FrameFlushAck => {
			// 对方收到了发送连续块的结束
			let _ = try_frame_id?;
			non_payload!(FrameFlushAck);
		},
		Head::FrameClear => {
			// 传输中出现错误
			let _ = try_frame_id?;
			payload_reason!(FrameClear);
		},
		Head::FrameBlockLater => {
			// 延迟连续传输
			let _ = try_frame_id?;
			non_payload!(FrameBlockLater);
		},
		Head::FrameBlockGo => {
			// 继续
			let _ = try_frame_id?;
			non_payload!(FrameBlockGo);
		},
		Head::FrameBlockLack => {
			// 对方请求缺失的分块
			let _ = try_frame_id?;
			with_payload!(FrameBlockLack, |reader| {
				use self::proc_convert::get_ubig;

				// 获取缺失分块的内容
				let orders_reader = reader.get_orders().map_err(|err| SerializeError::InvalidLackOrderIds(err))?;
				let mut order_ids = vec![];
				for reader in orders_reader.iter() {
					let order_id = get_ubig(reader)
						.map_err(|err| SerializeError::InvalidLackOrderId(Some(err)))?
						.ok_or(SerializeError::InvalidLackOrderId(None))?;
					order_ids.push(order_id);
				}

				let data = Data::FrameBlockLack(wrap_channel_data(packet_id!(), order_ids));
				boradcast(data)?;
			});
		},
		Head::FrameReqRetry => {
			// 对方请求重新传输
			let _ = try_frame_id?;
			non_payload!(FrameReqRetry);
		},
		Head::FrameReqRetryAck => {
			// 对方响应重新传输请求
			let _ = try_frame_id?;
			with_payload!(FrameReqRetryAck, |reader| {
				use frame::req_retry_ack;

				// 是否允许
				let is_accept = reader.get_accept();
				let mut reason: Option<Reason> = None;

				// 拒绝
				if !is_accept
				&& let req_retry_ack::Reason(try_reader) = reader.which()
					.map_err(|_| SerializeError::InvalidFrameReqRetryAckReject(None))? {
					let reader = try_reader.map_err(|err| SerializeError::InvalidFrameReqRetryAckReject(Some(err)))?;
					reason = Some(get_reason(reader)?);
				}

				let data = Data::FrameReqRetryAck(wrap_channel_data(packet_id!(), reason));
				boradcast(data)?;
			});
		},
		Head::HealthPong => {
			// 服务器响应心跳包
			non_payload!(HealthPong);
		},
		// 专属于服务端的协议头
		Head::SessionOpen => {
			// 请求创建会话
			non_payload!(SessionOpen);
		},
		Head::SessionReopen => {
			// 请求重连会话
			non_payload!(SessionReopen);
		},
		Head::StreamOpen => {
			// 请求创建 Stream
			let _ = try_session_id?;
			non_payload!(StreamOpen);
		},
		Head::StreamReopen => {
			// 请求重连 Stream
			let _ = try_session_id?;
			non_payload!(StreamReopen);
		},
		Head::HealthPing => {
			// 客户端发送心跳包
			non_payload!(HealthPing);
		}
	};

	Ok(())
}