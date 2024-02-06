#[cfg(feature = "ssr")]
mod config_output_handler {
    use axum::{
        extract::{
            ws::{Message, WebSocket, WebSocketUpgrade},
            Query,
        },
        response::Response,
        Extension,
    };
    use ferris_the_waiter::{
        check_session_token, ClientMessage, CommandId, CommandOutputs, ServerMessage, SessionTokens,
    };
    use log::warn;
    use serde::Deserialize;
    use std::future::Future;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use tokio::io::AsyncReadExt;
    use tokio::process::{ChildStderr, ChildStdout};
    use tokio::sync::mpsc;

    struct CommandOutput {
        command_id: CommandId,
        output: Vec<u8>,
    }

    async fn push_command_output<R: tokio::io::AsyncRead + std::marker::Unpin>(
        mut r: R,
        command_id: CommandId,
        tx: mpsc::Sender<CommandOutput>,
        child_finished: Arc<AtomicBool>,
    ) -> Option<R> {
        let mut buf = [0; 1024];

        loop {
            let bytes_read = match r.read(&mut buf).await {
                Ok(bytes_read) => {
                    if bytes_read == 0 && child_finished.load(Ordering::SeqCst) {
                        return None;
                    }
                    bytes_read
                }
                Err(e) => {
                    warn!("Error reading from read stream: {e}");
                    return None;
                }
            };

            if bytes_read == 0 {
                continue;
            }

            if let Err(e) = tx
                .send(CommandOutput {
                    command_id,
                    output: buf[0..bytes_read].to_vec(),
                })
                .await
            {
                warn!("Could not send to websocket handler task: {e}");
                return Some(r);
            };
        }
    }

    async fn stream_child_outputs(
        tx: mpsc::Sender<CommandOutput>,
        command_id: CommandId,
        mut child: tokio::process::Child,
        command_name: String,
        command_outputs: Arc<Mutex<CommandOutputs>>,
    ) {
        use tokio::task::JoinError;

        let child_finished = Arc::new(AtomicBool::new(false));
        let stdout_finished = Arc::new(AtomicBool::new(false));
        let stderr_finished = Arc::new(AtomicBool::new(false));

        use futures::future::FutureExt;
        let mut stdout_push: Box<
            dyn Future<Output = Result<Option<ChildStdout>, JoinError>> + Unpin + Send,
        > = Box::new(std::future::pending());
        if let Some(stdout) = child.stdout.take() {
            stdout_push = Box::new(
                tokio::spawn(push_command_output(
                    stdout,
                    command_id,
                    tx.clone(),
                    child_finished.clone(),
                ))
                .fuse(),
            );
        } else {
            stdout_finished.store(true, Ordering::SeqCst);
        }

        let mut stderr_push: Box<
            dyn Future<Output = Result<Option<ChildStderr>, JoinError>> + Unpin + Send,
        > = Box::new(std::future::pending());
        if let Some(stderr) = child.stderr.take() {
            stderr_push = Box::new(
                tokio::spawn(push_command_output(
                    stderr,
                    command_id,
                    tx.clone(),
                    child_finished.clone(),
                ))
                .fuse(),
            );
        } else {
            stderr_finished.store(true, Ordering::SeqCst);
        }

        loop {
            if stderr_finished.load(Ordering::SeqCst) && stdout_finished.load(Ordering::SeqCst) {
                if !child_finished.load(Ordering::SeqCst) {
                    command_outputs
                        .lock()
                        .unwrap()
                        .0
                        .insert(command_id, (command_name, child));
                }
                return;
            }

            tokio::select! {
                _ = child.wait() => {
                    child_finished.store(true, Ordering::SeqCst);
                },
                stderr = &mut stderr_push => {
                    stderr_finished.store(true, Ordering::SeqCst);

                    if let Ok(Some(stderr)) = stderr {
                        child.stderr = Some(stderr);
                    }
                },
                stdout = &mut stdout_push => {
                    stdout_finished.store(true, Ordering::SeqCst);

                    if let Ok(Some(stdout)) = stdout {
                        child.stdout = Some(stdout);
                    }
                }
            }
        }
    }

