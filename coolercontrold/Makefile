# CoolerControl Daemon Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
executable := 'coolercontrold'
release :=

# Detect cargo or fallback (prefer newer)
CARGO := $(shell command -v cargo || command -v cargo-1.91 || command -v cargo-1.88)

# Extra cargo feature flags for build/test/clippy, e.g. FEATURES=--features gated-tests
FEATURES :=

.PHONY: build sync-app test test-liqctld clippy clippy-tests ci-test dev-run openapi vendor clean clean-app install uninstall

# Mirror the UI build into the embedded asset dir: include_dir! compiles the whole
# directory in, so stale hashed assets must go first (the glob keeps .gitignore).
# Skipped without dist/: vendored and source-tarball builds ship a prebuilt app dir.
sync-app:
	@if [ -d ../coolercontrol-ui/dist ]; then \
		$(RM) -rf resources/app/*; \
		cp -rf ../coolercontrol-ui/dist/* resources/app/; \
	fi

build: release += --release
build: sync-app
	@$(CARGO) build --locked $(release) $(FEATURES)

test: release += --release
test: test-liqctld
	@$(CARGO) test --locked $(release) $(FEATURES)

# liqctld's tests call liquidctl's own implementations, so they need it installed. This is a
# Rust daemon and liquidctl is an optional runtime dep, so building it must never require a
# python package: a missing one skips. Only the pipeline job that installs liquidctl sets
# LIQCTLD_TESTS_REQUIRED, which turns that skip into the failure CI needs it to be.
test-liqctld:
	@if python3 -c 'import liquidctl, PIL' 2>/dev/null; then \
		python3 -m unittest discover -s daemon/resources/liqctld -t daemon/resources/liqctld -p 'test_*.py'; \
	elif [ -n "$(LIQCTLD_TESTS_REQUIRED)" ]; then \
		echo "liqctld tests cannot run: liquidctl or Pillow is missing"; \
		exit 1; \
	else \
		echo "skipping liqctld tests: liquidctl or Pillow is not installed"; \
	fi

# The spec comes from the route table, so no daemon, no port and no root are involved.
# cargo writes its own noise to stderr, leaving stdout as clean JSON.
# `--offline` not `--locked`: version_bump.sh calls this right after bumping the crate version,
# and --locked refuses to refresh the lockfile, which would abort the bump partway through.
openapi:
	@$(CARGO) run --offline --quiet $(FEATURES) -- openapi > ../openapi/openapi.json
	@echo "Wrote openapi/openapi.json"


clippy: release += --release
clippy:
	@$(CARGO) clippy --locked $(release) $(FEATURES) -- -D clippy::pedantic

# Lints test code, which `clippy` cannot: without --all-targets the held-across-await lints never
# fire on the test modules, which is where a borrow held across an await is easiest to write by
# accident, and where it costs a BorrowMutError panic on the single-threaded runtime.
#
# Named lints, and RUSTFLAGS cleared, so the gate means the same thing everywhere. CI sets a global
# `-Dwarnings`, and the toolchain is not pinned: under an older clippy that makes `unknown_lints`
# fatal, so a forward-looking `#[allow(clippy::..)]` the shipped toolchain does not know fails a job
# nobody can reproduce locally. The MSRV job clears RUSTFLAGS for the same reason. Both lints below
# predate the 1.95 MSRV, so they exist wherever this builds. Debug, to share artifacts with `test`.
clippy-tests:
	@RUSTFLAGS= $(CARGO) clippy --locked --all-targets $(FEATURES) -- \
		-D clippy::await_holding_refcell_ref -D clippy::await_holding_lock

# test execution is split from compilation so the timeout only bounds the run phase:
# a hung test binary (wedged FIFO pairing class) otherwise burns the whole CI job budget.
# JSON goes through a file so the junit report survives a kill and the exit status is kept.
ci-test: test-liqctld
	@$(CARGO) build --locked $(release)
	@./target/debug/$(executable) --version
	@RUSTC_BOOTSTRAP=1 $(CARGO) test --locked --no-fail-fast --features gated-tests $(release) --no-run
	@RUSTC_BOOTSTRAP=1 timeout -k 30s 15m $(CARGO) test --locked --no-fail-fast --features gated-tests $(release) -- -Z unstable-options --format json > target/ci-test.json; s=$$?; gitlab-report -p test < target/ci-test.json > report.xml || true; exit $$s
	@RUSTFLAGS= $(CARGO) clippy --locked --all-targets $(FEATURES) -- \
		-D clippy::await_holding_refcell_ref -D clippy::await_holding_lock
	@$(CARGO) clippy --locked $(release) --message-format=json | gitlab-report -p clippy > gl-code-quality-report.json
    
dev-run: sync-app
	@$(CARGO) build
	@sudo ./target/debug/coolercontrold

# Some crates ship .rs files with the exec bit set, which fails Fedora's brp-mangle-shebangs
# on the leading `#![allow(...)]`. See: https://bugzilla.redhat.com/show_bug.cgi?id=1541318#c13
vendor:
	@mkdir -p .cargo
	@$(CARGO) vendor --locked > .cargo/config.toml
	@find vendor -type f -name '*.rs' -exec chmod a-x {} +

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
