# CoolerControl Makefile
.DEFAULT_GOAL := build

build:
	@$(MAKE) -C ../ build
	@cargo build --release -F custom-protocol

test: build
	@cargo test --release -F custom-protocol

build-fast:
	@cargo build -F custom-protocol

test-fast: build-fast
	@cargo test -F custom-protocol

install:
	# todo
