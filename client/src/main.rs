use std::{
    time::Duration,
};

use std::net::TcpStream;
use std::net::TcpListener;
use std::io::Write;
use std::io::Read;
use std::io::ErrorKind;
use std::time::Instant;

use tinybmp::Bmp;

use esp_idf_svc::ipv4::SocketAddrV4;

use anyhow::Result;
use config;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::image::Image;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        i2c::{I2cConfig, I2cDriver},
        prelude::*,
    },
};

use ssd1306::mode::BufferedGraphicsMode;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::Arc;

use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use wifi::wifi;

use esp_idf_svc::hal::task::thread::ThreadSpawnConfiguration;

fn update_animation<DI, SIZE, MODE>(display: &Arc<Mutex<Box<Ssd1306<DI, SIZE, MODE>>>>, animation: Option<&str>, animation_switch: &Arc<AtomicBool>) {
    match animation {
        Some(s) => {
            animation_switch.store(true, Ordering::Relaxed);
        }
        None => (),
    }

}

fn update_message<DI, SIZE: ssd1306::prelude::DisplaySize, MODE>(display: Arc<Mutex<Box<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>>>, message: Option<&str>, animation_switch: &Arc<AtomicBool>) where DI: WriteOnlyDataCommand {
    match message {
        Some(s) => {
            animation_switch.store(false, Ordering::Relaxed);

            let text_style = MonoTextStyleBuilder::new()
                .font(&FONT_6X10)
                .text_color(BinaryColor::On)
                .build();

            {
                let mut active_display = display.lock().unwrap();
                active_display.clear(BinaryColor::Off).unwrap();
                Text::with_baseline(&s, Point::new(0, 0), text_style, Baseline::Top)
                    .draw(&mut **active_display)
                    .unwrap();
                active_display.flush().unwrap();
            }
        }
        None => (),
    }

}

fn update_timer<DI, SIZE, MODE>(display: &Arc<Mutex<Box<Ssd1306<DI, SIZE, MODE>>>>, message: Option<&str>, animation_switch: &Arc<AtomicBool>) {
    match message {
        Some(s) => {
            println!("{}",s);
        }
        None => (),
    }

}

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

    let display = Arc::new(Mutex::new(display));

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    {
        let mut display = display.lock().unwrap();
        Text::with_baseline("Connecting...", Point::zero(), text_style, Baseline::Top)
            .draw(&mut **display)
            .unwrap();
        display.flush().unwrap();
    }

    // Connect to the Wi-Fi network
    let wifi = wifi(
        config::WIFI_SSID,
        config::WIFI_PSK,
        peripherals.modem,
        sysloop,
    )?;


    {
        let mut display = display.lock().unwrap();
        Text::with_baseline("Connected!", Point::new(0, 16), text_style, Baseline::Top)
            .draw(&mut **display)
            .unwrap();
        display.flush().unwrap();
    }

    let server_addr = SocketAddrV4::new(config::SERVER_IP, config::BROADCAST_PORT);

    let mac_chunks = wifi.get_mac(esp_idf_svc::wifi::WifiDeviceId::Sta).unwrap();
    let registration_request = format!("REGISTER {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", mac_chunks[0], mac_chunks[1], mac_chunks[2], mac_chunks[3], mac_chunks[4], mac_chunks[5]).into_bytes();

    let mut current_cmd = "".to_string();

    let animation_display = display.clone();

    let animation_switch = Arc::new(AtomicBool::new(true));

    let animation_check = animation_switch.clone();

    // animation thread
    std::thread::spawn(move || {

        //let data = include_bytes!("../media/guardian.bmp");
        let data = include_bytes!("../media/eyes.bmp");
        // Parse the BMP file.
        let bmp = Box::new(Bmp::<BinaryColor>::from_slice(data)).unwrap();

        let width = 128;
        let height = 64;

        let mut sprites = Vec::new();
        for row in 0..4 {
            for col in 0..10 {
                let sprite = bmp.sub_image(&Rectangle::new(Point::new(col*width, row*height), Size::new(width.try_into().unwrap(), height.try_into().unwrap() )));
                sprites.push(sprite);
            }
        }

        let frames: Vec<_> = sprites.iter().map(|s| Image::new(s, Point::new(0, 0)) ).collect();

        loop {
            for frame in &frames {
                if animation_check.load(Ordering::Relaxed) {
                    println!("animating");
                    //let image = Image::new(frame, Point::new(0, 0));
                    let mut display = animation_display.lock().unwrap();
                    //display.clear(BinaryColor::Off).unwrap();
                    frame.draw(&mut **display).unwrap();
                    display.flush().unwrap();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    });


    // Main loop
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
                        let mut parts = cmd.split_ascii_whitespace();
                        match parts.next() {
                            Some("ANIMATE") => update_animation(&display, parts.next(), &animation_switch),
                            Some("MESSAGE") => update_message::<I2CInterface<I2cDriver<'_>>, ssd1306::prelude::DisplaySize128x64, BufferedGraphicsMode<ssd1306::prelude::DisplaySize128x64>>(display.clone(), parts.next(), &animation_switch),
                            Some("TIMER")   => update_timer(&display, parts.next(), &animation_switch),
                            Some(_) => panic!("Unrecognized command"),
                            None => panic!("Command Parsing Error"),
                        };
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
