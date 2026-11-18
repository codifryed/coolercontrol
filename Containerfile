FROM archlinux:base-devel

RUN pacman -Syu --noconfirm --needed \
        # one package ships rustc, cargo, rustfmt and clippy; rolling, so always
        # at or ahead of the 1.95 MSRV:
        rust rust-analyzer \
        # node/npm for the web UI:
        nodejs npm \
        # trunk (the bundled ./trunk launcher) links against libcrypt.so.1, which
        # Arch ships in its own package below:
        libxcrypt-compat \
        # repository plumbing (base-devel has neither):
        git curl \
        # Qt desktop app:
        cmake qt6-base qt6-webengine qt6-webchannel \
        # make validate-metadata runs these (reuse is the REUSE-spec linter):
        desktop-file-utils appstream-glib reuse \
        # daemon development: runtime libdrm, lm_sensors for the sensors CLI, and
        # liquidctl for the liqctld test-suite (pulls python, python-pillow and
        # the usb/hid/i2c stacks):
        libdrm lm_sensors liquidctl \
        # needed to link against different glibc versions
        cargo-zigbuild

# The checkout arrives bind-mounted (root writes back into it), so make the
# ownership check in git stand down: targets like pr-check shell out to git.
RUN git config --global --add safe.directory /src

# sudo cannot work in a rootless container, and nothing in the build needs
# elevation. It is shadowed by ccont-sudo (see its header): CoolerControl
# binary invocations are recorded to the marker file that ccont then runs on
# the host; every other call, including the flag-only keepalives, is a no-op.
COPY --chmod=755 ccont-sudo /usr/local/bin/sudo

# Everything here runs as uid 0, and Chromium's sandbox cannot drop privilege from
# root, so QtWebEngine render processes refuse to start without this and the GUI
# exits. Software OpenGL pre-empts the harmless Vulkan-probe warnings.
ENV QTWEBENGINE_DISABLE_SANDBOX=1 \
    QT_OPENGL=software

# In case anything needs to differentiate running inside a container.
ENV CC_DEV_CONTAINER=1

WORKDIR /src

CMD ["bash"]
