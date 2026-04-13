use anyhow::Result;
use serde::Serialize;
use std::sync::{LazyLock, Mutex};
use tokio::runtime::Handle;

use crate::payload::HostStat;

pub mod email;
pub mod log;
pub mod tgbot;
pub mod webhook;
pub mod wechat;

pub static NOTIFIER_HANDLE: LazyLock<Mutex<Option<Handle>>> = LazyLock::new(Default::default);

#[derive(Debug, Serialize, Clone)]
pub enum Event {
    NodeUp,
    NodeDown,
    Custom,
}

fn get_tag(e: &Event) -> &'static str {
    match *e {
        Event::NodeUp => "NodeUp",
        Event::NodeDown => "NodeDown",
        Event::Custom => "Custom",
    }
}

pub trait Notifier {
    fn kind(&self) -> &'static str;
    fn notify(&self, e: &Event, stat: &HostStat) -> Result<()>;
    // send notify impl
    fn send_notify(&self, content: String) -> Result<()>;
    fn notify_test(&self) -> Result<()> {
        self.send_notify("❗ServerStatus test msg".to_string())
    }
}
