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

#[derive(Clone)]
struct MicroWorker {
    mac_address: String,
    ip_address: SocketAddr,
    current_cmd: Option<String>,
}

struct AppState {
    workers: Arc<Mutex<Vec<MicroWorker>>>
}

#[derive(Deserialize)]
struct MessageRequest {
    id: String,
    message: String,
}

#[derive(Deserialize)]
struct TimerRequest {
    id: String,
    duration: String,
}

#[derive(Deserialize)]
struct AnimationRequest {
    id: String,
    animation: String,
}

#[derive(Serialize)]
struct RequestReceipt {
    status: String,
}

#[derive(TemplateOnce)] // automatically implement `TemplateOnce` trait
#[template(path = "portal.stpl")] // specify the path to template
struct PortalTemplate<'a> {
    workers: &'a Vec<MicroWorker>,
}

async fn register_worker(registry: &Arc<Mutex<Vec<MicroWorker>>>, mut socket: tokio::net::TcpStream) {
    println!("New connection from {:?}", socket.peer_addr().unwrap());

    // need to update "connected" workers?

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

                                println!("Registering MicroWorker {} ip_address: {}", mac_address, address);
                                let mut active_registry = registry.lock().unwrap();
                                active_registry.retain(|s| s.ip_address != rx_address);
                                active_registry.push(MicroWorker {mac_address: mac_address.to_string(), ip_address: rx_address, current_cmd: None });

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
        workers: &state.workers.lock().unwrap(),
    };

    let html_content = portal.render_once().unwrap();
    Html(html_content)
}

async fn message_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<MessageRequest>) -> Json<RequestReceipt> {

    println!("id: {}, message: {}", request.id, request.message);

    let mut workers = state.workers.lock().unwrap();

    for worker in workers.iter_mut() {
        worker.current_cmd = Some("MESSAGE ".to_string() + &request.message);
    }

    Json(RequestReceipt {status: "Complete".to_string() })
}

async fn timer_start_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<TimerRequest>) -> Json<RequestReceipt> {

    println!("id: {}, duration: {}", request.id, request.duration);

    let mut workers = state.workers.lock().unwrap();

    for worker in workers.iter_mut() {
        worker.current_cmd = Some("TIMER ".to_string() + &request.duration);
    }


    Json(RequestReceipt {status: "Complete".to_string() })
}

async fn timer_add_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<TimerRequest>) -> Json<RequestReceipt> {

    println!("id: {}, duration: {}", request.id, request.duration);

    let mut workers = state.workers.lock().unwrap();

    for worker in workers.iter_mut() {
        worker.current_cmd = Some("TIMER ".to_string() + &request.duration);
    }

    Json(RequestReceipt {status: "Complete".to_string() })
}

async fn animation_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<AnimationRequest>) -> Json<RequestReceipt> {

    println!("id: {}, animation: {}", request.id, request.animation);

    let mut workers = state.workers.lock().unwrap();

    for worker in workers.iter_mut() {
        worker.current_cmd = Some("ANIMATE ".to_string() + &request.animation);
    }

    Json(RequestReceipt {status: "Complete".to_string() })
}

#[tokio::main]
async fn main() {

    // build our application with a single route

    let workers: Vec<MicroWorker> = Vec::new();

    let workers = Arc::new(Mutex::new(workers));

    let shared_state = Arc::new(AppState { workers: workers.clone() });

    let app = Router::new().route("/messaging", post(message_handler))
        .route("/timerStart", post(timer_start_handler))
        .route("/timerAdd", post(timer_add_handler))
        .route("/animation", post(animation_handler))
        .route("/", get(portal_handler)).with_state(shared_state);

    let worker_registry = workers.clone();

    // Register thread
    tokio::spawn(async move {

        println!("Opening Registration");
        let registration_channel = tokio::net::TcpListener::bind(format!("0.0.0.0:{}",config::BROADCAST_PORT)).await.unwrap();

        loop {
            println!("Checking Registration Requests");

            match registration_channel.accept().await {
                Ok((socket, _)) => register_worker(&worker_registry, socket).await,
                Err(error) => println!("Connection failed: {}", error),
            };
        }
    });

    let worker_receivers = workers.clone();


    // Broadcasting thread
    tokio::spawn(async move {

        loop {
            let worker_snapshot: Vec<MicroWorker>;
            {
                worker_snapshot = worker_receivers.lock().unwrap().clone();
            }

            println!("broadcasting to {} receiver(s)", worker_snapshot.len());
            for worker in worker_snapshot
            {
                if let Some(cmd) = worker.current_cmd.or(Some("PING".to_string())) {
                    match timeout(Duration::from_millis(5000), tokio::net::TcpStream::connect(worker.ip_address)).await {
                        Ok(stream_s) => {
                            match stream_s {
                                Ok(mut stream) => {
                                    println!("broadcasting to cmd '{}' to {}", &cmd, worker.mac_address);
                                    match stream.write_all(&cmd.into_bytes()).await {
                                        Ok(()) => (),
                                        Err(e) => {
                                            println!("removeing worker after write failure. {}", e);
                                        },
                                    };
                                },

                                Err(e) => {
                                    println!("removeing worker after write failure. {}", e);
                                }
                            }
                        },
                        Err(e) => {
                            println!("removeing worker after connect failures. {}", e);
                            worker_receivers.lock().unwrap().retain(|s| s.ip_address != worker.ip_address);
                        }
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
