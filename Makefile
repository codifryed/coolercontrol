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
CARGO := $(shell command -v cargo || command -v cargo-1.91 || command -v cargo-1.88)

.PHONY: help \
	build build-ui build-daemon build-qt \
	build-offline build-daemon-offline build-ui-offline \
	test test-ui test-daemon test-qt \
	ci-install ci-test ci-test-ui ci-test-daemon ci-test-qt \
	ci-check ci-fmt ci-local pr-check validate-metadata \
	copyright-check copyright-fix \
	clean clean-ui install install-data install-source uninstall dev-run dev-install openapi

# Run `make help` for the common developer targets.
help:
	@printf '\nCoolerControl - common make targets\n\n'
	@printf '  \033[1mBuild\033[0m\n'
	@printf '    make build            Build everything (UI + daemon + Qt), release\n'
	@printf '    make build-ui         Build just the Vue UI (embedded in the daemon)\n'
	@printf '    make build-daemon     Build the Rust daemon (builds UI first)\n'
	@printf '    make build-qt         Build the Qt desktop app\n\n'
	@printf '  \033[1mTest & check\033[0m\n'
	@printf '    make test             Run all tests (UI + daemon + Qt)\n'
	@printf '    make test-daemon      Run daemon tests (Rust and liqctld)\n'
	@printf '    make test-ui          Run UI (Vitest) tests\n'
	@printf '    make pr-check         Pre-PR gate: lint diff, tests, clippy, Qt build\n'
	@printf '    make ci-local         Reproduce the CI pipeline locally (full clean, slow)\n\n'
	@printf '  \033[1mFormat & lint\033[0m\n'
	@printf '    make ci-fmt           Auto-format all files (trunk)\n'
	@printf '    make ci-check         Run formatting/lint checks (trunk)\n'
	@printf '    make openapi          Regenerate openapi/openapi.json (no daemon needed)\n'
	@printf '    make copyright-check  Verify SPDX headers (REUSE), as CI does\n'
	@printf '    make copyright-fix    Add missing SPDX headers, then re-check\n\n'
	@printf '  \033[1mRun & install\033[0m\n'
	@printf '    make dev-run          Incremental build + run daemon locally (sudo)\n'
	@printf '    make install          Install daemon + Qt binaries (DESTDIR aware)\n'
	@printf '    make install-source   Build + install everything from source (sudo)\n'
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

# Regenerate openapi/openapi.json. Needs no running daemon and no root: the spec is
# built from the daemon's route table. A test fails when the checked-in file is stale.
openapi:
	@$(MAKE) -C $(daemon_dir) openapi

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

# `reuse` is a pip package, not bundled with trunk. The charset-normalizer extra
# avoids a hard dependency on the libmagic C library.
require_reuse = command -v reuse >/dev/null || { \
	echo 'reuse not found. Install it with:'; \
	echo '    pipx install "reuse[charset-normalizer]"'; \
	echo '(replace pipx with pip if you do not use pipx)'; exit 1; }

# Verify every file declares copyright and license (REUSE spec). This is what
# the copyright CI job runs.
copyright-check:
	@$(require_reuse)
	@reuse lint

# Fix what copyright-check reports: adds SPDX headers to source files that lack
# them, dating each from its first commit. Safe to re-run. Non-source files are
# covered by REUSE.toml and are never touched.
copyright-fix:
	@$(require_reuse)
	@python3 scripts/copyright-fix.py
	@reuse lint

clean: clean-ui
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(qt_dir) $@
	@-$(RM) -rf assets-built

clean-ui:
	@$(MAKE) -C $(ui_dir) clean

install:
	@$(MAKE) -C $(daemon_dir) $@
	@$(MAKE) -C $(qt_dir) $@ 

# The data half of a source install. Split out of install-source so that target can
# run it under sudo on its own: prerequisites cannot be elevated selectively, so
# `sudo make install-source` would otherwise build as root and leave a root-owned
# target/, node_modules/ and build/ behind.
install-data:
	@install -Dm644 packaging/metadata/$(ap_id).desktop -t $(DESTDIR)/usr/share/applications/
	@install -Dm644 packaging/metadata/$(ap_id).metainfo.xml -t $(DESTDIR)/usr/share/metainfo/
	@install -Dm644 packaging/metadata/$(ap_id).png -t $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-alert.png -t $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/
	@install -Dm644 packaging/metadata/$(ap_id).svg -t $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-alert.svg -t $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-symbolic.svg -t $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/
	@install -Dm644 packaging/metadata/$(ap_id)-alert-symbolic.svg -t $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/
	@install -Dm644 packaging/systemd/coolercontrold.service -t $(DESTDIR)/etc/systemd/system/
