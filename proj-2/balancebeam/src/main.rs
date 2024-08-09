mod request;
mod response;

use clap::Parser;
use rand::{Rng, SeedableRng};
use request::write_to_stream;
use response::read_from_stream;
use std::{io::{self, Error, ErrorKind}, sync::Arc, time::Duration};
use tokio::{net::{TcpListener, TcpStream}, stream, sync::RwLock, time::Instant};

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[command(about = "Fun with load balancing")]
struct CmdOptions {
    /// "IP/port to bind to"
    #[arg(short, long, default_value = "0.0.0.0:1100")]
    bind: String,
    /// "Upstream host to forward requests to"
    #[arg(short, long)]
    upstream: Vec<String>,
    /// "Perform active health checks on this interval (in seconds)"
    #[arg(long, default_value = "10")]
    active_health_check_interval: usize,
    /// "Path to send request to for active health checks"
    #[arg(long, default_value = "/")]
    active_health_check_path: String,
    /// "Maximum number of requests to accept per IP per minute (0 = unlimited)"
    #[arg(long, default_value = "0")]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
#[derive(Clone)]
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Vec<String>,
    live_upstream: Arc<RwLock<Vec<String>>>,
}
#[tokio::main]
async fn main() -> io::Result<()> {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let listener =  TcpListener::bind(&options.bind).await?;
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        live_upstream: Arc::new(RwLock::new(options.upstream.clone())),
        upstream_addresses: options.upstream,    
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
    };

    let state_clone = state.clone();
    tokio::spawn(async move{
        active_health_check(&state_clone).await;
    });

    loop{
        if let Ok((stream, _)) = listener.accept().await {      
            // Handle the connection!
            let state = state.clone();
            tokio::spawn(async move{           
                handle_connection(stream, &state).await;
            });  
        }
    } 
}

async fn active_health_check(state: &ProxyState){   
    loop{        
        tokio::time::sleep(Duration::from_secs(state.active_health_check_interval.try_into().unwrap())).await;
        let mut write = state.live_upstream.write().await;
        write.clear();
        for stream in &state.upstream_addresses{          
            if let Ok(mut tcp_stream) = TcpStream::connect(stream).await{
                let request = http::Request::builder()
                    .method(http::Method::GET)
                    .uri(&state.active_health_check_path)
                    .header("Host", stream)
                    .body(Vec::new())
                    .unwrap();
                if let Err(err) = write_to_stream(&request, &mut tcp_stream).await{
                    log::error!("Fail to write request to stream {}: {}", stream, err);
                    return;
                }
                
                let response = match read_from_stream(&mut tcp_stream, request.method()).await{
                    Ok(response) => response,
                    Err(error) => {
                        log::error!("Error reading response from server: {:?}", error);
                        return;
                    }
                };
                match response.status().as_u16(){
                    200 => {
                        write.push(stream.clone());
                    }
                    _ => {
                        log::error!("Get unexpected response from upstream {}", stream);
                        return;
                    }
                }
            } 
            else{
                log::error!("Fail to connect to upstream {}",stream);
                return;
            }
        }  
    }  
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    let mut rng = rand::rngs::StdRng::from_entropy();   
    loop{
        let read = state.live_upstream.read().await;
        let upstream_idx = rng.gen_range(0..read.len());
        let upstream_ip = &read.get(upstream_idx).unwrap().clone();
        drop(read);
        
        match TcpStream::connect(upstream_ip).await {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                log::error!("Fail to connect to upstream {}: {}",upstream_ip, err);
                let mut write = state.live_upstream.write().await;
                write.swap_remove(upstream_idx);
                if write.len() == 0{
                    log::error!("All upstream failed!");
                    return Err(Error::new(ErrorKind::Other, "All upstreams are dead"));
                }
            },
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!(
        "{} <- {}",
        client_ip,
        response::format_response_line(&response)
    );
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = upstream_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!(
                "Failed to send request to upstream {}: {}",
                upstream_ip,
                error
            );
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
