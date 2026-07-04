# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrold'
release :=

# Detect cargo or fallback
CARGO := $(shell command -v cargo || command -v cargo-1.88 || command -v cargo-1.85 || command -v cargo-1.91)

# Default build uses the compio runtime (needs Rust 1.88). Older toolchains (>= 1.85) auto fall
# back to the tokio runtime via --no-default-features, so each OBS/distro target builds the best
# runtime its cargo supports.
CARGO_VER := $(if $(CARGO),$(shell $(CARGO) --version | awk '{print $$2}'))
# Empty (compio) when cargo >= 1.88, i.e. when [1.88, cargo version] is already in ascending
# order. Any 1.88+ release (1.88.1, 1.96.0, ...) selects compio; 1.85.x/1.87.x select tokio.
FEATURES := $(shell printf '1.88\n%s\n' "$(CARGO_VER)" | sort -V -C || echo --no-default-features)

.PHONY: build test clippy clean install uninstall

build: release += --release
build:
	@-cp -rf ../coolercontrol-ui/dist/* resources/app/
	@$(CARGO) build --locked $(release) $(FEATURES)

test: release += --release
test:
	@$(CARGO) test --locked $(release) $(FEATURES)

clippy: release += --release
clippy:
	@$(CARGO) clippy --locked $(release) $(FEATURES) -- -D clippy::pedantic

# test execution is split from compilation so the timeout only bounds the run phase:
# a hung test binary (wedged FIFO pairing class) otherwise burns the whole CI job budget.
# JSON goes through a file so the junit report survives a kill and the exit status is kept.
ci-test:
	@$(CARGO) build --locked $(release)
	@./target/debug/$(executable) --version
	@RUSTC_BOOTSTRAP=1 $(CARGO) test --locked --no-fail-fast --features gated-tests $(release) --no-run
	@RUSTC_BOOTSTRAP=1 timeout -k 30s 15m $(CARGO) test --locked --no-fail-fast --features gated-tests $(release) -- -Z unstable-options --format json > target/ci-test.json; s=$$?; gitlab-report -p test < target/ci-test.json > report.xml || true; exit $$s
	@$(CARGO) clippy --locked $(release) --message-format=json | gitlab-report -p clippy > gl-code-quality-report.json
	@#$(CARGO) audit
	@#$(CARGO) audit --json | jq '.settings.target_arch = null | .settings.target_os = null' | gitlab-report -p audit > gl-sast-report.json
    
dev-run:
	@$(CARGO) build
	@sudo ./target/debug/coolercontrold

# requires minor dependency patch to pass rh checks
# See: https://bugzilla.redhat.com/show_bug.cgi?id=1541318#c13
vendor:
	@mkdir -p .cargo
	@$(CARGO) vendor --locked > .cargo/config.toml
	@-chmod -x vendor/brotli-decompressor/src/*.rs

clean: clean-app
	@-$(RM) -rf target
	@-$(RM) -rf vendor
	@-$(RM) -f .cargo/config.toml

clean-app:
	@-$(RM) -rf resources/app/*

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)
