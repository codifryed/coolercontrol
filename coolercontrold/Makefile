# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrold'
release :=

.PHONY: build test clean install uninstall

build: release += --release
build:
	@-cp -rf ../coolercontrol-ui/dist/* resources/app/
	@cargo build --locked $(release)

test: release += --release
test:
	@cargo test --locked $(release)

ci-test:
	@cargo install cargo2junit
	@cargo build --locked $(release)
	@RUSTC_BOOTSTRAP=1 cargo test --locked $(release) -- -Z unstable-options --format json --report-time | tee results.json
	@cat results.json | cargo2junit > results.xml
	@./target/debug/$(executable) --version
    
vendor:
	@cargo vendor --locked

clean:
	@-$(RM) -rf target
	@-$(RM) -rf vendor
	@-$(RM) -rf resources/app/*

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)
