# CoolerControl-Liqctld Makefile
.DEFAULT_GOAL := build
prefix := '/usr'
entry_point_name := 'coolercontrol-liqctld'
package := 'coolercontrol_liqctld'

.PHONY: build-source build-binary test clean install uninstall install-pip-dependencies

build:

build-source: build
	@python3 -m pip install --upgrade pip setuptools build wheel
	@python3 -m build --wheel --no-isolation

# This is for AppImage building
build-binary: install-pip-dependencies
	@python3 -m pip install --upgrade nuitka
	@python3 -m nuitka --standalone --prefer-source-code --python-flag=-S,-O,no_docstrings coolercontrol_liqctld/liqctld.py
	@mv liqctld.dist/liqctld.bin liqctld.dist/liqctld
	@liqctld.dist/liqctld --version

install-pip-dependencies: build-source
	@python3 -m pip install --upgrade -r requirements.txt

test:

ci-test: install-pip-dependencies
	@python3 -m $(package) --version

clean:
	@-$(RM) -rf ./build
	@-$(RM) -rf ./dist
	@-$(RM) -rf ./$(package).egg-info
	@-$(RM) -rf liqctld.build
	@-$(RM) -rf liqctld.dist

install-source: build-source
	@python3 -m pip install .
	@-ln -sf /usr/local/bin/$(entry_point_name) $(DESTDIR)$(prefix)/bin

uninstall:
	@-rm -f $(DESTDIR)$(prefix)/bin/$(entry_point_name)
	@python3 -m pip uninstall $(package)
