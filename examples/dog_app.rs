use dioxus::prelude::*;
use std::collections::HashMap;

fn main() {
    dioxus_desktop::launch(app);
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct ListBreeds {
    message: HashMap<String, Vec<String>>,
}

fn app() -> Element {
    let breed = use_signal(|| "deerhound".to_string());

    let breeds = use_future(|| async move {
        reqwest::get("https://dog.ceo/api/breeds/list/all")
            .await
            .unwrap()
            .json::<ListBreeds>()
            .await
    });

    match breeds.value()? {
        Ok(breed_list) => rsx! {
            div { height: "500px",
                h1 { "Select a dog breed!" }
                div { display: "flex",
                    ul { flex: "50%",
                        for cur_breed in breed_list.message.keys().take(10) {
                            li { key: "{cur_breed}",
                                button {
                                    onclick: move |_| breed.set(cur_breed.clone()),
                                    "{cur_breed}"
                                }
                            }
                        }
                    }
                    div { flex: "50%", BreedPic { breed } }
                }
            }
        },
        Err(_e) => rsx! { div { "Error fetching breeds" } },
    }
}

#[derive(serde::Deserialize, Debug)]
struct DogApi {
    message: String,
}

#[component]
fn BreedPic(breed: Signal<String>) -> Element {
    let fut = use_future(|breed| async move {
        reqwest::get(format!("https://dog.ceo/api/breed/{breed}/images/random"))
            .await
            .unwrap()
            .json::<DogApi>()
            .await
    });

    match fut.value()? {
        Ok(resp) => render! {
            div {
                button {
                    onclick: move |_| {
                        println!("clicked");
                        fut.restart()
                    },
                    "Click to fetch another doggo"
                }
                img {
                    src: "{resp.message}",
                    max_width: "500px",
                    max_height: "500px",
                }
            }
        },
        Err(_) => render! { div { "loading dogs failed" } },
    }
}
