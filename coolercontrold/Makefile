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
	@cargo install cargo2junit
	@cargo test -- -Z unstable-options --format json --report-time | tee results.json
	@cat results.json | cargo2junit > results.xml
	@mv target/debug/coolercontrold ./
	@./coolercontrold --version

install:
	# todo
