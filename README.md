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
1) Initialize provider config and local state key:
```bash
vmcli ec2 init
```

2) Edit provider config:
```bash
${EDITOR} ~/.config/vmcli/config/ec2.toml
```

3) Add your cluster mapping in `ec2.toml`:
```toml
[clusters.dev-cluster]
region = "ap-northeast-1"
```

4) Create an instance:
```bash
vmcli ec2 up dev-cluster web-1
```

5) Check status:
```bash
vmcli ec2 status dev-cluster --json
```

6) Diagnose health:
```bash
vmcli ec2 health dev-cluster web-1 --json
```

## Other Providers
```bash
vmcli lightsail init
vmcli lightsail up dev-ls web-1
vmcli lightsail status dev-ls --json

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
Include ~/.config/vmcli/state/*/*/ssh_config
```

If you use custom dirs, update the include path accordingly.

## Commands
Shared lifecycle commands:

```bash
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> init
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> up <cluster> <name> [provider flags]
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> status <cluster> [--json]
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> health <cluster> <name> [--json]
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> show <cluster> <name> --json
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> ssh <cluster> <name> [-- <remote-cmd>]
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> reboot <cluster> <name>
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> destroy <cluster> <name> [-f]
vmcli [--root-dir <dir>] [--config-dir <dir>] [--state-dir <dir>] <provider> prune <cluster> [-f]
```

Where `<provider>` is one of: `ec2`, `lightsail`, `gce`, `droplet`.

Provider-specific `up` flags:
```bash
vmcli ec2 up <cluster> <name> [-T|--instance-type <type>]
vmcli lightsail up <cluster> <name> [-B|--bundle-id <bundle>]
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
Default root: `~/.config/vmcli`

Resolved defaults:
- `config-dir = <root>/config`
- `state-dir = <root>/state`

Runtime key pair is managed in state dir:
- Private: `<state-dir>/keys/vmcli`
- Public: `<state-dir>/keys/vmcli.pub`

Provider config files (EC2/Lightsail):
- `<config-dir>/ec2.toml`
- `<config-dir>/lightsail.toml`

Runtime state files:
- `<state-dir>/ec2/<cluster>/ssh_config`
- `<state-dir>/ec2/<cluster>/nodes/<node>.json`
- `<state-dir>/lightsail/<cluster>/ssh_config`
- `<state-dir>/lightsail/<cluster>/nodes/<node>.json`

### Example: `ec2.toml`
```toml
[defaults]
ssh_public_key_path = "~/.config/vmcli/state/keys/vmcli.pub"
default_instance_type = "t3.micro"
ami_id = ""

[clusters.dev]
region = "ap-northeast-1"
[clusters.prod]
region = "us-east-1"
```

### Example: `lightsail.toml`
```toml
[defaults]
ssh_public_key_path = "~/.config/vmcli/state/keys/vmcli.pub"
default_bundle_id = "nano_3_0"
blueprint_id = "ubuntu_24_04"
key_pair_name = ""

[clusters.dev]
region = "ap-northeast-1"
availability_zone = "ap-northeast-1a"
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
