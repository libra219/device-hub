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
                println!("{} [{}] {:?}", driver.info().name, info.id, mapping);
                #[cfg(windows)]
                if std::env::args()
                    .skip(1)
                    .any(|arg| arg == format!("--verify-alt-{:04x}", info.pid))
                {
                    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                        GetAsyncKeyState, VK_LMENU,
                    };
                    let slot = 3;
                    let set = |action| driver.apply(&Change::Button { slot, action });
                    let original = mapping
                        .get(slot)
                        .ok_or(HubError::Unsupported("サイドボタン"))?
                        .clone();
                    let result = set(ButtonAction::Key(devhub_core::KeyStroke {
                        mods: 4,
                        usage: 0,
                    }))
                    .and_then(|_| {
                        println!("READY: hold and release G4 on {}.", driver.info().name);
                        let deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(100);
                        let mut down = false;
                        while std::time::Instant::now() < deadline {
                            let pressed = unsafe { GetAsyncKeyState(i32::from(VK_LMENU)) } < 0;
                            if down && !pressed {
                                println!("Verified Windows left Alt down/up.");
                                return Ok(());
                            }
                            down |= pressed;
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(HubError::Protocol("Alt down/up timeout".into()))
                    });
                    set(original)?;
                    result?;
                }
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