# Distro packages refresh these from a post-install hook. Without it a panel keeps
# resolving the stale cache and a newly added icon name never appears. The stale
# entry is from source installs before the desktop file moved out of /usr/local;
# leaving it behind shows the app twice in the launcher.
ifeq ($(strip $(DESTDIR)),)
	@-$(RM) -f /usr/local/share/applications/$(ap_id).desktop
	@-gtk-update-icon-cache -f -t /usr/share/icons/hicolor
	@-update-desktop-database /usr/share/applications
# A restart alone will not re-read a changed unit. Best effort: OpenRC and
# container installs have no systemctl.
	@-systemctl daemon-reload
endif

# `sudo make install-source` is the documented way to install from source, so the
# build has to be handed back to the invoking user: run as root it leaves a
# root-owned target/, node_modules/ and build/ that later builds cannot write.
build_as_user = $(if $(SUDO_USER),sudo -u $(SUDO_USER) -H $(MAKE) build,$(MAKE) build)
# Empty when already root, so one recipe covers `make` and `sudo make` alike.
as_root = $(if $(filter 0,$(shell id -u)),,sudo)

# DESTDIR set means a staged install for packaging: no elevation, and the packager's
# own triggers own the caches.
install-source:
ifeq ($(strip $(DESTDIR)),)
	@$(sudo_keepalive) \
	$(build_as_user) && $(as_root) $(MAKE) install && $(as_root) $(MAKE) install-data
else
	@$(MAKE) build
	@$(MAKE) install
	@$(MAKE) install-data
endif

# Best effort throughout: a half-installed tree must still be removable, so no
# step may abort the rest.
uninstall:
	@-$(MAKE) -C $(daemon_dir) $@
	@-$(MAKE) -C $(qt_dir) $@
	@-$(RM) -f $(DESTDIR)/usr/share/applications/$(ap_id).desktop
# Pre-5.0.0 source installs put it under /usr/local; drop that too.
	@-$(RM) -f $(DESTDIR)/usr/local/share/applications/$(ap_id).desktop
	@-$(RM) -f $(DESTDIR)/usr/share/metainfo/$(ap_id).metainfo.xml
	@-$(RM) -f $(DESTDIR)/usr/share/pixmaps/$(ap_id).png
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/256x256/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/scalable/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/usr/share/icons/hicolor/symbolic/apps/$(ap_id)*
	@-$(RM) -f $(DESTDIR)/etc/systemd/system/coolercontrold.service

# helpful std development & testing targets
# For testing these make targets require that a system package is already installed

# Authenticate up front, then refresh the sudo timestamp every 50s so a long build never
# ends on a password prompt. Builds stay unprivileged; only the final step uses sudo. The
# trap reaps the refresher when the recipe exits, including on failure or Ctrl-C. If sudo
# caching is disabled the first refresh fails, the loop ends, and sudo just asks as before.
sudo_keepalive = sudo -v || exit 1; while sudo -n -v 2>/dev/null; do sleep 50; done & \
	keepalive=$$!; trap 'kill $$keepalive 2>/dev/null' EXIT;

dev-run:
	@$(sudo_keepalive) \
	$(MAKE) build-qt && $(MAKE) -C $(ui_dir) $@ && $(MAKE) -C $(daemon_dir) $@

# reproduce the CI pipeline locally: full clean, CI tooling, trunk checks, junit-producing tests
ci-local: clean ci-install validate-metadata ci-check ci-test-ui ci-test-daemon ci-test-qt

# pre-PR gate: lints the committed branch diff only, then UI and daemon tests, clippy pedantic, Qt build
# usage: make pr-check [base=<ref>]
base ?= main
pr-check: validate-metadata
	@./trunk install --ci
	@git merge-base $(base) HEAD > /dev/null
	@git diff -z --name-only --diff-filter=d $(base)...HEAD | xargs -0 -r ./trunk check --ci
	@$(MAKE) -C $(ui_dir) check
	@$(MAKE) -C $(daemon_dir) clippy
	@$(MAKE) -C $(daemon_dir) clippy-tests
	@$(MAKE) -C $(daemon_dir) test
	@$(MAKE) -C $(qt_dir) build

# installs the release coolercontrold daemon and desktop app binaries: (need CC pre-installed)
dev-install:
	@$(sudo_keepalive) \
	$(MAKE) build && sudo $(MAKE) install && sudo systemctl restart coolercontrold

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
