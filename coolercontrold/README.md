# coolercontrold

The CoolerControl daemon. A Cargo workspace containing two crates:

| Crate                       | Path         | Description                                            |
| --------------------------- | ------------ | ------------------------------------------------------ |
| [`coolercontrold`](daemon/) | `daemon/`    | daemon - core business logic and hardware access       |
| [`cc-detect`](cc-detect/)   | `cc-detect/` | Super-I/O hardware detection and kernel module loading |

For project-wide documentation, installation packages, and issue tracking see the
[CoolerControl project](https://gitlab.com/coolercontrol/coolercontrol) and
[CoolerControl Website](https://coolercontrol.org).

## Build

```bash
# Release build (embeds UI assets from ../coolercontrol-ui/dist/)
make

# Debug build and run (requires root)
make dev-run

# Run tests
make test

# Install to /usr/bin/
make install

# Install to a custom prefix
make install prefix=/usr/local
```

> The UI must be built before the daemon - UI assets are embedded into the binary at compile time
> from `daemon/resources/app/`.

## Development

See the daemon crate's [README](daemon/README.md) for development instructions.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
