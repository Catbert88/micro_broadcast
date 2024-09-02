use core::str;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

use std::net::TcpStream;
use std::net::TcpListener;
use std::net::Ipv4Addr;
use std::io::Write;
use std::io::Read;
//use esp_idf_svc::io::ErrorKind;
use std::io::ErrorKind;
use std::time::Instant;

use esp_idf_svc::ipv4::SocketAddrV4;

use anyhow::Result;
use config;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use embedded_svc::{http::Method, io::Write as svc_write};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        i2c::{I2cConfig, I2cDriver},
        io::EspIOError,
        prelude::*,
    },
    http::server::{Configuration, EspHttpServer},
};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use wifi::wifi;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;

    let sda = peripherals.pins.gpio6;
    let scl = peripherals.pins.gpio7;

    let config = I2cConfig::new().baudrate(400.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;

    let interface = I2CDisplayInterface::new(i2c);
    let display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0);

    // Place the buffered_graphics display on the stack to prevent Stack overflow
    let mut display = Box::new(display.into_buffered_graphics_mode());
    display.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline("Connecting...", Point::zero(), text_style, Baseline::Top)
        .draw(&mut *display)
        .unwrap();
    display.flush().unwrap();


    // Connect to the Wi-Fi network
    let _wifi = wifi(
        config::WIFI_SSID,
        config::WIFI_PSK,
        peripherals.modem,
        sysloop,
    )?;

    Text::with_baseline("Connected!", Point::new(0, 16), text_style, Baseline::Top)
        .draw(&mut *display)
        .unwrap();
    display.flush().unwrap();


    //let display_access = Arc::new(Mutex::new(display));

    // Replace with your server's IP and port
    // let server_addr = SocketAddrV4::new(Ipv4Addr::new(192, 168, 4, 209), 8092);
    let server_addr = SocketAddrV4::new(Ipv4Addr::new(192, 168, 4, 222), 8092);

    loop {
        println!("Searching for MicroBroadcaster at {:?}", server_addr);

        match TcpStream::connect(server_addr)
        {
            Ok(mut stream) => {
                stream.write_all(b"Hello from ESP32!")?;
            },
            Err(error) => {
                println!("Invalid Response: {}", error);
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        }

        // now read until an error occurs then break out of the loop
        let listener = TcpListener::bind("0.0.0.0:8092").unwrap();
        listener.set_nonblocking(true)?;

        let timeout = Duration::from_secs(5);
        let mut start_time = Instant::now();

        loop {
            match listener.accept() {
                Ok((mut socket, addr)) => {
                    start_time = Instant::now();
                    socket.set_read_timeout(Some(Duration::new(1, 0)))?;
                    let mut cmd = "".to_string();
                    match socket.read_to_string(&mut cmd) {
                        Ok(n) => (),
                        Err(e) => {
                            println!("Read Error: {}", e);
                            break;
                        }
                    }
                    println!("Received Directive: {}", &cmd);
                    Text::with_baseline(&cmd, Point::new(0, 32), text_style, Baseline::Top)
                        .draw(&mut *display)
                        .unwrap();
                    display.flush().unwrap();
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    // No incoming connection, check if we should timeout
                    if start_time.elapsed() >= timeout {
                        println!("Timeout reached, breaking out of accept loop");
                        break;
                    }
                    // Optional: sleep for a short duration to avoid busy-waiting
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => break
            }

        }
    }
}
