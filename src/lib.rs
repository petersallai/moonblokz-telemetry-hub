use spin_sdk::{
    http::{IntoResponse, Response},
    http_component,
};


#[http_component]
fn cloud_start(req: http::Request<()>) -> anyhow::Result<impl IntoResponse> {
    println!("{:?}", req.headers());

    let body = "
    <html>
    <body>
        <h1>Welcome to Moonblokz Telemetry Hub</h1>
    </body>
    </html>";

    Ok(Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(body)
        .build())
}
