# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrold'
release :=

.PHONY: build test clean install uninstall

build: release += --release
build:
	@-cp -rf ../coolercontrol-ui/dist/* resources/app/
	@cargo build $(release)

test: release += --release
test:
	@cargo test $(release)

ci-test:
	@cargo install cargo2junit
	@cargo build $(release)
	@RUSTC_BOOTSTRAP=1 cargo test $(release) -- -Z unstable-options --format json --report-time | tee results.json
	@cat results.json | cargo2junit > results.xml
	@./target/debug/$(executable) --version

clean:
	@-$(RM) -rf target
	@-$(RM) -rf resources/app/*

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)
