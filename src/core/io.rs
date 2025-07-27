use std::sync::Arc;
use crate::io::{IOError, LinkIO, ReaderStream, WritterStream};
use tokio_with_wasm::alias::{
	spawn,
	task::JoinHandle
};
use dashmap::DashMap;
use crossbeam::queue::SegQueue;
use futures::stream::FuturesUnordered;
use forever_safer::atomic_poll::AtomicPoll;
use ibig::UBig;
use once_cell::sync::Lazy;
use capnp::message::ReaderOptions;
use super::packet::channel_data;

const READER_OPTIONS: Lazy<ReaderOptions> = Lazy::new(|| ReaderOptions::default());

fn debugger_buffer(buffer: &[u8]) -> String {
	buffer
		.into_iter()
		.map(|b| format!("{:02X}", b))
		.collect::<Vec<_>>()
		.chunks(12)
		.map(|s| s.join(&" "))
		.collect::<Vec<_>>()
		.join(&"\n")
}

// 处理 IO
async fn io_handler(linkio: Arc<dyn LinkIO>, buffer_cache: Arc<SegQueue<Arc<Vec<u8>>>>, receiver: flume::Receiver<Arc<Vec<u8>>>) {
	let writer_stream: DashMap<UBig, Arc<dyn WritterStream>> = DashMap::new();
	let writer_stream_id = AtomicPoll::new();

	let reader_tasks = FuturesUnordered::new();

	// 处理 Reader
	fn wrap_reader_stream(cache: Arc<SegQueue<Arc<Vec<u8>>>>, tasks: &FuturesUnordered<JoinHandle<()>>, stream: Arc<dyn ReaderStream>) {
		let task = spawn(async move {
			let link_id = match stream.link_id() {
				Ok(id) => id,
				// 无法获取就退出，不用理由
				Err(_) => {
					return ();
				}
			};

			log::debug!("Get a new stream {}!", link_id);

			loop {
				// 尝试进行读取
				match stream.read().await {
					// 成功获取
					Ok(buffer) => cache.push(Arc::new(buffer)),
					// 流断连不可用
					Err(IOError::ClosedStream | IOError::Disconnected) => {
						log::debug!("The stream {} is disconnected!", link_id);
						break;
					},
					// 其他都可以忽略
					Err(error) => {
						log::debug!("The stream {}'s reading is failed! {}", link_id, error);
						continue;
					}
				};
			}
		});

		tasks.push(task);
	}
	
	let cache = buffer_cache.clone();
	loop {
		tokio::select! {
			// 获取到单向流 Reader
			try_uni_stream = linkio.accept_uni_stream() => {
				if let Ok(reader_stream) = try_uni_stream {
					wrap_reader_stream(cache.clone(), &reader_tasks, reader_stream);
				}
			}
			// 获取到双向流
			try_bi_stream = linkio.accept_bi_stream() => {
				if let Ok(bi_stream) = try_bi_stream {
					// 干湿分离
					let reader = bi_stream.clone() as Arc<dyn ReaderStream>;
					let writter = bi_stream.clone() as Arc<dyn WritterStream>;

					wrap_reader_stream(cache.clone(), &reader_tasks, reader);

					writer_stream.insert(writer_stream_id.get_and_increase(), writter);
				}
			}
			// 尝试读取数据
			try_buffer = receiver.recv_async() => {
				// 只管发送数据，不用管错误
				if let Ok(buffer) = try_buffer {
					log::debug!("Try to write bytes \n({})", debugger_buffer(buffer.as_slice()));

					let mut optional_stream: Option<Arc<dyn WritterStream>> = None;

					// 从现有流中查找
					for (key, stream) in writer_stream.clone().into_iter() {
						// 关闭那就删除下一个
						if stream.is_closed().await {
							writer_stream.remove(&key);
							continue;
						}

						optional_stream = Some(stream);
						break;
					}

					if optional_stream.is_none() {
						// 没有就创建一个单向流
						match linkio.open_uni_stream().await {
							Ok(stream) => optional_stream = Some(stream),
							Err(_) => {}
						};
					}

					// 错误那就没办法了
					if let Some(stream) = optional_stream
					&& let Ok(()) = stream.write((*buffer).clone()).await {
						log::debug!("Write successed.")
					} else {
						log::debug!("Write failed! No availed stream!")
					}
				}
			}
		}
	}
}

// 解析数据
async fn capnp_handler(buffer_cache: Arc<SegQueue<Arc<Vec<u8>>>>, channel: flume::Sender<channel_data::Data>) {
	loop {
		if let Some(buffer) = buffer_cache.pop() {
			log::debug!("Handle received bytes \n({})", debugger_buffer(buffer.as_slice()));

			// 只接受整片
			let try_message = capnp::serialize::read_message_from_flat_slice_no_alloc(&mut buffer.as_slice(), READER_OPTIONS.clone());

			// 解析 Capnp
			let message = match try_message {
				Ok(message) => {
					log::debug!("Handle for capnp codec successed.");
					message
				},
				Err(error) => {
					log::debug!("Handle for capnp codec failed! ({}).", error);
					continue;
				},
			};

			// 解析 Packet
			use crate::packet_capnp::packet;
			let reader = match message.get_root::<packet::Reader>() {
				Ok(reader) => {
					log::debug!("Handle for packet codec successed.");
					reader
				},
				Err(error) => {
					log::debug!("Handle for packet codec failed! ({}).", error);
					continue;
				}
			};


			use super::packet::handle_packet;
			match handle_packet(channel.clone(), reader) {
				Ok(()) => {
					log::debug!("Handle successed.");
				},
				Err(error) => {
					log::debug!("Handle failed! ({}).", error);
				}
			}
		}
	}
}

pub struct Link {

}

impl Link {
	pub fn new(linkio: Arc<dyn LinkIO>) -> Self {
		let buffer_cache = Arc::new(SegQueue::new());

		// IO 消息传递
		let (io_sender, io_receiver) = flume::unbounded::<Arc<Vec<u8>>>();

		// Packet 内容传递
		let (channel_sender, channel_receiver) = flume::unbounded::<channel_data::Data>();

		// 处理 IO 的线程
		spawn(io_handler(linkio, buffer_cache.clone(), io_receiver));
		spawn(capnp_handler(buffer_cache.clone(), channel_sender));

		Self {}
	}
}