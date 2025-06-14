#[cfg(target_os = "windows")]
pub mod platform {
   
    pub async fn run_ble_server(
        service_uuid: uuid::Uuid,
        characteristic_uuid: uuid::Uuid,
        name: &str,
        on_data_received: impl Fn(&[u8]) + Send + 'static
    ) {
        let name = CString::new(name).unwrap();
        let service = CString::new(service_uuid.to_string()).unwrap();
        let charac = CString::new(characteristic_uuid.to_string()).unwrap();

        unsafe { internal_start_ble_server(name.as_ptr(), service.as_ptr(), charac.as_ptr(), callback_stub(on_data_received)); }
    }

    pub async fn stop_ble_server() {
        unsafe { internal_stop_ble_server(); }
    }

    use std::ffi::CString;
    use std::os::raw::{c_char, c_uchar};

    type Callback = unsafe extern "C" fn(*const c_uchar, usize);

    #[link(name = "ble_server")]
    unsafe extern "C" {
        fn internal_start_ble_server(
            name: *const c_char,
            service_uuid: *const c_char,
            characteristic_uuid: *const c_char,
            callback: Callback,
        );
        fn internal_stop_ble_server();
    }

    use std::sync::Mutex;
    use std::sync::OnceLock;

    static CALLBACK: OnceLock<Mutex<Option<Box<dyn Fn(&[u8]) + Send + 'static>>>> = OnceLock::new();

    fn callback_stub<F>(callback: F) -> Callback
    where
        F: Fn(&[u8]) + Send + 'static,
    {
        CALLBACK.get_or_init(|| Mutex::new(None)).lock().unwrap().replace(Box::new(callback));

        unsafe extern "C" fn wrapper(
            data: *const c_uchar,
            len: usize,
        ) {
            let data = unsafe { std::slice::from_raw_parts(data, len) };
            if let Some(mutex) = CALLBACK.get() {
                if let Ok(mut guard) = mutex.lock() {
                    if let Some(callback) = guard.as_mut() {
                        callback(data);
                    }
                }
            }
        }
        wrapper
    }
    
}

#[cfg(target_os = "linux")]
pub mod platform {
    use bluer::{
        adv::Advertisement,
        gatt::{
            CharacteristicReader,
            local::{
                Application, Characteristic, CharacteristicControlEvent, CharacteristicNotify,
                CharacteristicNotifyMethod, CharacteristicWrite, CharacteristicWriteMethod,
                Service, characteristic_control, service_control,
            },
        },
    };
    use futures::{StreamExt, future, pin_mut};
    use log::{debug, error, info, trace};
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    pub async fn run_ble_server(
        service_uuid: uuid::Uuid,
        characteristic_uuid: uuid::Uuid,
        name: &str,
        on_data_received: impl Fn(&[u8]) + Send + 'static,
    ) -> bluer::Result<()> {
        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;
        adapter.set_powered(true).await?;

        debug!("UUIDs for this application:");
        debug!("  Service UUID: {service_uuid}");
        debug!("  Characteristic UUID: {characteristic_uuid}");

        trace!(
            "Advertising on Bluetooth adapter {} with address {}",
            adapter.name(),
            adapter.address().await?
        );

        let le_advertisement = Advertisement {
            service_uuids: vec![service_uuid].into_iter().collect(),
            discoverable: Some(true),
            local_name: Some(name.to_string()),
            min_interval: Some(Duration::from_millis(100)),
            max_interval: Some(Duration::from_millis(1000)),
            ..Default::default()
        };

        let _ = adapter.advertise(le_advertisement).await?;

        trace!(
            "Serving GATT service on Bluetooth adapter {}",
            adapter.name()
        );

        let (_, service_handle) = service_control();
        let (char_control, char_handle) = characteristic_control();
        let app = Application {
            services: vec![Service {
                uuid: service_uuid,
                primary: true,
                characteristics: vec![Characteristic {
                    uuid: characteristic_uuid,
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

        let _ = adapter.serve_gatt_application(app).await?;

        info!("Service ready. Press enter to quit.");

        let mut read_buf = Vec::new();
        let mut reader_opt: Option<CharacteristicReader> = None;

        pin_mut!(char_control);

        loop {
            tokio::select! {
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
                            on_data_received(&value);
                        }
                        Err(err) => {
                            error!("Error reading from stream: {err}");
                            reader_opt = None;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn stop_ble_server() {
        // No specific action needed for Linux, as the server will stop when the application exits.
        info!("Stopping BLE server on Linux is handled by exiting the application.");
    }
}
