# CoolerControl Makefile
.DEFAULT_GOAL := build

.PHONY: build test clean install uninstall

build:
	@cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/ 
	@$(MAKE) -C build

test:
	@echo "success"

ci-test:
	@echo "success"

clean:
	@-$(RM) -rf build

install:
	@$(MAKE) -C $@

uninstall:
	@$(MAKE) -C $@

