%global _enable_debug_packages 0
%global debug_package %{nil}

# prevent library files from being installed
%global __cargo_is_lib() 0

Name:           coolercontrol
Version:        0.17.2
Release:        1%{?dist}
Summary:        Monitor and control your cooling devices.

License:        GPLv3+
URL:            https://gitlab.com/coolercontrol/coolercontrol

BuildRequires:  systemd-rpm-macros
BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  libappstream-glib
BuildRequires:  python3-devel
BuildRequires:  python3-wheel
# liqctld dependencies
BuildRequires:  python3-liquidctl
BuildRequires:  python3-setproctitle
BuildRequires:  python3-fastapi
BuildRequires:  python3-uvicorn
# rust and npm dependencies are a WIP
BuildRequires:  nodejs
BuildRequires:  npm
# Tauri build dependencies
BuildRequires:  webkit2gtk4.0-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel
BuildRequires:  autoconf automake binutils bison flex gcc gcc-c++ gdb glibc-devel libtool make pkgconf strace
Requires:       hicolor-icon-theme
# most requires automoatically set
Requires:       libappindicator
BuildArch:      x86_64
# auto-tar-ing local git sources for CI pipelines. To use official release sources:
# Source0:        https://gitlab.com/coolercontrol/coolercontrol/-/archive/%{version}/%{name}-%{version}.tar.gz
Source0:        CoolerControl.tar.gz
# find-requires and find-provides doesn't work as intended due to nuitka-build linking todo: test if yes is ok
#AutoReqProv: yes

%description
CoolerControl is a program to monitor and control your cooling devices.

It offers an easy-to-use user interface with various control features and also provides live thermal performance details.

%prep
#autosetup
%autosetup -n coolercontrol
# rust and npm dependencies are a WIP
# (cd coolercontrold; #cargo_prep)
# (cd coolercontrol-ui/src-tauri; #cargo_prep)

%generate_buildrequires
(cd coolercontrol-liqctld; %pyproject_buildrequires)
# (cd coolercontrold; #cargo_generate_buildrequires)
# (cd coolercontrol-ui/src-tauri; #cargo_generate_buildrequires)

%build
# build web ui files:
make build-ui
(cd coolercontrol-liqctld; %pyproject_wheel)
# slightly modified build command from #cargo_build
(cd coolercontrol-ui/src-tauri; /usr/bin/env CARGO_TARGET_DIR=%{_builddir}/coolercontrol-ui/src-tauri/target RUSTC_BOOTSTRAP=1 'RUSTFLAGS=-Copt-level=3 -Cdebuginfo=2 -Ccodegen-units=1 -Cstrip=none -Cforce-frame-pointers=yes -Clink-arg=-Wl,-z,relro -Clink-arg=-Wl,-z,now -Clink-arg=-specs=/usr/lib/rpm/redhat/redhat-package-notes --cap-lints=warn' /usr/bin/cargo build -j${RPM_BUILD_NCPUS} -Z avoid-dev-deps --profile release) &
(cd coolercontrold; /usr/bin/env CARGO_TARGET_DIR=%{_builddir}/coolercontrold/target RUSTC_BOOTSTRAP=1 'RUSTFLAGS=-Copt-level=3 -Cdebuginfo=2 -Ccodegen-units=1 -Cstrip=none -Cforce-frame-pointers=yes -Clink-arg=-Wl,-z,relro -Clink-arg=-Wl,-z,now -Clink-arg=-specs=/usr/lib/rpm/redhat/redhat-package-notes --cap-lints=warn' /usr/bin/cargo build -j${RPM_BUILD_NCPUS} -Z avoid-dev-deps --profile release)


%install
(cd coolercontrol-liqctld; %pyproject_install)
(cd coolercontrol-liqctld; %pyproject_save_files coolercontrol_liqctld)
install -p -m 755 %{_builddir}/coolercontrol-ui/src-tauri/target/release/%{name} %{buildroot}%{_bindir}
install -p -m 755 %{_builddir}/coolercontrold/target/release/coolercontrold %{buildroot}%{_bindir}
desktop-file-install packaging/metadata/org.coolercontrol.CoolerControl.desktop
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
mkdir -p %{buildroot}%{_metainfodir}
cp -pr packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml %{buildroot}%{_metainfodir}
mkdir -p %{buildroot}%{_unitdir}
cp -p packaging/systemd/coolercontrol-liqctld.service %{buildroot}%{_unitdir}
cp -p packaging/systemd/coolercontrold.service %{buildroot}%{_unitdir}


