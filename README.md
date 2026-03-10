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
- `droplet`: active `doctl` auth (`DIGITALOCEAN_ACCESS_TOKEN` preferred, `DIGITALOCEAN_TOKEN` also supported, or `doctl auth init`)

## Core Model
- User-facing input is `node` + optional `--region`
- One `config-dir` + `state-dir` binds to exactly one workspace `project`
- `project` defaults to `vmcli`; set it explicitly during init only when needed
- `up` requires explicit `--region` for all providers
- `status` without `--region` lists all known regions for the project
- Node-targeting commands (`show`/`ssh`/`health`/`reboot`/`destroy`) without `--region` resolve region by node name via live cloud queries
- Config and runtime state are separated:
  - Config: `<config-dir>/*.toml`
  - Runtime: `<state-dir>/<project>/<provider>/<region>/ssh_config`

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

Shared lifecycle commands:
```bash
vmcli [global flags] <provider> init [--project <project>]   # default project: vmcli
vmcli [global flags] <provider> up <name> --region <region> [provider flags]
vmcli [global flags] <provider> status [--region <region>] [--json]
vmcli [global flags] <provider> health <name> [--region <region>] [--json]
vmcli [global flags] <provider> show <name> --json [--region <region>]
vmcli [global flags] <provider> ssh <name> [--region <region>] [-- <remote-cmd>]
vmcli [global flags] <provider> reboot <name> [--region <region>]
vmcli [global flags] <provider> destroy <name> [--region <region>] [-f]
vmcli [global flags] <provider> prune --region <region> [-f]
```

Provider-specific `up` flags:
```bash
vmcli ec2 up <name> --region <region> [-T|--instance-type <type>]
vmcli lightsail up <name> --region <region> [-B|--bundle-id <bundle>]
vmcli gce up <name> --region <region> [-M|--machine-type <type>]
vmcli droplet up <name> --region <region> [-S|--size <size>]
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
- Private: `<config-dir>/keys/vmcli-<project-slug>`
- Public: `<config-dir>/keys/vmcli-<project-slug>.pub`

Provider config files:
- `<config-dir>/ec2.toml`
- `<config-dir>/lightsail.toml`
- `<config-dir>/gce.toml`
- `<config-dir>/droplet.toml`

Runtime state files:
- `<state-dir>/<project>/<provider>/<region>/ssh_config`

Workspace binding:
- `<config-dir>/workspace.toml`

## Config Examples
`ec2.toml`:
```toml
[defaults]
region = "ap-northeast-1"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli-<project-slug>.pub"
default_instance_type = "t3.micro"
ami_id = ""
```

`lightsail.toml`:
```toml
[defaults]
region = "ap-northeast-1"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli-<project-slug>.pub"
default_bundle_id = "nano_3_0"
blueprint_id = "ubuntu_24_04"
key_pair_name = "vmcli-<project-slug>"
```
`availability_zone` is optional; if provided, it must match the resolved `region`.
New `init` configs write both `ssh_public_key_path` and Lightsail `key_pair_name` from `workspace.project`, e.g. `project = "vmcli"` -> `vmcli-vmcli`, `project = "vms"` -> `vmcli-vms`. Keep explicit existing values to preserve legacy deployments.

`gce.toml`:
```toml
[defaults]
region = "asia-northeast1"
project = "my-gcp-project"
zone = "asia-northeast1-a"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli-<project-slug>.pub"
default_machine_type = "e2-micro"
image_family = "ubuntu-2404-lts-amd64"
image_project = "ubuntu-os-cloud"
ssh_user = "ubuntu"
```
`zone` is optional; if provided, it must match `region`.

`droplet.toml`:
```toml
[defaults]
region = "sfo3"
ssh_public_key_path = "~/.config/vmcli/config/keys/vmcli-<project-slug>.pub"
default_size = "s-1vcpu-512mb-10gb"
image = "ubuntu-24-04-x64"
ssh_user = "root"
ssh_key_fingerprint = ""
```

`workspace.toml`:
```toml
[workspace]
project = "vmcli"
```

## Notes
- `ec2` and `lightsail` reject `AWS_PROFILE` / `AWS_DEFAULT_PROFILE`.
- `ec2 health` supports `--os-user` for EC2 Instance Connect probing.
- `lightsail up` configures public TCP ports `22`, `80`, and `443` by default.
- `lightsail up` ensures the configured key pair exists in Lightsail, verifies it matches the local public key when reusing a name, and always binds it on instance create.
- Default local SSH key files also follow `vmcli-<project-slug>` when `ssh_public_key_path` is omitted; an explicit `ssh_public_key_path` keeps the old local key path unchanged.
- `lightsail` defaults its remote key pair name to `vmcli-<project-slug>` only when `key_pair_name` is omitted; an explicit `key_pair_name` keeps the old behavior unchanged.
- `vmcli` default key generation uses RSA (`ssh-rsa`) for broader Lightsail compatibility.
- Managed resources are tagged/labeled by workspace project:
  - AWS/GCE/Lightsail: `vms=<project-slug>`
  - DigitalOcean: tag `vms-<project-slug>`
- `prune` operates on `vms`-managed resources in the target region and skips resources with in-use instances.
