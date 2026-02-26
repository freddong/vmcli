# vmcli

`vmcli` is a lightweight, Vagrant-like CLI for managing EC2 instances inside a single cluster VPC using the AWS CLI. It is provider-first (`vmcli aws ...`) to leave room for additional clouds later.

## Requirements
- Rust toolchain
- AWS CLI v2 in `PATH`
- AWS credentials via environment variables (no profiles)
- `gitleaks` (recommended for local commit secret scanning)

## Install (Prebuilt Binaries)
`vmcli` releases include prebuilt binaries for:
- Linux `x86_64`
- macOS Apple Silicon (`aarch64`, M1/M2/M3)

Download the matching `.tar.gz` from GitHub Releases, extract it, and place `vmcli` in your `PATH`.

Runtime requirement still applies: AWS CLI v2 must be installed in `PATH`.

## Enable Git Hooks
Use the repo-managed hook so commits are scanned for secrets before they are created:

```bash
git config core.hooksPath .githooks
```

The hook runs `gitleaks` against staged changes and blocks the commit if a secret is detected.

## Quick Start
1) Initialize a cluster:
```bash
vmcli aws init dev-cluster
```

2) Edit the cluster config:
```bash
${EDITOR} ~/.config/vmcli/aws/dev-cluster/config.toml
```

3) Create an instance:
```bash
vmcli aws up dev-cluster web-1
```

4) Check status and generate SSH config:
```bash
vmcli aws status dev-cluster
```

5) Connect:
```bash
ssh web-1
```

6) Diagnose instance health when local SSH is flaky:
```bash
vmcli aws health dev-cluster web-1
```

## SSH Config Include
Add this to `~/.ssh/config` (top-level, not inside a `Host` block):
```
Include ~/.config/vmcli/aws/*/ssh_config
```

Each `ssh_config` entry uses:
```
Host <name>
  HostName <public-ip>
  User ubuntu
  IdentitiesOnly yes
  IdentityFile <private-key>
```

The identity file is derived by stripping `.pub` from `ssh_public_key_path`.

## Commands
```bash
vmcli aws init <cluster>
vmcli aws up <cluster> <name> [-T <instance-type>] [-c <config>]
vmcli aws status <cluster> [-c <config>]
vmcli aws health <cluster> <name> [-c <config>] [--os-user <user>]
vmcli aws reboot <cluster> <name> [-c <config>]
vmcli aws destroy <cluster> <name> [-f] [-c <config>]
vmcli aws prune <cluster> [-f] [-c <config>]
```

## Configuration
Config is centralized under `~/.config/vmcli`.

### Global defaults
`~/.config/vmcli/config.toml`:
```toml
[aws]
region = "ap-northeast-1"
ssh_public_key_path = "/home/me/.ssh/vmcli.pub"
default_instance_type = "t3.micro"
```

### Cluster config
`~/.config/vmcli/aws/<cluster>/config.toml`:
```toml
cluster_name = "dev-cluster"

[aws]
region = "ap-northeast-1"
ssh_public_key_path = "/home/me/.ssh/vmcli.pub"
default_instance_type = "t3.micro"
ami_id = "" # optional, blank => Ubuntu 24.04 via SSM
```

Notes:
- Global config is loaded first; cluster config overrides it.
- `-c <config>` uses the provided cluster config path but still merges global defaults.
- `cluster_name` is optional; if set, it must match `<cluster>`.
- `ssh_config` is always written to `~/.config/vmcli/aws/<cluster>/ssh_config`.

## Credential Handling
`vmcli` reads credentials from environment variables:
- `AWS_ACCESS_KEY_ID`
- `AWS_SECRET_ACCESS_KEY`
- `AWS_SESSION_TOKEN` (optional)
- `AWS_REGION` / `AWS_DEFAULT_REGION`

AWS profiles are not supported.

## Health Checks (`vmcli aws health`)
`vmcli aws health <cluster> <name>` helps verify whether an instance is healthy even when your local SSH path is failing.

It checks:
- EC2 instance state + status checks
- EC2 Instance Connect diagnostics (security group port 22 + `send-ssh-public-key` probe, no actual SSH session)

Examples:
```bash
vmcli aws health dev-cluster web-1
vmcli aws health dev-cluster web-1 --os-user ec2-user
```

IAM permissions (minimum capability groups):
- `ec2:Describe*` (instances, instance status, security groups)
- `ec2-instance-connect:SendSSHPublicKey`

Notes / limits:
- `health` is information-first: degraded health still returns output (and usually exit code `0`) unless the command itself fails (e.g. auth/permission/config errors).
- EC2 Instance Connect probe currently assumes a public IP path; EC2 Instance Connect Endpoint (private subnet path) is not yet implemented.
- `health` does not perform an actual SSH login. It only validates the AWS-side prerequisites and probes.

## Behavior Notes
- VPC/SG/subnet/IGW/route-table are created and tagged by cluster.
- `up` fails fast if a non-terminated instance with the same `Name` exists.
- `health` targets instances by both `Name` and `Cluster` tags.
- `reboot` targets instances by `Name` tag.
- `destroy` only targets instances by `Name` tag.
- `prune` deletes VPC resources when no instances remain; `-f` also removes the key pair.

## Releasing (Maintainers)
GitHub Actions builds release artifacts for Linux `x86_64` and macOS Apple Silicon on tag pushes matching `v*`.

Typical release flow:
```bash
# update version in Cargo.toml first
git tag v0.1.0
git push origin v0.1.0
```

The workflow uploads `.tar.gz` archives to the corresponding GitHub Release.
