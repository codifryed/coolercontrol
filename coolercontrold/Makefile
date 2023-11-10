# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build

build:
	@cargo build --release

with-ui:
	@$(MAKE) -C ../coolercontrol-ui build
	@rm -rf resources/app/* || true
	@cp -r ../coolercontrol-ui/dist/* resources/app/
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
	@RUSTC_BOOTSTRAP=1 cargo test -- -Z unstable-options --format json --report-time | tee results.json
	@cat results.json | cargo2junit > results.xml
	@mv target/debug/coolercontrold ./
	@./coolercontrold --version

install:
	# todo