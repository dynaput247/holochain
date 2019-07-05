#[macro_use]
extern crate log;
use logging::FastLoggerBuilder;

fn main() {
    let toml = r#"
    [logger]
    level = "debug"
    file = "humpty_dumpty.log"

        [[logger.rules]]
        pattern = "Abort"
        color = "red"

        [[logger.rules]]
        pattern = "warned"
        color = "yellow"

        [[logger.rules]]
        pattern = "twice"
        exclude = true
        color = "blue"

    "#;

    FastLoggerBuilder::from_toml(toml)
        .expect("Fail to instantiate the logger from toml.")
        // .redirect_to_file("humpty_dumpty-blop.log")
        .build()
        .expect("Fail to build logger from toml.");

    trace!("Track me if you can.");
    debug!("What's bugging you today?");
    info!("Some interesting info here");
    warn!("You've been warned Sir!");
    // This next one will not be logged according to our rule defined in the toml
    warn!("Let's not warn twice about the same stuff.");
    // And this one will be printed in red
    error!("Abort the mission!!");

    // Let's give some time to the working thread to finish logging...
    std::thread::sleep(std::time::Duration::from_millis(10));
}
