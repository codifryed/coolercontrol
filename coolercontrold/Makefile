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
	@cargo audit --json | jq '.settings.target_arch = null | .settings.target_os = null' | gitlab-report -p audit > gl-sast-report.json
    
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
