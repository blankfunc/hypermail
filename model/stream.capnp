@0x9074736970a6724d;

using Value = import "value.capnp";

# 流的方向性
enum StreamMode {
	onlyRead	@0;	# 只读
	onlyWrite	@1;	# 只写
	twoWay		@2; # 双向流
}

# 创建流的配置
struct OpenOptions {
	# 方向性
	mode		@0: StreamMode = twoWay;
}

# 请求创建流
struct Open {
	options		@0: OpenOptions;
}