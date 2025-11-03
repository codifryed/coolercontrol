%bcond check 1
%global project coolercontrol

# prevent library files from being installed
%global cargo_install_lib 0

Name:           %{project}d
Version:        3.0.2
Release:        %autorelease
Summary:        Powerful cooling control and monitoring
Obsoletes:      coolercontrol-liqctld <= 2.2.2
ExclusiveArch:  x86_64 aarch64
License:        GPL-3.0-or-later
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  systemd-rpm-macros
BuildRequires:  cargo-rpm-macros >= 25
BuildRequires:  pkgconfig(libdrm_amdgpu)
BuildRequires:  pkgconfig(libdrm)
Recommends:     python3-liquidctl
Recommends:     lm_sensors

Source0:        https://gitlab.com/%{project}/%{project}/-/releases/%{version}/downloads/packages/%{project}-%{version}.tar.gz
Source1:        https://gitlab.com/%{project}/%{project}/-/releases/%{version}/downloads/packages/%{name}-vendor.tar.gz

%description
This is the system daemon for CoolerControl.
CoolerControl is an open-source application for monitoring and controlling supported cooling
devices. It features an intuitive interface, flexible control options, and live thermal data to keep
your system quiet, cool, and stable.

%prep
%autosetup -n %{project}-%{version}/%{name} -a 0
tar -xzf %{SOURCE1}
%cargo_prep -v vendor
#cargo_prep

%generate_buildrequires
#cargo_generate_buildrequires

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}

%install
install -Dpm 644 systemd/%{name}.service -t %{buildroot}%{_unitdir}
install -Dpm 644 man/%{name}.8 -t %{buildroot}%{_mandir}/man8
%cargo_install

%if %{with check}
%check
%cargo_test
%{buildroot}%{_bindir}/%{name} --version
%endif

%files
%{_bindir}/%{name}
%{_unitdir}/%{name}.service
%{_mandir}/man8/%{name}.8*
%license LICENSE
%doc CHANGELOG.md

%post
%systemd_post %{name}.service

%preun
%systemd_preun %{name}.service

%postun
%systemd_postun_with_restart %{name}.service

%changelog
* Mon Nov 03 2025 Guy Boldon <gb@guyboldon.com> - 3.0.2-1
- 3.0.2 Release

* Sat Oct 04 2025 Guy Boldon <gb@guyboldon.com> - 3.0.1-1
- 3.0.1 Release

* Fri Oct 03 2025 Guy Boldon <gb@guyboldon.com> - 3.0.0-1
- 3.0.0 Release

* Fri Jul 18 2025 Guy Boldon <gb@guyboldon.com> - 2.2.2-1
- 2.2.2 Release

* Fri Jun 13 2025 Guy Boldon <gb@guyboldon.com> - 2.2.1-1
- 2.2.1 Release

* Sat May 31 2025 Guy Boldon <gb@guyboldon.com> - 2.2.0-1
- 2.2.0 Release

* Sat Apr 05 2025 Guy Boldon <gb@guyboldon.com> - 2.1.0-1
- 2.1.0 Release

* Thu Mar 20 2025 Guy Boldon <gb@guyboldon.com> - 2.0.1-1
- 2.0.1 Release

* Sat Mar 15 2025 Guy Boldon <gb@guyboldon.com> - 2.0.0-1
- 2.0.0 Release

* Sat Dec 14 2024 Guy Boldon <gb@guyboldon.com> - 1.4.5-1
- 1.4.5 Release

* Sat Nov 02 2024 Guy Boldon <gb@guyboldon.com> - 1.4.4-1
- 1.4.4 Release

* Fri Nov 01 2024 Guy Boldon <gb@guyboldon.com> - 1.4.3-1
- 1.4.3 Release

* Fri Sep 27 2024 Guy Boldon <gb@guyboldon.com> - 1.4.2-1
- 1.4.2 Release

* Sat Sep 21 2024 Guy Boldon <gb@guyboldon.com> - 1.4.1-1
- 1.4.1 Release

* Sat Jul 27 2024 Guy Boldon <gb@guyboldon.com> - 1.4.0-1
- 1.4.0 Release

* Fri Jun 07 2024 Guy Boldon <gb@guyboldon.com> - 1.3.0-1
- 1.3.0 Release

* Sat Apr 13 2024 Guy Boldon <gb@guyboldon.com> - 1.2.2-1
- 1.2.2 Release

* Tue Apr 02 2024 Guy Boldon <gb@guyboldon.com> - 1.2.1-1
- 1.2.1 Release

* Mon Apr 01 2024 Guy Boldon <gb@guyboldon.com> - 1.2.0-1
- 1.2.0 Release

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
