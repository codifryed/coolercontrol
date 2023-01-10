# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build

build:
	@rm -f ./coolercontrold
	@cargo test
	@cargo build --release
	@mv target/release/coolercontrold ./
	@./coolercontrold --version

install:
	# todo
