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

test:
	@$(MAKE) -C ../ dist
	@cargo test $(release) $(features)

clean:
	@rm -rf ./target

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-rm -f $(DESTDIR)$(prefix)/bin/$(executable)