%check
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/*.metainfo.xml
%pyproject_check_import
(cd coolercontrol-ui/src-tauri; /usr/bin/env CARGO_TARGET_DIR=%{_builddir}/coolercontrol-ui/src-tauri/target RUSTC_BOOTSTRAP=1 RUSTFLAGS='-Copt-level=3 -Cdebuginfo=2 -Ccodegen-units=1 -Cstrip=none -Cforce-frame-pointers=yes -Clink-arg=-Wl,-z,relro -Clink-arg=-Wl,-z,now -Clink-arg=-specs=/usr/lib/rpm/redhat/redhat-package-notes --cap-lints=warn' /usr/bin/cargo test -j${RPM_BUILD_NCPUS} -Z avoid-dev-deps --profile release --no-fail-fast) &
(cd coolercontrold; /usr/bin/env CARGO_TARGET_DIR=%{_builddir}/coolercontrold/target RUSTC_BOOTSTRAP=1 RUSTFLAGS='-Copt-level=3 -Cdebuginfo=2 -Ccodegen-units=1 -Cstrip=none -Cforce-frame-pointers=yes -Clink-arg=-Wl,-z,relro -Clink-arg=-Wl,-z,now -Clink-arg=-specs=/usr/lib/rpm/redhat/redhat-package-notes --cap-lints=warn' /usr/bin/cargo test -j${RPM_BUILD_NCPUS} -Z avoid-dev-deps --profile release --no-fail-fast)
%{buildroot}%{_bindir}/coolercontrold --version


%files -f %{pyproject_files}
%{_bindir}/%{name}
%{_bindir}/coolercontrold
%{_bindir}/coolercontrol-liqctld
%{_datadir}/applications/org.%{name}.CoolerControl.desktop
%{_datadir}/icons/hicolor/scalable/apps/org.%{name}.CoolerControl.svg
%{_datadir}/icons/hicolor/256x256/apps/org.%{name}.CoolerControl.png
%{_metainfodir}/org.%{name}.CoolerControl.metainfo.xml
%{_unitdir}/coolercontrol-liqctld.service
%{_unitdir}/coolercontrold.service
%license LICENSE
%doc README.md CHANGELOG.md

%post
%systemd_post coolercontrold.service

%preun
%systemd_preun coolercontrold.service

%postun
%systemd_postun_with_restart coolercontrold.service

%changelog
* Tue Nov 28 2023 Guy Boldon <gb@guyboldon.com> - 0.17.2-0
- 0.17.2 Release

* Wed Sep 13 2023 Guy Boldon <gb@guyboldon.com> - 0.17.1-0
- 0.17.1 Release

* Sun Jul 16 2023 Guy Boldon <gb@guyboldon.com> - 0.17.0-0
- 0.17.0 Release

* Sun Apr 23 2023 Guy Boldon <gb@guyboldon.com> - 0.16.0-0
- 0.16.0 Release

* Tue Mar 14 2023 Guy Boldon <gb@guyboldon.com> - 0.15.0-0
- 0.15.0 Release

* Wed Mar 01 2023 Guy Boldon <gb@guyboldon.com> - 0.14.6-0
- 0.14.6 Release

* Mon Feb 27 2023 Guy Boldon <gb@guyboldon.com> - 0.14.5-0
- 0.14.5 Release

* Tue Feb 14 2023 Guy Boldon <gb@guyboldon.com> - 0.14.4-0
- 0.14.4 Release

* Thu Feb 09 2023 Guy Boldon <gb@guyboldon.com> - 0.14.3-0
- 0.14.3 Release

* Tue Feb 07 2023 Guy Boldon <gb@guyboldon.com> - 0.14.2-0
- 0.14.2 Release

* Mon Feb 06 2023 Guy Boldon <gb@guyboldon.com> - 0.14.1-0
- 0.14.1 Release

* Sun Feb 05 2023 Guy Boldon <gb@guyboldon.com> - 0.14.0-0
- 0.14.0 Release
