@0xd6dae762e32dbdee;

using Value = import "value.capnp";
using Session = import "session.capnp";
using Stream = import "stream.capnp";
using Frame = import "frame.capnp";

# 协议头
enum Head {
	# Session
	sessionOpen		@0;		# 请求创建会话
	sessionReopen	@1;		# 请求重连会话
	sessionAccept	@2;		# 响应(新)会话
	sessionReject	@3;		# 响应不允许(创建)会话
	sessionClose	@4;		# 请求关闭会话
	sessionCloseAck	@5;		# 响应即将关闭会话
	sessionDeath	@6;		# 强制关闭会话

	# Stream
	streamOpen		@7;		# 请求创建流
	streamReopen	@8;		# 请求重新使用流
	streamAccept	@9;		# 响应允许(创建)流
	streamReject	@10;	# 响应不允许(创建)流
	streamClose		@11;	# 请求关闭流
	streamCloseAck	@12;	# 响应即将关闭流
	streamDeath		@13;	# 强制关闭流

	# Frame
	framePacket		@14;	# 发送整包数据
	framePacketAck	@15;	# 响应收到整包数据
	frameReq		@16;	# 请求发送分块数据
	frameReqAccept	@17;	# 响应允许发送分块数据
	frameReqReject	@18;	# 响应拒绝接收分块数据
	frameBlock		@19;	# 发送分块数据
	frameBlockAck	@20;	# 响应分块数据
	frameFlush		@21;	# 分块数据发送完毕
	frameFlushAck	@22;	# 响应数据接收完毕
	frameClear		@23;	# 强制关闭（出现错误）
	frameBlockLater	@24;	# 响应需要延迟发送（应对并行发送可能的前后包顺序不一）
	frameBlockGo	@25;	# 响应继续发送
	frameBlockLack	@26;	# 响应缺失块
	frameReqRetry	@27;	# 断连后请求重新发送分块数据
	frameReqRetryAck@28;	# 响应是否允许重新发送

	# Health
	healthPing		@29;	# 发送心跳包
	healthPong		@30;	# 心跳包回应
}

struct PacketId {
	orderId		@0 :Value.Id;
	sessionId	@1 :Value.Id;
	streamId	@2 :Value.Id;
	frameId		@3 :Value.Id;
}

struct PacketPayload {
	union {
		sessionOpen		@0		:Void;
		sessionReopen	@1		:Void;
		sessionAccept	@2		:Void;
		sessionReject	@3		:Value.EventReason;
		sessionClose	@4		:Void;
		sessionCloseAck	@5		:Void;
		sessionDeath	@6		:Value.EventReason;

		streamOpen		@7		:Stream.Open;
		streamReopen	@8		:Void;
		streamAccept	@9		:Void;
		streamReject	@10		:Value.EventReason;
		streamClose		@11		:Void;
		streamCloseAck	@12		:Void;
		streamDeath		@13		:Value.EventReason;

		framePacket		@14		:Frame.Packet;
		framePacketAck	@15		:Void;
		frameReq		@16		:Frame.Req;
		frameReqAccept	@17		:Void;
		frameReqReject	@18		:Value.EventReason;
		frameBlock		@19		:Frame.Block;
		frameBlockAck	@20		:Frame.BlockAck;
		frameFlush		@21		:Void;
		frameFlushAck	@22		:Void;
		frameClear		@23		:Value.EventReason;
		frameBlockLater	@24		:Void;
		frameBlockGo	@25		:Void;
		frameBlockLack	@26		:Frame.BlockLack;
		frameReqRetry	@27		:Void;
		frameReqRetryAck@28		:Frame.ReqRetryAck;

		healthPing		@29		:Void;
		healthPong		@30		:Void;
	}
}

struct Packet {
	head		@0 :Head;
	id			@1 :PacketId;
	payload		@2 :PacketPayload;
}