use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CONFIG_FILE_NAME: &str = "config.toml";
const SSH_CONFIG_FILE: &str = "ssh_config";
const AWS_PROVIDER: &str = "aws";
const DEFAULT_INSTANCE_TYPE: &str = "t3.micro";
const DEFAULT_INSTANCE_OS_USER: &str = "ubuntu";
const DEFAULT_SSH_PUBLIC_KEY_PATH: &str = "~/.ssh/vmcli.pub";
const UBUNTU_2404_AMI_SSM: &str =
    "/aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id";
const NON_TERMINATED_STATES: &str = "pending,running,stopping,stopped,shutting-down";

#[derive(Parser)]
#[command(name = "vmcli", version, about = "vmcli multi-cloud helper")]
struct Cli {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Subcommand)]
enum TopCommand {
    Aws(AwsArgs),
}

#[derive(Args)]
struct AwsArgs {
    #[command(subcommand)]
    command: AwsCommand,
}

#[derive(Subcommand)]
enum AwsCommand {
    Init(AwsInitArgs),
    Up(AwsUpArgs),
    Status(AwsStatusArgs),
    Health(AwsHealthArgs),
    Reboot(AwsRebootArgs),
    Destroy(AwsDestroyArgs),
    Prune(AwsPruneArgs),
}

#[derive(Args)]
struct AwsInitArgs {
    cluster: String,
}

