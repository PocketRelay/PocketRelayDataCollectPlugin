/// Constant storing the application version
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The local redirector server port
pub const REDIRECTOR_PORT: u16 = 42127;
/// The local proxy main server port
pub const MAIN_PORT: u16 = 42128;
/// The local proxy telemetry server port
pub const TELEMETRY_PORT: u16 = 42129;
/// The local HTTP server port
pub const HTTP_PORT: u16 = 42131;
