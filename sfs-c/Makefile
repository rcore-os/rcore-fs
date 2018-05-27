ucore:
	rustup override set nightly-2018-04-01
	RUST_TARGET_PATH=$(shell pwd) xargo build --target ucore --release

install-rust:
	which cargo || ( curl https://sh.rustup.rs -sSf | sh )
	which xargo || ( rustup component add rust-src && cargo install xargo )

.PHONY: ucore install-rust