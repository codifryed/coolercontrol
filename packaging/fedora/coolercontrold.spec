%global _enable_debug_packages 0
%global debug_package %{nil}
%global project coolercontrol

# prevent library files from being installed
%global __cargo_is_lib() 0

Name:           %{project}d
Version:        0.18.0~BETA
Release:        1%{?dist}
Summary:        Monitor and control your cooling devices.

License:        GPLv3+
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  systemd-rpm-macros
BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  nodejs
BuildRequires:  npm

Requires: coolercontrol-liqctld

VCS:        {{{ git_dir_vcs }}}
Source:     {{{ git_dir_pack }}}

%description
CoolerControl is a program to monitor and control your cooling devices.

It offers an easy-to-use user interface with various control features and also provides live thermal performance details.

%prep
{{{ git_dir_setup_macro }}}
# non-Inet rust and npm dependencies are a WIP
# (cd coolercontrold; #cargo_prep)

%generate_buildrequires
# (cd coolercontrold; #cargo_generate_buildrequires)

%build
# build web ui files:
make build-ui
cp -rfp %{project}-ui/dist/* %{name}/resources/app/
(cd %{name}; /usr/bin/cargo build -j${RPM_BUILD_NCPUS} --profile release)

%install
install -Dpm 755 %{name}/target/release/%{name} -t %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_unitdir}
cp -p packaging/systemd/%{name}.service %{buildroot}%{_unitdir}

%check
(cd %{name}; /usr/bin/cargo test -j${RPM_BUILD_NCPUS} --profile release --no-fail-fast)
%{buildroot}%{_bindir}/%{name} --version

%files
%{_bindir}/%{name}
%{_unitdir}/%{name}.service
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
