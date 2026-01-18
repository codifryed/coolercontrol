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
	@cargo build --locked $(release)
	@./target/debug/$(executable) --version
	@RUSTC_BOOTSTRAP=1 cargo test --locked --no-fail-fast $(release) -- -Z unstable-options --format json | gitlab-report -p test > report.xml
	@cargo clippy --locked $(release) --message-format=json | gitlab-report -p clippy > gl-code-quality-report.json
	@#cargo audit
	@#cargo audit --json | jq '.settings.target_arch = null | .settings.target_os = null' | gitlab-report -p audit > gl-sast-report.json
    
dev-run:
	@cargo build
	@sudo ./target/debug/coolercontrold

# requires minor dependency patch to pass rh checks
# See: https://bugzilla.redhat.com/show_bug.cgi?id=1541318#c13
vendor:
	@mkdir .cargo
	@cargo vendor --locked > .cargo/config.toml
	@-chmod -x vendor/brotli-decompressor/src/*.rs

clean: clean-app
	@-$(RM) -rf target
	@-$(RM) -rf vendor
	@-$(RM) -rf .cargo

clean-app:
	@-$(RM) -rf resources/app/*

install:
	@mkdir -p $(DESTDIR)$(prefix)/bin
	@install -m755 ./target/release/$(executable) $(DESTDIR)$(prefix)/bin/

uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)
