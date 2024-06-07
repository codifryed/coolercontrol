%global _enable_debug_packages 0
%global debug_package %{nil}
%global project coolercontrol
%global ui_dir coolercontrol-ui/src-tauri
%global ap_id org.coolercontrol.CoolerControl

# prevent library files from being installed
%global __cargo_is_lib() 0

Name:           %{project}
Version:        1.3.0
Release:        1%{?dist}
Summary:        Monitor and control your cooling devices.

License:        GPLv3+
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  libappstream-glib
BuildRequires:  desktop-file-utils
BuildRequires:  nodejs
BuildRequires:  npm
BuildRequires:  make
# Tauri build dependencies
BuildRequires:  webkit2gtk4.0-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel
BuildRequires:  autoconf automake binutils bison flex gcc gcc-c++ gdb glibc-devel libtool pkgconf strace
Requires:       hicolor-icon-theme
Requires:       libappindicator-gtk3
Requires:       coolercontrold

VCS:        {{{ git_dir_vcs }}}
Source:     {{{ git_dir_pack }}}

%description
CoolerControl is a program to monitor and control your cooling devices.

It offers an easy-to-use user interface with various control features and also provides live thermal performance details.

%prep
{{{ git_dir_setup_macro }}}
# rust and npm dependencies are a WIP
# (cd coolercontrol-ui/src-tauri; #cargo_prep)

%generate_buildrequires
# (cd coolercontrol-ui/src-tauri; #cargo_generate_buildrequires)

%build
# build web ui files:
make build-ui
(cd %{ui_dir}; /usr/bin/cargo build -j${RPM_BUILD_NCPUS} --profile release -F custom-protocol)

%install
install -Dpm 755 %{ui_dir}/target/release/%{name} -t %{buildroot}%{_bindir}
desktop-file-install --dir=%{buildroot}%{_datadir}/applications packaging/metadata/%{ap_id}.desktop
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/scalable/apps
cp -p packaging/metadata/%{ap_id}.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/
mkdir -p %{buildroot}%{_datadir}/icons/hicolor/256x256/apps
cp -p packaging/metadata/%{ap_id}.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/
mkdir -p %{buildroot}%{_metainfodir}
cp -p packaging/metadata/%{ap_id}.metainfo.xml %{buildroot}%{_metainfodir}/

%check
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/*.metainfo.xml
(cd %{ui_dir}; /usr/bin/cargo test -j${RPM_BUILD_NCPUS} --profile release --no-fail-fast)

%files
%{_bindir}/%{name}
%{_datadir}/applications/%{ap_id}.desktop
%{_datadir}/icons/hicolor/scalable/apps/%{ap_id}.svg
%{_datadir}/icons/hicolor/256x256/apps/%{ap_id}.png
%{_metainfodir}/%{ap_id}.metainfo.xml
%license LICENSE
%doc README.md CHANGELOG.md

%changelog
* Fri Jun 07 2024 Guy Boldon <gb@guyboldon.com> - 1.3.0-1
- 1.3.0 Release

* Sat Apr 13 2024 Guy Boldon <gb@guyboldon.com> - 1.2.2-1
- 1.2.2 Release

* Tue Apr 02 2024 Guy Boldon <gb@guyboldon.com> - 1.2.1-1
- 1.2.1 Release

* Mon Apr 01 2024 Guy Boldon <gb@guyboldon.com> - 1.2.0-1
- 1.2.0 Release

* Sat Feb 24 2024 Guy Boldon <gb@guyboldon.com> - 1.1.1-2
- Package Update

* Fri Feb 16 2024 Guy Boldon <gb@guyboldon.com> - 1.1.1-0
- 1.1.1 Release

* Wed Jan 31 2024 Guy Boldon <gb@guyboldon.com> - 1.1.0-0
- 1.1.0 Release

* Mon Jan 15 2024 Guy Boldon <gb@guyboldon.com> - 1.0.4-0
- 1.0.4 Release

* Mon Jan 15 2024 Guy Boldon <gb@guyboldon.com> - 1.0.3-0
- 1.0.3 Release

* Sun Jan 14 2024 Guy Boldon <gb@guyboldon.com> - 1.0.2-0
- 1.0.2 Release

* Fri Jan 12 2024 Guy Boldon <gb@guyboldon.com> - 1.0.1-0
- 1.0.1 Release

* Sun Jan 07 2024 Guy Boldon <gb@guyboldon.com> - 1.0.0-0
- 1.0.0 Release

* Fri Dec 15 2023 Guy Boldon <gb@guyboldon.com> - 0.17.3-0
- 0.17.3 Release

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
