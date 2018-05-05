ucore:
	@RUST_TARGET_PATH=$(shell pwd) xargo build --target ucore --features ucore