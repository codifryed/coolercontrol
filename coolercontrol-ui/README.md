# coolercontrol-ui

The UI is designed to enhance the user experience of controlling cooling devices on Linux, which
until recently had only been achievable using terminal commands and manually editing configuration
files. These are valid methods, but become increasingly more complex as one begins to use more
advanced features like fan curves and using sensor outputs from various sources. In addition, the UI
offers ways to monitor cooling related data, so that the user can see and adjust to the effects of
their changes in real time.

This folder contains the UI assets served by the daemon for both the Web UI and the Desktop
Application.

The UI is a JavaScript SPA using the Vue framework. It communicates with the `coolercontrold` daemon
using a REST API. Cosmetic-specific features are handled completely by the UI, whereas core logic
and processes are handled by the daemon.

## Requirements

- make
- nodejs >= 22.22.2 (the minimum our dependency tree declares; CI and releases build with Node 24
  LTS, which is what we test against)
- npm

## Installation

Since these assets are embedded in the daemon binary, this folder itself doesn't install anything.
See `coolercontrold` for the daemon which contains the Web UI, and is also where the desktop
application retrieves the web assets.

## Development

Development can mostly be done using `npm`. Note that the Qt Desktop application uses an older
chromium browser engine on older distros. Such as Chrome v90 for Qt 6.2.4 on Ubuntu 22.04 LTS. This
means one needs to test functions and feature for compatibility with those older engines.

Install NPM dependencies & Build:

```bash
make build
```

Test:

```bash
make test
```

Hot-Reload in your browser:

```bash
npm run dev
# or
make dev
```

## Held-back Dependencies

- `"@types/node": "22.19.19"` for compat with the current tsconfig node version
- `"typescript": "^6.0.3"` because 7.x is the native (Go) port, which no longer ships the JavaScript
  compiler API. Its `exports` map offers only `lib/version.cjs` and `unstable/*`, so `vue-tsc` dies
  on startup resolving `typescript/lib/tsc` (`ERR_PACKAGE_PATH_NOT_EXPORTED`). Our own code is
  already 7.x clean: the native `tsc` reports no errors on this project beyond the `.vue` imports it
  cannot resolve. Revisit when vuejs/language-tools ships a `vue-tsc` that targets the native
  compiler.
- `"tailwindcss": "3.4.19",` the upgrade to 4.x looks to be significant work
  - https://tailwindcss.com/docs/upgrade-guide
  - Looks like 4.0 only works for Chrome 111+ (We need to support 90+ for older debian/ubuntu
    distros with QtWebEngine)
  - https://wiki.qt.io/QtWebEngine/ChromiumVersions
  - Tailwind 3 is also the only thing holding `braces` (via `micromatch`, `fast-glob` and its own
    `chokidar` 3), whose GHSA-vfj7-8cjw-p6xm has no fixed release. It is ignored in
    `osv-scanner.toml` until the 4.x bump removes it.
- "Overrides" section is to handle some current vulnerabilities in the dev dependencies.
  - `js-beautify: ^2.0.3` because `@vue/test-utils` still declares `^1.14.9`, whose `editorconfig`
    and `minimatch` chain holds the vulnerable `brace-expansion` (GHSA-mh99-v99m-4gvg). Only
    `brace-expansion` 5.0.8+ carries the fix and only `minimatch` 10 can consume it: 5.0.8 exports
    an object instead of a function, so forcing it under an older `minimatch` clears the audit but
    breaks at runtime with `expand is not a function`.
  - `postcss-selector-parser: ^7.1.6` because `tailwindcss` 3 and its `postcss-nested` still declare
    `^6.1.x`, and the fix for GHSA-rj75-hqrm-r3gf shipped only in 7.1.6, with no 6.x backport.
    `vite build` output is byte-identical under 6.1.4 and 7.1.6, but Tailwind 3 was never released
    against 7.x, so suspect this first if generated CSS looks wrong. Drop it with Tailwind 4.
- `npm-run-all2` replaces the unmaintained `npm-run-all`, which held `minimatch` 3 and with it the
  same vulnerable `brace-expansion`. Same `run-s` and `run-p` binaries, no script changes.

## Dependency Install Scripts

npm 12 blocks dependency install scripts unless the package is listed in the `allowScripts` field of
`package.json`. Builders on npm 10 (Ubuntu 22.04) never see this; newer ones warn on every `npm ci`,
and would fail outright under `--strict-allow-scripts`. All three packages that reach us are denied,
because none of their scripts do anything this project needs:

- `vue-demi` (via `reka-ui` and `@vueuse/*`) switches its entry files to the installed Vue major.
  The published `lib/index.mjs`, `lib/index.cjs` and `lib/index.d.ts` are already byte-identical to
  the `lib/v3/` variants, and we are on Vue 3, so the script rewrites those files with themselves.
  Only `lib/index.iife.js` differs, and that is the `unpkg`/`jsdelivr` entry, which no bundler uses.
- `@parcel/watcher` (via `sass`) compiles from source only when no prebuilt binary matches. The
  lockfile carries all twelve prebuilt platform packages.
- `core-js` (via `@vitejs/plugin-legacy`) prints a funding banner.

`false` denials are silent rather than merely blocked, and cannot be swept back in by
`npm install-scripts approve --all`. After dependency bumps, re-review with `npm install-scripts ls`
and add new entries rather than approving blindly.

## Formatting

CoolerControl uses [Trunk.io](https://github.com/trunk-io) to format all files for the entire
repository. The first time you run this, it may take a while as it downloads all the tools and
formatters needed for the project.

This will check if there are formatting or linting issues:

```bash
# cd to repository root directory
cd ..
make ci-check
```

This will auto-format all files. Afterward, commit any changes:

```bash
# cd to repository root directory
cd ..
make ci-fmt
```

## Internationalization / Languages

See the [language guide and files here](src/i18n/README.md).
