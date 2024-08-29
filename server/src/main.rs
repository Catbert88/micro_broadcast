use axum::{
    routing::get,
    Router,
};

use axum::response::Html;

use sailfish::TemplateOnce;

use tokio::io::AsyncReadExt;

#[derive(PartialEq)]
enum TemplateFormat {
    Portal,
    Report,
}

#[derive(TemplateOnce)] // automatically implement `TemplateOnce` trait
#[template(path = "portal.stpl")] // specify the path to template
struct PortalTemplate {
    // data to be passed to the template
    format: TemplateFormat,
}

async fn process_socket(mut socket: tokio::net::TcpStream) {
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
                // Print out the received data
                println!("As string: {}", std::str::from_utf8(&buffer[..n]).unwrap_or("[Invalid UTF-8]"));
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
    let portal = PortalTemplate {
        format: TemplateFormat::Portal,
    };

    let html_content = portal.render_once().unwrap();
    Html(html_content)
}

#[tokio::main]
async fn main() {

    // build our application with a single route

    let app = Router::new().route("/", get(handler));

    tokio::spawn(async move {
        let slave_channel = tokio::net::TcpListener::bind("0.0.0.0:8092").await.unwrap();

        loop {
            println!("Checking clients");
            match slave_channel.accept().await {
                Ok((socket, _)) => process_socket(socket).await,
                Err(error) => println!("Connection failed: {}", error),
            };
        }
    });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8091").await.unwrap();
    axum::serve(listener, app).await.unwrap();

}
