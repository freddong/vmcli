use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

const CONFIG_FILE_NAME: &str = "config.toml";
const SSH_CONFIG_FILE: &str = "ssh_config";
const EC2_PROVIDER: &str = "ec2";
const LIGHTSAIL_PROVIDER: &str = "lightsail";
const GCE_PROVIDER: &str = "gce";
const DROPLET_PROVIDER: &str = "droplet";
const DEFAULT_INSTANCE_TYPE: &str = "t3.micro";
const DEFAULT_INSTANCE_OS_USER: &str = "ubuntu";
const DEFAULT_CONFIG_DIR: &str = "~/.config/vmcli";
const DEFAULT_LIGHTSAIL_BUNDLE_ID: &str = "nano_3_0";
const DEFAULT_LIGHTSAIL_BLUEPRINT_ID: &str = "ubuntu_24_04";
const DEFAULT_GCE_MACHINE_TYPE: &str = "e2-micro";
const DEFAULT_GCE_IMAGE_FAMILY: &str = "ubuntu-2404-lts-amd64";
const DEFAULT_GCE_IMAGE_PROJECT: &str = "ubuntu-os-cloud";
const DEFAULT_GCE_SSH_USER: &str = "ubuntu";
const DEFAULT_DROPLET_SIZE: &str = "s-1vcpu-1gb";
const DEFAULT_DROPLET_IMAGE: &str = "ubuntu-24-04-x64";
const UBUNTU_2404_AMI_SSM: &str =
    "/aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id";
const NON_TERMINATED_STATES: &str = "pending,running,stopping,stopped,shutting-down";

#[derive(Parser)]
#[command(name = "vmcli", version, about = "vmcli multi-cloud helper")]
struct Cli {
    #[arg(long = "config-dir", global = true, default_value = DEFAULT_CONFIG_DIR)]
    config_dir: String,
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    Ec2(Ec2Args),
    Lightsail(LightsailArgs),
    Gce(GceArgs),
    Droplet(DropletArgs),
}

#[derive(Args)]
struct Ec2Args {
    #[command(subcommand)]
    command: Ec2Command,
}

#[derive(Args)]
struct LightsailArgs {
    #[command(subcommand)]
    command: LightsailCommand,
}

#[derive(Args)]
struct GceArgs {
    #[command(subcommand)]
    command: GceCommand,
}

#[derive(Args)]
struct DropletArgs {
    #[command(subcommand)]
    command: DropletCommand,
}

#[derive(Subcommand)]
enum Ec2Command {
    Init(InitArgs),
    Up(Ec2UpArgs),
    Status(StatusArgs),
    Health(Ec2HealthArgs),
    Reboot(RebootArgs),
    Destroy(DestroyArgs),
    Prune(PruneArgs),
    Regions(ListRegionsArgs),
}

#[derive(Subcommand)]
enum LightsailCommand {
    Init(InitArgs),
    Up(LightsailUpArgs),
    Status(StatusArgs),
    Health(HealthArgs),
    Reboot(RebootArgs),
    Destroy(DestroyArgs),
    Prune(PruneArgs),
    Regions(ListRegionsArgs),
}

#[derive(Subcommand)]
enum GceCommand {
    Init(InitArgs),
    Up(GceUpArgs),
    Status(StatusArgs),
    Health(HealthArgs),
    Reboot(RebootArgs),
    Destroy(DestroyArgs),
    Prune(PruneArgs),
    Regions(ListRegionsArgs),
    Zones(GceZonesArgs),
}

#[derive(Subcommand)]
enum DropletCommand {
    Init(InitArgs),
    Up(DropletUpArgs),
    Status(StatusArgs),
    Health(HealthArgs),
    Reboot(RebootArgs),
    Destroy(DestroyArgs),
    Prune(PruneArgs),
    Regions(ListRegionsArgs),
}

#[derive(Args)]
struct InitArgs {
    cluster: String,
}

