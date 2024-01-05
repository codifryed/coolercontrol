# CoolerControl Makefile
.DEFAULT_GOAL := build

.PHONY: build test clean install uninstall

dist:
	@npm ci --prefer-offline
	@npm run build

build: dist

offline:
	@npm run build --offline

test: build
	@npm run test:unit

ci-test: build
	@npm run test:unit

clean:
	@-$(RM) -rf dist

install:
	@$(MAKE) -C src-tauri $@

uninstall:
	@$(MAKE) -C src-tauri $@
