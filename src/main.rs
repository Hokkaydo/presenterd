use enigo::{Enigo, Key, Keyboard, Settings};
use log::{error, info};
use tokio::{time::sleep, io::{AsyncBufReadExt, BufReader}};
use std::sync::{Arc, Mutex};

mod ble_server;

/// UUIDs for the GATT service
const SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x1234567812345678123456789abcdef0);
/// UUID for the characteristic used to receive commands.
const CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x1234567812345678123456789abcdef0);

/// Name of the GATT service.
const NAME: &str = "Presenter Remote";

/// Handle the command received from the Bluetooth device.
/// This function interprets the command and executes the corresponding action
///
/// value: &[u8] is expected to contain the command byte(s).
#[inline(always)]
fn handle_command(value: &[u8], enigo: &mut Enigo) {
    let command = value.first().unwrap_or(&0x00);
    let command = match command {
        0x01 => Key::RightArrow,
        0x02 => Key::LeftArrow,
        _ => {
            error!("Unknown command received: {:x?}", value);
            return;
        }
    };
    enigo.key(command, enigo::Direction::Press).expect("");
    enigo.key(command, enigo::Direction::Release).expect("");
}


#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> bluer::Result<()> {
    env_logger::init();

    let enigo = Arc::new(Mutex::new(Enigo::new(&Settings::default()).unwrap()));

    let enigo_clone = enigo.clone();

    let ble_task = tokio::spawn(async move {
        ble_server::platform::run_ble_server(
            SERVICE_UUID,
            CHARACTERISTIC_UUID,
            NAME,
            move |value| {
                if let Ok(mut enigo) = enigo_clone.lock() {
                    handle_command(value, &mut *enigo)
                } else {
                    error!("Failed to lock Enigo mutex");
                }
            },
        )
        .await
    });

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    
    println!("Press Enter to stop the server...");

    tokio::select! {
        res = ble_task => {
            match res {
                Ok(_) => info!("BLE server exited successfully"),
                Err(e) => error!("BLE server error: {}", e),
            }
        }
        _ = lines.next_line() => {
            info!("User requested to stop the server.");
        }
    }

    info!("Cleaning up BLE server...");
    
    ble_server::platform::stop_ble_server().await;

    sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}
