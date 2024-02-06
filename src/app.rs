use std::sync::Arc;

use crate::api::*;
use crate::{terminal::TerminalFrontend, ClientMessage, CommandId, GlobalState, ServerMessage};
use leptos::ev::SubmitEvent;
use leptos::html::{Input, Select};
use leptos::logging::*;
use leptos::*;
use leptos_meta::*;
use leptos_use::core::ConnectionReadyState;
use web_sys::MouseEvent;

#[component]
pub fn CommandOutputWindow(session_token: String) -> impl IntoView {
    let state = expect_context::<RwSignal<GlobalState>>();

    let command_ids = create_memo(move |_| {
        state
            .get()
            .command_outputs
            .keys()
            .copied()
            .collect::<Vec<CommandId>>()
    });

    let (_visible_command_id, set_visible_command_id) = create_slice(
        state,
        |state| state.visible_command_id,
        |state, new_visible_command_id| state.visible_command_id = new_visible_command_id,
    );
    let (connected_commands, set_connected_commands) = create_signal::<Vec<CommandId>>(vec![]);
    let (message_queue, set_message_queue) = create_slice(
        state,
        |state| state.message_queue.clone(),
        |state, new_message_queue| state.message_queue = new_message_queue,
    );

    let websocket = leptos_use::use_websocket(&format!(
        "ws://localhost:3000/api/ws?session_token={}",
        session_token
    ));

    create_effect(move |_| {
        let command_ids = command_ids.get();
        let connected_commands = connected_commands.get();

        let new_commands: Vec<CommandId> = command_ids
            .iter()
            .filter(|command_id| !connected_commands.contains(command_id))
            .copied()
            .collect();

        let mut new_message_queue = message_queue.get().clone();
        let new_command_messages: Vec<ClientMessage> = new_commands
            .iter()
            .map(|new_command_id| ClientMessage::GetCommandOutputStream {
                command_id: new_command_id.0,
            })
            .collect();

        new_message_queue.extend(new_command_messages);
        set_message_queue(new_message_queue);
        set_connected_commands.update(|connected_commands| {
            connected_commands.extend(new_commands);
        });
    });

    create_effect(move |_| {
        let message_queue = message_queue.get();

        if websocket.ready_state.get() != ConnectionReadyState::Open {
            return;
        }

        for message in message_queue {
            let send = &websocket.send;
            send(&serde_json::to_string(&message).unwrap());
        }

        set_message_queue(vec![]);
    });

    create_effect(move |_| {
        if let Some(message) = websocket.message.get() {
            match serde_json::from_str::<ServerMessage>(&message) {
                Ok(message) => match message {
                    ServerMessage::NoCommandWithId { command_id } => {
                        error!("There is no command with the id: {command_id}");
                    }
                    ServerMessage::InvalidPassword => {
                        error!("Invalid password");
                    }
                    ServerMessage::CommandOutput { command_id, output } => {
                        state.update(|state| {
                            let command_output =
                                state.command_outputs.entry(command_id).or_default();

                            command_output.0 += 1;
                            command_output.1.extend_from_slice(&output);
                        });
                    }
                },
                Err(e) => {
                    error!("Websocket message serialization error: {e}");
                }
            }
        }
    });

    create_effect(move |_| {
        let connected_commands = connected_commands();
        if connected_commands.len() == 0 {
            set_visible_command_id(None);
        } else if connected_commands.len() == 1 {
            set_visible_command_id(Some(
                *connected_commands
                    .first()
                    .expect("already checked that len == 1"),
            ));
        }
    });

    view! {
        <For each=connected_commands key=|command_id| { command_id.0 } children=move |command_id| {
            let onclick = move |ev: MouseEvent| {
                ev.prevent_default();
                set_visible_command_id(Some(command_id))
            };

            (view! {
                <button on:click=onclick>{format!("{}", command_id.0)}</button>
            }).into_view()
        } />
        <div id="terminals">
            <div>
            <For each=connected_commands key=|command_id| { command_id.0 } children=move |command_id| {
                view! {
                    <TerminalFrontend command_id=command_id />
                }
            } />
            </div>
        </div>
    }
}

