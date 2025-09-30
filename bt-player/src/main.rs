mod player;

use std::error::Error;

use bluer::{agent::Agent, Adapter, Device, Session};
use tokio_stream::{StreamExt, StreamMap};
use zbus::{Connection, Proxy, fdo::PropertiesChanged};
use zvariant::Value;

use std::sync::{Arc, RwLock};
use player::Player;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Hash)]
enum StreamKey {
    Device,
    Player
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let player = Arc::new(RwLock::new(None));

    // DBus
    let connection = Connection::system().await?;

    // Bluetooth
    let session = Session::new().await?;
    // let agent_handle = session.register_agent(get_agent()).await?;
    let adapter = session.default_adapter().await?;

    // Maybe export this to a seperate function and spawn?
    // Wait for a device to be connected
    let device;
    loop {
        let Some(dev) = device_connected(&adapter).await else {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue
        };
        println!("Some device {} has connected", dev.address());
        device = dev;
        break;
    }
    let dev_addr = device.address().to_string().replace(":", "_");
    println!("Working with addr: {}", dev_addr);

    let dev_base_path = format!("/org/bluez/hci0/dev_{dev_addr}");
    let proxy_player = Proxy::new(
        &connection,
        "org.bluez",
        format!("{dev_base_path}/player0"),
        "org.freedesktop.DBus.Properties"
    ).await?;
    let proxy_dev = Proxy::new(
        &connection,
        "org.bluez",
        dev_base_path,
        "org.freedesktop.DBus.Properties"
    ).await?;

    let mut stream = StreamMap::new();
    stream.insert(StreamKey::Device, proxy_dev.receive_signal("PropertiesChanged").await?);
    stream.insert(StreamKey::Player, proxy_player.receive_signal("PropertiesChanged").await?);
    

    while let Some((key, msg)) = stream.next().await {
        let Some(props) = PropertiesChanged::from_message(msg) else {continue};
        let Ok(args) = props.args() else {continue};

        println!("Propertie changed for {key:?} -> {:?}", args.changed_properties());
        match key {
            StreamKey::Player => {
                // Update current Player Status
            }
            StreamKey::Device => {
                // Check if is still connected
                if let Some(Value::Bool(false)) = args.changed_properties.get("Connected") {
                    println!("Player has disconnected");
                    break;
                }
            }
        }
    }
    Ok(())
}

// Just our default agent
fn get_agent() -> Agent {
    Agent {
        request_pin_code: Some(Box::new(|_device| {
            Box::pin(async {
                println!("Device requested PIN code → returning empty string");
                Ok("".to_string())
            })
        })),
        request_passkey: Some(Box::new(|_device | {
            Box::pin(async {
                println!("Device requested passkey → returning 000000");
                Ok(0)
            })
        })),
        request_confirmation: Some(Box::new(|_device| {
            Box::pin(async move {
                println!("Device requested confirmation for passkey {} → accepting", _device.passkey);
                Ok(())
            })
        })),
        authorize_service: Some(Box::new(|_device| {
            Box::pin(async move {
                println!("Authorizing service {} → accepting", _device.service);
                Ok(())
            })
        })),
        request_authorization: Some(Box::new(|_device| {
            Box::pin(async {
                println!("Authorizing device → accepting");
                Ok(())
            })
        })),
        ..Default::default()
    }
}


// Check if any device is connected
async fn device_connected(adapter: &Adapter) -> Option<Device> {
    for addr in adapter.device_addresses().await.ok()? {
        let Ok(device) = adapter.device(addr) else {continue;};
        let Ok(connected) = device.is_connected().await else {continue;};
        if connected {
            return Some(device)
        }
    }
    None
}