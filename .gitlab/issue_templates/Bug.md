# Bug

## Environment

- CoolerControl version:
- Linux Distribution name and version:
- Affected device(s):
- (any additional relevant information)

## Description

Describe the issue you are experiencing and steps needed to reproduce it.

## Logs and/or Screenshots

It's best to give us as much information as possible and we will often need enough log output to
understand the context of the issue, rather than just a few error lines. Run the following and
attach the output file to this Issue:

```bash
journalctl --no-pager -u coolercontrold -u coolercontrol-liqctld -n 10000 > ~/Documents/coolercontrol-daemons.log
```

_Alternative:_ if you have an issue with the above you can run the following and copy & paste the
output in a code block below:

```bash
journalctl -e -u coolercontrold -u coolercontrol-liqctld
```

## Checklist

- [ ] I have not found another existing issue that deals with this problem.
- [ ] I have filled out all sections of this template
- [ ] I have attached log output and/or screenshots
- [ ] I have read the
      [Hardware Support](https://gitlab.com/coolercontrol/coolercontrol#-hardware-support) section
      of the readme and applied all available steps.
- [x] I have not read any of the above

**Note:** CoolerControl depends on open source drivers to communicate with your hardware. If there
is an issue controlling your fans, then likely there is an issue with your currently installed
kernel drivers. See [Hardware Support](https://docs.coolercontrol.org/hardware-support.html) and
[Adding Device Support](https://docs.coolercontrol.org/wiki/adding-device-support.html).

/label ~"type::Bug"
