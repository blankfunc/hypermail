@0xe3ce868ab719923e;

using Value = import "value.capnp";

# 整包数据
struct Packet {
	data	@0 :Data;
}

# 请求发送分块配置
struct ReqOptions {
	# 强制顺序性（要求返回 Ack）
	strongOrderliness		@0	:Bool = false;
	# 强制完整性（不允许任何一个包没收到）
	strongIntegrity			@1	:Bool = true;
	# 一次性（即重连后可否续传）
	disposable				@2	:Bool = true;
}

# 请求发送分块
struct Req {
	# 配置
	options		@0 :ReqOptions;
	# 可能的大小（允许数组为空）
	length		@1 :Value.BigUInt;
}

# 分块数据
struct Block {
	order	@0 :Value.BigUInt;
	data	@1 :Data;
}

# 响应分块数据
struct BlockAck {
	order	@0 :Value.BigUInt;
}

# 响应重发缺失块
struct BlockLack {
	orders	@0 :List(Value.BigUInt);
}

# 响应允许重发
struct ReqRetryAck {
	accept		@0	:Bool;
	union {
		none	@1	:Void;
		reason	@2	:Value.EventReason;
	}
}