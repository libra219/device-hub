//! Read the same capabilities and button map as the desktop app.
use devhub_core::{ButtonAction, Change, DeviceDriver, DriverKind, HubError, HubResult, MouseFn};

fn verify_side_button(driver: &dyn DeviceDriver, original: &[ButtonAction]) -> HubResult<()> {
    let slot = 3;
    let before = original
        .get(slot)
        .ok_or(HubError::Unsupported("サイドボタン"))?;
    let test = if *before == ButtonAction::Mouse(MouseFn::Forward) {
        ButtonAction::Mouse(MouseFn::Back)
    } else {
        ButtonAction::Mouse(MouseFn::Forward)
    };
    let edit = |action| driver.apply(&Change::Button { slot, action });
    let check = |expected: &[ButtonAction]| {
        if driver.read()?.mouse.is_some_and(|m| m.buttons == expected) {
            Ok(())
        } else {
            Err(HubError::Protocol("割当の読み戻し不一致".into()))
        }
    };
    let mut expected = original.to_vec();
    expected[slot] = test.clone();
    let verification = edit(test).and_then(|_| check(&expected));
    edit(before.clone())?;
    check(original)?;
    verification
}

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
                let mapping = state
                    .mouse
                    .ok_or(HubError::Unsupported("ボタンマップ"))?
                    .buttons;
                println!("{} {:?}", driver.info().name, mapping);
                if std::env::args().any(|arg| arg == "--verify-side-button") {
                    verify_side_button(driver.as_ref(), &mapping)?;
                    println!("Side-button change and restoration verified.");
                }
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
