#![allow(unused)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::similar_names, clippy::many_single_char_names)]
use regex::Regex;
use std::collections::HashMap;
use std::collections::LinkedList;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind::ConnectionRefused;
use std::net::TcpStream;
use std::net::{Shutdown, ToSocketAddrs};
use std::process::Command;
use std::str;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::vnstat;
use crate::Args;
use stat_common::server_status::{DiskInfo, StatRequest};

const SAMPLE_PERIOD: u64 = 1000; //ms
const TIMEOUT_MS: u64 = 1000;

pub fn get_uptime() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| parse_uptime(&contents))
        .unwrap_or(0)
}

pub fn get_loadavg() -> (f64, f64, f64) {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|contents| parse_loadavg(&contents))
        .unwrap_or((0.0, 0.0, 0.0))
}

fn parse_uptime(content: &str) -> Option<u64> {
    content
        .split('.').next()
        .and_then(|s| s.parse::<u64>().ok())
}

fn parse_loadavg(content: &str) -> Option<(f64, f64, f64)> {
    let vec = content.split_whitespace().collect::<Vec<_>>();
    if vec.len() < 3 {
        return None;
    }

    let a: Vec<f64> = vec[0..3]
        .iter()
        .filter_map(|v| v.parse::<f64>().ok())
        .collect();
    if a.len() == 3 {
        Some((a[0], a[1], a[2]))
    } else {
        None
    }
}

static MEMORY_REGEX: &str = r"^(?P<key>\S*):\s*(?P<value>\d*)\s*kB";
static MEMORY_REGEX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(MEMORY_REGEX).expect("Invalid memory regex"));
pub fn get_memory() -> (u64, u64, u64, u64) {
    let Ok(file) = File::open("/proc/meminfo") else {
        return (0, 0, 0, 0);
    };
    let buf_reader = BufReader::new(file);
    let mut res_dict = HashMap::new();
    for line in buf_reader.lines() {
        let Ok(l) = line else { continue };
        if let Some(caps) = MEMORY_REGEX_RE.captures(&l) {
            if let Ok(value) = caps["value"].parse::<u64>() {
                res_dict.insert(caps["key"].to_string(), value);
            }
        }
    }

    let mem_total = *res_dict.get("MemTotal").unwrap_or(&0);
    let swap_total = *res_dict.get("SwapTotal").unwrap_or(&0);
    let swap_free = *res_dict.get("SwapFree").unwrap_or(&0);

    let mem_used = mem_total
        .saturating_sub(*res_dict.get("MemFree").unwrap_or(&0))
        .saturating_sub(*res_dict.get("Buffers").unwrap_or(&0))
        .saturating_sub(*res_dict.get("Cached").unwrap_or(&0))
        .saturating_sub(*res_dict.get("SReclaimable").unwrap_or(&0));

    (mem_total, mem_used, swap_total, swap_free)
}

macro_rules! exec_shell_cmd_fetch_u32 {
    ($shell_cmd:expr) => {{
        Command::new("/bin/sh")
            .args(&["-c", $shell_cmd])
            .output()
            .ok()
            .and_then(|output| str::from_utf8(&output.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0)
    }};
}

pub fn tupd() -> (u32, u32, u32, u32) {
    let t = exec_shell_cmd_fetch_u32!("ss -t | wc -l").saturating_sub(1);
    let u = exec_shell_cmd_fetch_u32!("ss -u | wc -l").saturating_sub(1);
    let p = exec_shell_cmd_fetch_u32!("ps -ef | wc -l").saturating_sub(2);
    let d = exec_shell_cmd_fetch_u32!("ps -eLf | wc -l").saturating_sub(2);

    (t, u, p, d)
}

static TRAFFIC_REGEX: &str =
    r"([^\s]+):[\s]{0,}(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\d+)";
static TRAFFIC_REGEX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(TRAFFIC_REGEX).expect("Invalid traffic regex"));
pub fn get_sys_traffic(args: &Args) -> (u64, u64) {
    let (mut network_in, mut network_out) = (0, 0);
    let Ok(file) = File::open("/proc/net/dev") else {
        return (0, 0);
    };
    let buf_reader = BufReader::new(file);
    for line in buf_reader.lines() {
        let Ok(l) = line else { continue };

        TRAFFIC_REGEX_RE.captures(&l).and_then(|caps| {
            let name = caps.get(1)?.as_str();

            // spec iface
            if args.skip_iface(name) {
                return None;
            }

            let net_in = caps.get(2)?.as_str().parse::<u64>().ok()?;
            let net_out = caps.get(10)?.as_str().parse::<u64>().ok()?;

            network_in += net_in;
            network_out += net_out;
            Some(())
        });
    }

    (network_in, network_out)
}

