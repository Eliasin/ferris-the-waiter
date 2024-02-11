use leptos::ev::SubmitEvent;
use leptos::html::Input;
use leptos::*;
use leptos_meta::*;

#[server(CommandList, "/ferris/api")]
pub async fn command_list() -> Result<Vec<String>, ServerFnError> {
    use crate::Commands;
    use axum::extract::Extension;
    use leptos_axum::extract;
    use std::sync::Arc;
    let Extension(commands): Extension<Arc<Commands>> = extract().await?;

    Ok(commands.0.iter().map(|cmd| cmd.name.clone()).collect())
}

#[server(InvokeCommand, "/ferris/api")]
pub async fn invoke_command(command_name: String, password: String) -> Result<(), ServerFnError> {
    use crate::Commands;
    use crate::PasswordHashString;
    use crate::RateLimiting;
    use axum::extract::Extension;
    use leptos_axum::extract;
    use std::sync::{Arc, Mutex};
    use tokio::process::Command;

    let Extension(commands): Extension<Arc<Commands>> = extract().await?;
    let Extension(password_hash): Extension<Arc<PasswordHashString>> = extract().await?;
    let Extension(rate_limiting): Extension<Arc<Mutex<RateLimiting>>> = extract().await?;

    {
        let rate_limiting = rate_limiting.lock().unwrap();

        let now = std::time::Instant::now();

        if let Some(last_request_time) = (*rate_limiting).last_request_time {
            if now.duration_since(last_request_time) < std::time::Duration::from_millis(5000) {
                return Err(ServerFnError::ServerError("Rate limited".to_string()));
            }
        }
    }

    if !bcrypt::verify(password.as_bytes(), password_hash.0.as_str())? {
        {
            let mut rate_limiting = rate_limiting.lock().unwrap();

            let now = std::time::Instant::now();
            rate_limiting.last_request_time = Some(now);
        }
        return Err(ServerFnError::ServerError("Invalid password".to_string()));
    }
    {
        let mut rate_limiting = rate_limiting.lock().unwrap();

        let now = std::time::Instant::now();
        rate_limiting.last_request_time = Some(now);
    }

    match commands.0.iter().find(|cmd| cmd.name == command_name) {
        None => Err(ServerFnError::ServerError(
            "No command with that name".to_string(),
        )),
        Some(command_obj) => {
            let mut command = Command::new("sh");
            command
                .arg("-c")
                .args(shlex::Shlex::new(&command_obj.command));
            command.spawn()?.wait().await?;

            Ok(())
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let commands = create_resource(|| (), |_| async move { command_list().await });

    let (selected_command, set_selected_command) = create_signal(None::<String>);
    let password_input: NodeRef<Input> = create_node_ref();

    let submit_action = create_action(|(command_name, password): &(String, String)| {
        let command_name = command_name.to_owned();
        let password = password.to_owned();
        async move { invoke_command(command_name, password).await }
    });

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        if let Some(command_name) = selected_command.get().clone() {
            let password = password_input().expect("<input> to exist").value();

            submit_action.dispatch((command_name, password));
        } else {
            panic!("No command selected")
        }
    };

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/ferris-the-waiter.css"/>
        <div class="grid grid-flow-col grid-rows-4 py-[20dvh]">
            <div class="w-fit mx-auto mb-10 flex items-center justify-start [&>*]:drop-shadow-[0_20px_13px_rgba(0,0,0,.3)]">
                <img height="120" width="120" src="https://rustacean.net/assets/cuddlyferris.svg" />
                <div class="w-3 overflow-hidden">
                    <div class="h-4 bg-blue-400 rotate-45 transform origin-bottom-right rounded-sm"></div>
                </div>
                <div class="bg-blue-400 p-4 my-6 rounded-lg flex-1">
                    Let Ferris Help You Out!
                </div>
            </div>

            <Suspense fallback=move || view! { <select>LOADING</select> }>
                <ul class="grid grid-cols-4 gap-2 align-middle mb-5">
                    {move || {
                        match commands() {
                        Some(Ok(commands)) => {
                            commands
                            .iter()
                            .map(|command| {
                                let command_clone_for_text = command.clone();
                                let command_clone_for_click = command.clone();
                                let is_selected =
                                selected_command.get().as_ref() == Some(command);

                                view! {
                                <li
                                    on:click=move |_| {
                                        if is_selected {
                                            set_selected_command(None);
                                        } else {
                                            set_selected_command(Some(
                                                command_clone_for_click.clone()
                                            ));
                                        }
                                    }
                                >
                                    <button class=format!("p-4 w-full h-full rounded-md border-2 {}", {
                                        if is_selected {
                                            "border-black hover:bg-gray-400"
                                        } else {
                                            "border-transparent hover:bg-gray-200"
                                        }
                                    })>{command_clone_for_text}</button>
                                </li>
                                }
                            })
                            .collect::<Vec<_>>().into_view()
                        }
                        Some(Err(_)) => (view! {}).into_view(),
                        None => (view! {}).into_view(),
                        }
                    }}
                </ul>
                <form on:submit=on_submit class="flex flex-col gap-2">
                    <label for="password" class="text-gray-700">Password</label>
                    <input
                        type="password"
                        id="password"
                        node_ref=password_input
                        class="border border-gray-300 rounded-md pl-2 py-2"
                    />
                    <input
                        type="submit"
                        value="Pretty please, Ferris!"
                        class="bg-blue-500 hover:bg-blue-700 hover:cursor-pointer text-white font-bold py-2 px-4 rounded-md"
                    />
                </form>
            </Suspense>

            {
                move || {
                    match submit_action.value()() {
                    Some(Err(error)) => {
                        let error = match error {
                        ServerFnError::ServerError(error) => error,
                        _ => format!("{error}"),
                        };

                        (view! {
                        <div id="error" class="p-2 mx-auto text-red-500">{error.to_string()}</div>
                        }).into_view()
                    }
                    Some(Ok(_)) => {
                        (view! {
                        <div id="success" class="p-2 mx-auto text-green-500">Success!</div>
                        }).into_view()
                    }
                    _ => (view! {}).into_view(),
                    }
                }
            }
        </div>
    }
}
