use directories::UserDirs;
use log::LevelFilter;
use log4rs::{
    append::{console::ConsoleAppender, file::FileAppender},
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    init_config, Config,
};

/// The pattern to use when logging
const LOGGING_PATTERN: &str = "[{d} {h({l})} {M}] {m}{n}";

/// Setup function for setting up the Log4rs logging configuring it
/// for all the different modules and and setting up file and stdout logging
pub fn setup() {
    let user_dirs = UserDirs::new().expect("failed to get user dir");
    let doc_dir = user_dirs
        .document_dir()
        .expect("Failed to get document dir")
        .join("pocket-relay-dump.log");

    std::env::set_var("RUST_LOG", "debug");
    // Create logging appenders
    let pattern = Box::new(PatternEncoder::new(LOGGING_PATTERN));
    let console = Box::new(ConsoleAppender::builder().encoder(pattern.clone()).build());
    let file = Box::new(
        FileAppender::builder()
            .encoder(pattern)
            .build(doc_dir)
            .expect("Unable to create logging file appender"),
    );

    const APPENDERS: [&str; 2] = ["stdout", "file"];

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", console))
        .appender(Appender::builder().build("file", file))
        .build(
            Root::builder()
                .appenders(APPENDERS)
                .build(LevelFilter::Debug),
        )
        .expect("Failed to create logging config");

    init_config(config).expect("Unable to initialize logger");

    // Include panics in logging
    log_panics::init();
}
