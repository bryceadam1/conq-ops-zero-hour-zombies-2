"""Spot GPU machines for the voice models' data and training, with the spending guardrails the
project agreed (docs/COMMANDS.md section 11, "Cloud machines"):

    venv/bin/python tools/voice/cloud/cloud.py launch <job> --hours 2 --cap 3
    venv/bin/python tools/voice/cloud/cloud.py push <job> <local path> <remote path>
    venv/bin/python tools/voice/cloud/cloud.py ssh <job> '<command>'
    venv/bin/python tools/voice/cloud/cloud.py watch <job> --pull '<remote dir>' '<local dir>'
    venv/bin/python tools/voice/cloud/cloud.py status
    venv/bin/python tools/voice/cloud/cloud.py terminate <job>

Every machine:
  * is a one-time **spot** instance (interruption terminates it), with a maximum hourly price;
  * **shuts itself down after `--hours`** (a `shutdown` scheduled at boot, whatever happens to
    this Mac or the connection), and shutting down **terminates** it (no stopped disks left);
  * is reachable only by SSH, from this Mac's public address, with this project's own key.

`watch` (run it in the background) pulls the job's finished output every few minutes with rsync,
so terminating loses at most the shard in progress, and terminates the machine when the job
writes `~/job/DONE`, when its running cost reaches `--cap` dollars, or when `--hours` pass; it
pulls once more first. The AWS profile is `conquerops`, the region `us-west-2`.
"""

import argparse
import json
import subprocess
import sys
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

import boto3

PROFILE, REGION = "conquerops", "us-west-2"
STATE = Path.home() / ".conquerops" / "cloud"
KEY = STATE / "conquerops_ed25519"
KEY_NAME, GROUP = "conquerops", "conquerops-ssh"
IMAGE = "Deep Learning OSS Nvidia Driver AMI GPU PyTorch*Ubuntu 22.04*"
USER = "ubuntu"


def ec2():
    return boto3.Session(profile_name=PROFILE, region_name=REGION).client("ec2")


def log(msg):
    print(f"[{datetime.now().strftime('%H:%M:%S')}] {msg}", flush=True)


def state_path(job):
    return STATE / f"{job}.json"


def load(job):
    return json.loads(state_path(job).read_text())


def ensure_key(c):
    STATE.mkdir(parents=True, exist_ok=True)
    if not KEY.exists():
        subprocess.run(["ssh-keygen", "-t", "ed25519", "-N", "", "-C", "conquerops", "-f", str(KEY)], check=True, capture_output=True)
    names = [k["KeyName"] for k in c.describe_key_pairs()["KeyPairs"]]
    if KEY_NAME not in names:
        c.import_key_pair(KeyName=KEY_NAME, PublicKeyMaterial=KEY.with_suffix(".pub").read_bytes())


def ensure_group(c, vpc):
    """The SSH-only group, open to this Mac's public address (added if it changed)."""
    ip = urllib.request.urlopen("https://checkip.amazonaws.com", timeout=10).read().decode().strip()
    found = c.describe_security_groups(Filters=[{"Name": "group-name", "Values": [GROUP]}, {"Name": "vpc-id", "Values": [vpc]}])["SecurityGroups"]
    gid = found[0]["GroupId"] if found else c.create_security_group(GroupName=GROUP, Description="Conquer Ops voice jobs: SSH from the owner Mac", VpcId=vpc)["GroupId"]
    rules = found[0]["IpPermissions"] if found else []
    if not any(r.get("FromPort") == 22 and any(x["CidrIp"] == f"{ip}/32" for x in r.get("IpRanges", [])) for r in rules):
        c.authorize_security_group_ingress(GroupId=gid, IpPermissions=[{"IpProtocol": "tcp", "FromPort": 22, "ToPort": 22, "IpRanges": [{"CidrIp": f"{ip}/32", "Description": "owner Mac"}]}])
    return gid


def zones_by_price(c, itype):
    """The zones offering `itype` as spot, cheapest first, with their current price."""
    hist = c.describe_spot_price_history(InstanceTypes=[itype], ProductDescriptions=["Linux/UNIX"], StartTime=datetime.now(timezone.utc))["SpotPriceHistory"]
    latest = {}
    for h in hist:
        latest.setdefault(h["AvailabilityZone"], float(h["SpotPrice"]))
    return sorted(latest.items(), key=lambda z: z[1])


def spot_price(c, itype, zone):
    hist = c.describe_spot_price_history(InstanceTypes=[itype], ProductDescriptions=["Linux/UNIX"], AvailabilityZone=zone, StartTime=datetime.now(timezone.utc))["SpotPriceHistory"]
    return float(hist[0]["SpotPrice"]) if hist else None


