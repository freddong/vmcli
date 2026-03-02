# vmcli

`vmcli` is a lightweight, provider-first CLI for creating and managing small VM clusters.

Supported providers:
- `ec2`
- `lightsail`
- `gce`
- `droplet`

## Requirements
- Rust toolchain
- Provider CLIs (install what you use):
  - AWS CLI v2 (`ec2`, `lightsail`)
  - Google Cloud SDK `gcloud` (`gce`)
  - DigitalOcean `doctl` (`droplet`)
- `gitleaks` (recommended for local commit secret scanning)

## Credentials
- `ec2` / `lightsail`: AWS env credentials (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`)
- `gce`: active `gcloud` auth and project
- `droplet`: active `doctl` auth (`DIGITALOCEAN_TOKEN` or `doctl auth init`)

## Quick Start (EC2)
1) Initialize a cluster:
```bash
vmcli ec2 init dev-cluster
```

2) Edit the cluster config:
```bash
${EDITOR} ~/.config/vmcli/ec2/dev-cluster/config.toml
```

3) Create an instance:
```bash
vmcli ec2 up dev-cluster web-1
```

4) Check status:
```bash
vmcli ec2 status dev-cluster
```

5) Diagnose health:
```bash
vmcli ec2 health dev-cluster web-1
```

## Other Providers
```bash
vmcli lightsail init dev-ls
vmcli lightsail up dev-ls web-1
vmcli lightsail status dev-ls

vmcli gce init dev-gce
vmcli gce up dev-gce web-1
vmcli gce status dev-gce

vmcli droplet init dev-do
vmcli droplet up dev-do web-1
vmcli droplet status dev-do
```

## SSH Config Include
Add this to `~/.ssh/config` (top-level):
```
Include ~/.config/vmcli/*/*/ssh_config
```

If you use `--config-dir`, update the include path accordingly.

## Commands
Shared lifecycle commands:

```bash
vmcli [--config-dir <dir>] <provider> init <cluster>
vmcli [--config-dir <dir>] <provider> up <cluster> <name> [-c <config>]
vmcli [--config-dir <dir>] <provider> status <cluster> [-c <config>]
vmcli [--config-dir <dir>] <provider> health <cluster> <name> [-c <config>]
vmcli [--config-dir <dir>] <provider> reboot <cluster> <name> [-c <config>]
vmcli [--config-dir <dir>] <provider> destroy <cluster> <name> [-f] [-c <config>]
vmcli [--config-dir <dir>] <provider> prune <cluster> [-f] [-c <config>]
```

Where `<provider>` is one of: `ec2`, `lightsail`, `gce`, `droplet`.

Provider-specific `up` flags:
```bash
vmcli ec2 up <cluster> <name> [-T|--instance-type <type>] [-c <config>]
vmcli lightsail up <cluster> <name> [-B|--bundle-id <bundle>] [-c <config>]
vmcli gce up <cluster> <name> [-M|--machine-type <type>] [-c <config>]
vmcli droplet up <cluster> <name> [-S|--size <size>] [-c <config>]
```

Region/zone discovery commands:
```bash
vmcli ec2 regions [--json]
vmcli lightsail regions [--json]
vmcli gce regions [--json]
vmcli gce zones [--region <region>] [--json]
vmcli droplet regions [--json]
```

## Configuration
Default config root: `~/.config/vmcli`

Local SSH key pair is managed in config dir:
- Private: `<config-dir>/vmcli`
- Public: `<config-dir>/vmcli.pub`

Per-provider cluster config paths:
- `~/.config/vmcli/ec2/<cluster>/config.toml`
- `~/.config/vmcli/lightsail/<cluster>/config.toml`
- `~/.config/vmcli/gce/<cluster>/config.toml`
- `~/.config/vmcli/droplet/<cluster>/config.toml`

Global defaults file: `~/.config/vmcli/config.toml`

### Example: EC2
```toml
[ec2]
region = "ap-northeast-1"
ssh_public_key_path = "~/.config/vmcli/vmcli.pub"
default_instance_type = "t3.micro"
```

### Example: Lightsail
```toml
[lightsail]
region = "ap-northeast-1"
availability_zone = "ap-northeast-1a"
ssh_public_key_path = "~/.config/vmcli/vmcli.pub"
default_bundle_id = "nano_3_0"
blueprint_id = "ubuntu_24_04"
key_pair_name = ""
```

### Example: GCE
```toml
[gce]
project = "my-gcp-project"
zone = "asia-northeast1-a"
ssh_public_key_path = "~/.config/vmcli/vmcli.pub"
default_machine_type = "e2-micro"
image_family = "ubuntu-2404-lts-amd64"
image_project = "ubuntu-os-cloud"
ssh_user = "ubuntu"
```

### Example: Droplet
```toml
[droplet]
region = "sfo3"
ssh_public_key_path = "~/.config/vmcli/vmcli.pub"
default_size = "s-1vcpu-1gb"
image = "ubuntu-24-04-x64"
ssh_key_fingerprint = ""
```

## Notes
- EC2 legacy `[aws]` config sections are still accepted as aliases for `[ec2]`.
- `ec2` and `lightsail` reject `AWS_PROFILE` / `AWS_DEFAULT_PROFILE` to keep env-based credential behavior consistent.
- `ec2 health` supports `--os-user` for EC2 Instance Connect probing.
- `gce` uses a sanitized cluster label for filtering and lifecycle operations.
- `droplet` uses a cluster tag (`cluster-<normalized-cluster>`).
- `lightsail up` configures public TCP ports `22`, `80`, and `443` by default.
- `prune` asks whether to remove local provider cluster config directory when no instances remain.

## Releasing (Maintainers)
GitHub Actions builds release artifacts for Linux `x86_64` and macOS Apple Silicon on tag pushes matching `v*`.

Typical release flow:
```bash
# update version in Cargo.toml first
git tag v0.1.0
git push origin v0.1.0
```