static DF_CMD:&str = "df -Tlm --total -t ext4 -t ext3 -t ext2 -t reiserfs -t jfs -t ntfs -t fat32 -t btrfs -t fuseblk -t zfs -t simfs -t xfs";
pub fn get_hdd(stat: &mut StatRequest) {
    let output = Command::new("/bin/sh")
        .args(["-c", DF_CMD])
        .output();

    let Ok(output) = output else { return };
    let Ok(content) = str::from_utf8(&output.stdout) else { return };

    let vs: Vec<&str> = content.lines().collect();

    for (idx, s) in vs.iter().enumerate() {
        // header
        if idx == 0 {
            continue;
        }

        let vec: Vec<&str> = (*s).split_whitespace().collect();
        if vec.len() < 7 {
            continue;
        }

        if idx == vs.len() - 1 {
            // total
            stat.hdd_total = vec[2].parse::<u64>().unwrap_or(0);
            stat.hdd_used = vec[3].parse::<u64>().unwrap_or(0);
        } else {
            let total = vec[2].parse::<u64>().unwrap_or(0);
            let used = vec[3].parse::<u64>().unwrap_or(0);
            let free = vec[4].parse::<u64>().unwrap_or(0);

            let di = DiskInfo {
                name: vec[0].to_string(),
                mount_point: vec[6].to_string(),
                file_system: vec[1].to_string(),
                total: total * 1024 * 1024,
                used: used * 1024 * 1024,
                free: free * 1024 * 1024,
            };
            stat.disks.push(di);
        }
    }
}

#[derive(Debug, Default)]
pub struct NetSpeed {
    pub diff: f64,
    pub clock: f64,
    pub netrx: u64,
    pub nettx: u64,
    pub avgrx: u64,
    pub avgtx: u64,
}

pub static G_NET_SPEED: LazyLock<Arc<RwLock<NetSpeed>>> = LazyLock::new(|| Arc::new(RwLock::default()));

#[allow(unused)]
pub fn start_net_speed_collect_t(args: &Args) {
    let args_1 = args.clone();
    thread::spawn(move || loop {
        let _ = File::open("/proc/net/dev").map(|file| {
            let buf_reader = BufReader::new(file);
            let (mut avgrx, mut avgtx) = (0, 0);
            for line in buf_reader.lines() {
                let Ok(l) = line else { continue };
                let v: Vec<&str> = l.split(':').collect();
                if v.len() < 2 {
                    continue;
                }

                // spec iface
                if args_1.skip_iface(v[0]) {
                    continue;
                }

                let v1: Vec<&str> = v[1].split_whitespace().collect();
                if v1.len() <= 8 {
                    continue;
                }

                let Some(rx) = v1[0].parse::<u64>().ok() else {
                    continue;
                };
                let Some(tx) = v1[8].parse::<u64>().ok() else {
                    continue;
                };

                avgrx += rx;
                avgtx += tx;
            }

            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as f64;

            if let Ok(mut t) = G_NET_SPEED.write() {
                t.diff = now - t.clock;
                t.clock = now;
                if t.diff > 0.0 {
                    t.netrx = (avgrx.saturating_sub(t.avgrx) as f64 / t.diff) as u64;
                    t.nettx = (avgtx.saturating_sub(t.avgtx) as f64 / t.diff) as u64;
                } else {
                    t.netrx = 0;
                    t.nettx = 0;
                }
                t.avgrx = avgrx;
                t.avgtx = avgtx;

                // dbg!(&t);
            }
        });
        thread::sleep(Duration::from_millis(SAMPLE_PERIOD));
    });
}

// Store CPU percentage as u64 (value * 100) for atomic operations
pub static G_CPU_PERCENT: AtomicU64 = AtomicU64::new(0);
#[allow(unused)]
pub fn start_cpu_percent_collect_t() {
    let mut pre_cpu: Vec<u64> = vec![0, 0, 0, 0];
    thread::spawn(move || loop {
        let _ = File::open("/proc/stat").map(|file| {
            let mut buf_reader = BufReader::new(file);
            let mut buf = String::new();
            let _ = buf_reader.read_line(&mut buf).map(|_| {
                let cur_cpu = buf
                    .split_whitespace()
                    .enumerate()
                    .filter(|&(idx, _)| idx > 0 && idx < 5)
                    .map(|(_, e)| e.parse::<u64>().unwrap())
                    .collect::<Vec<_>>();

                let pre: u64 = pre_cpu.iter().sum();
                let cur: u64 = cur_cpu.iter().sum();
                let mut st = cur - pre;
                if st == 0 {
                    st = 1;
                }

                let res = 100.0 - (100.0 * (cur_cpu[3] - pre_cpu[3]) as f64 / st as f64);

                // dbg!(&pre_cpu);
                // dbg!(&cur_cpu);

                pre_cpu = cur_cpu;

                // Store as integer (percentage * 100) for atomic operations
                let cpu_int = (res.round() * 100.0) as u64;
                G_CPU_PERCENT.store(cpu_int, Ordering::Relaxed);
            });
        });

        thread::sleep(Duration::from_millis(SAMPLE_PERIOD));
    });
}