def request(args, image, subnet, group, user_data):
    """The launch request: one-time spot (interruption terminates), a price ceiling, terminate on
    shutdown, a disk deleted with the machine, IMDSv2, and the job's tags."""
    return dict(
        ImageId=image["ImageId"],
        InstanceType=args.type,
        MinCount=1,
        MaxCount=1,
        KeyName=KEY_NAME,
        NetworkInterfaces=[{"DeviceIndex": 0, "SubnetId": subnet, "Groups": [group], "AssociatePublicIpAddress": True}],
        InstanceMarketOptions={"MarketType": "spot", "SpotOptions": {"SpotInstanceType": "one-time", "InstanceInterruptionBehavior": "terminate", "MaxPrice": f"{args.max_price:.3f}"}},
        InstanceInitiatedShutdownBehavior="terminate",
        BlockDeviceMappings=[{"DeviceName": image["RootDeviceName"], "Ebs": {"VolumeSize": args.disk, "VolumeType": "gp3", "DeleteOnTermination": True}}],
        MetadataOptions={"HttpTokens": "required"},
        UserData=user_data,
        TagSpecifications=[{"ResourceType": "instance", "Tags": [{"Key": "Name", "Value": f"conquerops-{args.job}"}, {"Key": "conquerops-job", "Value": args.job}, {"Key": "max-hours", "Value": str(args.hours)}, {"Key": "cost-cap", "Value": str(args.cap)}]}],
    )


def launch(args):
    if state_path(args.job).exists() and load(args.job).get("instance") and not args.force:
        sys.exit(f"{args.job} already has a machine on record ({state_path(args.job)}); terminate it first")
    c = ec2()
    ensure_key(c)
    vpc = c.describe_vpcs(Filters=[{"Name": "isDefault", "Values": ["true"]}])["Vpcs"][0]["VpcId"]
    group = ensure_group(c, vpc)
    zones = zones_by_price(c, args.type)
    zone, price = zones[0]
    subnet_of = lambda z: c.describe_subnets(Filters=[{"Name": "availability-zone", "Values": [z]}, {"Name": "default-for-az", "Values": ["true"]}])["Subnets"][0]["SubnetId"]
    subnet = subnet_of(zone)
    images = c.describe_images(Owners=["amazon"], Filters=[{"Name": "name", "Values": [IMAGE]}, {"Name": "architecture", "Values": ["x86_64"]}, {"Name": "state", "Values": ["available"]}])["Images"]
    image = max(images, key=lambda i: i["CreationDate"])
    minutes = int(args.hours * 60)
    # The lifetime is enforced on the machine itself: shut down (and so terminate) after it.
    user_data = f"#!/bin/bash\nshutdown -h +{minutes} 'conquerops: maximum lifetime reached'\nmkdir -p /home/{USER}/job && chown {USER}:{USER} /home/{USER}/job\n"
    log(f"launching {args.type} spot in {zone} at ${price:.3f}/h (max ${args.max_price:.2f}/h), {image['Name']}, {args.disk} GB, lifetime {args.hours} h, cap ${args.cap:.2f}")
    if args.dry_run:
        # EC2 validates the whole request (permissions, quota, spot options) without launching.
        try:
            c.run_instances(DryRun=True, **request(args, image, subnet, group, user_data))
        except c.exceptions.ClientError as e:
            code = e.response["Error"]["Code"]
            log("dry run: the launch would succeed" if code == "DryRunOperation" else f"dry run refused: {code}: {e.response['Error']['Message']}")
        return
    # Spot capacity varies by zone: try each, cheapest first.
    r = None
    for zone, price in zones:
        if price > args.max_price:
            continue
        subnet = subnet_of(zone)
        try:
            r = c.run_instances(**request(args, image, subnet, group, user_data))
            break
        except c.exceptions.ClientError as e:
            if e.response["Error"]["Code"] not in ("InsufficientInstanceCapacity", "SpotMaxPriceTooLow", "Unsupported"):
                raise
            log(f"{zone}: {e.response['Error']['Code']}, trying the next zone")
    if r is None:
        sys.exit("no zone had spot capacity under the price ceiling; nothing was launched")
    log(f"launched in {zone} at ${price:.3f}/h")
    iid = r["Instances"][0]["InstanceId"]
    c.get_waiter("instance_running").wait(InstanceIds=[iid])
    inst = c.describe_instances(InstanceIds=[iid])["Reservations"][0]["Instances"][0]
    st = {"job": args.job, "instance": iid, "ip": inst.get("PublicIpAddress"), "type": args.type, "zone": zone, "price_at_launch": price, "launched": inst["LaunchTime"].isoformat(), "hours": args.hours, "cap": args.cap, "disk": args.disk}
    state_path(args.job).write_text(json.dumps(st, indent=2))
    log(f"{iid} running at {st['ip']}; state in {state_path(args.job)}")
    # Wait for SSH.
    for _ in range(60):
        if ssh_run(st, "true", quiet=True).returncode == 0:
            log("SSH ready")
            return
        time.sleep(10)
    log("SSH not ready after 10 minutes; check the machine")


def ssh_base(st):
    return ["ssh", "-i", str(KEY), "-o", "StrictHostKeyChecking=accept-new", "-o", f"UserKnownHostsFile={STATE / 'known_hosts'}", "-o", "ConnectTimeout=15", "-o", "ServerAliveInterval=30", f"{USER}@{st['ip']}"]


def ssh_run(st, command, quiet=False):
    return subprocess.run(ssh_base(st) + [command], capture_output=quiet, text=True)


