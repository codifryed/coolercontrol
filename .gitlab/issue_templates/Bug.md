# Bug

## Environment

- CoolerControl version:
- Linux Distribution name and version:
- Affected device(s):
- (any additional relevant information)

## Description

Describe the issue you are experiencing and steps needed to reproduce it.

## Logs and/or Screenshots

It's best to give us as much info as possible and we will often need a whole range of log output to
understand the context of the issue, rather than just a few error lines. Run the following and
attach the output file to this Issue:

```bash
journalctl --no-pager -u coolercontrold -u coolercontrol-liqctld -n 10000 > ~/Documents/coolercontrol-daemons.log
```

Alternatively, if you have an issue with the above you can read the
[Wiki Page](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/log-output-&-debugging) on how to
get logs and paste the output below:

```text
paste log output here (or attach a file)
```

## Checklist

- [ ] I have not found another existing issue that deals with this problem.
- [ ] I have filled out all sections of this template
- [ ] I have attached log output and/or screenshots
- [ ] I have read the
      [Hardware Support](https://gitlab.com/coolercontrol/coolercontrol#why-is-my-hardware-not-showingworking)
      section of the readme and applied all available steps.

**Note:** CoolerControl depends on open source drivers to communicate with your hardware. If there
is an issue controlling your fans, then likely your currently installed kernel drivers to not
support it. See
[Adding Device Support](https://gitlab.com/coolercontrol/coolercontrol/-/wikis/adding-device-support)

/label ~"type::Bug"
