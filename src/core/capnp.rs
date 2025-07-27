use ibig::UBig;
use derive_builder::Builder;

#[derive(Debug, Clone)]
pub enum PacketPayload {
    // Session
    SessionOpen,
    SessionReopen,
    SessionAccept,
    SessionReject(ValueEventReason),
    SessionClose,
    SessionCloseAck,
    SessionDeath(ValueEventReason),

    // Stream
    StreamOpen(StreamOpen),
    StreamReopen,
    StreamAccept,
    StreamReject(ValueEventReason),
    StreamClose,
    StreamCloseAck,
    StreamDeath(ValueEventReason),

    // Frame
    FramePacket(FramePacket),
    FramePacketAck,
    FrameReq(FrameReq),
    FrameReqAccept,
    FrameReqReject(ValueEventReason),
    FrameBlock(FrameBlock),
    FrameBlockAck(FrameBlockAck),
    FrameFlush,
    FrameFlushAck,
    FrameClear(ValueEventReason),
    FrameBlockLater,
    FrameBlockGo,
    FrameBlockLack(FrameBlockLack),
    FrameReqRetry,
    FrameReqRetryAck(FrameReqRetryAck),

    // Health
    HealthPing,
    HealthPong,
}

#[derive(Debug, Clone, Builder)]
pub struct Packet {
    pub data: Vec<u8>, // capnp Data -> Vec<u8>
}

#[derive(Debug, Clone, Builder)]
pub struct ReqOptions {
    /// 强制顺序性（要求返回 Ack）
    pub strong_orderliness: bool, // default = false
    /// 强制完整性（不允许任何一个包没收到）
    pub strong_integrity: bool, // default = true
    /// 一次性（即重连后可否续传）
    pub disposable: bool, // default = true
}

#[derive(Debug, Clone, Builder)]
pub struct FrameReq {
    pub options: ReqOptions,
    pub length: UBig,
}

#[derive(Debug, Clone, Builder)]
pub struct FramePacket {
	pub data: Vec<u8>,
}

#[derive(Debug, Clone, Builder)]
pub struct FrameBlock {
    pub order: UBig,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Builder)]
pub struct FrameBlockAck {
    pub order: UBig,
}

#[derive(Debug, Clone, Builder)]
pub struct FrameBlockLack {
    pub orders: Vec<UBig>,
}

#[derive(Debug, Clone)]
pub enum FrameReqRetryAck {
	Accept,
	Reject(ValueEventReason)
}

#[derive(Debug, Clone)]
pub enum StreamMode {
    OnlyRead,
    OnlyWrite,
    TwoWay,
}

#[derive(Debug, Clone, Builder)]
pub struct StreamOpenOptions {
    pub mode: StreamMode, // default = TwoWay
}

#[derive(Debug, Clone, Builder)]
pub struct StreamOpen {
    pub options: StreamOpenOptions,
}

#[derive(Debug, Clone, Builder)]
pub struct ValueEventReason {
    pub reason_code: u64,
}