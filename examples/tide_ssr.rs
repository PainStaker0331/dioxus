//! This demo shows how to use the to_string utility on VNodes to convert them into valid HTML.
//! You can use the html! macro to craft webpages generated by the server on-the-fly.
//!
//! Server-side-renderered webpages are a great use of Rust's async story, where servers can handle
//! thousands of simultaneous clients on minimal hardware.

use dioxus::prelude::*;
use tide::{Request, Response};

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    dioxus_examples::logger::set_up_logging("tide_ssr");

    // Build the API
    let mut app = tide::new();
    app.at("/fib/:n").get(fibsum);

    // Launch the server
    let addr = "127.0.0.1:8080";
    log::info!("App is ready at {}", addr);
    log::info!("Navigate to an fibonacci number, like so http://localhost:8080/fib/21");
    app.listen(addr).await?;

    Ok(())
}

/// Calculate the fibonacci number for a given request input
async fn fibsum(req: Request<()>) -> tide::Result<tide::Response> {
    use std::time::Instant;
    let n: usize = req.param("n")?.parse().unwrap_or(0);

    fn fib(n: usize) -> usize {
        if n == 0 || n == 1 {
            n
        } else {
            fib(n - 1) + fib(n - 2)
        }
    }

    // Start a stopwatch
    // Compute the nth number in the fibonacci sequence
    // Stop the stopwatch
    let start = Instant::now();
    let fib_n = fib(n);
    let duration = start.elapsed().as_nanos();

    let mut rng = rand::thread_rng();
    use rand::Rng;
    let other_fib_to_try = rng.gen_range(1..42);

    let nodes = html! {
        <html>

        <head>
            <meta content="text/html;charset=utf-8" />
            <meta charset="UTF-8" />
            <link href="https://unpkg.com/tailwindcss@^2/dist/tailwind.min.css" rel="stylesheet" />
        </head>

        <body>
            <div class="flex items-center justify-center flex-col">
                <div class="flex items-center justify-center">
                    <div class="flex flex-col bg-white rounded p-4 w-full max-w-xs">
                        // Title
                        <div class="font-bold text-xl">
                            {format!("Fibonacci Calculator: n = {}",n)}
                        </div>

                        // Subtext / description
                        <div class="text-sm text-gray-500">
                            {format!("Calculated in {} nanoseconds",duration)}
                        </div>

                        <div class="flex flex-row items-center justify-center mt-6">
                            // Main number
                            <div class="font-medium text-6xl">
                                {format!("{}",fib_n)}
                            </div>
                        </div>

                        // Try another
                        <div class="flex flex-row justify-between mt-6">
                            <a href=format!("http://localhost:8080/fib/{}", other_fib_to_try) class="underline">
                                {"Click to try another number"}
                            </a>
                        </div>
                    </div>
                </div>
            </div>
        </body>

        </html>
    };

    Ok(Response::builder(203)
        .body(nodes.to_string())
        .header("custom-header", "value")
        .content_type(tide::http::mime::HTML)
        .build())
}
