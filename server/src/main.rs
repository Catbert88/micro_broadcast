use axum::{
    routing::get,
    Router,
};

use std::sync::{Arc, Mutex};
use core::net::SocketAddr;

use axum::response::Html;
use axum::extract;
use axum::Json;
use axum::routing::post;

use serde::Deserialize;
use serde::Serialize;

use sailfish::TemplateOnce;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;
use tokio::time::timeout;

use axum::extract::State;

// One thread is broadcasting to all active slaves. If there is an issue communicating to a slave,
// that slave is dropped.
// Another thread is broadcasting to all known slave devices

#[derive(Clone)]
struct MicroSlave {
    mac_address: String,
    ip_address: SocketAddr,
}

struct AppState {
    slaves: Arc<Mutex<Vec<MicroSlave>>>
}

#[derive(Deserialize)]
struct MessageRequest {
    id: String,
    message: String,
}

#[derive(Serialize)]
struct MessageReceipt {
    status: String,
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

                let message = std::str::from_utf8(&buffer).unwrap_or("[Invalid UTF-8]");
                println!("Message: {}", message);

                let mut parts = message.split_ascii_whitespace();
                match parts.next() {
                    Some("REGISTER") => {
                        match parts.next() {
                            Some(mac_address) => {
                                let address = socket.peer_addr().unwrap();
                                let rx_address = SocketAddr::new(address.ip(), config::BROADCAST_PORT);

                                println!("Registering MicroSlave {} ip_address: {}", mac_address, address);
                                let mut active_registry = registry.lock().unwrap();
                                active_registry.retain(|s| s.ip_address != rx_address);
                                active_registry.push(MicroSlave {mac_address: mac_address.to_string(), ip_address: rx_address });

                            },
                            None => {
                            }
                        };
                    },
                    Some(_) => println!("Invalid Request"),
                    None => println!("Invalid Request"),
                };
                break;
            }
            Ok(_n) => (),
            Err(e) => {
                // An error occurred while reading
                println!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
    // do work with socket here
}


async fn portal_handler(State(state): State<Arc<AppState>>) -> Html<String> {

    let portal = PortalTemplate {
        slaves: &state.slaves.lock().unwrap(),
    };

    let html_content = portal.render_once().unwrap();
    Html(html_content)
}

async fn message_handler(extract::Json(request): extract::Json<MessageRequest>) -> Json<MessageReceipt> {
    println!("message: {}", request.message);
    Json(MessageReceipt {status: "Complete".to_string() })
}

#[tokio::main]
async fn main() {

    // build our application with a single route

    let slaves: Vec<MicroSlave> = Vec::new();

    let slaves = Arc::new(Mutex::new(slaves));

    let shared_state = Arc::new(AppState { slaves: slaves.clone() });

    let app = Router::new().route("/messaging",post(message_handler))
        .route("/", get(portal_handler)).with_state(shared_state);

    let slave_registry = slaves.clone();

    // Register thread
    tokio::spawn(async move {

        println!("Opening Registration");
        let registration_channel = tokio::net::TcpListener::bind(format!("0.0.0.0:{}",config::BROADCAST_PORT)).await.unwrap();

        loop {
            println!("Checking Registration Requests");

            match registration_channel.accept().await {
                Ok((socket, _)) => register_slave(&slave_registry, socket).await,
                Err(error) => println!("Connection failed: {}", error),
            };
        }
    });

    let slave_receivers = slaves.clone();


    // Broadcasting thread
    tokio::spawn(async move {

        let current_directive = b"ANIMATE Guardian";

        loop {
            let slave_snapshot: Vec<MicroSlave>;
            {
                slave_snapshot = slave_receivers.lock().unwrap().clone();
            }

            println!("broadcasting to {} slave(s)", slave_snapshot.len());
            for slave in slave_snapshot
            {
                match timeout(Duration::from_millis(5000), tokio::net::TcpStream::connect(slave.ip_address)).await {
                    Ok(stream_s) => {
                        match stream_s {
                            Ok(mut stream) => {
                                match stream.write_all(current_directive).await {
                                    Ok(()) => (),
                                    Err(e) => {
                                        println!("removeing slave after write failure. {}", e);
                                    },
                                };
                            },
                            Err(e) => {
                                println!("removeing slave after write failure. {}", e);
                            }
                        }
                    },
                    Err(e) => {
                        println!("removeing slave after connect failures. {}", e);
                        slave_receivers.lock().unwrap().retain(|s| s.ip_address != slave.ip_address);
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }

    });

    // Server thread
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8091").await.unwrap();
    axum::serve(listener, app).await.unwrap();

}
