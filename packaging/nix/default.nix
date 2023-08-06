{ lib
, fetchFromGitLab
, fetchFromGitHub
, rustPlatform
, systemd
, python3
, qt6
}:

let
  version = "0.17.0";
  baseSrc = fetchFromGitLab {
    owner = "coolercontrol";
    repo = "coolercontrol";
    rev = version;
    hash = "sha256-HZpJd3/Hk6icf+GFwW+inz7j/jFPlAZJVQbt0enK7xs=";
  };

  meta = with lib; {
    description = "Monitor and control your cooling devices.";
    longDescription = ''
      Monitor and control your cooling devices with a system daemon and GUI.
    '';
    homepage = "https://gitlab.com/coolercontrol/coolercontrol";
    license = licenses.gpl3Plus;
    platforms = [ "x86_64-linux" ];
    maintainers = with maintainers; [ codifryed ];
  };

in
rec {
  coolercontrold = rustPlatform.buildRustPackage {
    pname = "coolercontrold";
    inherit version meta;
    src = "${baseSrc}/coolercontrold";
    cargoHash = "sha256-Zgm1ROgZ4Ph/fPdIYW3OqTj2BtZ4KT7TNqCx5K2ZkCc=";

    buildInputs = [
      systemd
    ];

    postInstall = ''
      install -Dm444 "${baseSrc}/packaging/systemd/coolercontrold.service" -t "$out/lib/systemd/system"
      install -Dm444 "${baseSrc}/packaging/systemd/coolercontrol-liqctld.service" -t "$out/lib/systemd/system"
    '';

    postFixup = ''
      for f in $out/lib/systemd/system/*; do
        substituteInPlace $f --replace /usr/bin $out/bin
      done
    '';
  };

  coolercontrol-liqctld = python3.pkgs.buildPythonApplication {
    pname = "coolercontrol-liqctld";
    inherit version meta;
    format = "pyproject";
    src = "${baseSrc}/coolercontrol-liqctld";

    nativeBuildInputs = with python3.pkgs; [
      poetry-core
      setuptools
      pythonRelaxDepsHook
    ];
    pythonRelaxDeps = true;

    propagatedBuildInputs = with python3.pkgs; [
      liquidctl
      setproctitle
      fastapi
      uvicorn
      orjson
    ];
  };

  coolercontrol-gui = python3.pkgs.buildPythonApplication {
    pname = "coolercontrol";
    inherit version meta;
    format = "pyproject";
    src = "${baseSrc}/coolercontrol-gui";

    buildInputs = [ qt6.qtwayland ];
    nativeBuildInputs = with python3.pkgs; [
      poetry-core
      setuptools
      pythonRelaxDepsHook
      qt6.wrapQtAppsHook
    ];
    pythonRelaxDeps = true;
    pythonRemoveDeps = [ "pyside6" ]; # resolution issue with the wheel and relaxed dependencies

    propagatedBuildInputs = with python3.pkgs; [
      pyside6
      apscheduler
      matplotlib
      numpy
      setproctitle
      jeepney
      requests
      dataclass-wizard
    ];

    postInstall = ''
    install -Dm644 "${baseSrc}/packaging/metadata/org.coolercontrol.CoolerControl.desktop" -t "$out/share/applications/"
    install -Dm644 "${baseSrc}/packaging/metadata/org.coolercontrol.CoolerControl.metainfo.xml" -t "$out/share/metainfo/"
    install -Dm644 "${baseSrc}/packaging/metadata/org.coolercontrol.CoolerControl.png" -t "$out/share/icons/hicolor/256x256/apps/"
    install -Dm644 "${baseSrc}/packaging/metadata/org.coolercontrol.CoolerControl.svg" -t "$out/share/icons/hicolor/scalable/apps/"
    '';
  };
}
