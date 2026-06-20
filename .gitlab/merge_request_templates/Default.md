# Merge Request

## Before you submit

- **Adding a new feature?** Please open an issue to discuss scope and approach **before** writing
  code, especially for anything touching core functionality or the existing architecture. This
  avoids wasted effort on both sides. See
  [CONTRIBUTING](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md) and the
  [project Vision](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/VISION.md).
- **Used AI assistance?** That's fine, but **you** are responsible for the code: you must understand
  every change, have built and tested it yourself, and disclose AI use (and roughly where) in this
  description. Purely machine-generated MRs the author cannot explain may be closed. See
  [AI-Assisted Contributions](https://gitlab.com/coolercontrol/coolercontrol/-/blob/main/CONTRIBUTING.md#ai-assisted-contributions).
- **Adding liquidctl device support?** New device drivers live in
  [`supported_devices/`](https://gitlab.com/coolercontrol/coolercontrol/-/tree/main/coolercontrold/daemon/src/repositories/liquidctl/supported_devices).
  Follow the conventions in the existing driver modules: implement the `DeviceSupport` trait and
  reuse its default methods where possible instead of reimplementing parsing and clamping, register
  the module in `mod.rs`, and add unit tests like the existing devices. Include status output from
  your actual hardware so behavior can be verified.

## Description

## Related issues

## Checklist

- [ ] Application works as expected when running from source
- [ ] Dependency changes are intentional and minimal (or none)
- [ ] Liquidctl device tests are successful (if applicable)
- [ ] Any needed documentation changes have been made
- [ ] This MR is ready to be merged

/assign me

/assign_reviewer @codifryed
