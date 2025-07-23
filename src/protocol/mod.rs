pub mod packet;

/// 对 Capnp 的部分简单转换
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

	// 传输的数据
	#[derive(Clone)]
	pub enum Data {
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
	}

	// 封装 channel 消息
	pub fn wrap_channel_data<D>(packet_id: self::PacketId, data: D) -> self::WrapData<D> {
		self::WrapData {
			id: packet_id,
			data
		}
	}
}