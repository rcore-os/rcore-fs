ucore:
	rustup override set nightly-2019-01-28
	cargo xbuild --target ucore.json --release

install-rust:
	which cargo || ( curl https://sh.rustup.rs -sSf | sh )
	which cargo-xbuild || ( rustup component add rust-src && cargo install cargo-xbuild )

.PHONY: ucore install-rust