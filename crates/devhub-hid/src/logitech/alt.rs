//! A second HID handle receives unsolicited physical button events independently
//! of command replies. All mice share one left-Alt down/up reference count.
use super::{io, HubError, HubResult};
use hidapi::HidDevice;
use std::sync::{
    atomic::{AtomicBool, AtomicU16, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

static HOLDERS: Mutex<usize> = Mutex::new(0);

fn transition(held: &mut bool, wanted: bool) {
    if *held == wanted {
        return;
    }
    let mut holders = HOLDERS.lock().unwrap_or_else(|p| p.into_inner());
    if (wanted && *holders == 0) || (!wanted && *holders == 1) {
        let mut input = INPUT::default();
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki = KEYBDINPUT {
            wScan: 0x38,
            dwFlags: KEYEVENTF_SCANCODE | if wanted { 0 } else { KEYEVENTF_KEYUP },
            ..Default::default()
        };
        if unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) } != 1 && wanted {
            return;
        }
    }
    if wanted {
        *holders += 1
    } else {
        *holders -= 1
    }
    *held = wanted;
}

pub struct Listener {
    slots: Arc<AtomicU16>,
    alive: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Listener {
    pub fn open(dev: &HidDevice, index: u8, feature: u8) -> HubResult<Self> {
        let path = dev.get_device_info().map_err(io)?;
        let reader = hidapi::HidApi::new()
            .map_err(io)?
            .open_path(path.path())
            .map_err(io)?;
        let slots = Arc::new(AtomicU16::new(0));
        let alive = Arc::new(AtomicBool::new(true));
        let mask = slots.clone();
        let running = alive.clone();
        let worker = thread::Builder::new()
            .name("logicool-button-events".into())
            .spawn(move || {
                let mut packet = [0; 64];
                let mut pressed = 0u16;
                let mut held = false;
                while running.load(Ordering::Acquire) {
                    match reader.read_timeout(&mut packet, 35) {
                        Ok(n)
                            if n >= 6
                                && matches!(packet[0], 0x10 | 0x11)
                                && packet[1..4] == [index, feature, 0] =>
                        {
                            pressed = u16::from_be_bytes([packet[4], packet[5]]);
                        }
                        Ok(n) if n >= 4 && packet[1] == index && packet[2] == 0x40 => pressed = 0,
                        Err(_) => break,
                        _ => {}
                    }
                    let active = mask.load(Ordering::Acquire);
                    if active == 0 {
                        pressed = 0
                    }
                    transition(&mut held, pressed & active != 0);
                }
                transition(&mut held, false);
                running.store(false, Ordering::Release);
            })
            .map_err(|e| HubError::Io(e.to_string()))?;
        Ok(Self {
            slots,
            alive,
            worker: Some(worker),
        })
    }
    pub fn slots(&self) -> u16 {
        self.slots.load(Ordering::Acquire)
    }
    pub fn set(&self, mask: u16) {
        self.slots.store(mask, Ordering::Release);
    }
    pub fn live(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
