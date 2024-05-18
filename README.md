# Pocket Relay - Data Collection Plugin

This plugin is a tool for collecting data while playing on the official servers in order to create
parity or debug issues with Pocket Relay. If you are looking to play on a Pocket Relay server check
out the guide here: https://pocket-relay.pages.dev/docs/client/joining

> [!IMPORTANT]
> When using this plugin ensure that you don't have any existing Pocket Relay clients running and don't have
> any other Pocket Relay plugins (As they will conflict with each other)

This plugin hooks into the game networking routing the traffic through this plugin decoding and logging
the packets the game sends to a local file in your Documents folder named "pocket-relay-dump.log". These logs will usually be requested from you by me (The Pocket Relay developer, when trying to fix bugs that only happen on one server but not the other)

> [!WARNING]
> Do **NOT** share your log file with anyone you don't trust. Your log file contains personal information such as your email address and an access token. Its recommended that you don't share this file anywhere publicly

## Installing

To install this plugin you can follow the plugin client guide as the steps are similar. But instead of using the client plugin .asi file you can use the release provided on this repository https://pocket-relay.pages.dev/docs/client/plugin-client. You can download the latest release [Here](https://github.com/PocketRelay/PocketRelayDataCollectPlugin/releases/latest/download/pocket-relay-dump.asi)

You can ignore step "4) Connecting" as the data collection plugin doesn't have a UI since its directly connecting you to the official servers as a middle-man the relevant steps for using this are steps 1 and 3.


## Compiling

To compile this from source you will need Rust & Cargo with the `i686-pc-windows-msvc` target triple installed (ME3 is a 32bit executable so we must all use a 32bit build target) if you have rustup installed you can install this with `rustup target add i686-pc-windows-msvc`

To build run the following command:

```shell
cargo build --release
```

This will build a release version to `target/i686-pc-windows-msvc/release/pocket_relay_dump.dll` you will want to rename this file changing the .dll extension to .asi then you can use it as mentioned above in the installation guide

## 🌐 EA / BioWare Notice

The Pocket Relay software, in all its forms, is not supported, endorsed, or provided by BioWare or Electronic Arts. Mass Effect is a registered trademark of Bioware/EA International (Studio and Publishing), Ltd in the U.S. and/or other countries. 
