use std::{
    time::Duration,
};

use std::net::TcpStream;
use std::net::TcpListener;
use std::io::Write;
use std::io::Read;
use std::io::ErrorKind;
use std::time::Instant;
use std::str::FromStr;

use tinybmp::Bmp;

use embedded_graphics::image::GetPixel;

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
//use embedded_graphics::image::Image;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        i2c::{I2cConfig, I2cDriver},
        prelude::*,
    },
};


use embedded_graphics::{
    primitives::{Sector, PrimitiveStyle, PrimitiveStyleBuilder},
};

use ssd1306::mode::BufferedGraphicsMode;

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::sync::Arc;

use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::ascii::FONT_6X13;

use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use wifi::wifi;

fn update_animation<DI, SIZE, MODE>(display: &Arc<Mutex<Box<Ssd1306<DI, SIZE, MODE>>>>, animation: &str, animation_switch: &Arc<AtomicBool>) {
    animation_switch.store(true, Ordering::Relaxed);
}

fn update_message<DI: WriteOnlyDataCommand, SIZE: ssd1306::prelude::DisplaySize, MODE>(display: &Arc<Mutex<Box<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>>>, message: &str, animation_switch: &Arc<AtomicBool>) {

    animation_switch.store(false, Ordering::Relaxed);

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X13)
        .text_color(BinaryColor::On)
        .build();

    {
        let mut active_display = display.lock().unwrap();
        active_display.clear(BinaryColor::Off).unwrap();
        Text::with_baseline(message, Point::new(0, 0), text_style, Baseline::Top)
            .draw(&mut **active_display)
            .unwrap();
        active_display.flush().unwrap();
    }

}

fn update_timer<DI: WriteOnlyDataCommand, SIZE: ssd1306::prelude::DisplaySize, MODE>(display: &Arc<Mutex<Box<Ssd1306<DI, SIZE, BufferedGraphicsMode<SIZE>>>>>, timer: &str, animation_switch: &Arc<AtomicBool>) {

    animation_switch.store(false, Ordering::Relaxed);

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    let mut parts = timer.split('/');

    match (parts.next(), parts.next()) {
        (Some(current),Some(total)) => {
            let ratio = 360.0 * f32::from_str(current).unwrap() / f32::from_str(total).unwrap();

            let mut active_display = display.lock().unwrap();

            active_display.clear(BinaryColor::Off).unwrap();

            Text::with_baseline(current, Point::new(0, 0), text_style, Baseline::Top)
                .draw(&mut **active_display)
                .unwrap();

            Text::with_baseline(total, Point::new(0, 22), text_style, Baseline::Top)
                .draw(&mut **active_display)
                .unwrap();

            // Sector with 1 pixel wide white stroke with top-left point at (10, 20) with a diameter of 30
            Sector::new(Point::new(65, 1), 60, -90.0.deg(), 360.0.deg())
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
                .draw(&mut **active_display).unwrap();

            // Sector with blue fill and no stroke with a translation applied
            Sector::new(Point::new(65, 1), 60, -90.0.deg(), ratio.deg())
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(&mut **active_display).unwrap();

            active_display.flush().unwrap();
        }

        (_,_) => panic!("Unrecognized Timer Format"),
    }
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;

    let sda = peripherals.pins.gpio6;
    let scl = peripherals.pins.gpio7;

    let config = I2cConfig::new().baudrate(800.kHz().into());
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
        let bmp = Bmp::<BinaryColor>::from_slice(data).unwrap();

        let width = 128;
        let height = 64;

        loop {
            for row in 0..4 {
                for col in 0..10 {

                    if animation_check.load(Ordering::Relaxed) {

                        let frame_origin = Point::new(col*width, row*height);

                        let bounding_box = Rectangle::new(frame_origin, Size::new(width.try_into().unwrap(), height.try_into().unwrap() ));

                        let mut display = animation_display.lock().unwrap();

                        display.clear(BinaryColor::Off).unwrap();

                        for point in bounding_box.points() {
                            let pixel = bmp.pixel(point).unwrap();

                            if pixel == BinaryColor::On {
                                let draw_point = point - frame_origin;
                                display.set_pixel(draw_point.x.try_into().unwrap(), draw_point.y.try_into().unwrap(), true);
                            }
                        }

                        display.flush().unwrap();
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
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
                        if cmd != "PING" {
                            match cmd.split_once(' ') {
                                Some(("ANIMATE", a)) => update_animation(&display, a, &animation_switch),
                                Some(("MESSAGE", m)) => update_message::<I2CInterface<I2cDriver<'_>>, ssd1306::prelude::DisplaySize128x64, BufferedGraphicsMode<ssd1306::prelude::DisplaySize128x64>>(&display, m, &animation_switch),
                                Some(("TIMER", t))   => update_timer::<I2CInterface<I2cDriver<'_>>, ssd1306::prelude::DisplaySize128x64, BufferedGraphicsMode<ssd1306::prelude::DisplaySize128x64>>(&display, t, &animation_switch),
                                Some((_,_)) => panic!("Unrecognized command"),
                                None => panic!("Unrecognized command"),
                            };
                        }
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
