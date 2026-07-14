Format: 3.0 (quilt)
Source: coolercontrol
Binary: coolercontrold, coolercontrol
Architecture: amd64 arm64
Version: 4.3.1
Maintainer: Guy Boldon <gb@guyboldon.com>
Homepage: https://gitlab.com/coolercontrol/coolercontrol
Standards-Version: 4.6.2
Build-Depends: debhelper-compat (= 13), nodejs, cargo (>= 1.85) | cargo-1.85 | cargo-web (>=1.85), libdrm-dev, build-essential, cmake (>= 3.15), libgl1-mesa-dev, libqt6opengl6-dev, qt6-base-dev, qt6-webengine-dev, qt6-webengine-dev-tools
Package-List:
 coolercontrol deb admin optional arch=amd64,arm64
 coolercontrold deb admin optional arch=amd64,arm64
Checksums-Sha1:
 deadbeefdeadbeefdeadbeefdeadbeefdeadbeef 1433553 coolercontrol_.tar.gz
Checksums-Sha256:
 deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef 1433553 coolercontrol_.tar.gz
Files:
 deadbeefdeadbeefdeadbeefdeadbeef 1433553 coolercontrol_.tar.gz
Debtransform-Files:  coolercontrold-vendor-4.3.1.tar.gz
Debtransform-Files-Tar:  debian.tar.gz
Debtransform-Tar:  coolercontrol-4.3.1.tar.gz
