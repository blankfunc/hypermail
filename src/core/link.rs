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

use super::packet::{channel, handle_flatbuffer};

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
			
			for packet in packets {
				let try_data = handle_flatbuffer(packet, reader.link_id());
				let data = match try_data {
					Some(data) => data,
					// 不行拉倒
					None => continue
				};

				match channel.get_sender().send(data) {
					Ok(_) => {},
					Err(_) => {}
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