# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build

build:
	@cargo build --release
	@rm -f ./coolercontrold
	@mv target/release/coolercontrold ./
	@./coolercontrold --version

install:
	# todo
