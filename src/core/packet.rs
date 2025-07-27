use std::sync::Arc;
use super::handler::channel_data::PacketIdBuilder;
use super::capnp as protocol;
use ibig::UBig;
use thiserror::Error;
use once_cell::sync::Lazy;
use forever_safer::atomic_poll::AtomicPoll;
use crate::packet_capnp as packet;
use crate::stream_capnp as stream;

const PACKET_ID: Lazy<AtomicPoll> = Lazy::new(|| AtomicPoll::new());

fn set_ubig<'a, T>(builder: T, value: UBig) -> Result<(), PacketError>
where 
	T: FnOnce(u32) -> capnp::primitive_list::Builder<'a, u8>
{
	let bytes = value.to_be_bytes();
	let length: u32 = bytes.len().try_into().map_err(|_| PacketError::OverflowError)?;

	let mut the_builder = builder(length);
	let mut ubig_builder = the_builder.reborrow();
	for index in 0..length {
		ubig_builder.set(index, bytes[index as usize]);
	}

	Ok(())
}

#[derive(Debug, Error)]
pub enum PacketError {
	#[error("The ID configuration is incorrect.")]
	IdError,
	#[error("The data to be sent is too large (> {}).", u32::MAX)]
	OverflowError,
	#[error(transparent)]
	CapnpError(#[from] capnp::Error)
}

pub fn generate_packet(
	id_builder: PacketIdBuilder,
	payload: protocol::PacketPayload,
) -> Result<(UBig, Arc<Vec<u8>>), PacketError> {
	let id = {
		let mut builder = PacketIdBuilder::from(id_builder);

		let id = if let Ok(id) = builder.build() {
			id
		} else if let Ok(id) = builder.order_id(PACKET_ID.get_and_increase()).build() {
			id
		} else {
			return Err(PacketError::IdError);
		};

		if id.stream_id.is_some() && id.session_id.is_none() {
			return Err(PacketError::IdError);
		}

		if id.frame_id.is_some() && id.stream_id.is_none() {
			return Err(PacketError::IdError);
		}

		if id.block_order_id.is_some() && id.frame_id.is_none() {
			return Err(PacketError::IdError);
		}

		id
	};

	let mut builder = capnp::message::Builder::default();
	let mut message = builder.get_root::<packet::packet::Builder>()
		.map_err(|error| PacketError::CapnpError(error))?;

	// 设置 ID
	{
		let mut id_builder = message.reborrow().init_id();

		set_ubig(|size| id_builder
			.reborrow()
			.init_order_id()
			.init_value()
			.init_value(size), id.order_id.clone())?;

		// 快速设置 Option<> 的 ID
		macro_rules! set_id {
			($name:ident) => {
				{
					paste::paste! {
						let mut builder = id_builder.reborrow().[<init_ $name>]();
					}

					if let Some(id) = id.$name {
						set_ubig(|size| builder
							.reborrow()
							.init_value()
							.init_value(size), id)?;
					} else {
						builder.reborrow().set_none(());
					}
				}
			};
		}
		
		set_id!(session_id);
		set_id!(stream_id);
		set_id!(frame_id);
	}

	// 设置 Head 和 Payload
	{
		let payload_builder = message.reborrow().init_payload();

		use protocol::PacketPayload;
		use packet::Head;
		let head = match payload {
			PacketPayload::SessionOpen => Head::SessionOpen,
			PacketPayload::SessionReopen => Head::StreamReopen,
			PacketPayload::SessionAccept => Head::SessionAccept,
			PacketPayload::SessionReject(reason) => {
				payload_builder
					.init_session_reject()
					.set_reason_code(reason.reason_code);

				Head::SessionReject
			},
			PacketPayload::SessionClose => Head::SessionClose,
			PacketPayload::SessionCloseAck => Head::SessionCloseAck,
			PacketPayload::SessionDeath(reason) => {
				payload_builder
					.init_session_death()
					.set_reason_code(reason.reason_code);

				Head::SessionDeath
			},
			PacketPayload::StreamOpen(stream_open) => {
				use protocol::StreamMode;
				let mode = match stream_open.options.mode {
					StreamMode::OnlyRead => stream::StreamMode::OnlyRead,
					StreamMode::OnlyWrite => stream::StreamMode::OnlyWrite,
					StreamMode::TwoWay => stream::StreamMode::TwoWay,
				};

				payload_builder
					.init_stream_open()
					.init_options()
					.set_mode(mode);

				Head::StreamOpen
			},
			PacketPayload::StreamReopen => Head::StreamReopen,
			PacketPayload::StreamAccept => Head::StreamAccept,
			PacketPayload::StreamReject(reason) => {
				payload_builder
					.init_stream_reject()
					.set_reason_code(reason.reason_code);

				Head::StreamReject
			},
			PacketPayload::StreamClose => Head::StreamClose,
			PacketPayload::StreamCloseAck => Head::StreamCloseAck,
			PacketPayload::StreamDeath(reason) => {
				payload_builder
					.init_stream_death()
					.set_reason_code(reason.reason_code);

				Head::StreamDeath
			},
			PacketPayload::FramePacket(frame_packet) => {
				payload_builder
					.init_frame_packet()
					.set_data(&frame_packet.data);

				Head::FramePacket
			},
			PacketPayload::FramePacketAck => Head::FramePacketAck,
			PacketPayload::FrameReq(frame_req) => {
				let mut builder = payload_builder.init_frame_req();

				set_ubig(|size| builder
					.reborrow()
					.init_length()
					.init_value(size), frame_req.length)?;

				let mut options_builder = builder
					.reborrow()
					.init_options();

				options_builder.set_disposable(frame_req.options.disposable);
				options_builder.set_strong_integrity(frame_req.options.strong_integrity);
				options_builder.set_strong_orderliness(frame_req.options.strong_orderliness);

				Head::FrameReq
			},
			PacketPayload::FrameReqAccept => Head::FrameReqAccept,
			PacketPayload::FrameReqReject(reason) => {
				payload_builder
					.init_frame_req_reject()
					.set_reason_code(reason.reason_code);

				Head::FrameReqReject
			},
			PacketPayload::FrameBlock(frame_block) => {
				let mut block_builder = payload_builder
					.init_frame_block();
				
				set_ubig(|size| block_builder
					.reborrow()
					.init_order()
					.init_value(size), frame_block.order)?;
				block_builder.reborrow().set_data(&frame_block.data);

				Head::FrameBlock
			},
			PacketPayload::FrameBlockAck(frame_block_ack) => {
				set_ubig(|size| payload_builder
					.init_frame_block_ack()
					.init_order()
					.init_value(size), frame_block_ack.order)?;

				Head::FrameBlockAck
			},
			PacketPayload::FrameFlush => Head::FrameFlush,
			PacketPayload::FrameFlushAck => Head::FrameFlushAck,
			PacketPayload::FrameClear(reason) => {
				payload_builder
					.init_frame_clear()
					.set_reason_code(reason.reason_code);

				Head::FrameClear
			},
			PacketPayload::FrameBlockLater => Head::FrameBlockLater,
			PacketPayload::FrameBlockGo => Head::FrameBlockGo,
			PacketPayload::FrameBlockLack(frame_block_lack) => {
				let length = frame_block_lack.orders.len().try_into().map_err(|_| PacketError::OverflowError)?;

				let mut orders_builder = payload_builder
					.init_frame_block_lack()
					.init_orders(length);

				for index in 0..length {
					set_ubig(|size| orders_builder
						.reborrow()
						.get(index)
						.init_value(size), frame_block_lack.orders[index as usize].clone())?;
				}

				Head::FrameBlockLack
			},
			PacketPayload::FrameReqRetry => Head::FrameReqRetry,
			PacketPayload::FrameReqRetryAck(frame_req_retry_ack) => {
				let mut ack_builder = payload_builder
					.init_frame_req_retry_ack();

				use protocol::FrameReqRetryAck;
				match frame_req_retry_ack {
					FrameReqRetryAck::Accept => {
						ack_builder.reborrow().set_accept(true);
						ack_builder.reborrow().set_none(());
					},
					FrameReqRetryAck::Reject(reason) => {
						ack_builder.reborrow().set_accept(false);
						ack_builder
							.reborrow()
							.init_reason()
							.set_reason_code(reason.reason_code);
					},
				}

				Head::FrameReqRetryAck
			},
			PacketPayload::HealthPing => Head::HealthPing,
			PacketPayload::HealthPong => Head::HealthPong,
		};

		message.reborrow().set_head(head);
	}

	let mut bytes = vec![];
	capnp::serialize::write_message(&mut bytes, &builder).map_err(|error| PacketError::CapnpError(error))?;

	return Ok((id.order_id, Arc::new(bytes)));
}