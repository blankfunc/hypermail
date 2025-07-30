use super::packet::Reason;

/// Response to rejection or acceptance.
#[derive(Clone)]
pub enum Acceptable {
	Accept,
	Reject(Reason)
}

#[async_trait::async_trait]
pub trait Strategy {
	/// The other party requests to open a new session.
	async fn ack_session_open(&self) -> Acceptable;

	/// The other party requests to open a new stream.
	async fn ack_stream_open(&self) -> Acceptable;
}