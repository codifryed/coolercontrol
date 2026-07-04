# Release engineering: version bump, release, source packaging, vendoring, assets.
# Maintainer-only.
v = "patch"

.PHONY: bump release push-release ubuntu-source-package vendor \
	assets assets-daemon assets-ui assets-qt

# bump version: patch | minor | major  (e.g. make bump v=minor)
bump:
	@./packaging/version_bump.sh $(v)

# version from bump above applies to release as well:
release:
	@./packaging/release.sh

push-release:
	@git push --follow-tags

ubuntu-source-package:
	@sed -i 's/UNRELEASED/jammy/g' debian/changelog
	@debuild -S -sa --force-sign
	@cd .. && dput ppa:codifryed/coolercontrol ../coolercontrol_*_source.changes

# release assets under assets-built/ (run after the build targets)
assets: assets-daemon assets-ui assets-qt

assets-daemon:
	@mkdir -p assets-built
	@cp $(daemon_dir)/target/release/coolercontrold ./assets-built/

assets-ui:
	@mkdir -p assets-built
	@cd $(ui_dir) && tar -czf ../assets-built/coolercontrol-ui-vendor.tar.gz node_modules

assets-qt: build-qt
	@mkdir -p assets-built
	@cp $(qt_dir)/build/coolercontrol ./assets-built/

# cargo vendored crates (consumed by the deb/OBS source builds)
vendor:
	@$(MAKE) -C $(daemon_dir) vendor
	@cd $(daemon_dir) && tar -czf ../coolercontrold-vendor.tar.gz vendor .cargo
