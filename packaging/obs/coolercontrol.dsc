Format: 3.0 (quilt)
Source: coolercontrol
Binary: coolercontrold, coolercontrol
Architecture: amd64 arm64
Version: 3.1.0
Maintainer: Guy Boldon <gb@guyboldon.com>
Homepage: https://gitlab.com/coolercontrol/coolercontrol
Standards-Version: 4.6.2
Build-Depends: debhelper-compat (= 13), nodejs, cargo (>= 1.85) | cargo-1.85, libdrm-dev, protobuf-compiler, build-essential, cmake (>= 3.15), libgl1-mesa-dev, libqt6opengl6-dev, qt6-base-dev, qt6-webengine-dev, qt6-webengine-dev-tools
Package-List:
 coolercontrol deb admin optional arch=amd64,arm64
 coolercontrold deb admin optional arch=amd64,arm64
Checksums-Sha1:
 38d1c2c1678f9ff84fd0edb7df518ce9bb2e981b 1433553 coolercontrol_3.1.0.tar.gz
Checksums-Sha256:
 b912c33c722f043bb78c8fcc56d2ed0ef4afbc9009e6a25bf4420dbf090ddd00 1433553 coolercontrol_3.1.0.tar.gz
Files:
 55b6952cdc7f05994f2addfcea4dea4f 1433553 coolercontrol_3.1.0.tar.gz
Debtransform-Files:  coolercontrold-vendor.tar.gz
Debtransform-Files-Tar:  debian.tar.gz
Debtransform-Tar:  coolercontrol-3.1.0.tar.gz
