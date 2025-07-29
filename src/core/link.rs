use crate::io::LinkIO;
use crate::protocol;
use std::{sync::{atomic::AtomicBool, Arc}, vec};
use bytes::Bytes;
use dashmap::DashMap;
use forever_safer::atomic_poll::AtomicPoll;
use tokio_with_wasm::alias::{
	select,
	task::LocalSet,
	sync::{broadcast, mpsc, Notify}
};
use ibig::UBig;
use thiserror::Error;

use super::strategy::Acceptable;
use super::packet::{Reason, channel};

#[derive(Debug, Error)]
pub enum LinkError {

}

#[derive(Clone)]
pub struct InnerChannel {
	pub receiver_master: broadcast::Sender<channel::Datagram>,
	pub sender: mpsc::UnboundedSender<channel::Datagram>
}

impl InnerChannel {
	pub fn get_receiver(&self) -> broadcast::Receiver<channel::Datagram> {
		return self.receiver_master.subscribe();
	}

	pub fn get_sender(&self) -> mpsc::UnboundedSender<channel::Datagram> {
		return self.sender.clone();
	}
}

pub struct DisconnectedStatus {
	pub notify: Notify,
	pub status: AtomicBool
}

#[derive(Clone)]
pub struct InnerContext {
	pub runtime: Arc<LocalSet>,
	pub disconnected: Arc<DisconnectedStatus>
}

#[derive(Clone)]
pub struct Link {
	channel: InnerChannel,
	context: InnerContext
}

pub enum LinkMode {
	Server,
	Client
}

pub enum ModeSession {
	Server,
	Client,
}

impl Link {
	pub fn new(io: Arc<dyn LinkIO>, mode: LinkMode) -> Self {
		let runtime = Arc::new(LocalSet::new());

		// 建立内部数据交换通道
		let (receiver_master, _) = broadcast::channel::<channel::Datagram>(32);
		let (channel_sender, listener) = mpsc::unbounded_channel::<channel::Datagram>();
		let channel = InnerChannel {
			receiver_master,
			sender: channel_sender
		};

		// 建立状态机
		let context = InnerContext {
			runtime: runtime.clone(),
			disconnected: DisconnectedStatus {
				status: AtomicBool::new(false),
				notify: Notify::new()
			}.into()
		};

		// 建立 IO 二进制数据交换通道
		let (io_sender, io_receiver) = mpsc::unbounded_channel::<Bytes>();
		
		// 处理解析 IO 数据
		runtime.spawn_local(io_handler(io, channel.clone(), io_receiver, context.clone()));
		// 处理 Link 数据
		runtime.spawn_local(link_handler(channel.clone(), io_sender, context.clone()));

		Self {
			channel,
			context
		}
	}
}

