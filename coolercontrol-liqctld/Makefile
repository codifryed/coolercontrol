# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

build:
	@python3 -m pip install --upgrade build
	@rm -rf ./dist || true
	@python3 -m build

test: build

build-fast: build

test-fast: build-fast

check:
	@/usr/bin/coolercontrol-liqctld --version

install:
	@python3 -m pip install --upgrade installer
	@python3 -m installer dist/*.whl