#[derive(Args)]
struct StatusArgs {
    cluster: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct HealthArgs {
    cluster: String,
    name: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct RebootArgs {
    cluster: String,
    name: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct DestroyArgs {
    cluster: String,
    name: String,
    #[arg(short = 'f', long = "force")]
    force: bool,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct PruneArgs {
    cluster: String,
    #[arg(short = 'f', long = "force")]
    force: bool,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct Ec2UpArgs {
    cluster: String,
    name: String,
    #[arg(short = 'T', long = "instance-type")]
    instance_type: Option<String>,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct Ec2HealthArgs {
    cluster: String,
    name: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
    #[arg(long = "os-user", default_value = DEFAULT_INSTANCE_OS_USER)]
    os_user: String,
}

#[derive(Args)]
struct LightsailUpArgs {
    cluster: String,
    name: String,
    #[arg(short = 'B', long = "bundle-id")]
    bundle_id: Option<String>,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct GceUpArgs {
    cluster: String,
    name: String,
    #[arg(short = 'M', long = "machine-type")]
    machine_type: Option<String>,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct DropletUpArgs {
    cluster: String,
    name: String,
    #[arg(short = 'S', long = "size")]
    size: Option<String>,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct ListRegionsArgs {
    #[arg(long = "json")]
    json: bool,
}

#[derive(Args)]
struct GceZonesArgs {
    #[arg(long = "region")]
    region: Option<String>,
    #[arg(long = "json")]
    json: bool,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct GlobalConfig {
    #[serde(alias = "aws")]
    ec2: Option<AwsConfigSection>,
    lightsail: Option<LightsailConfigSection>,
    gce: Option<GceConfigSection>,
    droplet: Option<DropletConfigSection>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct ClusterConfig {
    cluster_name: Option<String>,
    #[serde(alias = "aws")]
    ec2: Option<AwsConfigSection>,
    lightsail: Option<LightsailConfigSection>,
    gce: Option<GceConfigSection>,
    droplet: Option<DropletConfigSection>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct AwsConfigSection {
    region: Option<String>,
    ssh_public_key_path: Option<String>,
    default_instance_type: Option<String>,
    ami_id: Option<String>,
}

#[derive(Debug, Clone)]
struct AwsEffectiveConfig {
    cluster_name: String,
    region: String,
    ssh_public_key_path: String,
    default_instance_type: String,
    ami_id: Option<String>,
    ssh_config_path: PathBuf,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct LightsailConfigSection {
    region: Option<String>,
    ssh_public_key_path: Option<String>,
    availability_zone: Option<String>,
    default_bundle_id: Option<String>,
    blueprint_id: Option<String>,
    key_pair_name: Option<String>,
}

#[derive(Debug, Clone)]
struct LightsailEffectiveConfig {
    cluster_name: String,
    region: String,
    ssh_public_key_path: String,
    availability_zone: String,
    default_bundle_id: String,
    blueprint_id: String,
    key_pair_name: Option<String>,
    ssh_config_path: PathBuf,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct GceConfigSection {
    project: Option<String>,
    zone: Option<String>,
    ssh_public_key_path: Option<String>,
    default_machine_type: Option<String>,
    image_family: Option<String>,
    image_project: Option<String>,
    ssh_user: Option<String>,
}

#[derive(Debug, Clone)]
struct GceEffectiveConfig {
    cluster_name: String,
    project: String,
    zone: String,
    ssh_public_key_path: String,
    default_machine_type: String,
    image_family: String,
    image_project: String,
    ssh_user: String,
    ssh_config_path: PathBuf,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct DropletConfigSection {
    region: Option<String>,
    ssh_public_key_path: Option<String>,
    default_size: Option<String>,
    image: Option<String>,
    ssh_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
struct DropletEffectiveConfig {
    cluster_name: String,
    region: String,
    ssh_public_key_path: String,
    default_size: String,
    image: String,
    ssh_key_fingerprint: Option<String>,
    ssh_config_path: PathBuf,
}

#[derive(Deserialize)]
struct CallerIdentity {
    #[serde(rename = "Account")]
    account: String,
    #[serde(rename = "Arn")]
    arn: String,
}

#[derive(Deserialize)]
struct DescribeVpcs {
    #[serde(rename = "Vpcs")]
    vpcs: Vec<Vpc>,
}

#[derive(Deserialize)]
struct Vpc {
    #[serde(rename = "VpcId")]
    vpc_id: String,
}

#[derive(Deserialize)]
struct DescribeSubnets {
    #[serde(rename = "Subnets")]
    subnets: Vec<Subnet>,
}

#[derive(Deserialize)]
struct Subnet {
    #[serde(rename = "SubnetId")]
    subnet_id: String,
}

#[derive(Deserialize)]
struct DescribeInternetGateways {
    #[serde(rename = "InternetGateways")]
    internet_gateways: Vec<InternetGateway>,
}

#[derive(Deserialize)]
struct InternetGateway {
    #[serde(rename = "InternetGatewayId")]
    internet_gateway_id: String,
    #[serde(rename = "Attachments")]
    attachments: Option<Vec<InternetGatewayAttachment>>,
}

#[derive(Deserialize)]
struct InternetGatewayAttachment {
    #[serde(rename = "VpcId")]
    vpc_id: Option<String>,
}

#[derive(Deserialize)]
struct DescribeRouteTables {
    #[serde(rename = "RouteTables")]
    route_tables: Vec<RouteTable>,
}

#[derive(Deserialize)]
struct RouteTable {
    #[serde(rename = "RouteTableId")]
    route_table_id: String,
    #[serde(rename = "Associations")]
    associations: Option<Vec<RouteTableAssociation>>,
}

#[derive(Deserialize)]
struct RouteTableAssociation {
    #[serde(rename = "SubnetId")]
    subnet_id: Option<String>,
    #[serde(rename = "RouteTableAssociationId")]
    association_id: Option<String>,
    #[serde(rename = "Main")]
    main: Option<bool>,
}

#[derive(Deserialize)]
struct DescribeSecurityGroups {
    #[serde(rename = "SecurityGroups")]
    security_groups: Vec<SecurityGroup>,
}

#[derive(Deserialize)]
struct SecurityGroup {
    #[serde(rename = "GroupId")]
    group_id: String,
    #[serde(rename = "IpPermissions")]
    ip_permissions: Option<Vec<IpPermission>>,
}

#[derive(Deserialize)]
struct IpPermission {
    #[serde(rename = "IpProtocol")]
    ip_protocol: Option<String>,
    #[serde(rename = "FromPort")]
    from_port: Option<i64>,
    #[serde(rename = "ToPort")]
    to_port: Option<i64>,
    #[serde(rename = "IpRanges")]
    ip_ranges: Option<Vec<IpRange>>,
    #[serde(rename = "Ipv6Ranges")]
    ipv6_ranges: Option<Vec<Ipv6Range>>,
    #[serde(rename = "UserIdGroupPairs")]
    user_id_group_pairs: Option<Vec<UserIdGroupPair>>,
    #[serde(rename = "PrefixListIds")]
    prefix_list_ids: Option<Vec<PrefixListId>>,
}

#[derive(Deserialize)]
struct IpRange {
    #[serde(rename = "CidrIp")]
    cidr_ip: Option<String>,
}

#[derive(Deserialize)]
struct Ipv6Range {
    #[serde(rename = "CidrIpv6")]
    cidr_ipv6: Option<String>,
}

#[derive(Deserialize)]
struct UserIdGroupPair {
    #[serde(rename = "GroupId")]
    group_id: Option<String>,
}

#[derive(Deserialize)]
struct PrefixListId {
    #[serde(rename = "PrefixListId")]
    prefix_list_id: Option<String>,
}

#[derive(Deserialize)]
struct DescribeInstances {
    #[serde(rename = "Reservations")]
    reservations: Vec<Reservation>,
}

#[derive(Deserialize)]
struct Reservation {
    #[serde(rename = "Instances")]
    instances: Vec<Instance>,
}

#[derive(Deserialize)]
struct Instance {
    #[serde(rename = "InstanceId")]
    instance_id: String,
    #[serde(rename = "State")]
    state: InstanceState,
    #[serde(rename = "Placement")]
    placement: Option<InstancePlacement>,
    #[serde(rename = "VpcId")]
    vpc_id: Option<String>,
    #[serde(rename = "SubnetId")]
    subnet_id: Option<String>,
    #[serde(rename = "PublicIpAddress")]
    public_ip: Option<String>,
    #[serde(rename = "PrivateIpAddress")]
    private_ip: Option<String>,
    #[serde(rename = "SecurityGroups")]
    security_groups: Option<Vec<InstanceSecurityGroupRef>>,
    #[serde(rename = "Tags")]
    tags: Option<Vec<Tag>>,
}

#[derive(Deserialize)]
struct InstanceState {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(Deserialize)]
struct InstancePlacement {
    #[serde(rename = "AvailabilityZone")]
    availability_zone: Option<String>,
}

#[derive(Deserialize)]
struct InstanceSecurityGroupRef {
    #[serde(rename = "GroupId")]
    group_id: Option<String>,
}

#[derive(Deserialize)]
struct DescribeInstanceStatuses {
    #[serde(rename = "InstanceStatuses")]
    instance_statuses: Vec<Ec2InstanceStatus>,
}

#[derive(Deserialize)]
struct Ec2InstanceStatus {
    #[serde(rename = "InstanceId")]
    instance_id: String,
    #[serde(rename = "SystemStatus")]
    system_status: Option<Ec2StatusSummary>,
    #[serde(rename = "InstanceStatus")]
    instance_status: Option<Ec2StatusSummary>,
}

#[derive(Deserialize)]
struct Ec2StatusSummary {
    #[serde(rename = "Status")]
    status: Option<String>,
}

#[derive(Deserialize)]
struct EicSendSshPublicKeyResponse {
    #[serde(rename = "Success")]
    success: Option<bool>,
}

#[derive(Debug, Clone)]
struct Ec2StatusChecks {
    system_status: String,
    instance_status: String,
    checks_pass: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Success,
    Failed,
    Skipped,
}

impl ProbeOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SgPort22Status {
    OpenWorld,
    Restricted,
    Closed,
    Unknown,
}

impl SgPort22Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenWorld => "open-world",
            Self::Restricted => "restricted",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct EicProbeResult {
    os_user: String,
    public_ip_present: bool,
    instance_running: bool,
    az_present: bool,
    sg_port22: SgPort22Status,
    send_ssh_public_key: ProbeOutcome,
    send_ssh_public_key_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HealthLevel {
    Ok,
    Degraded,
    Unreachable,
    Unknown,
}

impl HealthLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
            Self::Unreachable => "unreachable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct HealthSummary {
    level: HealthLevel,
    ssh_local_problem_likely: Option<bool>,
    notes: String,
}

struct InstanceEntry {
    name: Option<String>,
    instance_id: String,
    state: String,
    public_ip: Option<String>,
}

impl InstanceEntry {
    fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("N/A")
    }
}

#[derive(Debug, Clone)]
struct LightsailInstanceInfo {
    name: String,
    state: String,
    public_ip: Option<String>,
}

#[derive(Debug, Clone)]
struct GceInstanceInfo {
    name: String,
    instance_id: String,
    state: String,
    zone: Option<String>,
    public_ip: Option<String>,
}

#[derive(Debug, Clone)]
struct DropletInfo {
    id: u64,
    name: String,
    state: String,
    public_ip: Option<String>,
    region: Option<String>,
}

struct AwsCli {
    region: String,
}

impl AwsCli {
    fn new(region: String) -> Self {
        Self { region }
    }

    fn run_output(&self, args: &[String]) -> Result<Output> {
        let mut cmd = Command::new("aws");
        cmd.args(args);
        cmd.arg("--region").arg(&self.region);
        cmd.env("AWS_PAGER", "");
        let output = cmd.output().context("failed to execute aws CLI")?;
        Ok(output)
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let output = self.run_output(args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let mut message = format!("aws {} failed", args.join(" "));
            if !stderr.is_empty() {
                message.push_str(&format!(": {}", stderr));
            }
            if !stdout.is_empty() {
                message.push_str(&format!("\n{}", stdout));
            }
            bail!(message);
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_caller_identity(&self) -> Result<CallerIdentity> {
        let args = aws_args(&["sts", "get-caller-identity", "--output", "json"]);
        let output = self.run(&args)?;
        serde_json::from_str(&output).context("parse caller identity")
    }
}

struct GcloudCli {
    project: String,
}

impl GcloudCli {
    fn new(project: String) -> Self {
        Self { project }
    }

    fn run_output(&self, args: &[String]) -> Result<Output> {
        let mut cmd = Command::new("gcloud");
        cmd.args(args);
        cmd.arg("--project").arg(&self.project);
        cmd.arg("--quiet");
        cmd.env("CLOUDSDK_CORE_DISABLE_PROMPTS", "1");
        cmd.env("CLOUDSDK_PYTHON_SITEPACKAGES", "1");
        let output = cmd.output().context("failed to execute gcloud CLI")?;
        Ok(output)
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let output = self.run_output(args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let mut message = format!("gcloud {} failed", args.join(" "));
            if !stderr.is_empty() {
                message.push_str(&format!(": {}", stderr));
            }
            if !stdout.is_empty() {
                message.push_str(&format!("\n{}", stdout));
            }
            bail!(message);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn run_json(&self, args: &[String]) -> Result<serde_json::Value> {
        let stdout = self.run(args)?;
        serde_json::from_str(&stdout).context("parse gcloud json output")
    }
}

struct DoctlCli;

impl DoctlCli {
    fn new() -> Self {
        Self
    }

    fn run_output(&self, args: &[String]) -> Result<Output> {
        let mut cmd = Command::new("doctl");
        cmd.args(args);
        cmd.env("DIGITALOCEAN_COLOR", "false");
        let output = cmd.output().context("failed to execute doctl CLI")?;
        Ok(output)
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let output = self.run_output(args)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let mut message = format!("doctl {} failed", args.join(" "));
            if !stderr.is_empty() {
                message.push_str(&format!(": {}", stderr));
            }
            if !stdout.is_empty() {
                message.push_str(&format!("\n{}", stdout));
            }
            bail!(message);
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn run_json(&self, args: &[String]) -> Result<serde_json::Value> {
        let stdout = self.run(args)?;
        serde_json::from_str(&stdout).context("parse doctl json output")
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {:#}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_root = expand_home_path(&cli.config_dir)?;
    match cli.command {
        TopCommand::Ec2(ec2) => match ec2.command {
            Ec2Command::Init(args) => run_aws_init(args, &config_root),
            Ec2Command::Up(args) => run_aws_up(args, &config_root),
            Ec2Command::Status(args) => run_aws_status(args, &config_root),
            Ec2Command::Health(args) => run_aws_health(args, &config_root),
            Ec2Command::Reboot(args) => run_aws_reboot(args, &config_root),
            Ec2Command::Destroy(args) => run_aws_destroy(args, &config_root),
            Ec2Command::Prune(args) => run_aws_prune(args, &config_root),
            Ec2Command::Regions(args) => run_ec2_regions(args),
        },
        TopCommand::Lightsail(provider) => run_lightsail(provider, &config_root),
        TopCommand::Gce(provider) => run_gce(provider, &config_root),
        TopCommand::Droplet(provider) => run_droplet(provider, &config_root),
    }
}

fn run_lightsail(args: LightsailArgs, config_root: &Path) -> Result<()> {
    match args.command {
        LightsailCommand::Init(args) => run_lightsail_init(args, config_root),
        LightsailCommand::Up(args) => run_lightsail_up(args, config_root),
        LightsailCommand::Status(args) => run_lightsail_status(args, config_root),
        LightsailCommand::Health(args) => run_lightsail_health(args, config_root),
        LightsailCommand::Reboot(args) => run_lightsail_reboot(args, config_root),
        LightsailCommand::Destroy(args) => run_lightsail_destroy(args, config_root),
        LightsailCommand::Prune(args) => run_lightsail_prune(args, config_root),
        LightsailCommand::Regions(args) => run_lightsail_regions(args),
    }
}

fn run_gce(args: GceArgs, config_root: &Path) -> Result<()> {
    match args.command {
        GceCommand::Init(args) => run_gce_init(args, config_root),
        GceCommand::Up(args) => run_gce_up(args, config_root),
        GceCommand::Status(args) => run_gce_status(args, config_root),
        GceCommand::Health(args) => run_gce_health(args, config_root),
        GceCommand::Reboot(args) => run_gce_reboot(args, config_root),
        GceCommand::Destroy(args) => run_gce_destroy(args, config_root),
        GceCommand::Prune(args) => run_gce_prune(args, config_root),
        GceCommand::Regions(args) => run_gce_regions(args),
        GceCommand::Zones(args) => run_gce_zones(args),
    }
}

fn run_droplet(args: DropletArgs, config_root: &Path) -> Result<()> {
    match args.command {
        DropletCommand::Init(args) => run_droplet_init(args, config_root),
        DropletCommand::Up(args) => run_droplet_up(args, config_root),
        DropletCommand::Status(args) => run_droplet_status(args, config_root),
        DropletCommand::Health(args) => run_droplet_health(args, config_root),
        DropletCommand::Reboot(args) => run_droplet_reboot(args, config_root),
        DropletCommand::Destroy(args) => run_droplet_destroy(args, config_root),
        DropletCommand::Prune(args) => run_droplet_prune(args, config_root),
        DropletCommand::Regions(args) => run_droplet_regions(args),
    }
}

fn aws_metadata_region() -> String {
    env::var("AWS_REGION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("AWS_DEFAULT_REGION")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "us-east-1".to_string())
}

fn run_ec2_regions(args: ListRegionsArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let aws = AwsCli::new(aws_metadata_region());
    let query_args = aws_args(&[
        "ec2",
        "describe-regions",
        "--all-regions",
        "--output",
        "json",
    ]);
    let output = aws.run(&query_args)?;
    let payload: serde_json::Value =
        serde_json::from_str(&output).context("parse ec2 describe-regions")?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let mut regions = payload
        .get("Regions")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|region| {
            let name = region.get("RegionName")?.as_str()?.to_string();
            let opt_in = region
                .get("OptInStatus")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            Some((name, opt_in))
        })
        .collect::<Vec<_>>();
    regions.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, opt_in) in regions {
        println!("region={} opt-in-status={}", name, opt_in);
    }
    Ok(())
}

fn run_lightsail_regions(args: ListRegionsArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let aws = AwsCli::new(aws_metadata_region());
    let query_args = aws_args(&[
        "lightsail",
        "get-regions",
        "--include-availability-zones",
        "--output",
        "json",
    ]);
    let output = aws.run(&query_args)?;
    let payload: serde_json::Value =
        serde_json::from_str(&output).context("parse lightsail get-regions")?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let mut regions = payload
        .get("regions")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|region| {
            let name = region.get("name")?.as_str()?.to_string();
            let display_name = region
                .get("displayName")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            let az_count = region
                .get("availabilityZones")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0);
            Some((name, display_name, az_count))
        })
        .collect::<Vec<_>>();
    regions.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, display_name, az_count) in regions {
        println!(
            "region={} display-name=\"{}\" availability-zones={}",
            name, display_name, az_count
        );
    }
    Ok(())
}

fn run_gcloud_global_json(args: &[&str]) -> Result<serde_json::Value> {
    let mut cmd = Command::new("gcloud");
    cmd.args(args);
    cmd.arg("--quiet");
    cmd.env("CLOUDSDK_CORE_DISABLE_PROMPTS", "1");
    let output = cmd.output().context("failed to execute gcloud CLI")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let mut message = format!("gcloud {} failed", args.join(" "));
        if !stderr.is_empty() {
            message.push_str(&format!(": {}", stderr));
        }
        if !stdout.is_empty() {
            message.push_str(&format!("\n{}", stdout));
        }
        bail!(message);
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str(&stdout).context("parse gcloud json output")
}

fn run_gce_regions(args: ListRegionsArgs) -> Result<()> {
    check_gcloud_cli()?;
    let payload = run_gcloud_global_json(&["compute", "regions", "list", "--format", "json"])?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let mut regions = payload
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|region| {
            let name = region.get("name")?.as_str()?.to_string();
            let status = region
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            Some((name, status))
        })
        .collect::<Vec<_>>();
    regions.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, status) in regions {
        println!("region={} status={}", name, status);
    }
    Ok(())
}

fn run_gce_zones(args: GceZonesArgs) -> Result<()> {
    check_gcloud_cli()?;
    let payload = run_gcloud_global_json(&["compute", "zones", "list", "--format", "json"])?;
    let mut zones = payload.as_array().cloned().unwrap_or_default();
    if let Some(region_filter) = args.region.as_deref() {
        zones.retain(|zone| {
            let region_path = zone
                .get("region")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            region_name_from_path(region_path) == region_filter
        });
    }

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(zones))?
        );
        return Ok(());
    }

    let mut entries = zones
        .into_iter()
        .filter_map(|zone| {
            let name = zone.get("name")?.as_str()?.to_string();
            let region = zone
                .get("region")
                .and_then(|value| value.as_str())
                .map(region_name_from_path)
                .unwrap_or_else(|| "unknown".to_string());
            let status = zone
                .get("status")
                .and_then(|value| value.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            Some((name, region, status))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (zone, region, status) in entries {
        println!("zone={} region={} status={}", zone, region, status);
    }
    Ok(())
}

fn run_droplet_regions(args: ListRegionsArgs) -> Result<()> {
    check_doctl_cli()?;
    let doctl = DoctlCli::new();
    let payload = doctl.run_json(&[
        "compute".to_string(),
        "region".to_string(),
        "list".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ])?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    let mut regions = payload
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|region| {
            let slug = region.get("slug")?.as_str()?.to_string();
            let available = region
                .get("available")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Some((slug, available))
        })
        .collect::<Vec<_>>();
    regions.sort_by(|a, b| a.0.cmp(&b.0));
    for (slug, available) in regions {
        println!("region={} available={}", slug, available);
    }
    Ok(())
}

fn run_aws_up(args: Ec2UpArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    ensure_no_duplicate_instance(&aws, &config.cluster_name, &args.name)?;

    let vpc_id = ensure_vpc(&aws, &config)?;
    let subnet_id = ensure_subnet(&aws, &config, &vpc_id)?;
    let igw_id = ensure_internet_gateway(&aws, &config, &vpc_id)?;
    ensure_route_table(&aws, &config, &vpc_id, &subnet_id, &igw_id)?;
    let sg_id = ensure_security_group(&aws, &config, &vpc_id)?;
    let key_name = ensure_key_pair(&aws, &config)?;
    let ami_id = resolve_ami_id(&aws, &config)?;
    let instance_type = resolve_instance_type(&config, args.instance_type);

    let instance_id = launch_instance(
        &aws,
        &config,
        &args.name,
        &ami_id,
        &instance_type,
        &subnet_id,
        &sg_id,
        &key_name,
    )?;

    wait_for_instance_running(&aws, &instance_id)?;
    let public_ip = fetch_instance_public_ip(&aws, &instance_id)?;
    let public_ip_display = public_ip.unwrap_or_else(|| "N/A".to_string());

    println!(
        "name={} instance-id={} public-ip={}",
        args.name, instance_id, public_ip_display
    );

    print_aws_status_and_refresh_ssh_config(&aws, &config)?;
    Ok(())
}

fn run_aws_reboot(args: RebootArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    let instance = find_instance_by_name(&aws, &args.name)?;
    reboot_instance(&aws, &instance.instance_id)?;

    println!(
        "rebooted name={} instance-id={}",
        args.name, instance.instance_id
    );
    Ok(())
}

fn run_aws_health(args: Ec2HealthArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    let instance = find_instance_by_cluster_and_name(&aws, &config.cluster_name, &args.name)?;
    let ec2_checks = describe_ec2_status_checks(&aws, &instance.instance_id, &instance.state.name)?;

    let sg_ids = instance_security_group_ids(&instance);
    let security_groups = describe_security_groups_by_ids(&aws, &sg_ids)?;
    let sg_port22 = classify_sg_port_22(&security_groups);

    let eic_probe = run_eic_probe(&aws, &config, &instance, sg_port22, &args.os_user)?;
    let summary = summarize_health(&instance.state.name, ec2_checks.checks_pass, &eic_probe);

    print_health_report(
        &config.cluster_name,
        &args.name,
        &instance,
        &ec2_checks,
        &eic_probe,
        &summary,
    );

    Ok(())
}

fn run_aws_destroy(args: DestroyArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    let instance = find_instance_by_name(&aws, &args.name)?;
    if !args.force {
        let prompt = format!(
            "Destroy instance '{}' ({})? [y/N]: ",
            args.name, instance.instance_id
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    terminate_instance(&aws, &instance.instance_id)?;
    wait_for_instance_terminated(&aws, &instance.instance_id)?;
    println!(
        "terminated name={} instance-id={}",
        args.name, instance.instance_id
    );
    print_aws_status_and_refresh_ssh_config(&aws, &config)?;
    Ok(())
}

fn run_aws_status(args: StatusArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    print_aws_status_and_refresh_ssh_config(&aws, &config)
}

fn print_aws_status_and_refresh_ssh_config(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
) -> Result<()> {
    let vpc_id = find_vpc(&aws, &config.cluster_name)?;
    let sg_id = find_security_group(&aws, &config.cluster_name)?;

    let instances = if let Some(vpc_id) = vpc_id.as_ref() {
        describe_instances_by_vpc(&aws, vpc_id)?
    } else {
        Vec::new()
    };

    let entries = instances
        .into_iter()
        .map(|instance| InstanceEntry {
            name: tag_value(&instance.tags, "Name"),
            instance_id: instance.instance_id,
            state: instance.state.name,
            public_ip: instance.public_ip,
        })
        .collect::<Vec<_>>();

    println!("vpc-id={}", vpc_id.as_deref().unwrap_or("N/A"));
    println!("sg-id={}", sg_id.as_deref().unwrap_or("N/A"));

    for entry in &entries {
        let public_ip = entry.public_ip.as_deref().unwrap_or("N/A");
        println!(
            "name={} instance-id={} state={} public-ip={}",
            entry.display_name(),
            entry.instance_id,
            entry.state,
            public_ip
        );
    }

    let ssh_config_path = config.ssh_config_path.clone();
    let identity_file = derive_private_key_path(&config.ssh_public_key_path);
    write_ssh_config(
        &ssh_config_path,
        &entries,
        vpc_id.as_deref(),
        sg_id.as_deref(),
        &identity_file,
    )?;
    Ok(())
}

fn run_aws_prune(args: PruneArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(config_root, &args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    let vpc_id = match find_vpc(&aws, &config.cluster_name)? {
        Some(vpc_id) => vpc_id,
        None => {
            println!("nothing to prune");
            maybe_cleanup_provider_cluster_config(
                config_root,
                EC2_PROVIDER,
                &config.cluster_name,
                args.force,
            )?;
            return Ok(());
        }
    };

    let instances = describe_instances_by_vpc(&aws, &vpc_id)?;
    if !instances.is_empty() {
        let ids = instances
            .iter()
            .map(|instance| instance.instance_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "cannot prune while instances exist in vpc {}: {}",
            vpc_id,
            ids
        );
    }

    if !args.force {
        let prompt = format!(
            "Prune VPC resources for cluster '{}' (vpc-id {})? [y/N]: ",
            config.cluster_name, vpc_id
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    let route_table = find_route_table(&aws, &config.cluster_name)?;
    if let Some(route_table) = route_table.as_ref() {
        disassociate_route_table(&aws, route_table)?;
        delete_route_table(&aws, &route_table.route_table_id)?;
    }

    let subnet_id = find_subnet(&aws, &config.cluster_name)?;
    if let Some(subnet_id) = subnet_id.as_ref() {
        delete_subnet(&aws, subnet_id)?;
    }

    let igw = find_internet_gateway(&aws, &config.cluster_name)?;
    if let Some(igw) = igw.as_ref() {
        detach_internet_gateway(&aws, igw, &vpc_id)?;
        delete_internet_gateway(&aws, &igw.internet_gateway_id)?;
    }

    let sg_id = find_security_group(&aws, &config.cluster_name)?;
    if let Some(sg_id) = sg_id.as_ref() {
        delete_security_group(&aws, sg_id)?;
    }

    delete_vpc(&aws, &vpc_id)?;

    let key_name = resource_name(&config.cluster_name, "key");
    if args.force {
        delete_key_pair_if_exists(&aws, &key_name)?;
    } else {
        let prompt = format!("Delete key pair '{}' ? [y/N]: ", key_name);
        if confirm(&prompt)? {
            delete_key_pair_if_exists(&aws, &key_name)?;
        }
    }

    println!("pruned cluster={} vpc-id={}", config.cluster_name, vpc_id);
    maybe_cleanup_provider_cluster_config(
        config_root,
        EC2_PROVIDER,
        &config.cluster_name,
        args.force,
    )?;
    Ok(())
}

fn run_aws_init(args: InitArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    let config_dir = ec2_cluster_dir(config_root, &args.cluster)?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("create config dir {}", config_dir.display()))?;

    let config_path = config_dir.join(CONFIG_FILE_NAME);
    let ssh_config_path = config_dir.join(SSH_CONFIG_FILE);

    if !config_path.exists() {
        let defaults = load_global_config(config_root)?;
        let public_key_path = defaults
            .ec2
            .as_ref()
            .and_then(|aws| aws.ssh_public_key_path.clone())
            .unwrap_or_else(|| default_ssh_public_key_path(config_root));
        let region = defaults
            .ec2
            .as_ref()
            .and_then(|aws| aws.region.clone())
            .unwrap_or_else(|| "ap-northeast-1".to_string());
        let default_instance_type = defaults
            .ec2
            .as_ref()
            .and_then(|aws| aws.default_instance_type.clone())
            .unwrap_or_else(|| DEFAULT_INSTANCE_TYPE.to_string());
        let contents = default_ec2_config_contents(
            &args.cluster,
            &region,
            &public_key_path,
            &default_instance_type,
        );
        fs::write(&config_path, contents)
            .with_context(|| format!("write {}", config_path.display()))?;
        println!("created {}", config_path.display());
    } else {
        println!("exists {}", config_path.display());
    }

    if !ssh_config_path.exists() {
        fs::write(&ssh_config_path, "")
            .with_context(|| format!("write {}", ssh_config_path.display()))?;
        println!("created {}", ssh_config_path.display());
    } else {
        println!("exists {}", ssh_config_path.display());
    }
    Ok(())
}

fn run_lightsail_init(args: InitArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    let config_dir = lightsail_cluster_dir(config_root, &args.cluster)?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("create config dir {}", config_dir.display()))?;

    let config_path = config_dir.join(CONFIG_FILE_NAME);
    let ssh_config_path = config_dir.join(SSH_CONFIG_FILE);

    if !config_path.exists() {
        let defaults = load_global_config(config_root)?;
        let public_key_path = defaults
            .lightsail
            .as_ref()
            .and_then(|value| value.ssh_public_key_path.clone())
            .unwrap_or_else(|| default_ssh_public_key_path(config_root));
        let region = defaults
            .lightsail
            .as_ref()
            .and_then(|value| value.region.clone())
            .unwrap_or_else(|| "ap-northeast-1".to_string());
        let availability_zone = defaults
            .lightsail
            .as_ref()
            .and_then(|value| value.availability_zone.clone())
            .unwrap_or_else(|| format!("{}a", region));
        let default_bundle_id = defaults
            .lightsail
            .as_ref()
            .and_then(|value| value.default_bundle_id.clone())
            .unwrap_or_else(|| DEFAULT_LIGHTSAIL_BUNDLE_ID.to_string());
        let blueprint_id = defaults
            .lightsail
            .as_ref()
            .and_then(|value| value.blueprint_id.clone())
            .unwrap_or_else(|| DEFAULT_LIGHTSAIL_BLUEPRINT_ID.to_string());
        let contents = default_lightsail_config_contents(
            &args.cluster,
            &region,
            &public_key_path,
            &availability_zone,
            &default_bundle_id,
            &blueprint_id,
        );
        fs::write(&config_path, contents)
            .with_context(|| format!("write {}", config_path.display()))?;
        println!("created {}", config_path.display());
    } else {
        println!("exists {}", config_path.display());
    }

    if !ssh_config_path.exists() {
        fs::write(&ssh_config_path, "")
            .with_context(|| format!("write {}", ssh_config_path.display()))?;
        println!("created {}", ssh_config_path.display());
    } else {
        println!("exists {}", ssh_config_path.display());
    }
    Ok(())
}

fn run_lightsail_up(args: LightsailUpArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;

    if lightsail_find_instance(&aws, &config.cluster_name, &args.name)?.is_some() {
        bail!(
            "instance '{}' already exists in lightsail cluster '{}'",
            args.name,
            config.cluster_name
        );
    }

    let bundle_id = args
        .bundle_id
        .unwrap_or_else(|| config.default_bundle_id.clone());
    let mut create_args = aws_args(&[
        "lightsail",
        "create-instances",
        "--instance-names",
        &args.name,
        "--availability-zone",
        &config.availability_zone,
        "--blueprint-id",
        &config.blueprint_id,
        "--bundle-id",
        &bundle_id,
        "--tags",
    ]);
    create_args.push(format!("key=Cluster,value={}", config.cluster_name));
    create_args.push(format!("key=Name,value={}", args.name));
    if let Some(key_pair_name) = config.key_pair_name.as_deref() {
        create_args.push("--key-pair-name".to_string());
        create_args.push(key_pair_name.to_string());
    }
    let _ = aws.run(&create_args)?;

    ensure_lightsail_public_ports(&aws, &args.name)?;
    lightsail_wait_for_instance_state(&aws, &config.cluster_name, &args.name, "running")?;
    let instance = lightsail_find_instance(&aws, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("lightsail instance '{}' not found after create", args.name))?;
    let public_ip = instance.public_ip.unwrap_or_else(|| "N/A".to_string());
    println!(
        "name={} instance-id={} public-ip={}",
        args.name, args.name, public_ip
    );

    print_lightsail_status_and_refresh_ssh_config(&aws, &config)
}

fn ensure_lightsail_public_ports(aws: &AwsCli, instance_name: &str) -> Result<()> {
    let mut args = aws_args(&[
        "lightsail",
        "put-instance-public-ports",
        "--instance-name",
        instance_name,
    ]);
    for port in [22, 80, 443] {
        args.push("--port-infos".to_string());
        args.push(format!("fromPort={},toPort={},protocol=tcp", port, port));
    }
    let _ = aws.run(&args)?;
    Ok(())
}

fn run_lightsail_status(args: StatusArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;
    print_lightsail_status_and_refresh_ssh_config(&aws, &config)
}

fn run_lightsail_health(args: HealthArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;

    let instance = lightsail_find_instance(&aws, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("lightsail instance '{}' not found in cluster", args.name))?;
    let public_ip = instance.public_ip.as_deref().unwrap_or("N/A");
    let state_lower = instance.state.to_ascii_lowercase();
    let (health_level, notes) = if state_lower == "running" && instance.public_ip.is_some() {
        ("ok", "instance-running")
    } else if state_lower == "running" {
        ("degraded", "running-without-public-ip")
    } else {
        ("unreachable", "instance-not-running")
    };

    println!("provider=lightsail");
    println!("cluster={}", config.cluster_name);
    println!("name={}", args.name);
    println!("instance.state={}", instance.state);
    println!("instance.public-ip={}", public_ip);
    println!("health.level={}", health_level);
    println!("health.notes={}", notes);
    Ok(())
}

fn run_lightsail_reboot(args: RebootArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;

    let instance = lightsail_find_instance(&aws, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("lightsail instance '{}' not found in cluster", args.name))?;
    let reboot_args = aws_args(&[
        "lightsail",
        "reboot-instance",
        "--instance-name",
        &instance.name,
    ]);
    let _ = aws.run(&reboot_args)?;
    println!(
        "rebooted name={} instance-id={}",
        instance.name, instance.name
    );
    Ok(())
}

fn run_lightsail_destroy(args: DestroyArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;

    let instance = lightsail_find_instance(&aws, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("lightsail instance '{}' not found in cluster", args.name))?;
    if !args.force {
        let prompt = format!(
            "Delete lightsail instance '{}' in cluster '{}'? [y/N]: ",
            instance.name, config.cluster_name
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }
    let destroy_args = aws_args(&[
        "lightsail",
        "delete-instance",
        "--instance-name",
        &instance.name,
    ]);
    let _ = aws.run(&destroy_args)?;
    println!(
        "terminated name={} instance-id={}",
        instance.name, instance.name
    );
    print_lightsail_status_and_refresh_ssh_config(&aws, &config)
}

fn run_lightsail_prune(args: PruneArgs, config_root: &Path) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_lightsail_config(config_root, &args.cluster, args.config.as_deref())?;
    let aws = AwsCli::new(config.region.clone());
    print_banner(&aws)?;

    let entries = lightsail_list_cluster_instances(&aws, &config.cluster_name)?;
    if entries.is_empty() {
        println!("nothing to prune");
        maybe_cleanup_provider_cluster_config(
            config_root,
            LIGHTSAIL_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
        return Ok(());
    }

    if !args.force {
        let prompt = format!(
            "Delete all lightsail instances for cluster '{}' ({})? [y/N]: ",
            config.cluster_name,
            entries.len()
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    for entry in &entries {
        let destroy_args = aws_args(&[
            "lightsail",
            "delete-instance",
            "--instance-name",
            &entry.name,
        ]);
        let _ = aws.run(&destroy_args)?;
        println!("deleted name={}", entry.name);
    }

    print_lightsail_status_and_refresh_ssh_config(&aws, &config)?;
    if lightsail_list_cluster_instances(&aws, &config.cluster_name)?.is_empty() {
        maybe_cleanup_provider_cluster_config(
            config_root,
            LIGHTSAIL_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
    }
    Ok(())
}

fn print_lightsail_status_and_refresh_ssh_config(
    aws: &AwsCli,
    config: &LightsailEffectiveConfig,
) -> Result<()> {
    let entries = lightsail_list_cluster_instances(aws, &config.cluster_name)?;
    println!("region={}", config.region);
    for entry in &entries {
        let public_ip = entry.public_ip.as_deref().unwrap_or("N/A");
        println!(
            "name={} instance-id={} state={} public-ip={}",
            entry.name, entry.name, entry.state, public_ip
        );
    }

    let ssh_entries = entries
        .iter()
        .map(|entry| InstanceEntry {
            name: Some(entry.name.clone()),
            instance_id: entry.name.clone(),
            state: entry.state.clone(),
            public_ip: entry.public_ip.clone(),
        })
        .collect::<Vec<_>>();

    let identity_file = derive_private_key_path(&config.ssh_public_key_path);
    write_ssh_config(
        &config.ssh_config_path,
        &ssh_entries,
        Some(&config.region),
        None,
        &identity_file,
    )
}

fn lightsail_wait_for_instance_state(
    aws: &AwsCli,
    cluster: &str,
    name: &str,
    expected_state: &str,
) -> Result<()> {
    for _ in 0..60 {
        if let Some(instance) = lightsail_find_instance(aws, cluster, name)? {
            if instance.state.eq_ignore_ascii_case(expected_state) {
                return Ok(());
            }
        }
        sleep(Duration::from_secs(5));
    }
    bail!(
        "timeout waiting for lightsail instance '{}' to become {}",
        name,
        expected_state
    );
}

fn lightsail_find_instance(
    aws: &AwsCli,
    cluster: &str,
    name: &str,
) -> Result<Option<LightsailInstanceInfo>> {
    let instances = lightsail_list_cluster_instances(aws, cluster)?;
    let found = instances.into_iter().find(|entry| entry.name == name);
    Ok(found)
}

fn lightsail_list_cluster_instances(
    aws: &AwsCli,
    cluster: &str,
) -> Result<Vec<LightsailInstanceInfo>> {
    let args = aws_args(&["lightsail", "get-instances", "--output", "json"]);
    let output = aws.run(&args)?;
    let payload: serde_json::Value =
        serde_json::from_str(&output).context("parse lightsail get-instances")?;

    let mut instances = Vec::new();
    let list = payload
        .get("instances")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    for item in list {
        if !lightsail_has_cluster_tag(&item, cluster) {
            continue;
        }
        let Some(name) = item.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let state = item
            .get("state")
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string();
        let public_ip = item
            .get("publicIpAddress")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        instances.push(LightsailInstanceInfo {
            name: name.to_string(),
            state,
            public_ip,
        });
    }

    instances.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(instances)
}

fn lightsail_has_cluster_tag(instance: &serde_json::Value, cluster: &str) -> bool {
    let tags = instance
        .get("tags")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for tag in tags {
        let key = tag
            .get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let value = tag
            .get("value")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if key == "Cluster" && value == cluster {
            return true;
        }
    }
    false
}

fn run_gce_init(args: InitArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    let config_dir = gce_cluster_dir(config_root, &args.cluster)?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("create config dir {}", config_dir.display()))?;

    let config_path = config_dir.join(CONFIG_FILE_NAME);
    let ssh_config_path = config_dir.join(SSH_CONFIG_FILE);

    if !config_path.exists() {
        let defaults = load_global_config(config_root)?;
        let project = defaults
            .gce
            .as_ref()
            .and_then(|value| value.project.clone())
            .or_else(|| env::var("GOOGLE_CLOUD_PROJECT").ok())
            .or_else(|| env::var("GCLOUD_PROJECT").ok())
            .unwrap_or_default();
        let zone = defaults
            .gce
            .as_ref()
            .and_then(|value| value.zone.clone())
            .or_else(|| env::var("CLOUDSDK_COMPUTE_ZONE").ok())
            .unwrap_or_else(|| "asia-northeast1-a".to_string());
        let ssh_public_key_path = defaults
            .gce
            .as_ref()
            .and_then(|value| value.ssh_public_key_path.clone())
            .unwrap_or_else(|| default_ssh_public_key_path(config_root));
        let machine_type = defaults
            .gce
            .as_ref()
            .and_then(|value| value.default_machine_type.clone())
            .unwrap_or_else(|| DEFAULT_GCE_MACHINE_TYPE.to_string());
        let image_family = defaults
            .gce
            .as_ref()
            .and_then(|value| value.image_family.clone())
            .unwrap_or_else(|| DEFAULT_GCE_IMAGE_FAMILY.to_string());
        let image_project = defaults
            .gce
            .as_ref()
            .and_then(|value| value.image_project.clone())
            .unwrap_or_else(|| DEFAULT_GCE_IMAGE_PROJECT.to_string());
        let ssh_user = defaults
            .gce
            .as_ref()
            .and_then(|value| value.ssh_user.clone())
            .unwrap_or_else(|| DEFAULT_GCE_SSH_USER.to_string());

        let contents = default_gce_config_contents(
            &args.cluster,
            &project,
            &zone,
            &ssh_public_key_path,
            &machine_type,
            &image_family,
            &image_project,
            &ssh_user,
        );
        fs::write(&config_path, contents)
            .with_context(|| format!("write {}", config_path.display()))?;
        println!("created {}", config_path.display());
    } else {
        println!("exists {}", config_path.display());
    }

    if !ssh_config_path.exists() {
        fs::write(&ssh_config_path, "")
            .with_context(|| format!("write {}", ssh_config_path.display()))?;
        println!("created {}", ssh_config_path.display());
    } else {
        println!("exists {}", ssh_config_path.display());
    }
    Ok(())
}

fn run_gce_up(args: GceUpArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());

    if let Some(existing) = gce_find_instance(&gcloud, &config.cluster_name, &args.name)? {
        let state = existing.state.to_ascii_uppercase();
        if state != "TERMINATED" {
            bail!(
                "instance '{}' already exists in gce cluster '{}' (state={})",
                args.name,
                config.cluster_name,
                existing.state
            );
        }
    }

    let machine_type = args
        .machine_type
        .unwrap_or_else(|| config.default_machine_type.clone());
    let ssh_public_key = fs::read_to_string(&config.ssh_public_key_path)
        .with_context(|| format!("read ssh key {}", config.ssh_public_key_path))?;
    let ssh_public_key = ssh_public_key.trim();
    if ssh_public_key.is_empty() {
        bail!(
            "ssh public key file {} is empty",
            config.ssh_public_key_path
        );
    }
    let metadata = format!("ssh-keys={}:{}", config.ssh_user, ssh_public_key);
    let labels = format!(
        "cluster={},managed_by=vmcli",
        gce_cluster_label_value(&config.cluster_name)
    );

    let create_args = vec![
        "compute".to_string(),
        "instances".to_string(),
        "create".to_string(),
        args.name.clone(),
        "--zone".to_string(),
        config.zone.clone(),
        "--machine-type".to_string(),
        machine_type,
        "--image-family".to_string(),
        config.image_family.clone(),
        "--image-project".to_string(),
        config.image_project.clone(),
        "--labels".to_string(),
        labels,
        "--metadata".to_string(),
        metadata,
        "--format".to_string(),
        "json".to_string(),
    ];
    let _ = gcloud.run(&create_args)?;

    gce_wait_for_instance_state(&gcloud, &config.cluster_name, &args.name, "RUNNING")?;
    let created = gce_find_instance(&gcloud, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("gce instance '{}' not found after create", args.name))?;
    println!(
        "name={} instance-id={} public-ip={}",
        created.name,
        created.instance_id,
        created.public_ip.as_deref().unwrap_or("N/A")
    );

    print_gce_status_and_refresh_ssh_config(&gcloud, &config)
}

fn run_gce_status(args: StatusArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());
    print_gce_status_and_refresh_ssh_config(&gcloud, &config)
}

fn run_gce_health(args: HealthArgs, config_root: &Path) -> Result<()> {
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());
    let instance = gce_find_instance(&gcloud, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("gce instance '{}' not found in cluster", args.name))?;

    let state_upper = instance.state.to_ascii_uppercase();
    let (health_level, notes) = if state_upper == "RUNNING" && instance.public_ip.is_some() {
        ("ok", "instance-running")
    } else if state_upper == "RUNNING" {
        ("degraded", "running-without-public-ip")
    } else {
        ("unreachable", "instance-not-running")
    };

    println!("provider=gce");
    println!("project={}", config.project);
    println!("zone={}", instance.zone.as_deref().unwrap_or(&config.zone));
    println!("cluster={}", config.cluster_name);
    println!("name={}", instance.name);
    println!("instance.id={}", instance.instance_id);
    println!("instance.state={}", instance.state);
    println!(
        "instance.public-ip={}",
        instance.public_ip.as_deref().unwrap_or("N/A")
    );
    println!("health.level={}", health_level);
    println!("health.notes={}", notes);
    Ok(())
}

fn run_gce_reboot(args: RebootArgs, config_root: &Path) -> Result<()> {
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());
    let instance = gce_find_instance(&gcloud, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("gce instance '{}' not found in cluster", args.name))?;
    let zone = instance.zone.as_deref().unwrap_or(&config.zone);

    let reboot_args = vec![
        "compute".to_string(),
        "instances".to_string(),
        "reset".to_string(),
        instance.name.clone(),
        "--zone".to_string(),
        zone.to_string(),
    ];
    let _ = gcloud.run(&reboot_args)?;
    println!(
        "rebooted name={} instance-id={} zone={}",
        instance.name, instance.instance_id, zone
    );
    Ok(())
}

fn run_gce_destroy(args: DestroyArgs, config_root: &Path) -> Result<()> {
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());
    let instance = gce_find_instance(&gcloud, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("gce instance '{}' not found in cluster", args.name))?;
    let zone = instance.zone.as_deref().unwrap_or(&config.zone).to_string();

    if !args.force {
        let prompt = format!(
            "Delete gce instance '{}' in cluster '{}' (zone {})? [y/N]: ",
            instance.name, config.cluster_name, zone
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    let destroy_args = vec![
        "compute".to_string(),
        "instances".to_string(),
        "delete".to_string(),
        instance.name.clone(),
        "--zone".to_string(),
        zone.clone(),
    ];
    let _ = gcloud.run(&destroy_args)?;
    println!(
        "terminated name={} instance-id={} zone={}",
        instance.name, instance.instance_id, zone
    );

    print_gce_status_and_refresh_ssh_config(&gcloud, &config)
}

fn run_gce_prune(args: PruneArgs, config_root: &Path) -> Result<()> {
    check_gcloud_cli()?;
    let config = load_gce_config(config_root, &args.cluster, args.config.as_deref())?;
    let gcloud = GcloudCli::new(config.project.clone());
    let instances = gce_list_cluster_instances(&gcloud, &config.cluster_name)?;

    if instances.is_empty() {
        println!("nothing to prune");
        maybe_cleanup_provider_cluster_config(
            config_root,
            GCE_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
        return Ok(());
    }

    if !args.force {
        let prompt = format!(
            "Delete all gce instances for cluster '{}' ({})? [y/N]: ",
            config.cluster_name,
            instances.len()
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    for instance in &instances {
        let zone = instance.zone.as_deref().unwrap_or(&config.zone);
        let destroy_args = vec![
            "compute".to_string(),
            "instances".to_string(),
            "delete".to_string(),
            instance.name.clone(),
            "--zone".to_string(),
            zone.to_string(),
        ];
        let _ = gcloud.run(&destroy_args)?;
        println!("deleted name={} zone={}", instance.name, zone);
    }

    print_gce_status_and_refresh_ssh_config(&gcloud, &config)?;
    if gce_list_cluster_instances(&gcloud, &config.cluster_name)?.is_empty() {
        maybe_cleanup_provider_cluster_config(
            config_root,
            GCE_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
    }
    Ok(())
}

fn print_gce_status_and_refresh_ssh_config(
    gcloud: &GcloudCli,
    config: &GceEffectiveConfig,
) -> Result<()> {
    let instances = gce_list_cluster_instances(gcloud, &config.cluster_name)?;
    println!("project={}", config.project);
    for instance in &instances {
        let public_ip = instance.public_ip.as_deref().unwrap_or("N/A");
        println!(
            "name={} instance-id={} zone={} state={} public-ip={}",
            instance.name,
            instance.instance_id,
            instance.zone.as_deref().unwrap_or(&config.zone),
            instance.state,
            public_ip
        );
    }

    let ssh_entries = instances
        .iter()
        .map(|instance| InstanceEntry {
            name: Some(instance.name.clone()),
            instance_id: instance.instance_id.clone(),
            state: instance.state.clone(),
            public_ip: instance.public_ip.clone(),
        })
        .collect::<Vec<_>>();
    let identity_file = derive_private_key_path(&config.ssh_public_key_path);
    write_ssh_config(
        &config.ssh_config_path,
        &ssh_entries,
        Some(&config.project),
        Some(&config.zone),
        &identity_file,
    )
}

fn gce_wait_for_instance_state(
    gcloud: &GcloudCli,
    cluster: &str,
    name: &str,
    expected_state: &str,
) -> Result<()> {
    for _ in 0..60 {
        if let Some(instance) = gce_find_instance(gcloud, cluster, name)? {
            if instance.state.eq_ignore_ascii_case(expected_state) {
                return Ok(());
            }
        }
        sleep(Duration::from_secs(5));
    }
    bail!(
        "timeout waiting for gce instance '{}' to become {}",
        name,
        expected_state
    );
}

fn gce_find_instance(
    gcloud: &GcloudCli,
    cluster: &str,
    name: &str,
) -> Result<Option<GceInstanceInfo>> {
    let instances = gce_list_cluster_instances(gcloud, cluster)?;
    let found = instances.into_iter().find(|instance| instance.name == name);
    Ok(found)
}

fn gce_list_cluster_instances(gcloud: &GcloudCli, cluster: &str) -> Result<Vec<GceInstanceInfo>> {
    let filter = format!("labels.cluster={}", gce_cluster_label_value(cluster));
    let args = vec![
        "compute".to_string(),
        "instances".to_string(),
        "list".to_string(),
        "--filter".to_string(),
        filter,
        "--format".to_string(),
        "json".to_string(),
    ];
    let payload = gcloud.run_json(&args)?;
    let list = payload.as_array().cloned().unwrap_or_default();
    let mut instances = Vec::new();
    for item in list {
        let Some(name) = item.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let instance_id = value_to_string(item.get("id")).unwrap_or_else(|| name.to_string());
        let state = item
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("UNKNOWN")
            .to_string();
        let zone = item
            .get("zone")
            .and_then(|value| value.as_str())
            .map(zone_name_from_path);
        let public_ip = gce_public_ip(&item);
        instances.push(GceInstanceInfo {
            name: name.to_string(),
            instance_id,
            state,
            zone,
            public_ip,
        });
    }
    instances.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(instances)
}

fn gce_public_ip(instance: &serde_json::Value) -> Option<String> {
    let interfaces = instance
        .get("networkInterfaces")
        .and_then(|value| value.as_array())?;
    for interface in interfaces {
        let access_configs = interface
            .get("accessConfigs")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        for access_config in access_configs {
            if let Some(ip) = access_config.get("natIP").and_then(|value| value.as_str()) {
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn zone_name_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .map(|item| item.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn region_name_from_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .map(|item| item.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn gce_cluster_label_value(cluster: &str) -> String {
    sanitize_cloud_identifier(cluster)
}

fn run_droplet_init(args: InitArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    let config_dir = droplet_cluster_dir(config_root, &args.cluster)?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("create config dir {}", config_dir.display()))?;

    let config_path = config_dir.join(CONFIG_FILE_NAME);
    let ssh_config_path = config_dir.join(SSH_CONFIG_FILE);

    if !config_path.exists() {
        let defaults = load_global_config(config_root)?;
        let region = defaults
            .droplet
            .as_ref()
            .and_then(|value| value.region.clone())
            .unwrap_or_else(|| "sfo3".to_string());
        let ssh_public_key_path = defaults
            .droplet
            .as_ref()
            .and_then(|value| value.ssh_public_key_path.clone())
            .unwrap_or_else(|| default_ssh_public_key_path(config_root));
        let default_size = defaults
            .droplet
            .as_ref()
            .and_then(|value| value.default_size.clone())
            .unwrap_or_else(|| DEFAULT_DROPLET_SIZE.to_string());
        let image = defaults
            .droplet
            .as_ref()
            .and_then(|value| value.image.clone())
            .unwrap_or_else(|| DEFAULT_DROPLET_IMAGE.to_string());
        let contents = default_droplet_config_contents(
            &args.cluster,
            &region,
            &ssh_public_key_path,
            &default_size,
            &image,
        );
        fs::write(&config_path, contents)
            .with_context(|| format!("write {}", config_path.display()))?;
        println!("created {}", config_path.display());
    } else {
        println!("exists {}", config_path.display());
    }

    if !ssh_config_path.exists() {
        fs::write(&ssh_config_path, "")
            .with_context(|| format!("write {}", ssh_config_path.display()))?;
        println!("created {}", ssh_config_path.display());
    } else {
        println!("exists {}", ssh_config_path.display());
    }
    Ok(())
}

fn run_droplet_up(args: DropletUpArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    let fingerprint = ensure_droplet_ssh_key_fingerprint(&doctl, &config)?;

    if let Some(existing) = droplet_find_instance(&doctl, &config.cluster_name, &args.name)? {
        if !existing.state.eq_ignore_ascii_case("off") {
            bail!(
                "droplet '{}' already exists in cluster '{}' (state={})",
                args.name,
                config.cluster_name,
                existing.state
            );
        }
    }

    let size = args.size.unwrap_or_else(|| config.default_size.clone());
    let create_args = vec![
        "compute".to_string(),
        "droplet".to_string(),
        "create".to_string(),
        args.name.clone(),
        "--region".to_string(),
        config.region.clone(),
        "--size".to_string(),
        size,
        "--image".to_string(),
        config.image.clone(),
        "--tag-name".to_string(),
        droplet_cluster_tag(&config.cluster_name),
        "--ssh-keys".to_string(),
        fingerprint,
        "--output".to_string(),
        "json".to_string(),
    ];
    let _ = doctl.run(&create_args)?;

    droplet_wait_for_state(&doctl, &config.cluster_name, &args.name, "active")?;
    let created = droplet_find_instance(&doctl, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("droplet '{}' not found after create", args.name))?;
    println!(
        "name={} instance-id={} public-ip={}",
        created.name,
        created.id,
        created.public_ip.as_deref().unwrap_or("N/A")
    );

    print_droplet_status_and_refresh_ssh_config(&doctl, &config)
}

fn run_droplet_status(args: StatusArgs, config_root: &Path) -> Result<()> {
    ensure_vmcli_ssh_keypair(config_root)?;
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    print_droplet_status_and_refresh_ssh_config(&doctl, &config)
}

fn run_droplet_health(args: HealthArgs, config_root: &Path) -> Result<()> {
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    let droplet = droplet_find_instance(&doctl, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("droplet '{}' not found in cluster", args.name))?;
    let state_lower = droplet.state.to_ascii_lowercase();
    let (health_level, notes) = if state_lower == "active" && droplet.public_ip.is_some() {
        ("ok", "instance-running")
    } else if state_lower == "active" {
        ("degraded", "running-without-public-ip")
    } else {
        ("unreachable", "instance-not-running")
    };

    println!("provider=droplet");
    println!("cluster={}", config.cluster_name);
    println!("name={}", droplet.name);
    println!("instance.id={}", droplet.id);
    println!("instance.state={}", droplet.state);
    println!(
        "instance.public-ip={}",
        droplet.public_ip.as_deref().unwrap_or("N/A")
    );
    println!("health.level={}", health_level);
    println!("health.notes={}", notes);
    Ok(())
}

fn run_droplet_reboot(args: RebootArgs, config_root: &Path) -> Result<()> {
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    let droplet = droplet_find_instance(&doctl, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("droplet '{}' not found in cluster", args.name))?;
    let reboot_args = vec![
        "compute".to_string(),
        "droplet-action".to_string(),
        "reboot".to_string(),
        droplet.id.to_string(),
        "--wait".to_string(),
    ];
    let _ = doctl.run(&reboot_args)?;
    println!("rebooted name={} instance-id={}", droplet.name, droplet.id);
    Ok(())
}

fn run_droplet_destroy(args: DestroyArgs, config_root: &Path) -> Result<()> {
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    let droplet = droplet_find_instance(&doctl, &config.cluster_name, &args.name)?
        .ok_or_else(|| anyhow!("droplet '{}' not found in cluster", args.name))?;

    if !args.force {
        let prompt = format!(
            "Delete droplet '{}' in cluster '{}' ? [y/N]: ",
            droplet.name, config.cluster_name
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    let destroy_args = vec![
        "compute".to_string(),
        "droplet".to_string(),
        "delete".to_string(),
        droplet.id.to_string(),
        "--force".to_string(),
    ];
    let _ = doctl.run(&destroy_args)?;
    println!(
        "terminated name={} instance-id={}",
        droplet.name, droplet.id
    );
    print_droplet_status_and_refresh_ssh_config(&doctl, &config)
}

fn run_droplet_prune(args: PruneArgs, config_root: &Path) -> Result<()> {
    check_doctl_cli()?;
    let config = load_droplet_config(config_root, &args.cluster, args.config.as_deref())?;
    let doctl = DoctlCli::new();
    let droplets = droplet_list_cluster_instances(&doctl, &config.cluster_name)?;
    if droplets.is_empty() {
        println!("nothing to prune");
        maybe_cleanup_provider_cluster_config(
            config_root,
            DROPLET_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
        return Ok(());
    }

    if !args.force {
        let prompt = format!(
            "Delete all droplets for cluster '{}' ({})? [y/N]: ",
            config.cluster_name,
            droplets.len()
        );
        if !confirm(&prompt)? {
            println!("aborted");
            return Ok(());
        }
    }

    for droplet in &droplets {
        let destroy_args = vec![
            "compute".to_string(),
            "droplet".to_string(),
            "delete".to_string(),
            droplet.id.to_string(),
            "--force".to_string(),
        ];
        let _ = doctl.run(&destroy_args)?;
        println!("deleted name={} instance-id={}", droplet.name, droplet.id);
    }

    print_droplet_status_and_refresh_ssh_config(&doctl, &config)?;
    if droplet_list_cluster_instances(&doctl, &config.cluster_name)?.is_empty() {
        maybe_cleanup_provider_cluster_config(
            config_root,
            DROPLET_PROVIDER,
            &config.cluster_name,
            args.force,
        )?;
    }
    Ok(())
}

fn print_droplet_status_and_refresh_ssh_config(
    doctl: &DoctlCli,
    config: &DropletEffectiveConfig,
) -> Result<()> {
    let droplets = droplet_list_cluster_instances(doctl, &config.cluster_name)?;
    println!("region={}", config.region);
    for droplet in &droplets {
        let public_ip = droplet.public_ip.as_deref().unwrap_or("N/A");
        println!(
            "name={} instance-id={} state={} region={} public-ip={}",
            droplet.name,
            droplet.id,
            droplet.state,
            droplet.region.as_deref().unwrap_or("N/A"),
            public_ip
        );
    }

    let ssh_entries = droplets
        .iter()
        .map(|droplet| InstanceEntry {
            name: Some(droplet.name.clone()),
            instance_id: droplet.id.to_string(),
            state: droplet.state.clone(),
            public_ip: droplet.public_ip.clone(),
        })
        .collect::<Vec<_>>();
    let identity_file = derive_private_key_path(&config.ssh_public_key_path);
    write_ssh_config(
        &config.ssh_config_path,
        &ssh_entries,
        Some(&config.region),
        None,
        &identity_file,
    )
}

fn droplet_wait_for_state(
    doctl: &DoctlCli,
    cluster: &str,
    name: &str,
    expected_state: &str,
) -> Result<()> {
    for _ in 0..60 {
        if let Some(droplet) = droplet_find_instance(doctl, cluster, name)? {
            if droplet.state.eq_ignore_ascii_case(expected_state) {
                return Ok(());
            }
        }
        sleep(Duration::from_secs(5));
    }
    bail!(
        "timeout waiting for droplet '{}' to become {}",
        name,
        expected_state
    );
}

fn droplet_find_instance(
    doctl: &DoctlCli,
    cluster: &str,
    name: &str,
) -> Result<Option<DropletInfo>> {
    let droplets = droplet_list_cluster_instances(doctl, cluster)?;
    let found = droplets.into_iter().find(|droplet| droplet.name == name);
    Ok(found)
}

fn droplet_list_cluster_instances(doctl: &DoctlCli, cluster: &str) -> Result<Vec<DropletInfo>> {
    let args = vec![
        "compute".to_string(),
        "droplet".to_string(),
        "list".to_string(),
        "--tag-name".to_string(),
        droplet_cluster_tag(cluster),
        "--output".to_string(),
        "json".to_string(),
    ];
    let payload = doctl.run_json(&args)?;
    let list = payload.as_array().cloned().unwrap_or_default();
    let mut droplets = Vec::new();
    for item in list {
        let Some(id) = value_to_u64(item.get("id")) else {
            continue;
        };
        let Some(name) = item.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let state = item
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_string();
        let public_ip = droplet_public_ipv4(&item);
        let region = item
            .get("region")
            .and_then(|value| value.get("slug"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        droplets.push(DropletInfo {
            id,
            name: name.to_string(),
            state,
            public_ip,
            region,
        });
    }
    droplets.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(droplets)
}

fn droplet_public_ipv4(item: &serde_json::Value) -> Option<String> {
    let v4 = item
        .get("networks")
        .and_then(|value| value.get("v4"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for network in v4 {
        let net_type = network
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if net_type == "public" {
            if let Some(ip) = network.get("ip_address").and_then(|value| value.as_str()) {
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn ensure_droplet_ssh_key_fingerprint(
    doctl: &DoctlCli,
    config: &DropletEffectiveConfig,
) -> Result<String> {
    if let Some(fingerprint) = config.ssh_key_fingerprint.as_deref() {
        return Ok(fingerprint.to_string());
    }

    let key_name = format!(
        "vmcli-{}-key",
        sanitize_cloud_identifier(&config.cluster_name)
    );
    let list_args = vec![
        "compute".to_string(),
        "ssh-key".to_string(),
        "list".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    let payload = doctl.run_json(&list_args)?;
    let list = payload.as_array().cloned().unwrap_or_default();
    for item in list {
        let name = item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let fingerprint = item
            .get("fingerprint")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if name == key_name && !fingerprint.is_empty() {
            return Ok(fingerprint.to_string());
        }
    }

    let import_args = vec![
        "compute".to_string(),
        "ssh-key".to_string(),
        "import".to_string(),
        key_name,
        "--public-key-file".to_string(),
        config.ssh_public_key_path.clone(),
        "--output".to_string(),
        "json".to_string(),
    ];
    let payload = doctl.run_json(&import_args)?;
    if let Some(fingerprint) = payload.get("fingerprint").and_then(|value| value.as_str()) {
        return Ok(fingerprint.to_string());
    }
    if let Some(array) = payload.as_array() {
        for item in array {
            if let Some(fingerprint) = item.get("fingerprint").and_then(|value| value.as_str()) {
                return Ok(fingerprint.to_string());
            }
        }
    }
    bail!("unable to resolve imported droplet ssh key fingerprint")
}

fn droplet_cluster_tag(cluster: &str) -> String {
    format!("cluster-{}", sanitize_cloud_identifier(cluster))
}

fn sanitize_cloud_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out = "cluster".to_string();
    }
    if !out
        .chars()
        .next()
        .map(|ch| ch.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        out.insert(0, 'c');
    }
    out
}

fn value_to_string(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(number) = value.as_u64() {
        return Some(number.to_string());
    }
    if let Some(number) = value.as_i64() {
        return Some(number.to_string());
    }
    None
}

fn value_to_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    let value = value?;
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    if let Some(text) = value.as_str() {
        return text.parse::<u64>().ok();
    }
    None
}

fn home_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home))
}

fn default_ssh_private_key_path(config_root: &Path) -> PathBuf {
    config_root.join("vmcli")
}

fn default_ssh_public_key_path(config_root: &Path) -> String {
    config_root.join("vmcli.pub").to_string_lossy().to_string()
}

fn ensure_vmcli_ssh_keypair(config_root: &Path) -> Result<()> {
    fs::create_dir_all(config_root)
        .with_context(|| format!("create config dir {}", config_root.display()))?;

    let private_key_path = default_ssh_private_key_path(config_root);
    let public_key_path = config_root.join("vmcli.pub");
    let private_exists = private_key_path.exists();
    let public_exists = public_key_path.exists();

    if private_exists && public_exists {
        return Ok(());
    }

    if private_exists && !public_exists {
        let output = Command::new("ssh-keygen")
            .arg("-y")
            .arg("-f")
            .arg(&private_key_path)
            .output()
            .with_context(|| "failed to execute ssh-keygen for public key generation")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                bail!(
                    "failed to generate public key from {}",
                    private_key_path.display()
                );
            }
            bail!(
                "failed to generate public key from {}: {}",
                private_key_path.display(),
                stderr
            );
        }
        fs::write(&public_key_path, output.stdout)
            .with_context(|| format!("write {}", public_key_path.display()))?;
        return Ok(());
    }

    if public_exists && !private_exists {
        fs::remove_file(&public_key_path)
            .with_context(|| format!("remove stale {}", public_key_path.display()))?;
    }

    let output = Command::new("ssh-keygen")
        .arg("-q")
        .arg("-t")
        .arg("ed25519")
        .arg("-N")
        .arg("")
        .arg("-f")
        .arg(&private_key_path)
        .arg("-C")
        .arg("vmcli")
        .output()
        .with_context(|| "failed to execute ssh-keygen for key pair generation")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!(
                "failed to generate ssh key pair at {}",
                private_key_path.display()
            );
        }
        bail!(
            "failed to generate ssh key pair at {}: {}",
            private_key_path.display(),
            stderr
        );
    }
    Ok(())
}

fn global_config_path(config_root: &Path) -> PathBuf {
    config_root.join(CONFIG_FILE_NAME)
}

fn provider_cluster_dir(config_root: &Path, provider: &str, cluster: &str) -> Result<PathBuf> {
    Ok(config_root.join(provider).join(cluster))
}

fn provider_cluster_config_path(
    config_root: &Path,
    provider: &str,
    cluster: &str,
) -> Result<PathBuf> {
    Ok(provider_cluster_dir(config_root, provider, cluster)?.join(CONFIG_FILE_NAME))
}

fn provider_cluster_ssh_config_path(
    config_root: &Path,
    provider: &str,
    cluster: &str,
) -> Result<PathBuf> {
    Ok(provider_cluster_dir(config_root, provider, cluster)?.join(SSH_CONFIG_FILE))
}

fn maybe_cleanup_provider_cluster_config(
    config_root: &Path,
    provider: &str,
    cluster: &str,
    force: bool,
) -> Result<()> {
    let cluster_dir = provider_cluster_dir(config_root, provider, cluster)?;
    if !cluster_dir.exists() {
        return Ok(());
    }

    if force {
        fs::remove_dir_all(&cluster_dir)
            .with_context(|| format!("remove config dir {}", cluster_dir.display()))?;
        println!(
            "removed local config dir={} provider={} cluster={}",
            cluster_dir.display(),
            provider,
            cluster
        );
        return Ok(());
    }

    let prompt = format!(
        "No {} instances remain for cluster '{}'. Remove local config dir '{}' ? [y/N]: ",
        provider,
        cluster,
        cluster_dir.display()
    );
    if confirm(&prompt)? {
        fs::remove_dir_all(&cluster_dir)
            .with_context(|| format!("remove config dir {}", cluster_dir.display()))?;
        println!(
            "removed local config dir={} provider={} cluster={}",
            cluster_dir.display(),
            provider,
            cluster
        );
    } else {
        println!(
            "kept local config dir={} provider={} cluster={}",
            cluster_dir.display(),
            provider,
            cluster
        );
    }
    Ok(())
}

fn ec2_cluster_dir(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_dir(config_root, EC2_PROVIDER, cluster)
}

fn ec2_cluster_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_config_path(config_root, EC2_PROVIDER, cluster)
}

fn ec2_cluster_ssh_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_ssh_config_path(config_root, EC2_PROVIDER, cluster)
}

fn lightsail_cluster_dir(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_dir(config_root, LIGHTSAIL_PROVIDER, cluster)
}

fn lightsail_cluster_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_config_path(config_root, LIGHTSAIL_PROVIDER, cluster)
}

fn lightsail_cluster_ssh_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_ssh_config_path(config_root, LIGHTSAIL_PROVIDER, cluster)
}

fn gce_cluster_dir(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_dir(config_root, GCE_PROVIDER, cluster)
}

fn gce_cluster_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_config_path(config_root, GCE_PROVIDER, cluster)
}

fn gce_cluster_ssh_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_ssh_config_path(config_root, GCE_PROVIDER, cluster)
}

fn droplet_cluster_dir(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_dir(config_root, DROPLET_PROVIDER, cluster)
}

fn droplet_cluster_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_config_path(config_root, DROPLET_PROVIDER, cluster)
}

fn droplet_cluster_ssh_config_path(config_root: &Path, cluster: &str) -> Result<PathBuf> {
    provider_cluster_ssh_config_path(config_root, DROPLET_PROVIDER, cluster)
}

fn default_ec2_config_contents(
    cluster: &str,
    region: &str,
    ssh_public_key_path: &str,
    default_instance_type: &str,
) -> String {
    format!(
        "cluster_name = \"{}\"\n\n[ec2]\nregion = \"{}\"\nssh_public_key_path = \"{}\"\ndefault_instance_type = \"{}\"\nami_id = \"\"\n",
        cluster,
        region,
        ssh_public_key_path,
        default_instance_type
    )
}

fn default_lightsail_config_contents(
    cluster: &str,
    region: &str,
    ssh_public_key_path: &str,
    availability_zone: &str,
    default_bundle_id: &str,
    blueprint_id: &str,
) -> String {
    format!(
        "cluster_name = \"{}\"\n\n[lightsail]\nregion = \"{}\"\nssh_public_key_path = \"{}\"\navailability_zone = \"{}\"\ndefault_bundle_id = \"{}\"\nblueprint_id = \"{}\"\nkey_pair_name = \"\"\n",
        cluster, region, ssh_public_key_path, availability_zone, default_bundle_id, blueprint_id
    )
}

fn default_gce_config_contents(
    cluster: &str,
    project: &str,
    zone: &str,
    ssh_public_key_path: &str,
    machine_type: &str,
    image_family: &str,
    image_project: &str,
    ssh_user: &str,
) -> String {
    format!(
        "cluster_name = \"{}\"\n\n[gce]\nproject = \"{}\"\nzone = \"{}\"\nssh_public_key_path = \"{}\"\ndefault_machine_type = \"{}\"\nimage_family = \"{}\"\nimage_project = \"{}\"\nssh_user = \"{}\"\n",
        cluster, project, zone, ssh_public_key_path, machine_type, image_family, image_project, ssh_user
    )
}

fn default_droplet_config_contents(
    cluster: &str,
    region: &str,
    ssh_public_key_path: &str,
    default_size: &str,
    image: &str,
) -> String {
    format!(
        "cluster_name = \"{}\"\n\n[droplet]\nregion = \"{}\"\nssh_public_key_path = \"{}\"\ndefault_size = \"{}\"\nimage = \"{}\"\nssh_key_fingerprint = \"\"\n",
        cluster, region, ssh_public_key_path, default_size, image
    )
}

fn load_global_config(config_root: &Path) -> Result<GlobalConfig> {
    let path = global_config_path(config_root);
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("read config file {}", path.display()))?;
    let mut config: GlobalConfig =
        toml::from_str(&contents).with_context(|| format!("parse config {}", path.display()))?;
    normalize_aws_section(&mut config.ec2);
    normalize_lightsail_section(&mut config.lightsail);
    normalize_gce_section(&mut config.gce);
    normalize_droplet_section(&mut config.droplet);
    Ok(config)
}

fn load_cluster_config(path: &Path, provider: &str) -> Result<ClusterConfig> {
    if !path.exists() {
        bail!(
            "config file {} not found; run 'vmcli {} init <cluster>'",
            path.display(),
            provider
        );
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;
    let mut config: ClusterConfig =
        toml::from_str(&contents).with_context(|| format!("parse config {}", path.display()))?;
    config.cluster_name = normalize_optional(config.cluster_name);
    normalize_aws_section(&mut config.ec2);
    normalize_lightsail_section(&mut config.lightsail);
    normalize_gce_section(&mut config.gce);
    normalize_droplet_section(&mut config.droplet);
    Ok(config)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_aws_section(section: &mut Option<AwsConfigSection>) {
    let Some(ec2) = section.as_mut() else {
        return;
    };
    ec2.region = normalize_optional(ec2.region.take());
    ec2.ssh_public_key_path = normalize_optional(ec2.ssh_public_key_path.take());
    ec2.default_instance_type = normalize_optional(ec2.default_instance_type.take());
    ec2.ami_id = normalize_optional(ec2.ami_id.take());
}

fn normalize_lightsail_section(section: &mut Option<LightsailConfigSection>) {
    let Some(lightsail) = section.as_mut() else {
        return;
    };
    lightsail.region = normalize_optional(lightsail.region.take());
    lightsail.ssh_public_key_path = normalize_optional(lightsail.ssh_public_key_path.take());
    lightsail.availability_zone = normalize_optional(lightsail.availability_zone.take());
    lightsail.default_bundle_id = normalize_optional(lightsail.default_bundle_id.take());
    lightsail.blueprint_id = normalize_optional(lightsail.blueprint_id.take());
    lightsail.key_pair_name = normalize_optional(lightsail.key_pair_name.take());
}

fn normalize_gce_section(section: &mut Option<GceConfigSection>) {
    let Some(gce) = section.as_mut() else {
        return;
    };
    gce.project = normalize_optional(gce.project.take());
    gce.zone = normalize_optional(gce.zone.take());
    gce.ssh_public_key_path = normalize_optional(gce.ssh_public_key_path.take());
    gce.default_machine_type = normalize_optional(gce.default_machine_type.take());
    gce.image_family = normalize_optional(gce.image_family.take());
    gce.image_project = normalize_optional(gce.image_project.take());
    gce.ssh_user = normalize_optional(gce.ssh_user.take());
}

fn normalize_droplet_section(section: &mut Option<DropletConfigSection>) {
    let Some(droplet) = section.as_mut() else {
        return;
    };
    droplet.region = normalize_optional(droplet.region.take());
    droplet.ssh_public_key_path = normalize_optional(droplet.ssh_public_key_path.take());
    droplet.default_size = normalize_optional(droplet.default_size.take());
    droplet.image = normalize_optional(droplet.image.take());
    droplet.ssh_key_fingerprint = normalize_optional(droplet.ssh_key_fingerprint.take());
}

fn merge_aws_section(
    base: Option<AwsConfigSection>,
    overlay: Option<AwsConfigSection>,
) -> AwsConfigSection {
    let mut merged = base.unwrap_or_default();
    let overlay = overlay.unwrap_or_default();
    if overlay.region.is_some() {
        merged.region = overlay.region;
    }
    if overlay.ssh_public_key_path.is_some() {
        merged.ssh_public_key_path = overlay.ssh_public_key_path;
    }
    if overlay.default_instance_type.is_some() {
        merged.default_instance_type = overlay.default_instance_type;
    }
    if overlay.ami_id.is_some() {
        merged.ami_id = overlay.ami_id;
    }
    merged
}

fn merge_lightsail_section(
    base: Option<LightsailConfigSection>,
    overlay: Option<LightsailConfigSection>,
) -> LightsailConfigSection {
    let mut merged = base.unwrap_or_default();
    let overlay = overlay.unwrap_or_default();
    if overlay.region.is_some() {
        merged.region = overlay.region;
    }
    if overlay.ssh_public_key_path.is_some() {
        merged.ssh_public_key_path = overlay.ssh_public_key_path;
    }
    if overlay.availability_zone.is_some() {
        merged.availability_zone = overlay.availability_zone;
    }
    if overlay.default_bundle_id.is_some() {
        merged.default_bundle_id = overlay.default_bundle_id;
    }
    if overlay.blueprint_id.is_some() {
        merged.blueprint_id = overlay.blueprint_id;
    }
    if overlay.key_pair_name.is_some() {
        merged.key_pair_name = overlay.key_pair_name;
    }
    merged
}

fn merge_gce_section(
    base: Option<GceConfigSection>,
    overlay: Option<GceConfigSection>,
) -> GceConfigSection {
    let mut merged = base.unwrap_or_default();
    let overlay = overlay.unwrap_or_default();
    if overlay.project.is_some() {
        merged.project = overlay.project;
    }
    if overlay.zone.is_some() {
        merged.zone = overlay.zone;
    }
    if overlay.ssh_public_key_path.is_some() {
        merged.ssh_public_key_path = overlay.ssh_public_key_path;
    }
    if overlay.default_machine_type.is_some() {
        merged.default_machine_type = overlay.default_machine_type;
    }
    if overlay.image_family.is_some() {
        merged.image_family = overlay.image_family;
    }
    if overlay.image_project.is_some() {
        merged.image_project = overlay.image_project;
    }
    if overlay.ssh_user.is_some() {
        merged.ssh_user = overlay.ssh_user;
    }
    merged
}

fn merge_droplet_section(
    base: Option<DropletConfigSection>,
    overlay: Option<DropletConfigSection>,
) -> DropletConfigSection {
    let mut merged = base.unwrap_or_default();
    let overlay = overlay.unwrap_or_default();
    if overlay.region.is_some() {
        merged.region = overlay.region;
    }
    if overlay.ssh_public_key_path.is_some() {
        merged.ssh_public_key_path = overlay.ssh_public_key_path;
    }
    if overlay.default_size.is_some() {
        merged.default_size = overlay.default_size;
    }
    if overlay.image.is_some() {
        merged.image = overlay.image;
    }
    if overlay.ssh_key_fingerprint.is_some() {
        merged.ssh_key_fingerprint = overlay.ssh_key_fingerprint;
    }
    merged
}

fn load_aws_config(
    config_root: &Path,
    cluster: &str,
    override_path: Option<&str>,
) -> Result<AwsEffectiveConfig> {
    let global_config = load_global_config(config_root)?;
    let cluster_path = match override_path {
        Some(path) => PathBuf::from(path),
        None => ec2_cluster_config_path(config_root, cluster)?,
    };
    let cluster_config = load_cluster_config(&cluster_path, EC2_PROVIDER)?;
    if let Some(name) = cluster_config.cluster_name.as_ref() {
        if name != cluster {
            bail!(
                "cluster_name '{}' does not match requested cluster '{}' in {}",
                name,
                cluster,
                cluster_path.display()
            );
        }
    }

    let merged = merge_aws_section(global_config.ec2, cluster_config.ec2);
    let region = merged
        .region
        .ok_or_else(|| anyhow!("ec2.region must be set in config"))?;
    let ssh_public_key_path = merged
        .ssh_public_key_path
        .ok_or_else(|| anyhow!("ec2.ssh_public_key_path must be set in config"))?;
    let default_instance_type = merged
        .default_instance_type
        .unwrap_or_else(|| DEFAULT_INSTANCE_TYPE.to_string());

    let ssh_config_path = ec2_cluster_ssh_config_path(config_root, cluster)?;

    Ok(AwsEffectiveConfig {
        cluster_name: cluster.to_string(),
        region,
        ssh_public_key_path,
        default_instance_type,
        ami_id: merged.ami_id,
        ssh_config_path,
    })
}

fn load_lightsail_config(
    config_root: &Path,
    cluster: &str,
    override_path: Option<&str>,
) -> Result<LightsailEffectiveConfig> {
    let global_config = load_global_config(config_root)?;
    let cluster_path = match override_path {
        Some(path) => PathBuf::from(path),
        None => lightsail_cluster_config_path(config_root, cluster)?,
    };
    let cluster_config = load_cluster_config(&cluster_path, LIGHTSAIL_PROVIDER)?;
    if let Some(name) = cluster_config.cluster_name.as_ref() {
        if name != cluster {
            bail!(
                "cluster_name '{}' does not match requested cluster '{}' in {}",
                name,
                cluster,
                cluster_path.display()
            );
        }
    }

    let merged = merge_lightsail_section(global_config.lightsail, cluster_config.lightsail);
    let region = merged
        .region
        .unwrap_or_else(|| "ap-northeast-1".to_string());
    let ssh_public_key_path = merged
        .ssh_public_key_path
        .unwrap_or_else(|| default_ssh_public_key_path(config_root));
    let availability_zone = merged
        .availability_zone
        .unwrap_or_else(|| format!("{}a", region));
    let default_bundle_id = merged
        .default_bundle_id
        .unwrap_or_else(|| DEFAULT_LIGHTSAIL_BUNDLE_ID.to_string());
    let blueprint_id = merged
        .blueprint_id
        .unwrap_or_else(|| DEFAULT_LIGHTSAIL_BLUEPRINT_ID.to_string());
    let ssh_config_path = lightsail_cluster_ssh_config_path(config_root, cluster)?;

    Ok(LightsailEffectiveConfig {
        cluster_name: cluster.to_string(),
        region,
        ssh_public_key_path,
        availability_zone,
        default_bundle_id,
        blueprint_id,
        key_pair_name: merged.key_pair_name,
        ssh_config_path,
    })
}

fn load_gce_config(
    config_root: &Path,
    cluster: &str,
    override_path: Option<&str>,
) -> Result<GceEffectiveConfig> {
    let global_config = load_global_config(config_root)?;
    let cluster_path = match override_path {
        Some(path) => PathBuf::from(path),
        None => gce_cluster_config_path(config_root, cluster)?,
    };
    let cluster_config = load_cluster_config(&cluster_path, GCE_PROVIDER)?;
    if let Some(name) = cluster_config.cluster_name.as_ref() {
        if name != cluster {
            bail!(
                "cluster_name '{}' does not match requested cluster '{}' in {}",
                name,
                cluster,
                cluster_path.display()
            );
        }
    }

    let merged = merge_gce_section(global_config.gce, cluster_config.gce);
    let project = match merged.project {
        Some(value) => value,
        None => env::var("GOOGLE_CLOUD_PROJECT")
            .or_else(|_| env::var("GCLOUD_PROJECT"))
            .context("gce.project must be set in config or GOOGLE_CLOUD_PROJECT/GCLOUD_PROJECT")?,
    };
    let zone = merged
        .zone
        .unwrap_or_else(|| "asia-northeast1-a".to_string());
    let ssh_public_key_path = merged
        .ssh_public_key_path
        .unwrap_or_else(|| default_ssh_public_key_path(config_root));
    let default_machine_type = merged
        .default_machine_type
        .unwrap_or_else(|| DEFAULT_GCE_MACHINE_TYPE.to_string());
    let image_family = merged
        .image_family
        .unwrap_or_else(|| DEFAULT_GCE_IMAGE_FAMILY.to_string());
    let image_project = merged
        .image_project
        .unwrap_or_else(|| DEFAULT_GCE_IMAGE_PROJECT.to_string());
    let ssh_user = merged
        .ssh_user
        .unwrap_or_else(|| DEFAULT_GCE_SSH_USER.to_string());
    let ssh_config_path = gce_cluster_ssh_config_path(config_root, cluster)?;

    Ok(GceEffectiveConfig {
        cluster_name: cluster.to_string(),
        project,
        zone,
        ssh_public_key_path,
        default_machine_type,
        image_family,
        image_project,
        ssh_user,
        ssh_config_path,
    })
}

fn load_droplet_config(
    config_root: &Path,
    cluster: &str,
    override_path: Option<&str>,
) -> Result<DropletEffectiveConfig> {
    let global_config = load_global_config(config_root)?;
    let cluster_path = match override_path {
        Some(path) => PathBuf::from(path),
        None => droplet_cluster_config_path(config_root, cluster)?,
    };
    let cluster_config = load_cluster_config(&cluster_path, DROPLET_PROVIDER)?;
    if let Some(name) = cluster_config.cluster_name.as_ref() {
        if name != cluster {
            bail!(
                "cluster_name '{}' does not match requested cluster '{}' in {}",
                name,
                cluster,
                cluster_path.display()
            );
        }
    }

    let merged = merge_droplet_section(global_config.droplet, cluster_config.droplet);
    let region = merged.region.unwrap_or_else(|| "sfo3".to_string());
    let ssh_public_key_path = merged
        .ssh_public_key_path
        .unwrap_or_else(|| default_ssh_public_key_path(config_root));
    let default_size = merged
        .default_size
        .unwrap_or_else(|| DEFAULT_DROPLET_SIZE.to_string());
    let image = merged
        .image
        .unwrap_or_else(|| DEFAULT_DROPLET_IMAGE.to_string());
    let ssh_config_path = droplet_cluster_ssh_config_path(config_root, cluster)?;

    Ok(DropletEffectiveConfig {
        cluster_name: cluster.to_string(),
        region,
        ssh_public_key_path,
        default_size,
        image,
        ssh_key_fingerprint: merged.ssh_key_fingerprint,
        ssh_config_path,
    })
}

fn resolve_instance_type(config: &AwsEffectiveConfig, override_value: Option<String>) -> String {
    override_value.unwrap_or_else(|| config.default_instance_type.clone())
}

fn expand_home_path(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_dir();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(trimmed))
}

fn derive_private_key_path(public_key_path: &str) -> String {
    let trimmed = public_key_path.trim();
    trimmed.strip_suffix(".pub").unwrap_or(trimmed).to_string()
}

fn check_aws_cli() -> Result<()> {
    match Command::new("aws").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    bail!("AWS CLI failed to run")
                } else {
                    bail!("AWS CLI failed to run: {}", stderr)
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            bail!("AWS CLI not found in PATH")
        }
        Err(err) => Err(err).context("failed to execute aws CLI"),
    }
}

fn check_gcloud_cli() -> Result<()> {
    match Command::new("gcloud").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    bail!("gcloud CLI failed to run")
                } else {
                    bail!("gcloud CLI failed to run: {}", stderr)
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            bail!("gcloud CLI not found in PATH")
        }
        Err(err) => Err(err).context("failed to execute gcloud CLI"),
    }
}

fn check_doctl_cli() -> Result<()> {
    match Command::new("doctl").arg("version").output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    bail!("doctl CLI failed to run")
                } else {
                    bail!("doctl CLI failed to run: {}", stderr)
                }
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            bail!("doctl CLI not found in PATH")
        }
        Err(err) => Err(err).context("failed to execute doctl CLI"),
    }
}

fn ensure_no_profile_env() -> Result<()> {
    if env::var_os("AWS_PROFILE").is_some() || env::var_os("AWS_DEFAULT_PROFILE").is_some() {
        bail!("AWS profile is not supported; use AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY");
    }
    Ok(())
}

fn print_banner(aws: &AwsCli) -> Result<()> {
    let access_key_id = match env::var("AWS_ACCESS_KEY_ID") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => "N/A (role/SSO)".to_string(),
    };
    let identity = aws.get_caller_identity()?;
    println!(
        "profile=env region={} access_key_id={} account={} arn={}",
        aws.region, access_key_id, identity.account, identity.arn
    );
    Ok(())
}

fn resource_name(cluster: &str, suffix: &str) -> String {
    format!("{}-{}", cluster, suffix)
}

fn tag_spec(resource_type: &str, name: &str, cluster: &str) -> String {
    format!(
        "ResourceType={},Tags=[{{Key=Name,Value={}}},{{Key=Cluster,Value={}}}]",
        resource_type, name, cluster
    )
}

fn tag_filters(resource_name: &str, cluster: &str) -> Vec<String> {
    vec![
        format!("Name=tag:Name,Values={}", resource_name),
        format!("Name=tag:Cluster,Values={}", cluster),
    ]
}

fn aws_args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

fn append_filters(args: &mut Vec<String>, filters: &[String]) {
    for filter in filters {
        args.push("--filters".to_string());
        args.push(filter.clone());
    }
}

fn find_vpc(aws: &AwsCli, cluster: &str) -> Result<Option<String>> {
    let name = resource_name(cluster, "vpc");
    let mut args = aws_args(&["ec2", "describe-vpcs", "--output", "json"]);
    append_filters(&mut args, &tag_filters(&name, cluster));
    let output = aws.run(&args)?;
    let result: DescribeVpcs = serde_json::from_str(&output).context("parse describe-vpcs")?;
    match result.vpcs.len() {
        0 => Ok(None),
        1 => Ok(Some(result.vpcs[0].vpc_id.clone())),
        _ => bail!("multiple VPCs found for cluster {}", cluster),
    }
}

fn find_subnet(aws: &AwsCli, cluster: &str) -> Result<Option<String>> {
    let name = resource_name(cluster, "subnet");
    let mut args = aws_args(&["ec2", "describe-subnets", "--output", "json"]);
    append_filters(&mut args, &tag_filters(&name, cluster));
    let output = aws.run(&args)?;
    let result: DescribeSubnets =
        serde_json::from_str(&output).context("parse describe-subnets")?;
    match result.subnets.len() {
        0 => Ok(None),
        1 => Ok(Some(result.subnets[0].subnet_id.clone())),
        _ => bail!("multiple subnets found for cluster {}", cluster),
    }
}

fn find_internet_gateway(aws: &AwsCli, cluster: &str) -> Result<Option<InternetGateway>> {
    let name = resource_name(cluster, "igw");
    let mut args = aws_args(&["ec2", "describe-internet-gateways", "--output", "json"]);
    append_filters(&mut args, &tag_filters(&name, cluster));
    let output = aws.run(&args)?;
    let result: DescribeInternetGateways =
        serde_json::from_str(&output).context("parse describe-internet-gateways")?;
    let mut gateways = result.internet_gateways;
    match gateways.len() {
        0 => Ok(None),
        1 => Ok(gateways.pop()),
        _ => bail!("multiple internet gateways found for cluster {}", cluster),
    }
}

fn find_route_table(aws: &AwsCli, cluster: &str) -> Result<Option<RouteTable>> {
    let name = resource_name(cluster, "rt");
    let mut args = aws_args(&["ec2", "describe-route-tables", "--output", "json"]);
    append_filters(&mut args, &tag_filters(&name, cluster));
    let output = aws.run(&args)?;
    let result: DescribeRouteTables =
        serde_json::from_str(&output).context("parse describe-route-tables")?;
    let mut tables = result.route_tables;
    match tables.len() {
        0 => Ok(None),
        1 => Ok(tables.pop()),
        _ => bail!("multiple route tables found for cluster {}", cluster),
    }
}

fn find_security_group(aws: &AwsCli, cluster: &str) -> Result<Option<String>> {
    let name = resource_name(cluster, "sg");
    let mut args = aws_args(&["ec2", "describe-security-groups", "--output", "json"]);
    append_filters(&mut args, &tag_filters(&name, cluster));
    let output = aws.run(&args)?;
    let result: DescribeSecurityGroups =
        serde_json::from_str(&output).context("parse describe-security-groups")?;
    match result.security_groups.len() {
        0 => Ok(None),
        1 => Ok(Some(result.security_groups[0].group_id.clone())),
        _ => bail!("multiple security groups found for cluster {}", cluster),
    }
}

fn ensure_vpc(aws: &AwsCli, config: &AwsEffectiveConfig) -> Result<String> {
    if let Some(vpc_id) = find_vpc(aws, &config.cluster_name)? {
        return Ok(vpc_id);
    }

    let vpc_name = resource_name(&config.cluster_name, "vpc");
    let tag_spec = tag_spec("vpc", &vpc_name, &config.cluster_name);
    let mut args = aws_args(&[
        "ec2",
        "create-vpc",
        "--cidr-block",
        "10.0.0.0/16",
        "--tag-specifications",
    ]);
    args.push(tag_spec);
    args.extend(aws_args(&["--query", "Vpc.VpcId", "--output", "text"]));
    let vpc_id = aws.run(&args)?;
    Ok(vpc_id)
}

fn ensure_subnet(aws: &AwsCli, config: &AwsEffectiveConfig, vpc_id: &str) -> Result<String> {
    let subnet_id = if let Some(existing) = find_subnet(aws, &config.cluster_name)? {
        existing
    } else {
        let subnet_name = resource_name(&config.cluster_name, "subnet");
        let tag_spec = tag_spec("subnet", &subnet_name, &config.cluster_name);
        let mut args = aws_args(&[
            "ec2",
            "create-subnet",
            "--vpc-id",
            vpc_id,
            "--cidr-block",
            "10.0.1.0/24",
            "--tag-specifications",
        ]);
        args.push(tag_spec);
        args.extend(aws_args(&[
            "--query",
            "Subnet.SubnetId",
            "--output",
            "text",
        ]));
        aws.run(&args)?
    };

    let args = aws_args(&[
        "ec2",
        "modify-subnet-attribute",
        "--subnet-id",
        &subnet_id,
        "--map-public-ip-on-launch",
    ]);
    let _ = aws.run(&args)?;
    Ok(subnet_id)
}

fn ensure_internet_gateway(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
    vpc_id: &str,
) -> Result<String> {
    let mut igw = find_internet_gateway(aws, &config.cluster_name)?;
    let igw_id = if let Some(existing) = igw.as_ref() {
        existing.internet_gateway_id.clone()
    } else {
        let igw_name = resource_name(&config.cluster_name, "igw");
        let tag_spec = tag_spec("internet-gateway", &igw_name, &config.cluster_name);
        let mut args = aws_args(&["ec2", "create-internet-gateway", "--tag-specifications"]);
        args.push(tag_spec);
        args.extend(aws_args(&[
            "--query",
            "InternetGateway.InternetGatewayId",
            "--output",
            "text",
        ]));
        let igw_id = aws.run(&args)?;
        igw = Some(InternetGateway {
            internet_gateway_id: igw_id.clone(),
            attachments: None,
        });
        igw_id
    };

    let attached = igw
        .as_ref()
        .and_then(|igw| igw.attachments.as_ref())
        .map(|attachments| {
            attachments.iter().any(|attachment| {
                attachment
                    .vpc_id
                    .as_ref()
                    .map(|id| id == vpc_id)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if !attached {
        let args = aws_args(&[
            "ec2",
            "attach-internet-gateway",
            "--internet-gateway-id",
            &igw_id,
            "--vpc-id",
            vpc_id,
        ]);
        let _ = aws.run(&args)?;
    }

    Ok(igw_id)
}

fn ensure_route_table(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
    vpc_id: &str,
    subnet_id: &str,
    igw_id: &str,
) -> Result<String> {
    let route_table = find_route_table(aws, &config.cluster_name)?;
    let route_table_id = if let Some(existing) = route_table.as_ref() {
        existing.route_table_id.clone()
    } else {
        let rt_name = resource_name(&config.cluster_name, "rt");
        let tag_spec = tag_spec("route-table", &rt_name, &config.cluster_name);
        let mut args = aws_args(&[
            "ec2",
            "create-route-table",
            "--vpc-id",
            vpc_id,
            "--tag-specifications",
        ]);
        args.push(tag_spec);
        args.extend(aws_args(&[
            "--query",
            "RouteTable.RouteTableId",
            "--output",
            "text",
        ]));
        aws.run(&args)?
    };

    ensure_default_route(aws, &route_table_id, igw_id)?;
    ensure_route_table_association(aws, &route_table_id, subnet_id)?;
    Ok(route_table_id)
}

fn ensure_default_route(aws: &AwsCli, route_table_id: &str, igw_id: &str) -> Result<()> {
    let args = aws_args(&[
        "ec2",
        "create-route",
        "--route-table-id",
        route_table_id,
        "--destination-cidr-block",
        "0.0.0.0/0",
        "--gateway-id",
        igw_id,
    ]);

    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("RouteAlreadyExists") || stderr.contains("InvalidRoute.Duplicate") {
        let replace_args = aws_args(&[
            "ec2",
            "replace-route",
            "--route-table-id",
            route_table_id,
            "--destination-cidr-block",
            "0.0.0.0/0",
            "--gateway-id",
            igw_id,
        ]);
        let replace_output = aws.run_output(&replace_args)?;
        if replace_output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&replace_output.stderr)
            .trim()
            .to_string();
        bail!("failed to replace route: {}", stderr);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!("failed to create route: {}", stderr);
}

fn ensure_route_table_association(
    aws: &AwsCli,
    route_table_id: &str,
    subnet_id: &str,
) -> Result<()> {
    let mut args = aws_args(&["ec2", "describe-route-tables", "--output", "json"]);
    append_filters(
        &mut args,
        &[format!("Name=association.subnet-id,Values={}", subnet_id)],
    );
    let output = aws.run(&args)?;
    let result: DescribeRouteTables =
        serde_json::from_str(&output).context("parse describe-route-tables")?;

    for table in result.route_tables {
        if table.route_table_id == route_table_id {
            return Ok(());
        }
        if let Some(associations) = table.associations {
            for association in associations {
                if association
                    .subnet_id
                    .as_deref()
                    .map(|id| id == subnet_id)
                    .unwrap_or(false)
                {
                    if let Some(association_id) = association.association_id {
                        let replace_args = aws_args(&[
                            "ec2",
                            "replace-route-table-association",
                            "--association-id",
                            &association_id,
                            "--route-table-id",
                            route_table_id,
                        ]);
                        let _ = aws.run(&replace_args)?;
                        return Ok(());
                    }
                }
            }
        }
    }

    let assoc_args = aws_args(&[
        "ec2",
        "associate-route-table",
        "--route-table-id",
        route_table_id,
        "--subnet-id",
        subnet_id,
    ]);
    let output = aws.run_output(&assoc_args)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Resource.AlreadyAssociated") {
        return Ok(());
    }
    bail!("failed to associate route table: {}", stderr.trim());
}

fn ensure_security_group(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
    vpc_id: &str,
) -> Result<String> {
    let sg_id = if let Some(existing) = find_security_group(aws, &config.cluster_name)? {
        existing
    } else {
        let sg_name = resource_name(&config.cluster_name, "sg");
        let tag_spec = tag_spec("security-group", &sg_name, &config.cluster_name);
        let mut args = aws_args(&[
            "ec2",
            "create-security-group",
            "--group-name",
            &sg_name,
            "--description",
            "vmcli cluster security group",
            "--vpc-id",
            vpc_id,
            "--tag-specifications",
        ]);
        args.push(tag_spec);
        args.extend(aws_args(&["--query", "GroupId", "--output", "text"]));
        aws.run(&args)?
    };

    for port in [22, 80, 443, 9090, 9091, 9092] {
        authorize_sg_ingress(aws, &sg_id, port)?;
    }

    Ok(sg_id)
}

fn authorize_sg_ingress(aws: &AwsCli, sg_id: &str, port: u16) -> Result<()> {
    let mut args = aws_args(&[
        "ec2",
        "authorize-security-group-ingress",
        "--group-id",
        sg_id,
        "--protocol",
        "tcp",
        "--port",
    ]);
    args.push(port.to_string());
    args.extend(aws_args(&["--cidr", "0.0.0.0/0"]));
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidPermission.Duplicate") {
        return Ok(());
    }
    bail!(
        "failed to authorize security group ingress: {}",
        stderr.trim()
    );
}

fn ensure_key_pair(aws: &AwsCli, config: &AwsEffectiveConfig) -> Result<String> {
    let key_name = resource_name(&config.cluster_name, "key");
    if key_pair_exists(aws, &key_name)? {
        return Ok(key_name);
    }

    let tag_spec = tag_spec("key-pair", &key_name, &config.cluster_name);
    let public_key_path = expand_home_path(&config.ssh_public_key_path)?;
    let public_key_arg = format!("fileb://{}", public_key_path.display());
    let mut args = aws_args(&[
        "ec2",
        "import-key-pair",
        "--key-name",
        &key_name,
        "--public-key-material",
    ]);
    args.push(public_key_arg);
    args.extend(aws_args(&["--tag-specifications"]));
    args.push(tag_spec);
    let _ = aws.run(&args)?;
    Ok(key_name)
}

fn key_pair_exists(aws: &AwsCli, key_name: &str) -> Result<bool> {
    let args = aws_args(&[
        "ec2",
        "describe-key-pairs",
        "--key-names",
        key_name,
        "--output",
        "json",
    ]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidKeyPair.NotFound") {
        return Ok(false);
    }
    bail!("failed to describe key pairs: {}", stderr.trim());
}

fn delete_key_pair_if_exists(aws: &AwsCli, key_name: &str) -> Result<()> {
    if !key_pair_exists(aws, key_name)? {
        return Ok(());
    }
    let args = aws_args(&["ec2", "delete-key-pair", "--key-name", key_name]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidKeyPair.NotFound") {
        return Ok(());
    }
    bail!("failed to delete key pair: {}", stderr.trim());
}

fn resolve_ami_id(aws: &AwsCli, config: &AwsEffectiveConfig) -> Result<String> {
    if let Some(ami_id) = config.ami_id.as_ref() {
        return Ok(ami_id.clone());
    }

    let args = aws_args(&[
        "ssm",
        "get-parameter",
        "--name",
        UBUNTU_2404_AMI_SSM,
        "--query",
        "Parameter.Value",
        "--output",
        "text",
    ]);
    let ami_id = aws.run(&args)?;
    if ami_id.trim().is_empty() {
        bail!("resolved AMI id is empty")
    } else {
        Ok(ami_id)
    }
}

fn disassociate_route_table(aws: &AwsCli, route_table: &RouteTable) -> Result<()> {
    let Some(associations) = route_table.associations.as_ref() else {
        return Ok(());
    };

    for association in associations {
        if association.main.unwrap_or(false) {
            continue;
        }
        let Some(association_id) = association.association_id.as_ref() else {
            continue;
        };
        let args = aws_args(&[
            "ec2",
            "disassociate-route-table",
            "--association-id",
            association_id,
        ]);
        let output = aws.run_output(&args)?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("InvalidAssociationID.NotFound") {
            continue;
        }
        bail!("failed to disassociate route table: {}", stderr.trim());
    }
    Ok(())
}

fn delete_route_table(aws: &AwsCli, route_table_id: &str) -> Result<()> {
    let args = aws_args(&[
        "ec2",
        "delete-route-table",
        "--route-table-id",
        route_table_id,
    ]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidRouteTableID.NotFound") {
        return Ok(());
    }
    bail!("failed to delete route table: {}", stderr.trim());
}

fn delete_subnet(aws: &AwsCli, subnet_id: &str) -> Result<()> {
    let args = aws_args(&["ec2", "delete-subnet", "--subnet-id", subnet_id]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidSubnetID.NotFound") {
        return Ok(());
    }
    bail!("failed to delete subnet: {}", stderr.trim());
}

fn detach_internet_gateway(aws: &AwsCli, igw: &InternetGateway, vpc_id: &str) -> Result<()> {
    let attached = igw
        .attachments
        .as_ref()
        .map(|attachments| {
            attachments.iter().any(|attachment| {
                attachment
                    .vpc_id
                    .as_deref()
                    .map(|id| id == vpc_id)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !attached {
        return Ok(());
    }

    let args = aws_args(&[
        "ec2",
        "detach-internet-gateway",
        "--internet-gateway-id",
        &igw.internet_gateway_id,
        "--vpc-id",
        vpc_id,
    ]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Gateway.NotAttached")
        || stderr.contains("InvalidInternetGatewayID.NotFound")
        || stderr.contains("InvalidVpcID.NotFound")
    {
        return Ok(());
    }
    bail!("failed to detach internet gateway: {}", stderr.trim());
}

fn delete_internet_gateway(aws: &AwsCli, igw_id: &str) -> Result<()> {
    let args = aws_args(&[
        "ec2",
        "delete-internet-gateway",
        "--internet-gateway-id",
        igw_id,
    ]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidInternetGatewayID.NotFound") {
        return Ok(());
    }
    bail!("failed to delete internet gateway: {}", stderr.trim());
}

fn delete_security_group(aws: &AwsCli, sg_id: &str) -> Result<()> {
    let args = aws_args(&["ec2", "delete-security-group", "--group-id", sg_id]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidGroup.NotFound") {
        return Ok(());
    }
    bail!("failed to delete security group: {}", stderr.trim());
}

fn delete_vpc(aws: &AwsCli, vpc_id: &str) -> Result<()> {
    let args = aws_args(&["ec2", "delete-vpc", "--vpc-id", vpc_id]);
    let output = aws.run_output(&args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("InvalidVpcID.NotFound") {
        return Ok(());
    }
    bail!("failed to delete vpc: {}", stderr.trim());
}

fn ensure_no_duplicate_instance(aws: &AwsCli, cluster: &str, name: &str) -> Result<()> {
    let filters = vec![
        format!("Name=tag:Name,Values={}", name),
        format!("Name=tag:Cluster,Values={}", cluster),
        format!("Name=instance-state-name,Values={}", NON_TERMINATED_STATES),
    ];
    let instances = describe_instances(aws, &filters)?;
    if !instances.is_empty() {
        bail!("instance name '{}' already exists", name);
    }
    Ok(())
}

fn find_instance_by_name(aws: &AwsCli, name: &str) -> Result<Instance> {
    let filters = vec![
        format!("Name=tag:Name,Values={}", name),
        format!("Name=instance-state-name,Values={}", NON_TERMINATED_STATES),
    ];
    let instances = describe_instances(aws, &filters)?;
    if instances.is_empty() {
        bail!("no instance found with Name tag {}", name);
    }
    if instances.len() > 1 {
        let ids = instances
            .iter()
            .map(|instance| instance.instance_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("multiple instances found with Name tag {}: {}", name, ids);
    }
    Ok(instances.into_iter().next().unwrap())
}

fn find_instance_by_cluster_and_name(aws: &AwsCli, cluster: &str, name: &str) -> Result<Instance> {
    let filters = vec![
        format!("Name=tag:Name,Values={}", name),
        format!("Name=tag:Cluster,Values={}", cluster),
        format!("Name=instance-state-name,Values={}", NON_TERMINATED_STATES),
    ];
    let instances = describe_instances(aws, &filters)?;
    if instances.is_empty() {
        bail!(
            "no instance found with Name tag {} and Cluster tag {}",
            name,
            cluster
        );
    }
    if instances.len() > 1 {
        let ids = instances
            .iter()
            .map(|instance| instance.instance_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "multiple instances found with Name tag {} and Cluster tag {}: {}",
            name,
            cluster,
            ids
        );
    }
    Ok(instances.into_iter().next().unwrap())
}

fn describe_instances_by_vpc(aws: &AwsCli, vpc_id: &str) -> Result<Vec<Instance>> {
    let filters = vec![
        format!("Name=vpc-id,Values={}", vpc_id),
        format!("Name=instance-state-name,Values={}", NON_TERMINATED_STATES),
    ];
    describe_instances(aws, &filters)
}

fn describe_instances(aws: &AwsCli, filters: &[String]) -> Result<Vec<Instance>> {
    let mut args = aws_args(&["ec2", "describe-instances", "--output", "json"]);
    append_filters(&mut args, filters);
    let output = aws.run(&args)?;
    let result: DescribeInstances =
        serde_json::from_str(&output).context("parse describe-instances")?;
    let mut instances = Vec::new();
    for reservation in result.reservations {
        instances.extend(reservation.instances);
    }
    Ok(instances)
}

fn describe_security_groups_by_ids(
    aws: &AwsCli,
    group_ids: &[String],
) -> Result<Vec<SecurityGroup>> {
    if group_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut args = aws_args(&[
        "ec2",
        "describe-security-groups",
        "--output",
        "json",
        "--group-ids",
    ]);
    args.extend(group_ids.iter().cloned());
    let output = aws.run(&args)?;
    let result: DescribeSecurityGroups =
        serde_json::from_str(&output).context("parse describe-security-groups")?;
    Ok(result.security_groups)
}

fn describe_ec2_status_checks(
    aws: &AwsCli,
    instance_id: &str,
    instance_state: &str,
) -> Result<Ec2StatusChecks> {
    let args = aws_args(&[
        "ec2",
        "describe-instance-status",
        "--include-all-instances",
        "--instance-ids",
        instance_id,
        "--output",
        "json",
    ]);
    let output = aws.run(&args)?;
    let result: DescribeInstanceStatuses =
        serde_json::from_str(&output).context("parse describe-instance-status")?;

    let status_entry = result
        .instance_statuses
        .into_iter()
        .find(|entry| entry.instance_id == instance_id);

    let checks = if let Some(entry) = status_entry {
        let system_status = entry
            .system_status
            .and_then(|s| normalize_optional(s.status))
            .unwrap_or_else(|| "unknown".to_string());
        let instance_status = entry
            .instance_status
            .and_then(|s| normalize_optional(s.status))
            .unwrap_or_else(|| "unknown".to_string());
        let checks_pass = match (system_status.as_str(), instance_status.as_str()) {
            ("ok", "ok") => Some(true),
            ("unknown", _) | (_, "unknown") => None,
            _ => Some(false),
        };
        Ec2StatusChecks {
            system_status,
            instance_status,
            checks_pass,
        }
    } else if instance_state == "stopped"
        || instance_state == "stopping"
        || instance_state == "shutting-down"
    {
        Ec2StatusChecks {
            system_status: "not-applicable".to_string(),
            instance_status: "not-applicable".to_string(),
            checks_pass: None,
        }
    } else {
        Ec2StatusChecks {
            system_status: "unknown".to_string(),
            instance_status: "unknown".to_string(),
            checks_pass: None,
        }
    };

    Ok(checks)
}

fn tag_value(tags: &Option<Vec<Tag>>, key: &str) -> Option<String> {
    tags.as_ref().and_then(|tags| {
        tags.iter()
            .find(|tag| tag.key == key)
            .map(|tag| tag.value.clone())
    })
}

fn instance_availability_zone(instance: &Instance) -> Option<&str> {
    instance
        .placement
        .as_ref()
        .and_then(|placement| placement.availability_zone.as_deref())
}

fn instance_security_group_ids(instance: &Instance) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(groups) = instance.security_groups.as_ref() {
        for group in groups {
            let Some(group_id) = group.group_id.as_ref() else {
                continue;
            };
            if !ids.iter().any(|id| id == group_id) {
                ids.push(group_id.clone());
            }
        }
    }
    ids
}

fn classify_sg_port_22(security_groups: &[SecurityGroup]) -> SgPort22Status {
    if security_groups.is_empty() {
        return SgPort22Status::Unknown;
    }

    let mut has_restricted = false;
    for sg in security_groups {
        let Some(permissions) = sg.ip_permissions.as_ref() else {
            continue;
        };
        for permission in permissions {
            if !permission_allows_tcp_port(permission, 22) {
                continue;
            }
            if permission_has_world_source(permission) {
                return SgPort22Status::OpenWorld;
            }
            if permission_has_any_source(permission) {
                has_restricted = true;
            }
        }
    }

    if has_restricted {
        SgPort22Status::Restricted
    } else {
        SgPort22Status::Closed
    }
}

fn permission_allows_tcp_port(permission: &IpPermission, port: i64) -> bool {
    let protocol = permission
        .ip_protocol
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if protocol == "-1" {
        return true;
    }
    if protocol != "tcp" {
        return false;
    }

    let from = permission.from_port.unwrap_or(port);
    let to = permission.to_port.unwrap_or(port);
    from <= port && port <= to
}

fn permission_has_world_source(permission: &IpPermission) -> bool {
    let has_world_v4 = permission
        .ip_ranges
        .as_ref()
        .map(|ranges| {
            ranges.iter().any(|range| {
                range
                    .cidr_ip
                    .as_deref()
                    .map(|cidr| cidr == "0.0.0.0/0")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    let has_world_v6 = permission
        .ipv6_ranges
        .as_ref()
        .map(|ranges| {
            ranges.iter().any(|range| {
                range
                    .cidr_ipv6
                    .as_deref()
                    .map(|cidr| cidr == "::/0")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    has_world_v4 || has_world_v6
}

fn permission_has_any_source(permission: &IpPermission) -> bool {
    permission
        .ip_ranges
        .as_ref()
        .map(|ranges| ranges.iter().any(|range| range.cidr_ip.is_some()))
        .unwrap_or(false)
        || permission
            .ipv6_ranges
            .as_ref()
            .map(|ranges| ranges.iter().any(|range| range.cidr_ipv6.is_some()))
            .unwrap_or(false)
        || permission
            .user_id_group_pairs
            .as_ref()
            .map(|pairs| pairs.iter().any(|pair| pair.group_id.is_some()))
            .unwrap_or(false)
        || permission
            .prefix_list_ids
            .as_ref()
            .map(|ids| ids.iter().any(|id| id.prefix_list_id.is_some()))
            .unwrap_or(false)
}

fn eic_send_key_skip_reason(
    instance_running: bool,
    public_ip_present: bool,
    az_present: bool,
    public_key_exists: bool,
) -> Option<&'static str> {
    if !instance_running {
        return Some("instance-not-running");
    }
    if !public_ip_present {
        return Some("no-public-ip");
    }
    if !az_present {
        return Some("availability-zone-missing");
    }
    if !public_key_exists {
        return Some("ssh-public-key-not-found");
    }
    None
}

fn tri_bool_to_str(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn aws_error_text(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match (stderr.is_empty(), stdout.is_empty()) {
        (true, true) => "unknown aws cli error".to_string(),
        (false, true) => stderr,
        (true, false) => stdout,
        (false, false) => format!("{} | {}", stderr, stdout),
    }
}

fn is_access_denied_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("accessdenied")
        || lower.contains("access denied")
        || lower.contains("unauthorizedoperation")
        || lower.contains("not authorized")
}

fn run_eic_probe(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
    instance: &Instance,
    sg_port22: SgPort22Status,
    os_user: &str,
) -> Result<EicProbeResult> {
    let instance_running = instance.state.name == "running";
    let public_ip_present = instance
        .public_ip
        .as_deref()
        .map(|ip| !ip.trim().is_empty())
        .unwrap_or(false);
    let availability_zone = instance_availability_zone(instance).map(|az| az.to_string());
    let az_present = availability_zone.is_some();
    let public_key_path = expand_home_path(&config.ssh_public_key_path)?;
    let public_key_exists = public_key_path.exists();

    let mut result = EicProbeResult {
        os_user: os_user.to_string(),
        public_ip_present,
        instance_running,
        az_present,
        sg_port22,
        send_ssh_public_key: ProbeOutcome::Skipped,
        send_ssh_public_key_reason: None,
    };

    if let Some(reason) = eic_send_key_skip_reason(
        instance_running,
        public_ip_present,
        az_present,
        public_key_exists,
    ) {
        result.send_ssh_public_key_reason = Some(reason.to_string());
        return Ok(result);
    }

    let mut args = aws_args(&[
        "ec2-instance-connect",
        "send-ssh-public-key",
        "--instance-id",
    ]);
    args.push(instance.instance_id.clone());
    args.extend(aws_args(&["--instance-os-user"]));
    args.push(os_user.to_string());
    args.extend(aws_args(&["--availability-zone"]));
    args.push(availability_zone.unwrap());
    args.extend(aws_args(&["--ssh-public-key"]));
    args.push(format!("file://{}", public_key_path.display()));
    args.extend(aws_args(&["--output", "json"]));

    let output = aws.run_output(&args)?;
    if !output.status.success() {
        let message = aws_error_text(&output);
        if is_access_denied_error(&message) {
            bail!(
                "ec2-instance-connect send-ssh-public-key failed: {}",
                message
            );
        }
        result.send_ssh_public_key = ProbeOutcome::Failed;
        result.send_ssh_public_key_reason = Some(message);
        return Ok(result);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let parsed: EicSendSshPublicKeyResponse =
        serde_json::from_str(&stdout).context("parse ec2-instance-connect send-ssh-public-key")?;
    if parsed.success.unwrap_or(false) {
        result.send_ssh_public_key = ProbeOutcome::Success;
    } else {
        result.send_ssh_public_key = ProbeOutcome::Failed;
        result.send_ssh_public_key_reason = Some("success=false".to_string());
    }

    Ok(result)
}

fn summarize_health(
    instance_state: &str,
    ec2_checks_pass: Option<bool>,
    eic_probe: &EicProbeResult,
) -> HealthSummary {
    if instance_state != "running" {
        return HealthSummary {
            level: HealthLevel::Unreachable,
            ssh_local_problem_likely: Some(false),
            notes: "instance-not-running".to_string(),
        };
    }

    if ec2_checks_pass == Some(false) {
        return HealthSummary {
            level: HealthLevel::Degraded,
            ssh_local_problem_likely: Some(false),
            notes: "ec2-status-checks-not-passing".to_string(),
        };
    }

    let remote_probe_success = eic_probe.send_ssh_public_key == ProbeOutcome::Success;

    if ec2_checks_pass == Some(true) && remote_probe_success {
        return HealthSummary {
            level: HealthLevel::Ok,
            ssh_local_problem_likely: Some(true),
            notes: "aws-control-plane-probe-succeeded".to_string(),
        };
    }

    if eic_probe.sg_port22 == SgPort22Status::Closed {
        return HealthSummary {
            level: HealthLevel::Degraded,
            ssh_local_problem_likely: Some(false),
            notes: "security-group-port-22-closed".to_string(),
        };
    }

    if ec2_checks_pass.is_none() {
        return HealthSummary {
            level: if remote_probe_success {
                HealthLevel::Degraded
            } else {
                HealthLevel::Unknown
            },
            ssh_local_problem_likely: None,
            notes: if remote_probe_success {
                "ec2-status-checks-unknown".to_string()
            } else {
                "ec2-status-checks-unknown-and-no-remote-probe-confirmation".to_string()
            },
        };
    }

    HealthSummary {
        level: HealthLevel::Degraded,
        ssh_local_problem_likely: Some(false),
        notes: "instance-running-but-remote-probe-not-confirmed".to_string(),
    }
}

fn one_line_value(value: &str) -> String {
    value.replace('\n', "\\n")
}

fn print_health_report(
    cluster: &str,
    requested_name: &str,
    instance: &Instance,
    ec2_checks: &Ec2StatusChecks,
    eic_probe: &EicProbeResult,
    summary: &HealthSummary,
) {
    let resolved_name =
        tag_value(&instance.tags, "Name").unwrap_or_else(|| requested_name.to_string());
    let az = instance_availability_zone(instance).unwrap_or("N/A");
    let public_ip = instance.public_ip.as_deref().unwrap_or("N/A");
    let private_ip = instance.private_ip.as_deref().unwrap_or("N/A");
    let vpc_id = instance.vpc_id.as_deref().unwrap_or("N/A");
    let subnet_id = instance.subnet_id.as_deref().unwrap_or("N/A");
    let security_groups = instance_security_group_ids(instance);
    let security_groups_display = if security_groups.is_empty() {
        "N/A".to_string()
    } else {
        security_groups.join(",")
    };

    println!("cluster={}", cluster);
    println!("name={}", resolved_name);
    println!("instance-id={}", instance.instance_id);
    println!("state={}", instance.state.name);
    println!("az={}", az);
    println!("vpc-id={}", vpc_id);
    println!("subnet-id={}", subnet_id);
    println!("public-ip={}", public_ip);
    println!("private-ip={}", private_ip);
    println!("security-groups={}", security_groups_display);

    println!("ec2.system-status={}", ec2_checks.system_status);
    println!("ec2.instance-status={}", ec2_checks.instance_status);
    println!(
        "ec2.status-checks-pass={}",
        tri_bool_to_str(ec2_checks.checks_pass)
    );

    println!("eic.support-path=aws ec2-instance-connect send-ssh-public-key");
    println!("eic.os-user={}", eic_probe.os_user);
    println!("eic.public-ip-present={}", eic_probe.public_ip_present);
    println!("eic.instance-running={}", eic_probe.instance_running);
    println!("eic.az-present={}", eic_probe.az_present);
    println!("eic.sg-port22={}", eic_probe.sg_port22.as_str());
    println!(
        "eic.send-ssh-public-key={}",
        eic_probe.send_ssh_public_key.as_str()
    );
    if let Some(reason) = eic_probe.send_ssh_public_key_reason.as_deref() {
        println!("eic.send-ssh-public-key-reason={}", one_line_value(reason));
    }

    println!("summary.health={}", summary.level.as_str());
    println!(
        "summary.ssh-local-problem-likely={}",
        tri_bool_to_str(summary.ssh_local_problem_likely)
    );
    println!("summary.notes={}", summary.notes);
}

fn launch_instance(
    aws: &AwsCli,
    config: &AwsEffectiveConfig,
    name: &str,
    ami_id: &str,
    instance_type: &str,
    subnet_id: &str,
    sg_id: &str,
    key_name: &str,
) -> Result<String> {
    let tag_spec = format!(
        "ResourceType=instance,Tags=[{{Key=Name,Value={}}},{{Key=Cluster,Value={}}}]",
        name, config.cluster_name
    );
    let mut args = aws_args(&[
        "ec2",
        "run-instances",
        "--image-id",
        ami_id,
        "--instance-type",
        instance_type,
        "--key-name",
        key_name,
        "--subnet-id",
        subnet_id,
        "--security-group-ids",
        sg_id,
        "--count",
        "1",
        "--tag-specifications",
    ]);
    args.push(tag_spec);
    args.extend(aws_args(&[
        "--query",
        "Instances[0].InstanceId",
        "--output",
        "text",
    ]));
    aws.run(&args)
}

fn wait_for_instance_running(aws: &AwsCli, instance_id: &str) -> Result<()> {
    let args = aws_args(&[
        "ec2",
        "wait",
        "instance-running",
        "--instance-ids",
        instance_id,
    ]);
    let _ = aws.run(&args)?;
    Ok(())
}

fn fetch_instance_public_ip(aws: &AwsCli, instance_id: &str) -> Result<Option<String>> {
    let args = aws_args(&[
        "ec2",
        "describe-instances",
        "--instance-ids",
        instance_id,
        "--output",
        "json",
    ]);
    let output = aws.run(&args)?;
    let result: DescribeInstances =
        serde_json::from_str(&output).context("parse describe-instances")?;
    for reservation in result.reservations {
        for instance in reservation.instances {
            return Ok(instance.public_ip);
        }
    }
    Ok(None)
}

fn terminate_instance(aws: &AwsCli, instance_id: &str) -> Result<()> {
    let args = aws_args(&["ec2", "terminate-instances", "--instance-ids", instance_id]);
    let _ = aws.run(&args)?;
    Ok(())
}

fn reboot_instance(aws: &AwsCli, instance_id: &str) -> Result<()> {
    let args = aws_args(&["ec2", "reboot-instances", "--instance-ids", instance_id]);
    let _ = aws.run(&args)?;
    Ok(())
}

fn wait_for_instance_terminated(aws: &AwsCli, instance_id: &str) -> Result<()> {
    let args = aws_args(&[
        "ec2",
        "wait",
        "instance-terminated",
        "--instance-ids",
        instance_id,
    ]);
    let _ = aws.run(&args)?;
    Ok(())
}

fn write_ssh_config(
    path: &Path,
    entries: &[InstanceEntry],
    vpc_id: Option<&str>,
    sg_id: Option<&str>,
    identity_file: &str,
) -> Result<()> {
    let mut lines = Vec::new();
    lines.push(format!("# vpc-id: {}", vpc_id.unwrap_or("N/A")));
    lines.push(format!("# sg-id: {}", sg_id.unwrap_or("N/A")));
    lines.push(String::new());

    for entry in entries {
        if entry.name.is_none() || entry.public_ip.is_none() {
            continue;
        }
        lines.push(format!("Host {}", entry.display_name()));
        lines.push(format!("  HostName {}", entry.public_ip.as_ref().unwrap()));
        lines.push("  User ubuntu".to_string());
        lines.push("  IdentitiesOnly yes".to_string());
        lines.push(format!("  IdentityFile {}", identity_file));
        lines.push(String::new());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config dir {}", parent.display()))?;
    }
    fs::write(path, lines.join("\n"))
        .with_context(|| format!("write ssh config {}", path.display()))?;
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush().context("flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("read confirmation")?;
    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sg_with_permissions(permissions: Vec<IpPermission>) -> SecurityGroup {
        SecurityGroup {
            group_id: "sg-test".to_string(),
            ip_permissions: Some(permissions),
        }
    }

    fn tcp22_permission_with_ipv4(cidr: &str) -> IpPermission {
        IpPermission {
            ip_protocol: Some("tcp".to_string()),
            from_port: Some(22),
            to_port: Some(22),
            ip_ranges: Some(vec![IpRange {
                cidr_ip: Some(cidr.to_string()),
            }]),
            ipv6_ranges: None,
            user_id_group_pairs: None,
            prefix_list_ids: None,
        }
    }

    fn eic_probe_stub(sg_port22: SgPort22Status, send_result: ProbeOutcome) -> EicProbeResult {
        EicProbeResult {
            os_user: DEFAULT_INSTANCE_OS_USER.to_string(),
            public_ip_present: true,
            instance_running: true,
            az_present: true,
            sg_port22,
            send_ssh_public_key: send_result,
            send_ssh_public_key_reason: None,
        }
    }

    #[test]
    fn cli_parses_health_command_defaults() {
        let cli = Cli::try_parse_from(["vmcli", "ec2", "health", "dev-cluster", "web-1"])
            .expect("parse health args");
        assert_eq!(cli.config_dir, DEFAULT_CONFIG_DIR);

        match cli.command {
            TopCommand::Ec2(ec2) => match ec2.command {
                Ec2Command::Health(args) => {
                    assert_eq!(args.cluster, "dev-cluster");
                    assert_eq!(args.name, "web-1");
                    assert_eq!(args.os_user, DEFAULT_INSTANCE_OS_USER);
                    assert!(args.config.is_none());
                }
                _ => panic!("expected health command"),
            },
            _ => panic!("expected ec2 command"),
        }
    }

    #[test]
    fn cli_parses_health_command_overrides() {
        let cli = Cli::try_parse_from([
            "vmcli",
            "--config-dir",
            "/tmp/vmcli-custom",
            "ec2",
            "health",
            "dev-cluster",
            "web-1",
            "--os-user",
            "ec2-user",
            "-c",
            "/tmp/config.toml",
        ])
        .expect("parse health args with overrides");
        assert_eq!(cli.config_dir, "/tmp/vmcli-custom");

        match cli.command {
            TopCommand::Ec2(ec2) => match ec2.command {
                Ec2Command::Health(args) => {
                    assert_eq!(args.os_user, "ec2-user");
                    assert_eq!(args.config.as_deref(), Some("/tmp/config.toml"));
                }
                _ => panic!("expected health command"),
            },
            _ => panic!("expected ec2 command"),
        }
    }

    #[test]
    fn cli_parses_new_provider_commands() {
        let lightsail = Cli::try_parse_from(["vmcli", "lightsail", "status", "dev"]).unwrap();
        match lightsail.command {
            TopCommand::Lightsail(args) => match args.command {
                LightsailCommand::Status(status) => assert_eq!(status.cluster, "dev"),
                _ => panic!("expected lightsail status"),
            },
            _ => panic!("expected lightsail command"),
        }

        let gce = Cli::try_parse_from(["vmcli", "gce", "up", "dev", "web-1"]).unwrap();
        match gce.command {
            TopCommand::Gce(args) => match args.command {
                GceCommand::Up(up) => {
                    assert_eq!(up.cluster, "dev");
                    assert_eq!(up.name, "web-1");
                }
                _ => panic!("expected gce up"),
            },
            _ => panic!("expected gce command"),
        }

        let droplet = Cli::try_parse_from(["vmcli", "droplet", "prune", "dev", "-f"]).unwrap();
        match droplet.command {
            TopCommand::Droplet(args) => match args.command {
                DropletCommand::Prune(prune) => {
                    assert_eq!(prune.cluster, "dev");
                    assert!(prune.force);
                }
                _ => panic!("expected droplet prune"),
            },
            _ => panic!("expected droplet command"),
        }
    }

    #[test]
    fn cli_parses_provider_specific_up_flags() {
        let lightsail = Cli::try_parse_from([
            "vmcli",
            "lightsail",
            "up",
            "dev",
            "web-1",
            "--bundle-id",
            "nano_2_0",
        ])
        .unwrap();
        match lightsail.command {
            TopCommand::Lightsail(args) => match args.command {
                LightsailCommand::Up(up) => {
                    assert_eq!(up.bundle_id.as_deref(), Some("nano_2_0"));
                }
                _ => panic!("expected lightsail up"),
            },
            _ => panic!("expected lightsail command"),
        }

        let gce = Cli::try_parse_from([
            "vmcli",
            "gce",
            "up",
            "dev",
            "web-1",
            "--machine-type",
            "e2-small",
        ])
        .unwrap();
        match gce.command {
            TopCommand::Gce(args) => match args.command {
                GceCommand::Up(up) => {
                    assert_eq!(up.machine_type.as_deref(), Some("e2-small"));
                }
                _ => panic!("expected gce up"),
            },
            _ => panic!("expected gce command"),
        }

        let droplet = Cli::try_parse_from([
            "vmcli",
            "droplet",
            "up",
            "dev",
            "web-1",
            "--size",
            "s-2vcpu-2gb",
        ])
        .unwrap();
        match droplet.command {
            TopCommand::Droplet(args) => match args.command {
                DropletCommand::Up(up) => {
                    assert_eq!(up.size.as_deref(), Some("s-2vcpu-2gb"));
                }
                _ => panic!("expected droplet up"),
            },
            _ => panic!("expected droplet command"),
        }
    }

    #[test]
    fn cli_parses_region_discovery_commands() {
        let ec2 = Cli::try_parse_from(["vmcli", "ec2", "regions"]).unwrap();
        match ec2.command {
            TopCommand::Ec2(args) => match args.command {
                Ec2Command::Regions(regions) => assert!(!regions.json),
                _ => panic!("expected ec2 regions"),
            },
            _ => panic!("expected ec2 command"),
        }

        let zones =
            Cli::try_parse_from(["vmcli", "gce", "zones", "--region", "us-central1", "--json"])
                .unwrap();
        match zones.command {
            TopCommand::Gce(args) => match args.command {
                GceCommand::Zones(zones) => {
                    assert_eq!(zones.region.as_deref(), Some("us-central1"));
                    assert!(zones.json);
                }
                _ => panic!("expected gce zones"),
            },
            _ => panic!("expected gce command"),
        }
    }

    #[test]
    fn cli_rejects_generic_instance_type_for_non_ec2() {
        let lightsail = Cli::try_parse_from([
            "vmcli",
            "lightsail",
            "up",
            "dev",
            "web-1",
            "--instance-type",
            "nano_2_0",
        ]);
        assert!(lightsail.is_err());
    }

    #[test]
    fn classify_sg_port_22_open_world() {
        let sg = sg_with_permissions(vec![tcp22_permission_with_ipv4("0.0.0.0/0")]);
        assert_eq!(classify_sg_port_22(&[sg]), SgPort22Status::OpenWorld);
    }

    #[test]
    fn classify_sg_port_22_restricted() {
        let sg = sg_with_permissions(vec![tcp22_permission_with_ipv4("10.0.0.0/8")]);
        assert_eq!(classify_sg_port_22(&[sg]), SgPort22Status::Restricted);
    }

    #[test]
    fn classify_sg_port_22_closed() {
        let sg = sg_with_permissions(vec![IpPermission {
            ip_protocol: Some("tcp".to_string()),
            from_port: Some(80),
            to_port: Some(80),
            ip_ranges: Some(vec![IpRange {
                cidr_ip: Some("0.0.0.0/0".to_string()),
            }]),
            ipv6_ranges: None,
            user_id_group_pairs: None,
            prefix_list_ids: None,
        }]);
        assert_eq!(classify_sg_port_22(&[sg]), SgPort22Status::Closed);
    }

    #[test]
    fn eic_skip_reason_precedence_and_ready_state() {
        assert_eq!(
            eic_send_key_skip_reason(false, true, true, true),
            Some("instance-not-running")
        );
        assert_eq!(
            eic_send_key_skip_reason(true, false, true, true),
            Some("no-public-ip")
        );
        assert_eq!(
            eic_send_key_skip_reason(true, true, false, true),
            Some("availability-zone-missing")
        );
        assert_eq!(
            eic_send_key_skip_reason(true, true, true, false),
            Some("ssh-public-key-not-found")
        );
        assert_eq!(eic_send_key_skip_reason(true, true, true, true), None);
    }

    #[test]
    fn summarize_health_ok_when_control_plane_probe_succeeds() {
        let eic = eic_probe_stub(SgPort22Status::OpenWorld, ProbeOutcome::Success);
        let summary = summarize_health("running", Some(true), &eic);
        assert_eq!(summary.level, HealthLevel::Ok);
        assert_eq!(summary.ssh_local_problem_likely, Some(true));
    }

    #[test]
    fn summarize_health_degraded_when_ssh_port_closed() {
        let eic = eic_probe_stub(SgPort22Status::Closed, ProbeOutcome::Failed);
        let summary = summarize_health("running", Some(true), &eic);
        assert_eq!(summary.level, HealthLevel::Degraded);
        assert_eq!(summary.notes, "security-group-port-22-closed");
        assert_eq!(summary.ssh_local_problem_likely, Some(false));
    }

    #[test]
    fn summarize_health_unreachable_when_instance_not_running() {
        let eic = eic_probe_stub(SgPort22Status::Restricted, ProbeOutcome::Skipped);
        let summary = summarize_health("stopped", None, &eic);
        assert_eq!(summary.level, HealthLevel::Unreachable);
        assert_eq!(summary.notes, "instance-not-running");
    }

    #[test]
    fn summarize_health_unknown_when_checks_unknown_and_no_probe_success() {
        let eic = eic_probe_stub(SgPort22Status::Restricted, ProbeOutcome::Skipped);
        let summary = summarize_health("running", None, &eic);
        assert_eq!(summary.level, HealthLevel::Unknown);
        assert_eq!(summary.ssh_local_problem_likely, None);
    }

    #[test]
    fn expand_home_path_supports_tilde() {
        let home = home_dir().expect("home dir");
        assert_eq!(expand_home_path("~").expect("expand ~"), home);
        assert_eq!(
            expand_home_path("~/.ssh/vmcli.pub").expect("expand ~/"),
            home.join(".ssh").join("vmcli.pub")
        );
        assert_eq!(
            expand_home_path("/tmp/vmcli.pub").expect("expand absolute"),
            PathBuf::from("/tmp/vmcli.pub")
        );
    }

    #[test]
    fn default_config_contents_keeps_tilde_public_key_path() {
        let contents = default_ec2_config_contents(
            "dev-cluster",
            "ap-northeast-1",
            "~/.ssh/vmcli.pub",
            "t3.micro",
        );
        assert!(contents.contains("ssh_public_key_path = \"~/.ssh/vmcli.pub\""));
    }

    #[test]
    fn default_ssh_public_key_path_uses_config_dir() {
        let path = default_ssh_public_key_path(Path::new("/tmp/vmcli-alt-config"));
        assert_eq!(path, "/tmp/vmcli-alt-config/vmcli.pub");
    }
}
