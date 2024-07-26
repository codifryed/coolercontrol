# CoolerControl Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrol'
release :=
features := '-F custom-protocol'

.PHONY: build test clean install uninstall

build: release += --release
build:
	@cargo build --locked $(release) $(features)

test: release += --release
test:
	@cargo test --locked $(release) $(features)

ci-test:
	@cargo test --locked $(release) $(features)

vendor:
	@cargo vendor --locked

clean:
	@-$(RM) -rf target
	@-$(RM) -rf vendor

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)

# @NO_STRIP=true cargo tauri build --ci --verbose
build-appimage:
	cargo install tauri-cli --version "^2.0.0-beta.22"
	mkdir -p /tmp/appimage
	cp -r ../* /tmp/appimage
	cd /tmp/appimage/src-tauri; NO_STRIP=true cargo tauri build --ci --verbose
	cp -r /tmp/appimage/src-tauri/target /builds/coolercontrol/coolercontrol/coolercontrol-ui/src-tauri/
