# CoolerControl Makefile
.DEFAULT_GOAL := build

build:
	@rm -rf dist
	@npm install
	@npm run build

test: build
	@npm run test:unit

build-fast: build

test-fast: test

install:
	# todo
