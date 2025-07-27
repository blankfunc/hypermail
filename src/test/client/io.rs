use async_trait::async_trait;
use std::{
	sync::Arc,
	net::TcpListener
};
use crate::io::{
	IOError,
	LinkIO,
	WritterStream,
	ReaderStream,
	BidirectionalStream
};

pub struct ClientIO {

}

#[async_trait]
impl LinkIO for ClientIO {
	async fn open_uni_stream(&self) -> Result<Arc<dyn WritterStream>, IOError> {
		todo!()
	}

	async fn open_bi_stream(&self) -> Result<Arc<dyn BidirectionalStream>, IOError> {
		todo!()
	}

	async fn accept_uni_stream(&self) -> Result<Arc<dyn ReaderStream>, IOError> {
		todo!()
	}

	async fn accept_bi_stream(&self) -> Result<Arc<dyn BidirectionalStream>, IOError> {
		todo!()
	}
}