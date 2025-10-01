#[cfg(test)]
mod test;

// mod webserver;

use std::error::Error;

use bluer::monitor::DeviceId;
use bluer::Address;
use bluer::{agent::Agent, Adapter, Device, Session};
use tokio_stream::{StreamExt, StreamMap};
use zbus::Connection;
use zbus::fdo::{ObjectManagerProxy, PropertiesChanged, PropertiesProxy, PropertiesChangedStream};
use zvariant::ObjectPath;


// TODO error handeling
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let player = Arc::new(RwLock::new(None));
    println!("[INFO] Starting up...");

    // DBus
    let connection = Connection::system().await?;
    println!("[INFO] Got DBus connection...");


    // Bluetooth
    let session = Session::new().await?;
    let agent_handle = session.register_agent(get_agent()).await?;      // So we dont need to open bluetoothctl
    let adapter = session.default_adapter().await?;
    println!("[INFO] Connected to bluez...");
    
    adapter.set_powered(true).await?;
    adapter.set_discoverable(true).await?;
    adapter.set_pairable(true).await?;

    //
    let obj_mgr_proxy = ObjectManagerProxy::new(
        &connection,
        "org.bluez",
        "/"
    ).await?;
    let mut interface_added_stream = obj_mgr_proxy.receive_interfaces_added().await?;
    let mut interface_removed_stream = obj_mgr_proxy.receive_interfaces_removed().await?;

    let mut device_map = StreamMap::new();
    // TODO find a better solution
    // Because im lazy i just add the adapter to properties changed so i can use the stream map in the select! otherwise need to copy code...
    {
        let adapter_proxy = PropertiesProxy::new(
            &connection,
            "org.bluez",
            "/org/bluez/hci0",      // TODO maybe dont hardcode
        ).await?;
        device_map.insert(
            StreamKey::Adapter,
            adapter_proxy.receive_properties_changed().await?,
        );
        
    }

    // Gather all devices and check if they are connect and if so add them to the stream map
    for addr in adapter.device_addresses().await? {
        let device = adapter.device(addr)?;
        if !device.is_connected().await? {
            continue
        }
        // TODO maybe get all properties because at this point i dont know nothing about the player
        insert_player_stream(&connection, addr, &mut device_map).await?;
        println!("[INFO] Added Device with address: {addr:?}");

    }

    println!("[INFO] Starting event loop...");
    // Now our "event loop" yeah
    loop {
        tokio::select! {
            if_added = interface_added_stream.next() => {
                let Some(if_added) = if_added else {break};
                // Only care about added Interfaces for a MediaPlayer, but maybe instead of that maybe check the args not the object path but for me this works
                let obj_path = if_added.args()?.object_path;
                if !obj_path.ends_with("player0") {
                    continue;
                }

                let Some(addr) = parse_device_address(obj_path) else {continue};
                insert_player_stream(&connection, addr, &mut device_map).await?;
                println!("[INFO] Added Device with address: {addr:?}");

            }
            if_removed = interface_removed_stream.next() => {
                let Some(if_removed) = if_removed else {break};
                let obj_path = if_removed.args()?.object_path;
                // See in if_added for the reason of this workaround
                if !obj_path.ends_with("player0") {
                    continue;
                }

                let Some(addr) = parse_device_address(obj_path) else {continue};
                device_map.remove(&StreamKey::Device(addr));
                println!("[WARNING] Removed Device with address: {addr:?}")
            }
            dev_change = device_map.next() => {
                let Some((key, change)) = dev_change else {break};
                // TODO do something with this thingy
                println!("[INFO] Property changed: {key:?} -> {:?}", change.args()?);
            }
            _ = tokio::signal::ctrl_c() => {
                println!("[WARNING] Execution was aborted by Ctrl+C...");
                break;
            }
        }
    }
    drop(agent_handle);
    println!("[INFO] Cleaning up...");

    Ok(())

}

/// Just a door with out a lock...
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
        request_default: true,
        ..Default::default()
    }
}


/// Opens a PropertiesProxy for player0 on a device with addr
/// Adds the stream to the map
async fn insert_player_stream(connection: &Connection, address: Address, stream_map: &mut StreamMap<StreamKey, PropertiesChangedStream>, ) -> Result<(), zbus::Error> {
    let player_path = format!("/org/bluez/hci0/dev_{}/player0", address.to_string().replace(":", "_"));
    let player_proxy = PropertiesProxy::new(
        connection,
        "org.bluez",
        player_path
    ).await?;


    let stream: zbus::fdo::PropertiesChangedStream = player_proxy.receive_properties_changed().await?;
    stream_map.insert(
        StreamKey::Device(address), 
        stream
    );

    Ok(())
}


/// Parses the ObjectPath and returns the device address
fn parse_device_address(obj_path: ObjectPath) -> Option<Address> {
    let dev = obj_path.split("/").nth(4)?;
    let addr = dev.trim_start_matches("dev_");
    
    let mut address = Address::any();
    for (index, base16) in addr.split("_").enumerate() {
        let number = u8::from_str_radix(base16, 16).unwrap();
        address.0[index] = number;
    }
    Some(address)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum StreamKey {
    /// Dummy ignore those, except you find something interesting, just here because im lazy and didnt want to think for a better solution for now
    Adapter,
    ///
    Device(Address),
}
