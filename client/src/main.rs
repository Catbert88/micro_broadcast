use std::{
    time::Duration,
};

use std::net::TcpStream;
use std::net::TcpListener;
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
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        i2c::{I2cConfig, I2cDriver},
        prelude::*,
    },
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
    let wifi = wifi(
        config::WIFI_SSID,
        config::WIFI_PSK,
        peripherals.modem,
        sysloop,
    )?;


    Text::with_baseline("Connected!", Point::new(0, 16), text_style, Baseline::Top)
        .draw(&mut *display)
        .unwrap();
    display.flush().unwrap();

    let server_addr = SocketAddrV4::new(config::SERVER_IP, config::BROADCAST_PORT);

    let mac_chunks = wifi.get_mac(esp_idf_svc::wifi::WifiDeviceId::Sta).unwrap();
    let registration_request = format!("REGISTER {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", mac_chunks[0], mac_chunks[1], mac_chunks[2], mac_chunks[3], mac_chunks[4], mac_chunks[5]).into_bytes();

    let mut current_cmd = "".to_string();

    loop {
        println!("Searching for MicroBroadcaster at {:?}", server_addr);

        match TcpStream::connect(server_addr)
        {
            Ok(mut stream) => {
                println!("Sending Registration request.");
                match stream.write_all(&registration_request) {
                    Ok(_n) => println!("Registration Successfull"),
                    Err(e) => println!("Registration failed {}", e)
                };
            },
            Err(error) => {
                println!("Invalid Response: {}", error);
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        }

        // now read until an error occurs then break out of the loop
        let listener = TcpListener::bind(format!("0.0.0.0:{}",config::BROADCAST_PORT)).unwrap();
        listener.set_nonblocking(true)?;

        let timeout = Duration::from_secs(5);
        let mut start_time = Instant::now();

        loop {
            match listener.accept() {
                Ok((mut socket, _addr)) => {
                    start_time = Instant::now();
                    socket.set_read_timeout(Some(Duration::new(1, 0)))?;
                    let mut cmd = "".to_string();
                    match socket.read_to_string(&mut cmd) {
                        Ok(_n) => (),
                        Err(e) => {
                            println!("Read Error: {}", e);
                            break;
                        }
                    }

                    println!("Received Directive: {}", &cmd);
                    if cmd != current_cmd
                    {
                        display.clear_buffer();
                        display.flush().unwrap();
                        Text::with_baseline(&cmd, Point::new(0, 0), text_style, Baseline::Top)
                            .draw(&mut *display)
                            .unwrap();
                        display.flush().unwrap();
                        current_cmd = cmd.to_string();
                    }
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
                Err(e) => {
                    println!("Error: {}", e);
                    break;
                }
            }

        }
    }
}
