use axum::{
    routing::get,
    Router,
};

use phf::phf_map;

use std::sync::{Arc, Mutex};
use std::net::SocketAddr;

use axum::response::Html;
use axum::extract;
use axum::Json;
use axum::routing::post;

use serde::Deserialize;
use serde::Serialize;

use sailfish::TemplateOnce;

use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::Error;
use tokio::time::Duration;
use tokio::time::timeout;

use axum::extract::State;

use std::ops::AddAssign;

use std::str::FromStr;

//use std::collections::HashSet;

static PERSISTENT_WORKERS: phf::Map<&'static str, &'static str> = phf_map! {
    "EC:DA:3B:BF:46:9C" => "Georgia",
    "EC:DA:3B:BF:49:2C" => "Asher",
    "EC:DA:3B:BF:39:74" => "Lila",
};


#[derive(Clone)]
enum MicroCommand {
    Ping(MicroPing),
    Message(MicroMessage),
    Timer(MicroTimer),
    Animation(MicroAnimation),
}

impl MicroCommand {
    async fn execute(&self, worker_connection: tokio::net::TcpStream) -> Result<(),Error> {
        match self {
            MicroCommand::Ping(cmd) => cmd.execute(worker_connection).await,
            MicroCommand::Message(cmd) => cmd.execute(worker_connection).await,
            MicroCommand::Timer(cmd) => cmd.execute(worker_connection).await,
            MicroCommand::Animation(cmd) => cmd.execute(worker_connection).await,
        }
    }
}

#[derive(Clone)]
struct MicroPing {
}

impl MicroPing {
    async fn execute(&self, mut worker_connection: tokio::net::TcpStream) -> Result<(),Error>  {
        println!("Executing Ping Command");
        let directive = "PING".to_string();
        worker_connection.write_all(&directive.into_bytes()).await
    }
}

#[derive(Clone)]
struct MicroMessage {
    message: String,
}

impl MicroMessage {

    async fn execute(&self, mut worker_connection: tokio::net::TcpStream) -> Result<(),Error>  {
        println!("Executing Message Command");

        let directive = "MESSAGE ".to_string() + &self.message;

        //println!("broadcasting cmd '{}' to {}", &cmd.to_string(), worker.mac_address);
        worker_connection.write_all(&directive.into_bytes()).await
    }

    fn raw(&self) -> String {
        self.message.to_string()
    }

    fn extract_last_message(cmd: &Option<MicroCommand>) -> String {
        if let Some(MicroCommand::Message(c)) = cmd {
            c.raw()
        } else {
            "".to_string()
        }
    }
}


#[derive(Clone)]
struct MicroTimer {
    start: tokio::time::Instant,
    duration: tokio::time::Duration,
}


impl MicroTimer {

    async fn execute(&self, mut worker_connection: tokio::net::TcpStream) -> Result<(),Error>  {
        println!("Executing Timer Command");
        let remaining = self.duration.checked_sub(tokio::time::Instant::now().duration_since(self.start)).unwrap_or(tokio::time::Duration::new(0,0));
        let directive = "TIMER ".to_string() + &remaining.as_secs().to_string() + "/" + &self.duration.as_secs().to_string();
        worker_connection.write_all(&directive.into_bytes()).await
    }

    fn raw(&self) -> String {
        match self.duration.checked_sub(tokio::time::Instant::now().duration_since(self.start)) {
            Some(remaining) => remaining.as_secs().to_string(),
            None => "00:00".to_string(),
        }
    }

    fn extract_remaining_time(cmd: &Option<MicroCommand>) -> String {
        if let Some(MicroCommand::Timer(c)) = cmd {
            c.raw()
        } else {
            "00:00".to_string()
        }
    }
}

#[derive(Clone)]
struct MicroAnimation {
    animation: String
}

impl MicroAnimation {

    async fn execute(&self, mut worker_connection: tokio::net::TcpStream) -> Result<(),Error>  {
        println!("Executing Animate Command");
        let directive = "ANIMATE ".to_string() + &self.animation;
        worker_connection.write_all(&directive.into_bytes()).await
    }

    fn raw(&self) -> String {
        self.animation.to_string()
    }

    fn extract_animation(cmd: &Option<MicroCommand>) -> String {
        if let Some(MicroCommand::Animation(c)) = cmd {
            c.raw()
        } else {
            "".to_string()
        }
    }
}

#[derive(Clone)]
struct MicroWorker {
    mac_address: String,
    alias: Option<String>,
    ip_address: Option<SocketAddr>,
    active: bool,
    persistent: bool,
    current_cmd: Option<MicroCommand>,
}

impl MicroWorker {

    fn get_alias(mac_address: &str) -> Option<String> {
        if let Some(a) = PERSISTENT_WORKERS.get(mac_address) {
            return Some(a.to_string());
        } else {
            return None;
        }
    }

