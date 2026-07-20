//! Serial port node (design §7.1). Faces host in the normal role.
//!
//! Slice 1: open the device (raw path for now — the resolver lands in phase 7
//! without a config-format change), apply configured termios, and take
//! `TIOCEXCL` on the raw fd (serial2 sets `O_NOCTTY` but not `TIOCEXCL`, per the
//! nexus-doctor P3 finding). A missing device does not fail the load — the node
//! comes up `waiting` and heals later (§7.1 faulted-and-wait, phase 7). Byte
//! flow via a `tokio AsyncFd` wrapper lands in slice 2.

use std::os::fd::AsRawFd;
use std::path::PathBuf;

use nexus_core::NodeStatus;
use nexus_core::config::{
    DataBits, FlowControl as CfgFlow, NodeConfig, Parity as CfgParity, StopBits as CfgStop,
};
use serde_json::json;
use serial2::{CharSize, FlowControl, Parity, SerialPort, Settings, StopBits};

use crate::sys;

pub struct SerialNode {
    pub name: String,
    device: PathBuf,
    baud: u32,
    port: Option<SerialPort>,
    status: NodeStatus,
}

impl SerialNode {
    pub fn create(config: &NodeConfig) -> SerialNode {
        let NodeConfig::Serial {
            name,
            device,
            baud,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            ..
        } = config
        else {
            unreachable!("SerialNode::create called with non-Serial config");
        };

        let mut node = SerialNode {
            name: name.clone(),
            device: PathBuf::from(device),
            baud: *baud,
            port: None,
            status: NodeStatus::Active,
        };

        match open_port(
            &node.device,
            *baud,
            *data_bits,
            *parity,
            *stop_bits,
            *flow_control,
        ) {
            Ok(port) => {
                node.port = Some(port);
                node.status = NodeStatus::Active;
            }
            // A device that isn't present yet is `waiting` (it will heal when it
            // reappears, §7.1); any other open error is `faulted`.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                node.status = NodeStatus::Waiting {
                    reason: format!("device {} not present", node.device.display()),
                };
            }
            Err(e) => {
                node.status = NodeStatus::Faulted {
                    reason: format!("open {}: {e}", node.device.display()),
                };
            }
        }
        node
    }

    pub fn status(&self) -> NodeStatus {
        self.status.clone()
    }

    pub fn state_extra(&self) -> serde_json::Value {
        json!({
            "resolved_path": self.device.display().to_string(),
            "baud": self.baud,
            "open": self.port.is_some(),
        })
    }
}

fn open_port(
    device: &std::path::Path,
    baud: u32,
    data_bits: DataBits,
    parity: CfgParity,
    stop_bits: CfgStop,
    flow: CfgFlow,
) -> std::io::Result<SerialPort> {
    let port = SerialPort::open(device, |mut s: Settings| {
        s.set_raw();
        s.set_baud_rate(baud)?;
        s.set_char_size(char_size(data_bits));
        s.set_parity(map_parity(parity));
        s.set_stop_bits(map_stop(stop_bits));
        s.set_flow_control(map_flow(flow));
        Ok(s)
    })?;
    // serial2 does not take TIOCEXCL; the daemon does, so stray processes cannot
    // share the port (§7.1, P3 finding).
    sys::set_exclusive(port.as_raw_fd(), true)
        .map_err(|e| std::io::Error::other(format!("TIOCEXCL: {e}")))?;
    Ok(port)
}

fn char_size(d: DataBits) -> CharSize {
    match d {
        DataBits::Five => CharSize::Bits5,
        DataBits::Six => CharSize::Bits6,
        DataBits::Seven => CharSize::Bits7,
        DataBits::Eight => CharSize::Bits8,
    }
}

fn map_parity(p: CfgParity) -> Parity {
    match p {
        CfgParity::None => Parity::None,
        CfgParity::Odd => Parity::Odd,
        CfgParity::Even => Parity::Even,
    }
}

fn map_stop(s: CfgStop) -> StopBits {
    match s {
        CfgStop::One => StopBits::One,
        CfgStop::Two => StopBits::Two,
    }
}

fn map_flow(f: CfgFlow) -> FlowControl {
    match f {
        CfgFlow::None => FlowControl::None,
        CfgFlow::XonXoff => FlowControl::XonXoff,
        CfgFlow::RtsCts => FlowControl::RtsCts,
    }
}
