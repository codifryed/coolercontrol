# CoolerControl Makefile
.DEFAULT_GOAL := build

build:
	@$(MAKE) -C ../ build
	@cargo build --release --all-features

test: build
	@cargo test --release --all-features

build-fast:
	@cargo build --all-features

test-fast: build-fast
	@cargo test --all-features

install:
	# todo
