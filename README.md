# vmcli

`vmcli` is a lightweight, provider-first node launcher.

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

## Credentials
- `ec2` / `lightsail`: AWS env credentials (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional `AWS_SESSION_TOKEN`)
- `gce`: active `gcloud` auth and project
- `droplet`: active `doctl` auth (`DIGITALOCEAN_TOKEN` or `doctl auth init`)

## Core Model
- User-facing input is `node` + optional `--region`
- `--scope` is internal grouping key (default: `ss2022`)
- Config and runtime state are separated:
  - Config: `<config-dir>/*.toml`
  - Runtime: `<state-dir>/<provider>/<scope>/<region>/...`

## Quick Start (EC2)
1) Initialize provider config and state key:
```bash
vmcli ec2 init
```

2) Edit config:
```bash
${EDITOR} ~/.config/vmcli/config/ec2.toml
```

3) Launch a node in a region:
```bash
vmcli ec2 up web-1 --region ap-northeast-1
```

4) Check status:
```bash
vmcli ec2 status --region ap-northeast-1 --json
```

5) Show machine-readable node info:
```bash
vmcli ec2 show web-1 --region ap-northeast-1 --json
```

6) SSH:
```bash
vmcli ec2 ssh web-1 --region ap-northeast-1
```

## Other Providers
```bash
vmcli lightsail init
vmcli lightsail up web-1 --region us-west-2
vmcli lightsail status --region us-west-2 --json
vmcli lightsail show web-1 --region us-west-2 --json
vmcli lightsail ssh web-1 --region us-west-2

vmcli gce init
vmcli gce up web-1 --region us-central1
vmcli gce status --region us-central1 --json
vmcli gce show web-1 --region us-central1 --json
vmcli gce ssh web-1 --region us-central1

vmcli droplet init
vmcli droplet up web-1 --region sfo3
vmcli droplet status --region sfo3 --json
vmcli droplet show web-1 --region sfo3 --json
vmcli droplet ssh web-1 --region sfo3
```

## SSH Config Include
Add this to `~/.ssh/config`:
```text
Include ~/.config/vmcli/state/*/*/*/ssh_config
```

## Commands
Global flags:
- `--root-dir` (default `~/.config/vmcli`)
- `--config-dir` (default `<root>/config`)
- `--state-dir` (default `<root>/state`)
- `--scope` (default `ss2022`)

Shared lifecycle commands:
```bash
vmcli [global flags] <provider> init
vmcli [global flags] <provider> up <name> [--region <region>] [provider flags]
vmcli [global flags] <provider> status [--region <region>] [--json]
vmcli [global flags] <provider> health <name> [--region <region>] [--json]
vmcli [global flags] <provider> show <name> --json [--region <region>]
vmcli [global flags] <provider> ssh <name> [--region <region>] [-- <remote-cmd>]
vmcli [global flags] <provider> reboot <name> [--region <region>]
vmcli [global flags] <provider> destroy <name> [--region <region>] [-f]
vmcli [global flags] <provider> prune [--region <region>] [-f]
```

Provider-specific `up` flags:
```bash
vmcli ec2 up <name> [-T|--instance-type <type>] [--region <region>]
vmcli lightsail up <name> [-B|--bundle-id <bundle>] [--region <region>]
vmcli gce up <name> [-M|--machine-type <type>] [--region <region>]
vmcli droplet up <name> [-S|--size <size>] [--region <region>]
```

Discovery commands:
```bash
vmcli ec2 regions [--json]
vmcli lightsail regions [--json]
vmcli gce regions [--json]
vmcli gce zones [--region <region>] [--json]
vmcli droplet regions [--json]
```

## Paths
Default root: `~/.config/vmcli`

Resolved defaults:
- `config-dir = <root>/config`
- `state-dir = <root>/state`

Persistent SSH key pair:
- Private: `<config-dir>/keys/vmcli`
- Public: `<config-dir>/keys/vmcli.pub`

Provider config files:
- `<config-dir>/ec2.toml`
- `<config-dir>/lightsail.toml`
- `<config-dir>/gce.toml`
- `<config-dir>/droplet.toml`

Runtime state files:
- `<state-dir>/<provider>/<scope>/<region>/ssh_config`
- `<state-dir>/<provider>/<scope>/<region>/nodes/<node>.json`

## Config Examples
`ec2.toml`:
```toml
[defaults]
region = "ap-northeast-1"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli.pub"
default_instance_type = "t3.micro"
ami_id = ""

[scopes.ss2022]
region = "ap-northeast-1"
```

`lightsail.toml`:
```toml
[defaults]
region = "ap-northeast-1"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli.pub"
default_bundle_id = "nano_3_0"
blueprint_id = "ubuntu_24_04"
key_pair_name = ""

[scopes.ss2022]
region = "ap-northeast-1"
availability_zone = "ap-northeast-1a"
```

`gce.toml`:
```toml
[defaults]
region = "asia-northeast1"
project = "my-gcp-project"
zone = "asia-northeast1-a"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli.pub"
default_machine_type = "e2-micro"
image_family = "ubuntu-2404-lts-amd64"
image_project = "ubuntu-os-cloud"
ssh_user = "ubuntu"
```

`droplet.toml`:
```toml
[defaults]
region = "sfo3"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli.pub"
default_size = "s-1vcpu-1gb"
image = "ubuntu-24-04-x64"
ssh_key_fingerprint = ""
```

## Notes
- `ec2` and `lightsail` reject `AWS_PROFILE` / `AWS_DEFAULT_PROFILE`.
- `ec2 health` supports `--os-user` for EC2 Instance Connect probing.
- `lightsail up` configures public TCP ports `22`, `80`, and `443` by default.
