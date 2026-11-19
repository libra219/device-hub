//! Read the same capabilities and button map as the desktop app.
use devhub_core::{DriverKind, HubResult};

fn main() -> HubResult<()> {
    let hub = devhub_hid::connect()?;
    let mut online = 0;
    for info in hub.scan()? {
        if info.driver != DriverKind::Logitech {
            continue;
        }
        match hub.driver(&info.id) {
            Ok(driver) => {
                let state = driver.read()?;
                println!(
                    "{} {:?}",
                    driver.info().name,
                    state.mouse.map(|m| m.buttons)
                );
                online += 1;
            }
            Err(err) => eprintln!("{}: {err}", info.name),
        }
    }
    if online == 0 {
        return Err(devhub_core::HubError::Disconnected);
    }
    Ok(())
}
