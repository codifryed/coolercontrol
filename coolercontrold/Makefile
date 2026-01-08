# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrold'
release :=

# Detect cargo or fallback to cargo-1.88
CARGO := $(shell command -v cargo || command -v cargo-1.88)

.PHONY: build test clean install uninstall

build: release += --release
build:
	@-cp -rf ../coolercontrol-ui/dist/* resources/app/
	@$(CARGO) build --locked $(release)

test: release += --release
test:
	@$(CARGO) test --locked $(release)

ci-test:
	@$(CARGO) build --locked $(release)
	@./target/debug/$(executable) --version
	@RUSTC_BOOTSTRAP=1 $(CARGO) test --locked --no-fail-fast $(release) -- -Z unstable-options --format json | gitlab-report -p test > report.xml
	@$(CARGO) clippy --locked $(release) --message-format=json | gitlab-report -p clippy > gl-code-quality-report.json
	@#$(CARGO) audit
	@#$(CARGO) audit --json | jq '.settings.target_arch = null | .settings.target_os = null' | gitlab-report -p audit > gl-sast-report.json
    
dev-run:
	@$(CARGO) build
	@sudo ./target/debug/coolercontrold

# requires minor dependency patch to pass rh checks
# See: https://bugzilla.redhat.com/show_bug.cgi?id=1541318#c13
vendor:
	@mkdir .cargo
	@$(CARGO) vendor --locked > .cargo/config.toml
	@-chmod -x vendor/brotli-decompressor/src/*.rs

clean:
	@-$(RM) -rf target
	@-$(RM) -rf vendor
	@-$(RM) -rf .cargo
	@-$(RM) -rf resources/app/*

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)
