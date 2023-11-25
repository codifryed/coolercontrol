# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

.PHONY: build test check install

build:
	@python3 -m pip install --upgrade setuptools build wheel
	@rm -rf ./build || true
	@rm -rf ./dist || true
	@rm -rf ./coolercontrol_liqctld.egg-info || true
	@python3 -m build --wheel --no-isolation

test: build
	@python3 -m coolercontrol_liqctld --version

build-fast: build

test-fast: build-fast
	@python3 -m coolercontrol_liqctld --version

install:
	@python3 -m pip install .
