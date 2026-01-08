%bcond check 1
%global branch main
%global project_id 30707566
%global project coolercontrol

# prevent library files from being installed
%global cargo_install_lib 0

Name:           %{project}d
Version:        3.1.0~rc1
Release:        %{?autorelease}%{!?autorelease:0%{?dist}}
Summary:        Powerful cooling control and monitoring
Obsoletes:      coolercontrol-liqctld <= 2.2.2
ExclusiveArch:  x86_64 aarch64
License:        GPL-3.0-or-later
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  systemd-rpm-macros
BuildRequires:  rpm_macro(cargo_build)
BuildRequires:  pkgconfig(libdrm_amdgpu)
BuildRequires:  pkgconfig(libdrm)
BuildRequires:  pkgconfig(protobuf)
Recommends:     python3-liquidctl
Recommends:     lm_sensors

Source0:        https://gitlab.com/api/v4/projects/%{project_id}/packages/generic/%{project}/%{branch}/%{project}-%{branch}.tar.gz
Source1:        https://gitlab.com/api/v4/projects/%{project_id}/packages/generic/%{project}/%{branch}/%{name}-vendor.tar.gz

%description
This is the system daemon for CoolerControl.
CoolerControl is an open-source application for monitoring and controlling supported cooling
devices. It features an intuitive interface, flexible control options, and live thermal data to keep
your system quiet, cool, and stable.

%prep
%autosetup -n %{project}-%{branch}/%{name} -a 0
tar -xzf %{SOURCE1}
%{?cargo_prep:%cargo_prep -v vendor}

%{?generate_buildrequires}

%build
%cargo_build
%{?cargo_license_summary}
%{?cargo_license:%{cargo_license} > LICENSE.dependencies}
%{?cargo_vendor_manifest}

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

%if 0%{?suse_version}
%pre
%systemd_pre %{name}.service
%endif

%post
%systemd_post %{name}.service

%preun
%systemd_preun %{name}.service

%postun
%systemd_postun_with_restart %{name}.service

%changelog
%autochangelog
