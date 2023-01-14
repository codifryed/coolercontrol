# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build

build:
	@cargo build --release

test: build
	@rm -f ./coolercontrold
	@cargo test --release
	@mv target/release/coolercontrold ./
	@./coolercontrold --version

build-fast:
	@cargo build

test-fast: build-fast
	@rm -f ./coolercontrold
	@cargo test
	@mv target/debug/coolercontrold ./
	@./coolercontrold --version

install:
	# todo