#[derive(Args)]
struct AwsUpArgs {
    cluster: String,
    name: String,
    #[arg(short = 'T', long = "instance-type")]
    instance_type: Option<String>,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct AwsDestroyArgs {
    cluster: String,
    name: String,
    #[arg(short = 'f', long = "force")]
    force: bool,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct AwsRebootArgs {
    cluster: String,
    name: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct AwsStatusArgs {
    cluster: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Args)]
struct AwsHealthArgs {
    cluster: String,
    name: String,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
    #[arg(long = "os-user", default_value = DEFAULT_INSTANCE_OS_USER)]
    os_user: String,
}

#[derive(Args)]
struct AwsPruneArgs {
    cluster: String,
    #[arg(short = 'f', long = "force")]
    force: bool,
    #[arg(short = 'c', long = "config")]
    config: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct GlobalConfig {
    aws: Option<AwsConfigSection>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct ClusterConfig {
    cluster_name: Option<String>,
    aws: Option<AwsConfigSection>,
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

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {:#}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        TopCommand::Aws(aws) => match aws.command {
            AwsCommand::Init(args) => run_aws_init(args),
            AwsCommand::Up(args) => run_aws_up(args),
            AwsCommand::Status(args) => run_aws_status(args),
            AwsCommand::Health(args) => run_aws_health(args),
            AwsCommand::Reboot(args) => run_aws_reboot(args),
            AwsCommand::Destroy(args) => run_aws_destroy(args),
            AwsCommand::Prune(args) => run_aws_prune(args),
        },
    }
}

fn run_aws_up(args: AwsUpArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
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
    Ok(())
}

fn run_aws_reboot(args: AwsRebootArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
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

fn run_aws_health(args: AwsHealthArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
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

fn run_aws_destroy(args: AwsDestroyArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
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
    Ok(())
}

fn run_aws_status(args: AwsStatusArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

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

fn run_aws_prune(args: AwsPruneArgs) -> Result<()> {
    ensure_no_profile_env()?;
    check_aws_cli()?;
    let config = load_aws_config(&args.cluster, args.config.as_deref())?;
    let region = config.region.clone();
    let aws = AwsCli::new(region);
    print_banner(&aws)?;

    let vpc_id = match find_vpc(&aws, &config.cluster_name)? {
        Some(vpc_id) => vpc_id,
        None => {
            println!("nothing to prune");
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
    Ok(())
}

fn run_aws_init(args: AwsInitArgs) -> Result<()> {
    let config_dir = aws_cluster_dir(&args.cluster)?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("create config dir {}", config_dir.display()))?;

    let config_path = config_dir.join(CONFIG_FILE_NAME);
    let ssh_config_path = config_dir.join(SSH_CONFIG_FILE);

    if !config_path.exists() {
        let defaults = load_global_config()?;
        let public_key_path = defaults
            .aws
            .as_ref()
            .and_then(|aws| aws.ssh_public_key_path.clone())
            .unwrap_or_else(|| DEFAULT_SSH_PUBLIC_KEY_PATH.to_string());
        let region = defaults
            .aws
            .as_ref()
            .and_then(|aws| aws.region.clone())
            .unwrap_or_else(|| "ap-northeast-1".to_string());
        let default_instance_type = defaults
            .aws
            .as_ref()
            .and_then(|aws| aws.default_instance_type.clone())
            .unwrap_or_else(|| DEFAULT_INSTANCE_TYPE.to_string());
        let contents = default_config_contents(
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

fn home_dir() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home))
}

fn config_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config").join("vmcli"))
}

fn global_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILE_NAME))
}

fn aws_cluster_dir(cluster: &str) -> Result<PathBuf> {
    Ok(config_dir()?.join(AWS_PROVIDER).join(cluster))
}

fn aws_cluster_config_path(cluster: &str) -> Result<PathBuf> {
    Ok(aws_cluster_dir(cluster)?.join(CONFIG_FILE_NAME))
}

fn aws_cluster_ssh_config_path(cluster: &str) -> Result<PathBuf> {
    Ok(aws_cluster_dir(cluster)?.join(SSH_CONFIG_FILE))
}

fn default_config_contents(
    cluster: &str,
    region: &str,
    ssh_public_key_path: &str,
    default_instance_type: &str,
) -> String {
    format!(
        "cluster_name = \"{}\"\n\n[aws]\nregion = \"{}\"\nssh_public_key_path = \"{}\"\ndefault_instance_type = \"{}\"\nami_id = \"\"\n",
        cluster,
        region,
        ssh_public_key_path,
        default_instance_type
    )
}

fn load_global_config() -> Result<GlobalConfig> {
    let path = global_config_path()?;
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("read config file {}", path.display()))?;
    let mut config: GlobalConfig =
        toml::from_str(&contents).with_context(|| format!("parse config {}", path.display()))?;
    normalize_aws_section(&mut config.aws);
    Ok(config)
}

fn load_cluster_config(path: &Path) -> Result<ClusterConfig> {
    if !path.exists() {
        bail!(
            "config file {} not found; run 'vmcli aws init <cluster>'",
            path.display()
        );
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("read config file {}", path.display()))?;
    let mut config: ClusterConfig =
        toml::from_str(&contents).with_context(|| format!("parse config {}", path.display()))?;
    config.cluster_name = normalize_optional(config.cluster_name);
    normalize_aws_section(&mut config.aws);
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
    let Some(aws) = section.as_mut() else {
        return;
    };
    aws.region = normalize_optional(aws.region.take());
    aws.ssh_public_key_path = normalize_optional(aws.ssh_public_key_path.take());
    aws.default_instance_type = normalize_optional(aws.default_instance_type.take());
    aws.ami_id = normalize_optional(aws.ami_id.take());
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

fn load_aws_config(cluster: &str, override_path: Option<&str>) -> Result<AwsEffectiveConfig> {
    let global_config = load_global_config()?;
    let cluster_path = match override_path {
        Some(path) => PathBuf::from(path),
        None => aws_cluster_config_path(cluster)?,
    };
    let cluster_config = load_cluster_config(&cluster_path)?;
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

    let merged = merge_aws_section(global_config.aws, cluster_config.aws);
    let region = merged
        .region
        .ok_or_else(|| anyhow!("aws.region must be set in config"))?;
    let ssh_public_key_path = merged
        .ssh_public_key_path
        .ok_or_else(|| anyhow!("aws.ssh_public_key_path must be set in config"))?;
    let default_instance_type = merged
        .default_instance_type
        .unwrap_or_else(|| DEFAULT_INSTANCE_TYPE.to_string());

    let ssh_config_path = aws_cluster_ssh_config_path(cluster)?;

    Ok(AwsEffectiveConfig {
        cluster_name: cluster.to_string(),
        region,
        ssh_public_key_path,
        default_instance_type,
        ami_id: merged.ami_id,
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
        let cli = Cli::try_parse_from(["vmcli", "aws", "health", "dev-cluster", "web-1"])
            .expect("parse health args");

        match cli.command {
            TopCommand::Aws(aws) => match aws.command {
                AwsCommand::Health(args) => {
                    assert_eq!(args.cluster, "dev-cluster");
                    assert_eq!(args.name, "web-1");
                    assert_eq!(args.os_user, DEFAULT_INSTANCE_OS_USER);
                    assert!(args.config.is_none());
                }
                _ => panic!("expected health command"),
            },
        }
    }

    #[test]
    fn cli_parses_health_command_overrides() {
        let cli = Cli::try_parse_from([
            "vmcli",
            "aws",
            "health",
            "dev-cluster",
            "web-1",
            "--os-user",
            "ec2-user",
            "-c",
            "/tmp/config.toml",
        ])
        .expect("parse health args with overrides");

        match cli.command {
            TopCommand::Aws(aws) => match aws.command {
                AwsCommand::Health(args) => {
                    assert_eq!(args.os_user, "ec2-user");
                    assert_eq!(args.config.as_deref(), Some("/tmp/config.toml"));
                }
                _ => panic!("expected health command"),
            },
        }
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
        let contents = default_config_contents(
            "dev-cluster",
            "ap-northeast-1",
            "~/.ssh/vmcli.pub",
            "t3.micro",
        );
        assert!(contents.contains("ssh_public_key_path = \"~/.ssh/vmcli.pub\""));
    }
}
