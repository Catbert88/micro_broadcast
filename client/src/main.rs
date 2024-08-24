use anyhow::Result;
use core::str;
use embedded_svc::{http::Method, io::Write};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        io::EspIOError,
        prelude::*,
    },
    http::server::{Configuration, EspHttpServer},
};
use std::{
    thread::sleep,
    time::Duration,
};
use wifi::wifi;

use config;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;

    // Connect to the Wi-Fi network
    let _wifi = wifi(
        config::WIFI_SSID,
        config::WIFI_PSK,
        peripherals.modem,
        sysloop,
    )?;

    // Set the HTTP server
    let mut server = EspHttpServer::new(&Configuration::default())?;
    // http://<sta ip>/ handler
    server.fn_handler(
        "/",
        Method::Get,
        |request| -> core::result::Result<(), EspIOError> {
            let html = index_html();
            let mut response = request.into_ok_response()?;
            response.write_all(html.as_bytes())?;
            Ok(())
        },
    )?;

    println!("Server awaiting connection");

    // Prevent program from exiting
    loop {
        sleep(Duration::from_millis(1000));
    }

}

fn templated(content: impl AsRef<str>) -> String {
    format!(
        r#"
<!DOCTYPE html>
<html>
<meta name="viewport" content="width=device-width, initial-scale=1">
    <head>
        <meta charset="utf-8">
        <title>esp-rs web server</title>
    </head>
    <body>
        {}
    </body>
</html>
"#,
        content.as_ref()
    )
}

fn index_html() -> String {
    templated("Hello from ESP32-C3!")
}
