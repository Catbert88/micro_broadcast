use axum::{
    routing::get,
    Router,
};

use std::sync::{Arc, Mutex};
use core::net::SocketAddr;

use axum::response::Html;

use sailfish::TemplateOnce;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;

// One thread is broadcasting to all active slaves. If there is an issue communicating to a slave,
// that slave is dropped.
// Another thread is broadcasting to all known slave devices

#[derive(Clone)]
struct MicroSlave {
    mac_address: String,
    ip_address: SocketAddr,
}

#[derive(TemplateOnce)] // automatically implement `TemplateOnce` trait
#[template(path = "portal.stpl")] // specify the path to template
struct PortalTemplate<'a> {
    slaves: &'a Vec<MicroSlave>,
}

async fn register_slave(registry: &Arc<Mutex<Vec<MicroSlave>>>, mut socket: tokio::net::TcpStream) {
    println!("New connection from {:?}", socket.peer_addr().unwrap());

    // need to update "connected" slaves?

    let mut buffer = [0u8; 1024];
    loop {
        // Read data from the client
        match socket.read(&mut buffer).await {
            Ok(0) => {
                // Connection was closed by the client
                println!("Client disconnected.");
                break;
            }
            Ok(n) => {
                let address = socket.peer_addr().unwrap();
                let rx_address = SocketAddr::new(address.ip(), 8092);
                let mut active_registry = registry.lock().unwrap();
                println!("Registering MicroSlave with address: {} saying {}", address,  std::str::from_utf8(&buffer[..n]).unwrap_or("[Invalid UTF-8]"));
                active_registry.push(MicroSlave {mac_address: "hi".to_string(), ip_address: rx_address });
                // Print out the received data
            }
            Err(e) => {
                // An error occurred while reading
                println!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
    // do work with socket here
}


async fn handler() -> Html<String> {
    let mut slaves: Vec<MicroSlave> = Vec::new();
    //slaves.push(MicroSlave {mac_address: "Georgia".to_string(), ip_address: "144".to_string()});
    //slaves.push(MicroSlave {mac_address: "Asher".to_string(), ip_address: "144".to_string()});
    //slaves.push(MicroSlave {mac_address: "Lila".to_string(), ip_address: "144".to_string()});
    let portal = PortalTemplate {
        slaves: &slaves,
    };

    let html_content = portal.render_once().unwrap();
    Html(html_content)
}

#[tokio::main]
async fn main() {

    // build our application with a single route

    let mut slaves: Vec<MicroSlave> = Vec::new();
    //slaves.push(MicroSlave {mac_address: "Georgia".to_string(), ip_address: "localhost:9000".to_string()});
    //slaves.push(MicroSlave {mac_address: "Asher".to_string(), ip_address: "localhost:9000".to_string()});
    //slaves.push(MicroSlave {mac_address: "Lila".to_string(), ip_address: "localhost:9000".to_string()});

    let slaves = Arc::new(Mutex::new(slaves));

    let app = Router::new().route("/", get(handler));

    let slave_registry = slaves.clone();

    // Register thread
    tokio::spawn(async move {
        let registration_channel = tokio::net::TcpListener::bind("0.0.0.0:8092").await.unwrap();

        loop {
            println!("Checking clients");
            match registration_channel.accept().await {
                Ok((socket, _)) => register_slave(&slave_registry, socket).await,
                Err(error) => println!("Connection failed: {}", error),
            };
        }
    });

    let slave_receivers = slaves.clone();
    // Broadcasting thread
    tokio::spawn(async move {

        loop {
            let slave_snapshot: Vec<MicroSlave>;
            {
                slave_snapshot = slave_receivers.lock().unwrap().clone();
            }

            for slave in slave_snapshot
            {
                match tokio::net::TcpStream::connect(slave.ip_address).await {
                    Ok(mut stream) => {
                        println!("broadcasting to slave");
                        match stream.write_all(b"start timer").await {
                            Ok(()) => println!("wrote to slave"),
                            Err(e) => {
                                println!("removeing slave after write failure. {}", e);
                            },
                        }
                    },
                    Err(e) => {
                        println!("removeing slave after connect failures. {}", e);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1000));
        }

    });

    // Server thread
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8091").await.unwrap();
    axum::serve(listener, app).await.unwrap();

}