static ONLINE_IPV4: u8 = 1;
static ONLINE_IPV6: u8 = 2;
pub fn get_network(args: &Args) -> (bool, bool) {
    let mut network: [bool; 2] = [(args.online & ONLINE_IPV4) != 0, (args.online & ONLINE_IPV6) != 0];
    if network.iter().any(|&x| x) {
        return network.into();
    }
    let addrs = vec![&args.ipv4_address, &args.ipv6_address];
    for (idx, probe_addr) in addrs.into_iter().enumerate() {
        let _ = probe_addr.to_socket_addrs().map(|mut iter| {
            if let Some(addr) = iter.next() {
                info!("{probe_addr} => {addr}");

                let r = TcpStream::connect_timeout(&addr, Duration::from_millis(TIMEOUT_MS)).map(|s| {
                    network[idx] = true;
                    s.shutdown(Shutdown::Both)
                });

                info!("{r:?}");
            }
        });
    }

    network.into()
}

#[derive(Debug, Default)]
pub struct PingData {
    pub probe_uri: String,
    pub lost_rate: u32,
    pub ping_time: u32,
}

fn start_ping_collect_t(data: &Arc<Mutex<PingData>>) {
    let mut package_list: LinkedList<i32> = LinkedList::new();
    let mut package_lost: u32 = 0;
    let pt = &*data.lock().unwrap();
    let addr = pt
        .probe_uri
        .to_socket_addrs()
        .unwrap()
        .next()
        .expect("can't get addr info");
    info!("{} => {:?}", pt.probe_uri, addr);

    let ping_data = data.clone();
    thread::spawn(move || loop {
        if package_list.len() > 100 && package_list.pop_front().unwrap() == 0 {
            package_lost -= 1;
        }

        let instant = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_millis(TIMEOUT_MS)) {
            Ok(s) => {
                let _ = s.shutdown(Shutdown::Both);
                package_list.push_back(1);
            }
            Err(e) => {
                // error!("{:?}", e);
                if e.kind() == ConnectionRefused {
                    package_list.push_back(1);
                } else {
                    package_lost += 1;
                    package_list.push_back(0);
                }
            }
        }
        let time_cost_ms = instant.elapsed().as_millis();

        if let Ok(mut o) = ping_data.lock() {
            o.ping_time = u32::try_from(time_cost_ms).unwrap_or(u32::MAX);
            if package_list.len() > 30 {
                o.lost_rate = package_lost * 100 / u32::try_from(package_list.len()).unwrap_or(u32::MAX);
            }
        }

        thread::sleep(Duration::from_millis(SAMPLE_PERIOD));
    });
}

pub static G_PING_10010: OnceLock<Arc<Mutex<PingData>>> = OnceLock::new();
pub static G_PING_189: OnceLock<Arc<Mutex<PingData>>> = OnceLock::new();
pub static G_PING_10086: OnceLock<Arc<Mutex<PingData>>> = OnceLock::new();

pub fn start_all_ping_collect_t(args: &Args) {
    G_PING_10010
        .set(Arc::new(Mutex::new(PingData {
            probe_uri: args.cu_addr.clone(),
            lost_rate: 0,
            ping_time: 0,
        })))
        .unwrap();
    G_PING_189
        .set(Arc::new(Mutex::new(PingData {
            probe_uri: args.ct_addr.clone(),
            lost_rate: 0,
            ping_time: 0,
        })))
        .unwrap();
    G_PING_10086
        .set(Arc::new(Mutex::new(PingData {
            probe_uri: args.cm_addr.clone(),
            lost_rate: 0,
            ping_time: 0,
        })))
        .unwrap();

    if !args.disable_ping {
        start_ping_collect_t(G_PING_10010.get().unwrap());
        start_ping_collect_t(G_PING_189.get().unwrap());
        start_ping_collect_t(G_PING_10086.get().unwrap());
    }
}

