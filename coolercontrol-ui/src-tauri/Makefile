# CoolerControl Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrol'
release :=
features := '-F custom-protocol'

.PHONY: build test clean install uninstall

build: release += --release
build:
	@cargo build $(release) $(features)

test: release += --release
test:
	@cargo test $(release) $(features)

ci-test:
	@cargo test $(release) $(features)

clean:
	@-$(RM) -rf ./target

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)

build-appimage:
	@cargo install tauri-cli
	@NO_STRIP=true cargo tauri build
