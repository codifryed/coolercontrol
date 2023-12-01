# CoolerControl Makefile
.DEFAULT_GOAL := build

.PHONY: build test clean install uninstall

build:
	@npm ci --prefer-offline
	@npm run build

test:
	@npm run test

clean:
	@-rm -rf dist

install:
	@$(MAKE) -C src-tauri $@

uninstall:
	@$(MAKE) -C src-tauri $@