def rsync(st, src, dst):
    e = " ".join(ssh_base(st)[:-1])
    return subprocess.run(["rsync", "-az", "--partial", "-e", e, src, dst], capture_output=True, text=True)


def push(args):
    st = load(args.job)
    r = rsync(st, args.local, f"{USER}@{st['ip']}:{args.remote}")
    print(r.stdout + r.stderr, end="")
    sys.exit(r.returncode)


def ssh(args):
    sys.exit(ssh_run(load(args.job), args.command).returncode)


def cost(c, st):
    """The machine's cost so far: hours since launch at the higher of its launch and current spot price, and its disk."""
    hours = (datetime.now(timezone.utc) - datetime.fromisoformat(st["launched"])).total_seconds() / 3600
    now = spot_price(c, st["type"], st["zone"]) or st["price_at_launch"]
    disk = st["disk"] * 0.08 / 730 * hours  # gp3, $0.08 per GB-month
    return hours, hours * max(now, st["price_at_launch"]) + disk


def terminate_now(c, st, why):
    log(f"terminating {st['instance']}: {why}")
    c.terminate_instances(InstanceIds=[st["instance"]])
    st["terminated"] = datetime.now(timezone.utc).isoformat()
    st["why"] = why
    state_path(st["job"]).write_text(json.dumps(st, indent=2))


def watch(args):
    c = ec2()
    st = load(args.job)
    pulls = [(args.pull[i], args.pull[i + 1]) for i in range(0, len(args.pull or []), 2)]

    def pull_all():
        for remote, local in pulls:
            Path(local).mkdir(parents=True, exist_ok=True)
            r = rsync(st, f"{USER}@{st['ip']}:{remote}", local)
            if r.returncode not in (0, 24):
                log(f"pull {remote}: {r.stderr.strip()[:200]}")

    last_pull = 0.0
    while True:
        inst = c.describe_instances(InstanceIds=[st["instance"]])["Reservations"][0]["Instances"][0]
        status = inst["State"]["Name"]
        hours, spent = cost(c, st)
        if status in ("shutting-down", "terminated", "stopped", "stopping"):
            log(f"machine is {status} after {hours:.2f} h, about ${spent:.2f}")
            return
        done = ssh_run(st, "test -f ~/job/DONE", quiet=True).returncode == 0
        if time.time() - last_pull > args.interval or done:
            pull_all()
            last_pull = time.time()
        if done:
            pull_all()
            terminate_now(c, st, f"job done after {hours:.2f} h, about ${spent:.2f}")
            return
        if spent >= st["cap"] or hours >= st["hours"]:
            pull_all()
            terminate_now(c, st, f"guardrail: {hours:.2f} h, about ${spent:.2f} (cap ${st['cap']:.2f}, lifetime {st['hours']} h)")
            return
        log(f"{status}, {hours:.2f} h, about ${spent:.2f} of ${st['cap']:.2f}")
        time.sleep(60)


def status(args):
    c = ec2()
    res = c.describe_instances(Filters=[{"Name": "tag-key", "Values": ["conquerops-job"]}, {"Name": "instance-state-name", "Values": ["pending", "running", "stopping", "stopped", "shutting-down"]}])
    found = [i for r in res["Reservations"] for i in r["Instances"]]
    if not found:
        print("no Conquer Ops machines running")
    for i in found:
        tags = {t["Key"]: t["Value"] for t in i.get("Tags", [])}
        print(f"{tags.get('conquerops-job')}: {i['InstanceId']} {i['InstanceType']} {i['State']['Name']} since {i['LaunchTime']:%H:%M} UTC, {i.get('PublicIpAddress')}")


def terminate(args):
    terminate_now(ec2(), load(args.job), "asked")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("launch")
    p.add_argument("job")
    p.add_argument("--type", default="g5.xlarge")
    p.add_argument("--hours", type=float, required=True, help="maximum lifetime; the machine shuts itself down (and terminates) after it")
    p.add_argument("--cap", type=float, required=True, help="dollars; the watchdog terminates the machine at this running cost")
    p.add_argument("--max-price", type=float, default=1.0, help="the most to pay per hour for the spot machine")
    p.add_argument("--disk", type=int, default=150)
    p.add_argument("--force", action="store_true")
    p.add_argument("--dry-run", action="store_true", help="prepare and validate the launch with EC2, without starting anything")
    p.set_defaults(fn=launch)
    p = sub.add_parser("push")
    p.add_argument("job")
    p.add_argument("local")
    p.add_argument("remote")
    p.set_defaults(fn=push)
    p = sub.add_parser("ssh")
    p.add_argument("job")
    p.add_argument("command")
    p.set_defaults(fn=ssh)
    p = sub.add_parser("watch")
    p.add_argument("job")
    p.add_argument("--pull", nargs="*", help="pairs of remote dir and local dir to keep pulled")
    p.add_argument("--interval", type=int, default=300, help="seconds between pulls")
    p.set_defaults(fn=watch)
    sub.add_parser("status").set_defaults(fn=status)
    p = sub.add_parser("terminate")
    p.add_argument("job")
    p.set_defaults(fn=terminate)
    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
