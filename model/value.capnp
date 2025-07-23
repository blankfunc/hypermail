@0xb28d8280b8dce544;

# 高精度表达
struct BigUInt {
	value @0 :List(UInt8) = [];
}

# ID
struct Id {
	union {
		none	@0 :Void;
		value	@1 :BigUInt;
	}
}

# 各种事件发送的代码
struct EventReason {
	reasonCode	@0	:UInt64;
}