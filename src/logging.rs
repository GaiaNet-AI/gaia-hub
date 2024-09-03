use log::LevelFilter;
use log4rs;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;

pub fn configure_logging() {
    let log_path = std::env::var("LOG_FILE").expect("No LOG_FILE in env");

    let pattern =
        r#"\{"level": "{l}", "time": "{d}", "message": "{m}",  "file": "{f}", "line":{L}\}"#;
    let pattern_with_newline = format!("{}\n", pattern);

    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(&pattern_with_newline)))
        .build(log_path)
        .unwrap();

    let console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "[{d(%Y-%m-%d %H:%M:%S)}][{l}] {m}\n",
        )))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .appender(Appender::builder().build("console", Box::new(console)))
        .build(
            Root::builder()
                .appender("logfile")
                .appender("console")
                .build(LevelFilter::Info),
        )
        .unwrap();

    log4rs::init_config(config).unwrap();
}
