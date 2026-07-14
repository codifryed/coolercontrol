# CoolerControl Makefile
#
# Core build/test/dev targets. Run `make help` for the common ones.
# Maintainer plumbing is split into make/*.mk (included at the bottom).
.DEFAULT_GOAL := build
ap_id := 'org.coolercontrol.CoolerControl'
liqctld_dir := 'coolercontrol-liqctld'
daemon_dir := 'coolercontrold'
ui_dir := 'coolercontrol-ui'
qt_dir := 'coolercontrol'

# Detect cargo or fallback (prefer newer; mirrors coolercontrold/Makefile ordering)
CARGO := $(shell command -v cargo || command -v cargo-1.88 || command -v cargo-1.85 || command -v cargo-1.91)

.PHONY: help \
	build build-ui build-daemon build-qt dev-build \
	build-offline build-daemon-offline build-ui-offline \
	test test-ui test-daemon test-qt dev-test \
	ci-install ci-test ci-test-ui ci-test-daemon ci-test-qt \
	ci-check ci-fmt pr-check validate-metadata \
	clean clean-ui install install-source uninstall dev-run dev-install

# Run `make help` for the common developer targets.
help:
	@printf '\nCoolerControl - common make targets\n\n'
	@printf '  \033[1mBuild\033[0m\n'
	@printf '    make build            Build everything (UI + daemon + Qt), release\n'
	@printf '    make build-ui         Build just the Vue UI (embedded in the daemon)\n'
	@printf '    make build-daemon     Build the Rust daemon (builds UI first)\n'
	@printf '    make build-qt         Build the Qt desktop app\n'
	@printf '    make dev-build        Clean UI, then full release build\n\n'
	@printf '  \033[1mTest & check\033[0m\n'
	@printf '    make test             Run all tests (UI + daemon + Qt)\n'
	@printf '    make test-daemon      Run daemon (Rust) tests\n'
	@printf '    make test-ui          Run UI (Vitest) tests\n'
	@printf '    make pr-check         Pre-PR gate: lint diff, tests, clippy, Qt build\n\n'
	@printf '  \033[1mFormat & lint\033[0m\n'
	@printf '    make ci-fmt           Auto-format all files (trunk)\n'
	@printf '    make ci-check         Run formatting/lint checks (trunk)\n\n'
	@printf '  \033[1mRun & install\033[0m\n'
	@printf '    make dev-run          Incremental build + run daemon locally (sudo)\n'
	@printf '    make install          Install daemon + Qt binaries (DESTDIR aware)\n'
	@printf '    make dev-install      Install release binaries + restart the service\n'
	@printf '    make clean            Remove all build artifacts\n\n'
	@printf '  Maintainer targets (appimages, docker-*, bump, release, vendor, assets)\n'
	@printf '  live in make/*.mk.\n\n'

# Release goals
# can be run in parallel with make -j2
build: build-daemon build-qt

build-daemon: build-ui
	@$(MAKE) -C $(daemon_dir) build

build-qt:
	@$(MAKE) -C $(qt_dir) build

build-ui:
	@$(MAKE) -C $(ui_dir) build

build-offline: build-daemon-offline build-qt

build-daemon-offline: build-ui-offline
	@$(MAKE) -C $(daemon_dir) build

build-ui-offline:
	@$(MAKE) -C $(ui_dir) offline

# parallelize with make -j4
test: validate-metadata test-daemon test-ui test-qt

test-daemon:
	@$(MAKE) -C $(daemon_dir) test

test-ui:
	@$(MAKE) -C $(ui_dir) test

test-qt:
	@$(MAKE) -C $(qt_dir) test

# install any needed CI tools
# Trunk needs: libxcrypt-compat on local system
ci-install:
	@./trunk install --ci
	@$(CARGO) install gitlab-report --locked
 
ci-test: validate-metadata ci-test-ui ci-test-daemon ci-test-qt

ci-test-daemon:
	@$(MAKE) -C $(daemon_dir) ci-test

ci-test-ui:
	@$(MAKE) -C $(ui_dir) ci-test

ci-test-qt:
	@$(MAKE) -C $(qt_dir) ci-test

ci-check:
	@./trunk install --ci
	@./trunk check --ci

ci-fmt:
	@./trunk fmt --all

clean: clean-ui
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(qt_dir) $@
	@-$(RM) -rf assets-built

clean-ui:
	@$(MAKE) -C $(ui_dir) clean

install:
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(qt_dir) $@ 

install-source: build install
	@install -Dm644 packaging/metadata/$(ap_id).desktop -t $(DESTDIR)/usr/local/share/applications/
	@install -Dm644 packaging/metadata/$(ap_id).metainfo.xml -t $(DESTDIR)/usr/share/metainfo/
	@install -Dm644 packaging/metadata/$(ap_id).png -t $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-alert.png -t $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/
	@install -Dm644 packaging/metadata/$(ap_id).svg -t $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-alert.svg -t $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-symbolic.svg -t $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-symbolic-alert.svg -t $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/
	@install -Dm644 packaging/systemd/coolercontrold.service -t $(DESTDIR)/etc/systemd/system/

uninstall:
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(qt_dir) $@
	@-$(RM) -f $(DESTDIR)/usr/local/share/applications/$(ap_id).desktop
	@-$(RM) -f $(DESTDIR)/usr/share/metainfo/$(ap_id).metainfo.xml
	@-$(RM) -f $(DESTDIR)/usr/share/pixmaps/$(ap_id).png
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/etc/systemd/system/coolercontrold.service

# helpful std development & testing targets
# For testing these make targets require that a system package is already installed
dev-run: build-qt
	@sudo echo "Running incremental build"
	@$(MAKE) -C $(ui_dir) $@
	@$(MAKE) -C $(daemon_dir) $@

# full release build of daemon and UI binaries:
dev-build: clean-ui build

dev-test: clean ci-install validate-metadata ci-check ci-test-ui ci-test-daemon ci-test-qt

# pre-PR gate: lints the committed branch diff only, then UI and daemon tests, clippy pedantic, Qt build
# usage: make pr-check [base=<ref>]
base ?= main
pr-check: validate-metadata
	@./trunk install --ci
	@git merge-base $(base) HEAD > /dev/null
	@git diff -z --name-only --diff-filter=d $(base)...HEAD | xargs -0 -r ./trunk check --ci
	@$(MAKE) -C $(ui_dir) check
	@$(MAKE) -C $(daemon_dir) clippy
	@$(MAKE) -C $(daemon_dir) test
	@$(MAKE) -C $(qt_dir) build

# installs the release coolercontrold daemon and desktop app binaries: (need CC pre-installed)
dev-install:
	@sudo $(MAKE) install
	@sudo systemctl restart coolercontrold

validate-metadata:
	@appstream-util --version || true
	@desktop-file-validate packaging/metadata/org.coolercontrol.CoolerControl.desktop
	@desktop-file-validate packaging/appimage/coolercontrol.desktop
	@desktop-file-validate packaging/appimage/coolercontrold.desktop
	@appstream-util validate-relax packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml

# Maintainer / release-engineering targets live in these includes. Optional (-include) so a partial
# checkout that has only this Makefile (the dockerhub images run `make build-daemon`) still parses.
-include make/docker.mk
-include make/release.mk
-include make/appimage.mk
