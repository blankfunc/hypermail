use std::sync::Arc;
use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq, Hash, uniffi::Error)]
pub enum IOError {
	#[error("Timeout while waiting to open stream.")]
	OpenTimeout,
	#[error("Timeout while waiting to accept stream.")]
	AcceptTimeout,

	// Stream
	#[error("This stream has been closed.")]
	ClosedStream,
	#[error("Stream read failed.")]
	ReadError,
	#[error("Stream write failed.")]
	WriteError,

	// Common
	#[error("Connection dropped.")]
	Disconnected,

	#[error("({code}) Errors not within the preset: {error}")]
	Unknown { code: u32, error: String },
}

#[uniffi::export]
#[async_trait]
pub trait LinkIO: Send + Sync {
	// 打开一个单向流（只写流）
	async fn open_bi_stream(&self) -> Result<Arc<dyn WritterStream>, IOError>;

	// 打开一个双向流
	async fn open_uni_stream(&self) -> Result<Arc<dyn BidirectionalStream>, IOError>;

	// 接收一个单向流（只读流）
	async fn accept_bi_stream(&self) -> Result<Arc<dyn ReaderStream>, IOError>;

	// 接收一个双向流
	async fn accept_uni_stream(&self) -> Result<Arc<dyn BidirectionalStream>, IOError>;
}

#[uniffi::export]
#[async_trait]
pub trait IOStream: Send + Sync {
	async fn close(&self) -> Result<(), IOError>;
}

#[uniffi::export]
#[async_trait]
pub trait ReaderStream: IOStream {
	/// Read partial data from a continuous data stream.
	async fn read(&self) -> Result<Vec<u8>, IOError>;
}

#[uniffi::export]
#[async_trait]
pub trait WritterStream: IOStream {
	/// Write a portion of the continuous data stream into the connection.
	async fn write(&self, buffer: Vec<u8>) -> Result<(), IOError>;
}

#[uniffi::export]
pub trait BidirectionalStream: ReaderStream + WritterStream {}