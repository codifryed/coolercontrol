# CoolerControl Makefile
.DEFAULT_GOAL := build

.PHONY: build test ci-test clean install uninstall

# Where `install` lands: GNUInstallDirs maps the configured prefix of / to usr/bin.
prefix := '/usr'
executable := 'coolercontrol'
# CMake caches are bound to the source path, so ccont (at /src) builds in its own dir.
CC_QT_BUILD_DIR ?= build

build:
	@cmake -S . -B $(CC_QT_BUILD_DIR) -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/
	@$(MAKE) -C $(CC_QT_BUILD_DIR) -j8

test:
	@echo "success"

ci-test: build
	@echo "success"

clean:
	@-$(RM) -rf $(CC_QT_BUILD_DIR)

install:
	@$(MAKE) -C $(CC_QT_BUILD_DIR) $@

# CMake generates no uninstall target, so delegating to the build dir fails even
# when it exists, and taking the whole recipe down with it. Remove the installed
# binary directly, the way the daemon does.
uninstall:
	@-$(RM) -f $(DESTDIR)$(prefix)/bin/$(executable)

