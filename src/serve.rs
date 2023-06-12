use std::net::SocketAddr;

use hyper::http::Error;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server};
use lazy_static::lazy_static;
use prometheus::{register_histogram, register_int_counter, register_int_gauge};
use prometheus::{Histogram, IntCounter, IntGauge};
use tracing::{debug, error, info};

lazy_static! {
    static ref HTTP_COUNTER: IntCounter = register_int_counter!(
        "ruuvi_exporter_requests_total",
        "Number of HTTP requests received."
    )
    .expect("Could not create HTTP_COUNTER metric. This should never fail.");
    static ref HTTP_BODY_GAUGE: IntGauge = register_int_gauge!(
        "ruuvi_exporter_response_size_bytes",
        "The HTTP response sizes in bytes."
    )
    .expect("Could not create HTTP_BODY_GAUGE metric. This should never fail.");
    static ref HTTP_REQ_HISTOGRAM: Histogram = register_histogram!(
        "ruuvi_exporter_request_duration_seconds",
        "The HTTP request latencies in seconds."
    )
    .expect("Could not create HTTP_REQ_HISTOGRAM metric. This should never fail.");
}

#[tracing::instrument]
fn prometheus_to_response() -> Result<Response<Body>, Error> {
    let metric_families = prometheus::gather();
    debug!(
        message = "Generating text output for metrics",
        count = metric_families.len()
    );
    let encoder = prometheus::TextEncoder::new();
    let resp = match encoder.encode_to_string(&metric_families) {
        Ok(body_text) => {
            HTTP_BODY_GAUGE.set(body_text.len() as i64);
            let body = Body::from(body_text);
            Response::new(body)
        }
        Err(error) => {
            error!(message = "Error serializing prometheus data", err = %error);
            let body = Body::from("Error serializing to prometheus");
            Response::builder().status(500).body(body)?
        }
    };
    Ok(resp)
}

fn redirect() -> Result<Response<Body>, Error> {
    debug!(
        message = "Redirecting user",
        location = "/metrics",
        status = 301
    );
    let body = Body::from("Go to /metrics \n");
    let resp = Response::builder()
        .status(301)
        .header("Location", "/metrics")
        .body(body)?;
    Ok(resp)
}

fn four_oh_four() -> Result<Response<Body>, Error> {
    debug!(message = "Redirecting user", status = 404);
    let resp = Response::builder().status(404).body(Body::empty())?;
    Ok(resp)
}

async fn router(req: Request<Body>) -> Result<Response<Body>, Error> {
    HTTP_COUNTER.inc();
    let timer = HTTP_REQ_HISTOGRAM.start_timer();
    let resp = match (req.method(), req.uri().path()) {
        (&Method::GET, "/metrics") => prometheus_to_response()?,
        (&Method::GET, "/") => redirect()?,
        _ => four_oh_four()?,
    };
    drop(timer);
    // This yield is really only to make clippy pedantic mode happy,
    // as this function had no await statements in it.
    tokio::task::yield_now().await;
    Ok(resp)
}

#[tracing::instrument]
pub(crate) async fn webserver(addr: SocketAddr) -> anyhow::Result<()> {
    use anyhow::Context;

    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc = make_service_fn(|_conn| {
        // This is the `Service` that will handle the connection.
        // `service_fn` is a helper to convert a function that
        // returns a Response into a `Service`.
        async { Ok::<_, hyper::Error>(service_fn(router)) }
    });

    let server = Server::try_bind(&addr).with_context(|| format!("Failed to bind port {addr}"))?;
    info!(message = "Listening on ", %addr);

    server.serve(make_svc).await?;
    info!("Happy webserver ending");
    Ok(())
}
