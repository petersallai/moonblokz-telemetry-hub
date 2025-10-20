use spin_sdk::{
    http::{IntoResponse, Response},
    http_component,
    variables
};


#[http_component]
fn cloud_start(req: http::Request<()>) -> anyhow::Result<impl IntoResponse> {
    println!("{:?}", req.headers());

    let api_key = variables::get("api_key")?;

    let body = format!("
    <html>
    <body>
        <h1>Welcome to Moonblokz Telemetry Hub</h1>
        <p>Your API key is: {}</p>
    </body>
    </html>", api_key);

    Ok(Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(body)
        .build())
}
