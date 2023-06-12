# Ruuvitag Prometheus Exporter

So I wanted my ruuvitags in Grafana for monitoring, and since I did not find a
proper version of it, I thought I'd build one myself.

This is my ruuvitag exporter, it uses:

- [blteplug](https://github.com/deviceplug/btleplug) for the BLE stack. 
  This means it needs a C compiler and headers to build. I might fix that.

- [ruuvi-sensor-protocol-rs](https://github.com/lautat/ruuvi-sensor-protocol-rs) To decode the ruuvitag data

- [rust-prometheus](https://github.com/tikv/rust-prometheus) for the prometheus parts.

- [hyper](https://hyper.rs/) for the simple http1.1 server

- [tokio](https://tokio.rs/) For the async runtime

- [tracing](https://tracing.rs/tracing/)  for the logging and tracing
  (Because I wanted to experiment)



It's made to run in a container, so it always listens on 0.0.0.0:9185 I'm open
to making that a command line argument, but I didn't want to bother


License: Since it's a network service, I licensed it under Affero GPLv3. In
short, it means that if you fork it, modify it, and use it, you still need to
share your changes with the users. Not a very difficult requirement.


# cross compiling for armv7

I documented this in the past [over at work](https://www.modio.se/cross-compiling-rust-binaries-to-armv7.html)

The steps there are still relevant, I had to look them up to cross compile this.

# Cross compiling for aarch64

The process is the same as armv7, but the packages and targets are different:


Use podman to run the official rust container:

    podman run --rm -ti -v $(pwd):/code:rw,z --workdir /code docker.io/library/rust:latest

Inside, add the target, and necessary debian packages to cross compile

    rustup target add aarch64-unknown-linux-gnu
    dpkg --add-architecture arm64
    apt update
    apt install gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu  libdbus-1-dev:arm64 libc6-dev-arm64-cross

Export the environment variables to make it useful:

    export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig/ 
    export PKG_CONFIG_ALLOW_CROSS="true"


Then build:

    cargo build --target aarch64-unknown-linux-gnu --release

# For a modio logger rather than prometheus exporter

The Modio logger works over DBus and acts as a cache/forwarder to online
services, like MQTT and others.

To build for that as a target, use:

    RUST_LOG=info cargo run --features modio --no-default-features -- --session

Note that a modio-logger needs to listen to the session bus (or system bus,
default) for it to function. 