async fn io_handler(io: Arc<dyn LinkIO>, channel: InnerChannel, mut io_receiver: mpsc::UnboundedReceiver<Bytes>, context: InnerContext) {
	use crate::io::{WritterStream, ReaderStream};
	let writter_streams = DashMap::new();
	let writter_stream_id = AtomicPoll::new();

	// 处理 Reader
	async fn wrap_reader(reader: Arc<dyn ReaderStream>, context: InnerContext, channel: InnerChannel) {
		loop {
			let buffer = match reader.read().await {
				Ok(buffer) => Bytes::copy_from_slice(&buffer),
				Err(_) => {
					continue;
				}
			};

			use crate::protocol::packet::{Packet, MutPacket};
			let mut packets = vec![];

			if let Ok(mut_root) = flatbuffers::root::<MutPacket>(&buffer) {
				// 多包粘合 Batch
				if let Some(roots) = mut_root.batch() {
					packets.append(&mut roots.iter().collect::<Vec<_>>());
				}
			} else if let Ok(root) = flatbuffers::root::<Packet>(&buffer) {
				// 单个包
				packets.push(root);
			} else {
				// 纯杂种
				continue;
			}
			
			use protocol::packet::Head;
			for packet in packets {
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
					let link_id = reader.link_id();
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
						Err(_) => continue,
					}
				};

				// 简单校验 ID
				if [
					// 没有 event_id
					id_set.event.is_none(),
					// 有 stream_id 但是没有 session_id
					id_set.stream.is_some() && id_set.session.is_none()
				].iter().any(|v| *v) {
					continue;
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

				// 发送数据
				macro_rules! channel_send {
					($value:ident) => {
						match channel.get_sender().send($value) {
							Ok(_) => {},
							// 发不出去拉倒
							Err(_) => {}
						}
					};
				}

				// 快速处理 None Payload
				macro_rules! quickly_none {
					($event:expr) => {
						if let Some(_) = packet.payload_as_none() {
							let data = impl_data!($event);

							channel_send!(data);
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
								continue;
							};

							let event = $builder(response);
							let data = impl_data!(event);
							channel_send!(data);
						}
					};
				}

				use channel::{
					Event as WrapEvent,
					session::Event as SessionEvent,
					stream::Event as StreamEvent,
					link::{Event as LinkEvent, Health}
				};
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
									_ => continue
								};

								builder
									.allow_reconnect(options.allow_reconnect())
									.way(way);
							}

							let try_options = builder.build();

							match try_options {
								Ok(options) => options,
								// 不行拉倒
								Err(_) => continue
							}
						};

						let data = impl_data!(WrapEvent::Session(SessionEvent::Open(options)));
						channel_send!(data);
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
						channel_send!(data);
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
						channel_send!(data);
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
								Err(_) => continue
							}
						};

						let length = quickly_ubig(payload.length());

						let data = impl_data!(WrapEvent::Stream(StreamEvent::Open { options, length }));
						channel_send!(data);
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
							None => continue
						};

						let chunk = channel::stream::Chunk {
							data: bytes,
							order
						};

						let data = impl_data!(WrapEvent::Stream(StreamEvent::Chunk(chunk)));
						channel_send!(data);
					},
					// 响应接收分块
					Head::StreamChunkAck	=> if let Some(payload) = packet.payload_as_stream_chunk_ack() {
						let order = match quickly_ubig(Some(payload.order())) {
							Some(order) => order,
							None => continue
						};

						let chunk_ack = channel::stream::ChunkAck {
							order
						};

						let data = impl_data!(WrapEvent::Stream(StreamEvent::ChunkAck(chunk_ack)));
						channel_send!(data);
					},
					Head::StreamFlush		=> if let Some(payload) = packet.payload_as_stream_flush() {
						let length = match quickly_ubig(Some(payload.length())) {
							Some(order) => order,
							None => continue
						};

						let flush = channel::stream::Flush {
							length
						};

						let data = impl_data!(WrapEvent::Stream(StreamEvent::Flush(flush)));
						channel_send!(data);
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
								continue;
							}

							let lack = channel::stream::Lack {
								orders
							};

							let data = impl_data!(WrapEvent::Stream(StreamEvent::Lack(lack)));
							channel_send!(data);
						}
					},
					Head::StreamLater		=> quickly_none!(WrapEvent::Stream(StreamEvent::Later)),
					Head::StreamGo			=> quickly_none!(WrapEvent::Stream(StreamEvent::Go)),
					Head::StreamClear		=> if let Some(payload) = packet.payload_as_stream_clear() {
						let data = impl_data!(WrapEvent::Stream(StreamEvent::Clear(Reason { code: payload.reason().code() })));
						channel_send!(data);
					},
					Head::HealthPing		=> quickly_none!(WrapEvent::Link(LinkEvent::Health(Health::Ping))),
					Head::HealthPong		=> quickly_none!(WrapEvent::Link(LinkEvent::Health(Health::Pong))),
					// 不兼容？？？
					_ => {}
				}
			}
		}
	}
	
	loop {
		select! {
			try_bi_stream = io.accept_bi_stream() => {
				if let Ok(stream) = try_bi_stream {
					// 干湿分离
					let writter = stream.clone() as Arc<dyn WritterStream>;
					writter_streams.insert(writter_stream_id.get_and_increase(), writter);

					let reader = stream.clone() as Arc<dyn ReaderStream>;
					context.runtime.spawn_local(wrap_reader(reader, context.clone(), (&channel).clone()));
				}
			},
			try_uni_stream = io.accept_uni_stream() => {
				if let Ok(reader) = try_uni_stream {
					context.runtime.spawn_local(wrap_reader(reader, context.clone(), (&channel).clone()));
				}
			},
			try_buffer = io_receiver.recv() => {
				if let Some(buffer) = try_buffer {
					let ubytes = buffer.to_vec();
					let mut removed = vec![];

					// 先尝试从已有发送流中发送
					for (id, writter) in writter_streams.clone().into_iter() {
						match writter.write(&ubytes).await {
							Ok(()) => {
								// 那就是发送了呗
								continue;
							},
							Err(crate::io::IOError::ClosedStream) => {
								// 关闭就要删除
								removed.push(id);
							},
							Err(_) => {},
						}
					}

					// 顺手的事
					for id in &removed {
						writter_stream_id.release(id.clone());
						writter_streams.remove(id);
					}
					drop(removed);

					// 到这里说明需要自己创建流
					let stream = match io.open_uni_stream().await {
						Ok(writter) => writter,
						Err(_) => {
							// 失败就放弃发送
							continue;
						},
					};
					
					match stream.write(&ubytes).await {
						Ok(_) => {},
						Err(_) => {}
					}
				}
			}
		}
	}
}

async fn link_handler(channel: InnerChannel, io_sender: mpsc::UnboundedSender<Bytes>, context: InnerContext) {
	let mut receiver = channel.get_receiver();
	loop {
		let data = match receiver.recv().await {
			Ok(value) => value,
			Err(error) => {
				continue;
			}
		};

		let event = match data.event {
			channel::Event::Link(event) => event,
			// 其他不关我们的事
			_ => {
				continue;
			}
		};


		use channel::link::Event::*;
		match event {
			SessionAck(event) => todo!(),

			StreamAck(event) => todo!(),

			Health(health) => todo!(),
		}
	}
}