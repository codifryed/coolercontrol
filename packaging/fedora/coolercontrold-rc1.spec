%bcond check 1
# main unless overridden, as the CI prerelease build does with --define for its own ref.
%{!?branch:%global branch main}
%global project_id 30707566
%global project coolercontrol
# Fedora's default of one codegen unit compiles the daemon crate on a single core. 6 matches the
# Cargo release profile.
%global rustflags_codegen_units 6
# Full debuginfo drives rustc past the 8 GB of a Copr builder. 1 keeps the line tables.
%global rustflags_debuginfo 1

Name:           %{project}d
Version:        5.1.1~rc1
Release:        %{?autorelease}%{!?autorelease:0%{?dist}}
Summary:        Powerful cooling control and monitoring
Obsoletes:      coolercontrol-liqctld <= 2.2.2
ExclusiveArch:  x86_64 aarch64
License:        GPL-3.0-or-later AND OFL-1.1
URL:            https://gitlab.com/%{project}/%{project}

BuildRequires:  systemd-rpm-macros
BuildRequires:  rpm_macro(cargo_build)
BuildRequires:  pkgconfig(libdrm_amdgpu)
BuildRequires:  pkgconfig(libdrm)
Recommends:     python3-liquidctl
Recommends:     lm_sensors

Source0:        https://gitlab.com/api/v4/projects/%{project_id}/packages/generic/%{project}/%{branch}/%{project}-%{branch}.tar.gz
# rc still uses unversioned Vendor
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
# Not %%cargo_install: it ignores Cargo.lock, so it resolves the crates afresh and rebuilds the daemon.
install -Dpm 755 target/rpm/%{name} -t %{buildroot}%{_bindir}

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