    fn new(mac_address: String, ip_address: Option<SocketAddr>) -> Self {
        Self {
            alias: MicroWorker::get_alias(&mac_address),
            mac_address: mac_address,
            ip_address: ip_address,
            active: true,
            persistent: false,
            current_cmd: None,
        }
    }

    fn name(&self) -> &str {
        if let Some(a) = &self.alias {
            return &a;
        } else {
            return &self.mac_address;
        }
    }
}


struct AppState {
    micro_manager: Arc<Mutex<MicroManager>>
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

struct MicroManager {
    workers: Vec<MicroWorker>
}

impl MicroManager {

    fn new() -> Self {
       let mut workers: Vec<MicroWorker> = Vec::new();

        for (mac_address, alias) in PERSISTENT_WORKERS.entries() {
            workers.push( MicroWorker {
                mac_address: mac_address.to_string(),
                alias: Some(alias.to_string()),
                ip_address: None,
                active: false,
                persistent: true,
                current_cmd: None,
            });
        }

        Self { workers: workers }
    }

    fn add_worker(&mut self, mac_address: String, ip_address: SocketAddr) {
        if let Some(w) = self.get_worker_mut(&mac_address) {
            println!("Setting persistent worker {} to active", w.name());
            w.active = true;
            w.ip_address = Some(ip_address);
        } else {
            self.workers.push(MicroWorker::new(mac_address, Some(ip_address)) );
        }
    }

    fn remove_worker(&mut self, mac_address: &str) {
        if let Some(w) = self.get_worker_mut(mac_address) {
            if w.persistent {
                w.active = false;
            } else {
                self.workers.retain(|w| w.mac_address != mac_address);
            }
        }
    }

    fn get_worker_mut(&mut self, mac_address: &str) -> Option<&mut MicroWorker> {
        self.workers.iter_mut().find(|w| w.mac_address == mac_address)
    }

    #[allow(dead_code)]
    fn get_worker(&mut self, mac_address: &str) -> Option<&MicroWorker> {
        self.workers.iter().find(|w| w.mac_address == mac_address)
    }

}

#[derive(TemplateOnce)] // automatically implement `TemplateOnce` trait
#[template(path = "portal.stpl")] // specify the path to template
struct PortalTemplate<'a> {
    workers: &'a Vec<MicroWorker>,
}

