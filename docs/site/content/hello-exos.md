# Hello, exos

```rust
use exos::{Page, view};

#[exos::get("/")]
async fn home() -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body><h1>"Hello"</h1></body>
        </html>
    })
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, exos::app()).await
}
```

`exos::app()` finds every route in the binary and returns an `axum::Router`, so
exos composes into an axum application rather than replacing one.