    async fn handle_incoming_socket_message(
        socket: &mut WebSocket,
        msg: Result<Message, axum::Error>,
        command_outputs: Arc<Mutex<CommandOutputs>>,
        tx: mpsc::Sender<CommandOutput>,
    ) {
        let msg: ClientMessage = if let Ok(msg) = msg {
            match msg {
                Message::Text(msg) => match serde_json::from_str(&msg) {
                    Ok(client_msg) => client_msg,
                    Err(e) => {
                        warn!("Failed to parse client message: {e}");
                        return;
                    }
                },
                _ => {
                    return;
                }
            }
        } else {
            return;
        };

        match msg {
            ClientMessage::GetCommandOutputStream { command_id } => {
                let ws_msg: Option<Message> = {
                    let mut command_outputs_inner = command_outputs.lock().unwrap();

                    if let Some((command_name, child)) =
                        command_outputs_inner.0.remove(&CommandId(command_id))
                    {
                        tokio::task::spawn(stream_child_outputs(
                            tx.clone(),
                            CommandId(command_id),
                            child,
                            command_name,
                            command_outputs.clone(),
                        ));

                        None
                    } else {
                        let msg =
                            serde_json::to_string(&ServerMessage::NoCommandWithId { command_id })
                                .unwrap();
                        Some(Message::Text(msg))
                    }
                };
                if let Some(ws_msg) = ws_msg {
                    if let Err(e) = socket.send(ws_msg).await {
                        warn!("Error sending WS message {e}, terminating connection");
                        return;
                    };
                }
            }
            ClientMessage::UnsubscribeFromCommandOutput {
                command_id: _command_id,
            } => {}
        }
    }

    async fn handle_socket(mut socket: WebSocket, command_outputs: Arc<Mutex<CommandOutputs>>) {
        let (tx, mut rx) = mpsc::channel::<CommandOutput>(128);

        loop {
            tokio::select! {
                msg = socket.recv() => {
                    if let Some(msg) = msg {
                        handle_incoming_socket_message(&mut socket, msg, command_outputs.clone(), tx.clone()).await;
                    } else {
                        return;
                    };
                },
                command_output = rx.recv() => {
                    let Some(command_output) = command_output else {
                        return;
                    };
                    let CommandOutput { command_id, output } = command_output;

                    if let Ok(msg) = serde_json::to_string(&ServerMessage::CommandOutput { command_id, output }) {
                        if let Err(e) = socket.send(Message::Text(msg)).await {
                            warn!("Could not send message to client over WebSocket: {e}");
                        }
                    };
                }
            }
        }
    }

    #[derive(Deserialize, Debug)]
    pub struct WsAuthQuery {
        session_token: String,
    }

    #[axum::debug_handler]
    pub async fn config_output_ws(
        ws: WebSocketUpgrade,
        Query(WsAuthQuery { session_token }): Query<WsAuthQuery>,
        Extension(session_tokens): Extension<Arc<Mutex<SessionTokens>>>,
        Extension(command_outputs): Extension<Arc<Mutex<CommandOutputs>>>,
    ) -> Result<Response, String> {
        check_session_token(&mut session_tokens.lock().unwrap(), &session_token)
            .map_err(|e| e.to_string())?;

        Ok(ws.on_upgrade(move |socket| handle_socket(socket, command_outputs)))
    }
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use axum::routing::get;
    use axum::Router;
    use ferris_the_waiter::app::*;
    use ferris_the_waiter::fileserv::file_and_error_handler;
    use ferris_the_waiter::{
        CommandOutputs, Commands, PasswordHashString, RateLimiting, SessionTokens,
    };
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::env;
    use std::fs::File;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    env_logger::init();

    #[derive(Serialize, Deserialize, Debug)]
    struct Config {
        commands: Commands,
        password_hash: String,
    }

    // Setting get_configuration(None) means we'll be using cargo-leptos's env values
    // For deployment these variables are:
    // <https://github.com/leptos-rs/start-axum#executing-a-server-on-a-remote-machine-without-the-toolchain>
    // Alternately a file can be specified such as Some("Cargo.toml")
    // The file would need to be included with the executable when moved to deployment
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let routes = generate_route_list(App);

    let mut config_file = File::open(
        env::var("FERRIS_WAITER_CONFIG")
            .expect("config path to be configured with 'FERRIS_WAITER_CONFIG'"),
    )?;

    let mut contents = String::new();
    config_file.read_to_string(&mut contents)?;

    let config: Config = toml::from_str(&contents)?;

    let commands_resource = Arc::new(config.commands);
    let password_hash_resource = Arc::new(PasswordHashString(config.password_hash));
    let command_outputs = Arc::new(Mutex::new(CommandOutputs(HashMap::new())));
    let session_tokens = Arc::new(Mutex::new(SessionTokens(HashMap::new())));

    // build our application with a route
    let app = Router::new()
        .route("/api/ws", get(config_output_handler::config_output_ws))
        .leptos_routes(&leptos_options, routes, App)
        .fallback(file_and_error_handler)
        .with_state(leptos_options)
        .layer(axum::Extension(commands_resource))
        .layer(axum::Extension(password_hash_resource))
        .layer(axum::Extension(session_tokens))
        .layer(axum::Extension(Arc::new(Mutex::new(
            RateLimiting::default(),
        ))))
        .layer(axum::Extension(command_outputs));

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    logging::log!("listening on http://{}", &addr);
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();

    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for a purely client-side app
    // see lib.rs for hydration function instead
}
