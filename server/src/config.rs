#![deny(warnings)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use uuid::Uuid;

use crate::notifier;

fn default_as_true() -> bool {
    true
}
fn default_grpc_addr() -> String {
    "0.0.0.0:9394".to_string()
}
fn default_http_addr() -> String {
    "0.0.0.0:8080".to_string()
}
fn default_workspace() -> String {
    "/opt/ServerStatus".to_string()
}
fn default_tls_dir() -> String {
    "tls".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Host {
    pub name: String,
    pub password: String,
    #[serde(default = "Default::default")]
    pub alias: String,
    #[serde(default = "Default::default")]
    pub location: String,
    #[serde(default = "Default::default")]
    pub r#type: String,
    #[serde(default = "u32::default")]
    pub monthstart: u32,
    #[serde(default = "default_as_true")]
    pub notify: bool,
    #[serde(default = "bool::default")]
    pub disabled: bool,
    #[serde(default = "Default::default")]
    pub labels: String,

    #[serde(skip_deserializing)]
    pub last_network_in: u64,
    #[serde(skip_deserializing)]
    pub last_network_out: u64,

    // user data
    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(default = "Default::default", skip_serializing)]
    pub weight: u64,
    #[serde(default = "Default::default")]
    pub gid: String,
    #[serde(default = "Default::default")]
    pub latest_ts: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostGroup {
    pub gid: String,
    pub password: String,
    #[serde(default = "Default::default")]
    pub location: String,
    #[serde(default = "Default::default")]
    pub r#type: String,
    #[serde(default = "default_as_true")]
    pub notify: bool,
    // user data
    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(default = "Default::default", skip_serializing)]
    pub weight: u64,
    #[serde(default = "Default::default")]
    pub labels: String,
}

impl HostGroup {
    pub fn inst_host(&self, name: &str) -> Host {
        Host {
            name: name.to_owned(),
            gid: self.gid.clone(),
            password: self.password.clone(),
            location: self.location.clone(),
            r#type: self.r#type.clone(),
            monthstart: 1,
            notify: self.notify,
            pos: self.pos,
            weight: self.weight,
            labels: self.labels.clone(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_http_addr")]
    pub http_addr: String,
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "Default::default")]
    pub notify_interval: u64,
    #[serde(default = "Default::default")]
    pub offline_threshold: u64,
    #[serde(default = "Default::default")]
    pub grpc_tls: u32,
    #[serde(default = "default_tls_dir")]
    pub tls_dir: String,
    // admin user & pass
    pub admin_user: Option<String>,
    pub admin_pass: Option<String>,
    pub jwt_secret: Option<String>,

    #[serde(default = "Default::default")]
    pub tgbot: notifier::tgbot::Config,
    #[serde(default = "Default::default")]
    pub wechat: notifier::wechat::Config,
    #[serde(default = "Default::default")]
    pub email: notifier::email::Config,
    #[serde(default = "Default::default")]
    pub log: notifier::log::Config,
    #[serde(default = "Default::default")]
    pub webhook: notifier::webhook::Config,

    #[serde(default = "Default::default")]
    pub hosts: Vec<Host>,
    #[serde(default = "Default::default")]
    pub hosts_group: Vec<HostGroup>,
    #[serde(default = "Default::default")]
    pub group_gc: u64,

    // deploy
    #[serde(default = "Default::default")]
    pub server_url: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,

    #[serde(skip_deserializing)]
    pub hosts_map: HashMap<String, Host>,

    #[serde(skip_deserializing)]
    pub hosts_group_map: HashMap<String, HostGroup>,
}

impl Config {
    pub fn auth(&self, user: &str, pass: &str) -> bool {
        if let Some(o) = self.hosts_map.get(user) {
            return pass.eq(o.password.as_str());
        }
        false
    }
    pub fn group_auth(&self, gid: &str, pass: &str) -> bool {
        if let Some(o) = self.hosts_group_map.get(gid) {
            return pass.eq(o.password.as_str());
        }
        false
    }
    pub fn admin_auth(&self, user: &str, pass: &str) -> bool {
        if let (Some(u), Some(p)) = (self.admin_user.as_ref(), self.admin_pass.as_ref()) {
            return user.eq(u.as_str()) && pass.eq(p.as_str());
        }
        false
    }

