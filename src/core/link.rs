use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::core::capnp::ValueEventReason;
use crate::io::LinkIO;
use crossbeam::queue::SegQueue;
use ibig::UBig;
use tokio_with_wasm::alias::{
	spawn,
	select,
	sync::Notify
};
use super::handler::channel_data::{self, PacketIdBuilder};
use super::io::{io_handler, capnp_handler};
use super::packet::{PacketError, generate_packet};
use super::capnp::PacketPayload;
use once_cell::sync::Lazy;
use forever_safer::atomic_poll::AtomicPoll;

const SESSION_ID: Lazy<AtomicPoll> = Lazy::new(|| AtomicPoll::new());

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
	#[error("An error occurred while attempting to encode the packet: {}.", .0)]
	PacketError(PacketError),
	#[error("An error occurred during internal channel transmission: {}.", .0)]
	SendError(flume::SendError<Arc<Vec<u8>>>),
	#[error("An error occurred during internal channel transmission: {}.", .0)]
	RecvError(flume::RecvError),
	#[error("Packet is invalid.")]
	InvalidData,
	#[error("Session connection attempt rejected. Code: {}.", .0)]
	SessionReject(u64),
	#[error("Connection disconnected while attempting operation.")]
	Disconnected,
	#[error("Method closed.")]
	Closed
}

impl From<PacketError> for LinkError {
	fn from(value: PacketError) -> Self {
		LinkError::PacketError(value)
	}
}

impl From<flume::SendError<Arc<Vec<u8>>>> for LinkError {
	fn from(value: flume::SendError<Arc<Vec<u8>>>) -> Self {
		LinkError::SendError(value)
	}
}

impl From<flume::RecvError> for LinkError {
	fn from(value: flume::RecvError) -> Self {
		LinkError::RecvError(value)
	}
}

#[derive(Clone)]
pub struct InnerChannel {
	writter: flume::Sender<Arc<Vec<u8>>>,
	reader: flume::Receiver<channel_data::Data>
}

#[derive(Clone)]
pub struct Link {
	mode: Mode,
	channel: InnerChannel
}

#[derive(Clone)]
pub enum Mode {
	Client,
	Server
}

#[derive(Clone)]
pub enum ModeSession {
	Client(crate::client::session::Session),
	Server(crate::server::session::Session),
}

impl Link {
	pub fn new(linkio: Arc<dyn LinkIO>, mode: Mode) -> Self {
		let buffer_cache = Arc::new(SegQueue::new());

		// IO 消息传递
		let (io_sender, io_receiver) = flume::unbounded::<Arc<Vec<u8>>>();

		// Packet 内容传递
		let (channel_sender, channel_receiver) = flume::unbounded::<channel_data::Data>();

		// 处理 IO 的线程
		spawn(io_handler(linkio, buffer_cache.clone(), io_receiver));

		// 解析数据的线程
		spawn(capnp_handler(buffer_cache.clone(), channel_sender));

		Self {
			channel: InnerChannel {
				writter: io_sender,
				reader: channel_receiver
			},
			mode
		}
	}

	pub async fn wait_session(&self) -> Result<SessionWaiter, LinkError> {
		loop {
			let data = self.channel.reader
				.recv_async()
				.await?;

			use channel_data::Data;
			match data {
				Data::StreamOpen(data) => {
					return Ok(SessionWaiter::new(data.id.order_id, None, self.channel.clone(), self.mode.clone()));
				},
				// 其他什么问题都不管我的事
				_ => {}
			}
		}
	}

	pub async fn create_session(&self) -> Result<ModeSession, LinkError> {
		let (id, buffer) = generate_packet(
			PacketIdBuilder::default(),
			PacketPayload::SessionOpen
		)?;

		// 发送
		self.channel.writter
			.clone()
			.send_async(buffer)
			.await
			.map_err(|error| LinkError::SendError(error))?;

		// 尝试接收
		loop {
			let data = self.channel.reader
				.recv_async()
				.await
				.map_err(|error| LinkError::RecvError(error))?;
			
			use channel_data::{WhereError, Data};
			match data {
				// 连接成功
				Data::StreamAccept(data) => {
					if data.id.order_id == id && let Some(id) = data.id.session_id {
						let session = match self.mode {
							// 客户端
							Mode::Client => ModeSession::Client(crate::client::session::Session::new(id, self.channel.clone())),
							// 服务端
							Mode::Server => ModeSession::Server(crate::server::session::Session::new(id, self.channel.clone())),
						};

						return Ok(session);
					}

					return Err(LinkError::InvalidData);
				},
				// 对方拒绝连接
				Data::StreamReject(data) => {
					return Err(LinkError::SessionReject(data.data.reason_code));
				},
				// 断连了
				Data::Error(WhereError::Disconnected) => {
					return Err(LinkError::Disconnected);
				},
				// 其他不关我的事
				_ => {}
			}
		}
	}
}

#[derive(Clone)]
pub struct SessionWaiter {
	order_id: UBig,
	session_id: Option<UBig>,
	channel: InnerChannel,
	handled: Arc<AtomicBool>,
	mode: Mode
}

impl SessionWaiter {
	pub fn new(order_id: UBig, session_id: Option<UBig>, channel: InnerChannel, mode: Mode) -> Self {
		Self {
			order_id,
			session_id,
			channel,
			mode,
			handled: Arc::new(AtomicBool::new(false))
		}
	}

	fn closed(&self) -> Result<(), LinkError> {
		if self.handled.load(Ordering::SeqCst) {
			return Err(LinkError::Closed);
		}

		Ok(())
	}

	pub fn reject(&self, reason: u64) -> Result<(), LinkError> {
		self.closed()?;

		let (_, buffer) = generate_packet(
			PacketIdBuilder::default()
				.order_id(self.order_id.clone())
				.session_id(self.session_id.clone())
				.to_owned(),
			PacketPayload::SessionReject(ValueEventReason { reason_code: reason })
		)?;

		self.channel.writter.send(buffer)?;
		self.handled.store(true, Ordering::SeqCst);
		
		Ok(())
	}

	pub fn accept(&self) -> Result<ModeSession, LinkError> {
		self.closed()?;

		let (_, buffer) = generate_packet(
			PacketIdBuilder::default()
				.order_id(self.order_id.clone())
				.session_id(self.session_id.clone())
				.to_owned(),
			PacketPayload::SessionAccept
		)?;

		self.channel.writter.send(buffer)?;
		self.handled.store(true, Ordering::SeqCst);
		
		let session = match self.mode {
			Mode::Client => ModeSession::Client(crate::client::session::Session::new(self.order_id.clone(), self.channel.clone())),
			Mode::Server => ModeSession::Server(crate::server::session::Session::new(self.order_id.clone(), self.channel.clone())),
		};

		return Ok(session);
	}
}