#[component]
pub fn Login() -> impl IntoView {
    let password_input: NodeRef<Input> = create_node_ref();

    let state = expect_context::<RwSignal<GlobalState>>();

    let (_session_token, set_session_token) = create_slice(
        state,
        move |state| state.session_token.clone(),
        move |state, new_session_token| {
            state.session_token = new_session_token;
        },
    );

    let submit_action = create_action(move |password: &String| {
        let password = password.clone();
        async move { login(password).await }
    });

    let response = move || submit_action.value().get();

    create_effect(move |_| {
        if let Some(response) = response() {
            match response {
                Ok(session_token) => {
                    set_session_token(Some(session_token));
                }
                Err(e) => {
                    web_sys::console::log_1(&format!("Error logging in: {e}").into());
                }
            }
        }
    });

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();

        let password = password_input().expect("<input> to exist").value();
        submit_action.dispatch(password);
    };

    view! {
        <form on:submit=on_submit>
        <div class="input">
            <label for="password">Password</label>
            <input id="password" type="password" node_ref=password_input></input>
        </div>
        <input type="submit" value="Login"></input>
        </form>
    }
}

#[component]
pub fn LoggedIn(session_token: String) -> impl IntoView {
    let state = expect_context::<RwSignal<GlobalState>>();
    let session_token = Arc::new(session_token);

    let command_input: NodeRef<Select> = create_node_ref();

    let submit_action = create_action(|(command_name, session_token): &(String, String)| {
        let command_name = command_name.to_owned();
        let session_token = session_token.to_owned();
        async move { invoke_command(command_name, session_token).await }
    });

    let session_token_clone = session_token.clone();
    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();

        let command_name = command_input().expect("<input> to exist").value();
        submit_action.dispatch((command_name, session_token_clone.to_string()));
    };

    let session_token_clone = session_token.clone();
    let commands = create_resource(
        || (),
        move |_| {
            let session_token_clone = session_token_clone.clone();
            async move { command_list(session_token_clone.to_string()).await }
        },
    );

    let status_element = move || match submit_action.value()() {
        Some(Err(error)) => {
            let error = match error {
                ServerFnError::ServerError(error) => error,
                _ => format!("{error}"),
            };

            (view! {
                <div class="error">{error.to_string()}</div>
            })
            .into_view()
        }
        Some(Ok(command_id)) => {
            state.update(|state| {
                state.command_outputs.insert(command_id, (0, vec![]));
            });
            (view! {
                <div class="success">Success!</div>
            })
            .into_view()
        }
        _ => (view! {}).into_view(),
    };

    let command_options = move || {
        let commands = move || {
            commands()
                .map(|api_call_result| api_call_result.unwrap_or_default())
                .unwrap_or(vec![])
        };

        view! {
            <For each=commands key=|command| command.clone() children=move |command| {
                view! {
                    <option value=command.clone()>{command}</option>
                }
            } />
        }
    };

    view! {
        {move || status_element()}
        <CommandOutputWindow session_token=session_token.to_string() />
        <form on:submit=on_submit>
            <Suspense fallback=move || view! { <select>LOADING</select> }>
            <div class="input">
                <label for="command">Command</label>
                <select id="command" node_ref=command_input>
                {move || command_options()}
                </select>
            </div>
            </Suspense>
            <input type="submit" value="Pretty please, Ferris!"></input>
        </form>

    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_context(create_rw_signal(GlobalState::default()));

    let state = expect_context::<RwSignal<GlobalState>>();
    let session_token = create_memo(move |_| state.get().session_token);

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/ferris-the-waiter.css"/>
        <Stylesheet href="node_modules/xterm/css/xterm.css" />
        <Script src="node_modules/xterm/lib/xterm.js"></Script>
        <h1>Let Ferris Help You Out!</h1>
        <svg width="120" height="120">
            <image xlink:href="https://rustacean.net/assets/cuddlyferris.svg" src="https://rustacean.net/assets/cuddlyferris.svg" width="120" height="120"/>
        </svg>
        {
        move || {
            match session_token.get() {
               Some(session_token) => {
                   view! { <LoggedIn session_token=session_token /> }
               },
               None => view! { <Login /> },
            }
        }

        }
    }
}
