use bluer::{
    adv::Advertisement,
    gatt::{
        CharacteristicReader,
        local::{
            Application, Characteristic, CharacteristicControlEvent, CharacteristicNotify,
            CharacteristicNotifyMethod, CharacteristicWrite, CharacteristicWriteMethod, Service,
            characteristic_control, service_control,
        },
    },
};
use futures::{StreamExt, future, pin_mut};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    time::sleep,
};
use log::{debug, error, trace};
use enigo::{Enigo, Key, Keyboard, Settings};

/// UUIDs for the GATT service
const SERVICE_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x1234567812345678123456789abcdef0);
/// UUID for the characteristic used to receive commands.
const CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0x1234567812345678123456789abcdef0);

/// Name of the GATT service.
const NAME : &str = "Presenter Remote";

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
        _ =>  {
            error!("Unknown command received: {:x?}", value);
            return;
        }
    };
    enigo.key(command, enigo::Direction::Press).expect("");
    enigo.key(command, enigo::Direction::Release).expect("");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> bluer::Result<()> {
    env_logger::init();

    let mut enigo = Enigo::new(&Settings::default()).unwrap();

    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    debug!("UUIDs for this application:");
    debug!("  Service UUID: {SERVICE_UUID}");
    debug!("  Characteristic UUID: {CHARACTERISTIC_UUID}");

    trace!(
        "Advertising on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await?
    );
    
    let le_advertisement = Advertisement {
        service_uuids: vec![SERVICE_UUID].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some(NAME.to_string()),
        min_interval: Some(Duration::from_millis(100)),
        max_interval: Some(Duration::from_millis(1000)),
        ..Default::default()
    };

    let adv_handle = adapter.advertise(le_advertisement).await?;

    trace!(
        "Serving GATT service on Bluetooth adapter {}",
        adapter.name()
    );

    let (_, service_handle) = service_control();
    let (char_control, char_handle) = characteristic_control();
    let app = Application {
        services: vec![Service {
            uuid: SERVICE_UUID,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: CHARACTERISTIC_UUID,
                write: Some(CharacteristicWrite {
                    write: true,
                    write_without_response: true,
                    method: CharacteristicWriteMethod::Io,
                    ..Default::default()
                }),
                notify: Some(CharacteristicNotify {
                    notify: true,
                    method: CharacteristicNotifyMethod::Io,
                    ..Default::default()
                }),
                control_handle: char_handle,
                ..Default::default()
            }],
            control_handle: service_handle,
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    trace!("Service ready. Press enter to quit.");
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    let mut read_buf = Vec::new();
    let mut reader_opt: Option<CharacteristicReader> = None;

    pin_mut!(char_control);

    loop {
        tokio::select! {
            _ = lines.next_line() => break,
            evt = char_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        trace!("Accepting write event with MTU {} from {}", req.mtu(), req.device_address());
                        read_buf = vec![0; req.mtu()];
                        reader_opt = Some(req.accept()?);
                    },
                    _ => break,
                }
            }
            read_res = async {
                match &mut reader_opt {
                    Some(reader) => reader.read(&mut read_buf).await,
                    None => future::pending().await,
                }
            } => {
                match read_res {
                    Ok(0) => {
                        trace!("Write stream ended");
                        reader_opt = None;
                    }
                    Ok(n) => {
                        let value = read_buf[0..n].to_vec();
                        trace!("Write request with {} bytes: {:x?}", n, &value);
                        handle_command(&value, &mut enigo);
                    }
                    Err(err) => {
                        error!("Error reading from stream: {err}");
                        reader_opt = None;
                    }
                }
            }
        }
    }

    println!("Removing service and advertisement");
    drop(app_handle);
    drop(adv_handle);
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
