# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities **confidentially** via
[GitLab confidential issues](https://gitlab.com/coolercontrol/coolercontrol/-/issues/new?issue[confidential]=true).
Do **not** open a regular public issue.

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected version(s) and platform
- Impact or severity assessment
- Suggested fix, if available

### CVE Coordination

After a fix is released, we wait **30 days** before publishing a public security advisory to give
users and downstream repositories time to update.

When filing a confidential issue for a CVE request, please fill in the following template as best
you can. We will help with anything you are unsure of.

```yaml
vulnerability:
  description: 'TODO' # "[VULNTYPE] in [COMPONENT] in [VENDOR][PRODUCT] [VERSION] allows [ATTACKER] to [IMPACT] via [VECTOR]"
  cwe: 'TODO' # "CWE-22" # Find an appropriate CWE ID at https://cwe.mitre.org/index.html
  product:
    gitlab_path: 'coolercontrol/coolercontrol'
    vendor: 'CoolerControl'
    name: 'coolercontrold'
    affected_versions:
      - 'TODO' # "1.2.3"
      - 'TODO' # ">1.3.0, <=1.3.9"
    fixed_versions:
      - 'TODO' # "1.2.4"
      - 'TODO' # "1.3.10"
  impact: 'TODO' # CVSS v3.1 Base Score vector from https://nvd.nist.gov/vuln-metrics/cvss/v3-calculator
  solution: 'TODO' # "Upgrade to version 1.2.4 or 1.3.10"
  credit: 'TODO'
  references:
    - 'TODO' # "https://some.domain.tld/a/reference"
```

We appreciate responsible disclosure and will credit reporters in the CVE and release notes unless
you prefer to remain anonymous.