async fn register_worker(registry: Arc<Mutex<MicroManager>>, mut socket: tokio::net::TcpStream) {
    println!("New connection from {:?}", socket.peer_addr().unwrap());

    // need to update "connected" workers?

    let mut buffer = [0u8; 1024];
    let mut length = 0;
    loop {
        // Read data from the client
        match socket.read(&mut buffer).await {
            Ok(0) => {

                let message = std::str::from_utf8(&buffer[0..length]).unwrap_or("[Invalid UTF-8]");
                println!("Message: {}", message);

                let mut parts = message.split_ascii_whitespace();
                match parts.next() {
                    Some("REGISTER") => {
                        match parts.next() {
                            Some(mac_address) => {
                                let address = socket.peer_addr().unwrap();
                                let rx_address = SocketAddr::new(address.ip(), config::BROADCAST_PORT);

                                println!("Registering MicroWorker {} ip_address: {}", mac_address, address);
                                registry.lock().unwrap().add_worker(mac_address.to_string(), rx_address);
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
            Ok(n) => length.add_assign(n),
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
        workers: &state.micro_manager.lock().unwrap().workers,
    };

    let html_content = portal.render_once().unwrap();
    Html(html_content)
}

async fn message_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<MessageRequest>) -> Json<RequestReceipt> {

    println!("id: {}, message: {}", request.id, request.message);

    let message_cmd = MicroMessage {message: request.message.to_string() };

    if request.id == "Broadcast" {
        for w in &mut state.micro_manager.lock().unwrap().workers {
            w.current_cmd = Some(MicroCommand::Message(message_cmd.clone()));
        }
        Json(RequestReceipt {status: "Complete".to_string() })
    } else {

        if let Some(w) = state.micro_manager.lock().unwrap().get_worker_mut(&request.id) {
            w.current_cmd = Some(MicroCommand::Message(message_cmd.clone()));
            Json(RequestReceipt {status: "Complete".to_string() })
        } else {
            Json(RequestReceipt {status: "Unavailable".to_string() })
        }
    }
}

async fn timer_start_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<TimerRequest>) -> Json<RequestReceipt> {

    println!("id: {}, duration: {}", request.id, request.duration);

    let timer_cmd = MicroTimer {start: tokio::time::Instant::now(), duration: tokio::time::Duration::from_secs(u64::from_str(&request.duration).unwrap()*60)};

    if request.id == "Broadcast" {
        for w in &mut state.micro_manager.lock().unwrap().workers {
            w.current_cmd = Some(MicroCommand::Timer(timer_cmd.clone()));
        }
        Json(RequestReceipt {status: "Complete".to_string() })
    } else {
        if let Some(w) = state.micro_manager.lock().unwrap().get_worker_mut(&request.id) {
            w.current_cmd = Some(MicroCommand::Timer(timer_cmd.clone()));
            Json(RequestReceipt {status: "Complete".to_string() })
        } else {
            Json(RequestReceipt {status: "Unavailable".to_string() })
        }
    }
}

async fn timer_add_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<TimerRequest>) -> Json<RequestReceipt> {

    println!("id: {}, duration: {}", request.id, request.duration);

    let timer_cmd = MicroTimer {start: tokio::time::Instant::now(), duration: tokio::time::Duration::from_secs(u64::from_str(&request.duration).unwrap()*60)};

    if request.id == "Broadcast" {
        for w in &mut state.micro_manager.lock().unwrap().workers {
            if let Some(MicroCommand::Timer(ref mut existing_cmd)) = w.current_cmd {
                existing_cmd.duration = existing_cmd.duration.checked_add(tokio::time::Duration::from_secs(u64::from_str(&request.duration).unwrap()*60)).unwrap();
            } else {
                w.current_cmd = Some(MicroCommand::Timer(timer_cmd.clone()));
            }
        }
        Json(RequestReceipt {status: "Complete".to_string() })
    } else {
        if let Some(w) = state.micro_manager.lock().unwrap().get_worker_mut(&request.id) {
            if let Some(MicroCommand::Timer(ref mut existing_cmd)) = w.current_cmd {
                existing_cmd.duration = existing_cmd.duration.checked_add(tokio::time::Duration::from_secs(u64::from_str(&request.duration).unwrap()*60)).unwrap();
            } else {
                w.current_cmd = Some(MicroCommand::Timer(timer_cmd.clone()));
            }
            Json(RequestReceipt {status: "Complete".to_string() })
        } else {
            Json(RequestReceipt {status: "Unavailable".to_string() })
        }
    }

}

async fn animation_handler(State(state): State<Arc<AppState>>, extract::Json(request): extract::Json<AnimationRequest>) -> Json<RequestReceipt> {

    println!("id: {}, animation: {}", request.id, request.animation);

    let animation_cmd = MicroAnimation {animation: request.animation};

    if request.id == "Broadcast" {
        for w in &mut state.micro_manager.lock().unwrap().workers {
            w.current_cmd = Some(MicroCommand::Animation(animation_cmd.clone()));
        }
        Json(RequestReceipt {status: "Complete".to_string() })
    } else {
        if let Some(w) = state.micro_manager.lock().unwrap().get_worker_mut(&request.id) {
            w.current_cmd = Some(MicroCommand::Animation(animation_cmd.clone()));
            Json(RequestReceipt {status: "Complete".to_string() })
        } else {
            Json(RequestReceipt {status: "Unavailable".to_string() })
        }
    }
}

#[tokio::main]
async fn main() {


    let micro_manager = Arc::new(Mutex::new(MicroManager::new()));

    let shared_state = Arc::new(AppState { micro_manager: micro_manager.clone() });

    let app = Router::new().route("/messaging", post(message_handler))
        .route("/timerStart", post(timer_start_handler))
        .route("/timerAdd", post(timer_add_handler))
        .route("/animation", post(animation_handler))
        .route("/", get(portal_handler)).with_state(shared_state);

    // Register thread
    tokio::spawn({

        let micro_manager = micro_manager.clone();

        async move {

            println!("Opening Registration");
            let registration_channel = tokio::net::TcpListener::bind(format!("0.0.0.0:{}",config::BROADCAST_PORT)).await.unwrap();

            loop {
                println!("Checking Registration Requests");

                match registration_channel.accept().await {
                    Ok((socket, _)) => register_worker(micro_manager.clone(), socket).await,
                    Err(error) => println!("Connection failed: {}", error),
                };
            }
        }
    });


    // Broadcasting thread
    tokio::spawn({
        let micro_manager = micro_manager.clone();

        async move {

            loop {

                let workers: Vec<MicroWorker>;
                {
                    workers = micro_manager.lock().unwrap().workers.clone();
                }

                //println!("Managing to {} worker(s)", workers.len());
                for worker in workers
                {
                    if let Some(ip_address) = worker.ip_address {
                        match timeout(Duration::from_millis(5000), tokio::net::TcpStream::connect(ip_address)).await {
                            Ok(stream_s) => {
                                match stream_s {
                                    Ok(stream) => {
                                        match worker.current_cmd.unwrap_or(MicroCommand::Ping(MicroPing{})).execute(stream).await {
                                            Ok(()) => (),
                                            Err(e) => println!("failed: {}", e),
                                        };
                                    },

                                    Err(e) => {
                                        println!("removing worker after write failure. {}", e);
                                        micro_manager.lock().unwrap().remove_worker(&worker.mac_address);
                                    }
                                }
                            },
                            Err(e) => {
                                println!("removing worker after connect failures. {}", e);
                                micro_manager.lock().unwrap().remove_worker(&worker.mac_address);
                            }
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }

    });

    // Server thread
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8091").await.unwrap();
    axum::serve(listener, app).await.unwrap();

}