    pub fn to_json_value(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(anyhow::Error::new)
    }

    // pub fn to_string(&self) -> Result<String> {
    //     serde_json::to_string(&self).map_err(anyhow::Error::new)
    // }
}

pub fn from_str(content: &str) -> Option<Config> {
    let mut o = toml::from_str::<Config>(content).unwrap();
    o.hosts_map = HashMap::new();

    for (idx, host) in o.hosts.iter_mut().enumerate() {
        host.pos = idx;
        if host.alias.is_empty() {
            host.alias = host.name.clone();
        }
        if host.monthstart < 1 || host.monthstart > 31 {
            host.monthstart = 1;
        }
        host.weight = 10000_u64 - idx as u64;
        o.hosts_map.insert(host.name.clone(), host.clone());
    }

    for (idx, group) in o.hosts_group.iter_mut().enumerate() {
        group.pos = idx;
        group.weight = (10000 - (1 + idx) * 100) as u64;
        o.hosts_group_map.insert(group.gid.clone(), group.clone());
    }

    if o.offline_threshold < 30 {
        o.offline_threshold = 30;
    }
    if o.notify_interval < 30 {
        o.notify_interval = 30;
    }
    if o.group_gc < 30 {
        o.group_gc = 30;
    }

    if o.admin_user.is_none() || o.admin_user.as_ref()?.is_empty() {
        o.admin_user = Some("admin".to_string());
    }
    if o.admin_pass.is_none() || o.admin_pass.as_ref()?.is_empty() {
        o.admin_pass = Some(Uuid::new_v4().to_string());
    }
    if o.jwt_secret.is_none() || o.jwt_secret.as_ref()?.is_empty() {
        o.jwt_secret = Some(Uuid::new_v4().to_string());
    }

    eprintln!("✨ admin_user: {}", o.admin_user.as_ref()?);
    eprintln!("✨ admin_pass: {}", o.admin_pass.as_ref()?);

    Some(o)
}

pub fn from_env() -> Option<Config> {
    from_str(
        env::var("SRV_CONF")
            .expect("can't load config from env `SRV_CONF")
            .as_str(),
    )
}

pub fn from_file(cfg: &str) -> Option<Config> {
    fs::read_to_string(cfg)
        .map(|contents| from_str(contents.as_str()))
        .ok()?
}

pub fn test_from_file(cfg: &str) -> Result<Config> {
    fs::read_to_string(cfg)
        .map(|contents| toml::from_str::<Config>(&contents))
        .unwrap()
        .map_err(anyhow::Error::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_auth_success() {
        let mut config = Config {
            hosts_map: HashMap::new(),
            ..Default::default()
        };

        let host = Host {
            name: "test_host".to_string(),
            password: "test_pass".to_string(),
            ..Default::default()
        };

        config.hosts_map.insert("test_host".to_string(), host);

        assert!(config.auth("test_host", "test_pass"));
    }

    #[test]
    fn test_host_auth_wrong_password() {
        let mut config = Config {
            hosts_map: HashMap::new(),
            ..Default::default()
        };

        let host = Host {
            name: "test_host".to_string(),
            password: "correct_pass".to_string(),
            ..Default::default()
        };

        config.hosts_map.insert("test_host".to_string(), host);

        assert!(!config.auth("test_host", "wrong_pass"));
    }

    #[test]
    fn test_host_auth_nonexistent_user() {
        let config = Config {
            hosts_map: HashMap::new(),
            ..Default::default()
        };

        assert!(!config.auth("nonexistent", "any_pass"));
    }

    #[test]
    fn test_group_auth_success() {
        let mut config = Config {
            hosts_group_map: HashMap::new(),
            ..Default::default()
        };

        let group = HostGroup {
            gid: "test_group".to_string(),
            password: "group_pass".to_string(),
            location: String::new(),
            r#type: String::new(),
            notify: true,
            pos: 0,
            weight: 0,
            labels: String::new(),
        };

        config.hosts_group_map.insert("test_group".to_string(), group);

        assert!(config.group_auth("test_group", "group_pass"));
    }

    #[test]
    fn test_group_auth_failure() {
        let mut config = Config {
            hosts_group_map: HashMap::new(),
            ..Default::default()
        };

        let group = HostGroup {
            gid: "test_group".to_string(),
            password: "correct_pass".to_string(),
            location: String::new(),
            r#type: String::new(),
            notify: true,
            pos: 0,
            weight: 0,
            labels: String::new(),
        };

        config.hosts_group_map.insert("test_group".to_string(), group);

        assert!(!config.group_auth("test_group", "wrong_pass"));
        assert!(!config.group_auth("nonexistent_group", "any_pass"));
    }

    #[test]
    fn test_admin_auth_success() {
        let config = Config {
            admin_user: Some("admin".to_string()),
            admin_pass: Some("admin_pass".to_string()),
            ..Default::default()
        };

        assert!(config.admin_auth("admin", "admin_pass"));
    }

    #[test]
    fn test_admin_auth_failure() {
        let config = Config {
            admin_user: Some("admin".to_string()),
            admin_pass: Some("correct_pass".to_string()),
            ..Default::default()
        };

        assert!(!config.admin_auth("admin", "wrong_pass"));
        assert!(!config.admin_auth("wrong_user", "correct_pass"));
    }

    #[test]
    fn test_admin_auth_no_credentials() {
        let config = Config {
            admin_user: None,
            admin_pass: None,
            ..Default::default()
        };

        assert!(!config.admin_auth("any_user", "any_pass"));
    }

    #[test]
    fn test_host_group_inst_host() {
        let group = HostGroup {
            gid: "group1".to_string(),
            password: "pass123".to_string(),
            location: "Beijing".to_string(),
            r#type: "VPS".to_string(),
            notify: true,
            pos: 5,
            weight: 100,
            labels: "prod,web".to_string(),
        };

        let host = group.inst_host("server1");

        assert_eq!(host.name, "server1");
        assert_eq!(host.gid, "group1");
        assert_eq!(host.password, "pass123");
        assert_eq!(host.location, "Beijing");
        assert_eq!(host.r#type, "VPS");
        assert_eq!(host.monthstart, 1);
        assert!(host.notify);
        assert_eq!(host.pos, 5);
        assert_eq!(host.weight, 100);
        assert_eq!(host.labels, "prod,web");
    }

    #[test]
    fn test_config_to_json_value() {
        let config = Config {
            http_addr: "0.0.0.0:8080".to_string(),
            grpc_addr: "0.0.0.0:9394".to_string(),
            ..Default::default()
        };

        let result = config.to_json_value();
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["http_addr"], "0.0.0.0:8080");
        assert_eq!(json["grpc_addr"], "0.0.0.0:9394");
    }
}

impl Default for Config {
    fn default() -> Self {
        // 注意：这里返回的是未规范化配置，仅用于 serde/default 初始化与测试构造。
        // 最低阈值、缺省填充等规范化逻辑应由配置加载/校验路径负责。
        Self {
            http_addr: default_http_addr(),
            grpc_addr: default_grpc_addr(),
            notify_interval: 0,
            offline_threshold: 0,
            grpc_tls: 0,
            tls_dir: default_tls_dir(),
            admin_user: None,
            admin_pass: None,
            jwt_secret: None,
            tgbot: notifier::tgbot::Config::default(),
            wechat: notifier::wechat::Config::default(),
            email: notifier::email::Config::default(),
            log: notifier::log::Config::default(),
            webhook: notifier::webhook::Config::default(),
            hosts: Vec::new(),
            hosts_group: Vec::new(),
            group_gc: 0,
            server_url: String::new(),
            workspace: default_workspace(),
            hosts_map: HashMap::new(),
            hosts_group_map: HashMap::new(),
        }
    }
}