pub fn sample(args: &Args, stat: &mut StatRequest) {
    stat.version = env!("CARGO_PKG_VERSION").to_string();
    stat.vnstat = args.vnstat;

    stat.uptime = get_uptime();

    let (load_1, load_5, load_15) = get_loadavg();
    stat.load_1 = load_1;
    stat.load_5 = load_5;
    stat.load_15 = load_15;

    let (mem_total, mem_used, swap_total, swap_free) = get_memory();
    stat.memory_total = mem_total;
    stat.memory_used = mem_used;
    stat.swap_total = swap_total;
    stat.swap_used = swap_total - swap_free;

    get_hdd(stat);

    let (t, u, p, d) = if args.disable_tupd { (0, 0, 0, 0) } else { tupd() };
    stat.tcp = t;
    stat.udp = u;
    stat.process = p;
    stat.thread = d;

    if args.vnstat {
        let (network_in, network_out, m_network_in, m_network_out) = vnstat::get_traffic(args).unwrap();
        stat.network_in = network_in;
        stat.network_out = network_out;
        stat.last_network_in = network_in - m_network_in;
        stat.last_network_out = network_out - m_network_out;
    } else {
        let (network_in, network_out) = get_sys_traffic(args);
        stat.network_in = network_in;
        stat.network_out = network_out;
    }

    // Load CPU percentage from atomic (stored as value * 100)
    stat.cpu = G_CPU_PERCENT.load(Ordering::Relaxed) as f64 / 100.0;

    if let Ok(o) = G_NET_SPEED.read() {
        stat.network_rx = o.netrx;
        stat.network_tx = o.nettx;
    }
    {
        let o = &*G_PING_10010.get().unwrap().lock().unwrap();
        stat.ping_10010 = o.lost_rate.into();
        stat.time_10010 = o.ping_time.into();
    }
    {
        let o = &*G_PING_189.get().unwrap().lock().unwrap();
        stat.ping_189 = o.lost_rate.into();
        stat.time_189 = o.ping_time.into();
    }
    {
        let o = &*G_PING_10086.get().unwrap().lock().unwrap();
        stat.ping_10086 = o.lost_rate.into();
        stat.time_10086 = o.ping_time.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_regex_valid_line() {
        let line = "MemTotal:       16384000 kB";
        let caps = MEMORY_REGEX_RE.captures(line).unwrap();
        assert_eq!(caps.name("key").unwrap().as_str(), "MemTotal");
        assert_eq!(caps.name("value").unwrap().as_str(), "16384000");
    }

    #[test]
    fn test_memory_regex_with_spaces() {
        let line = "MemFree:        8192000 kB";
        let caps = MEMORY_REGEX_RE.captures(line).unwrap();
        assert_eq!(caps.name("key").unwrap().as_str(), "MemFree");
        assert_eq!(caps.name("value").unwrap().as_str(), "8192000");
    }

    #[test]
    fn test_memory_regex_invalid_line() {
        let line = "Invalid line without proper format";
        assert!(MEMORY_REGEX_RE.captures(line).is_none());
    }

    #[test]
    fn test_traffic_regex_valid_line() {
        let line = "eth0: 1000 2000 0 0 0 0 0 0 3000 4000 0 0 0 0 0 0";
        assert!(TRAFFIC_REGEX_RE.is_match(line));
        
        let caps = TRAFFIC_REGEX_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "eth0");
        assert_eq!(caps.get(2).unwrap().as_str(), "1000");  // rx bytes
        assert_eq!(caps.get(10).unwrap().as_str(), "3000"); // tx bytes
    }

    #[test]
    fn test_traffic_regex_with_interface_name() {
        let line = "wlan0:  500000 1000 0 0 0 0 0 0 250000 500 0 0 0 0 0 0";
        assert!(TRAFFIC_REGEX_RE.is_match(line));
        
        let caps = TRAFFIC_REGEX_RE.captures(line).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "wlan0");
        assert_eq!(caps.get(2).unwrap().as_str(), "500000");
        assert_eq!(caps.get(10).unwrap().as_str(), "250000");
    }

    #[test]
    fn test_traffic_regex_invalid_line() {
        let line = "Invalid: not enough fields";
        assert!(!TRAFFIC_REGEX_RE.is_match(line));
    }

    #[test]
    fn test_get_uptime_fallback() {
        assert_eq!(parse_uptime("12345.67 890.12\n"), Some(12345));
        assert_eq!(parse_uptime("invalid"), None);
        assert_eq!(parse_uptime(""), None);
    }

    #[test]
    fn test_get_loadavg_fallback() {
        assert_eq!(
            parse_loadavg("0.12 0.34 0.56 1/234 5678\n"),
            Some((0.12, 0.34, 0.56))
        );
        assert_eq!(parse_loadavg("0.12 0.34"), None);
        assert_eq!(parse_loadavg("0.12 nope 0.56"), None);
    }
}
