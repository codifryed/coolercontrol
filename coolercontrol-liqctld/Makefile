# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build

.PHONY: build test check install install-pip-dependencies

build:
	@python3 -m pip install --upgrade pip setuptools build wheel
	@rm -rf ./build || true
	@rm -rf ./dist || true
	@rm -rf ./coolercontrol_liqctld.egg-info || true
	@python3 -m build --wheel --no-isolation

install-pip-dependencies:
	@python3 -m pip install --upgrade -r requirements.txt

test: build
	@python3 -m coolercontrol_liqctld --version

build-fast: build

test-fast: install-pip-dependencies build-fast
	@python3 -m coolercontrol_liqctld --version

install:
	@python3 -m pip install .
	@ln -sf /usr/local/bin/coolercontrol-liqctld /usr/bin
