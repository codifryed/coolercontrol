# CoolerControl Makefile
.DEFAULT_GOAL := build

.PHONY: build dist dev dev-run offline test check ci-test clean install uninstall

dist:
	@npm ci --prefer-offline
	@npm run build

build: dist

dev:
	@npm run dev

dev-run:
	@npm ci --prefer-offline
	@npm exec vite build -- --outDir ../coolercontrold/resources/app --emptyOutDir
	@git restore ../coolercontrold/resources/app/.gitignore

offline:
	@npm run build --offline

test: build
	@npm run test:unit

# one-shot type-check + unit tests, no build, used by the root pr-check gate
check:
	@test -d node_modules || npm ci --prefer-offline
	@npm run type-check
	@npm run test:unit -- run

ci-test: build
	@npm run test:unit

clean:
	@-$(RM) -rf dist

install:
	#@$(MAKE) -C src-tauri $@

uninstall:
	#@$(MAKE) -C src-tauri $@
