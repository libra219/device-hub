//! HID++ 2.0 host-mode button maps. Foreground changes only touch volatile state.
use devhub_core::{backend::*, model::*};
use hidapi::HidDevice;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

fn io(err: hidapi::HidError) -> HubError {
    HubError::Io(err.to_string())
}

fn request(
    dev: &HidDevice,
    index: u8,
    feature: u8,
    function: u8,
    params: &[u8],
) -> HubResult<[u8; 16]> {
    let mut packet = [0; 20];
    packet[..4].copy_from_slice(&[0x11, index, feature, function << 4 | 14]);
    packet[4..4 + params.len()].copy_from_slice(params);
    dev.write(&packet).map_err(io)?;
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut reply = [0; 64];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let n = dev
            .read_timeout(&mut reply, left.as_millis() as i32 + 1)
            .map_err(io)?;
        if n >= 20 && reply[..4] == packet[..4] {
            return Ok(reply[4..20].try_into().unwrap());
        }
        if n >= 7
            && reply[1] == index
            && matches!(reply[2], 0xff | 0x8f)
            && reply[3..5] == packet[2..4]
        {
            return Err(HubError::Protocol(format!("HID++ error {:02x}", reply[5])));
        }
    }
    Err(HubError::Disconnected)
}

fn feature(dev: &HidDevice, index: u8, id: u16) -> HubResult<u8> {
    let result = request(dev, index, 0, 0, &id.to_be_bytes())?[0];
    if result == 0 {
        return Err(HubError::Unsupported("HID++ feature"));
    }
    Ok(result)
}

fn identify(dev: &HidDevice, index: u8) -> HubResult<String> {
    let f = feature(dev, index, 5)?;
    let len = request(dev, index, f, 0, &[])?[0] as usize;
    let mut name = Vec::new();
    while name.len() < len {
        name.extend(request(dev, index, f, 1, &[name.len() as u8])?);
    }
    let name = String::from_utf8_lossy(&name[..len]).into_owned();
    let lower = name.to_ascii_lowercase();
    if !(lower.contains("g309")
        || lower == "g pro wireless gaming mouse"
        || lower == "pro wireless gaming mouse")
    {
        return Err(HubError::Unsupported("対象外のLogicoolマウス"));
    }
    Ok(name)
}

pub struct Driver {
    dev: Mutex<HidDevice>,
    info: DeviceInfo,
    index: u8,
    map: u8,
    onboard: u8,
    count: usize,
}

pub fn open(dev: HidDevice, mut info: DeviceInfo) -> HubResult<Driver> {
    let indices: &[u8] = if info.transport == Transport::Wired {
        &[255]
    } else {
        &[1, 2, 3, 4, 5, 6]
    };
    for &index in indices {
        let Ok(name) = identify(&dev, index) else {
            continue;
        };
        let map = feature(&dev, index, 0x8110)?;
        let onboard = feature(&dev, index, 0x8100)?;
        let count = request(&dev, index, map, 0, &[])?[0] as usize;
        if !(1..=16).contains(&count) {
            return Err(HubError::Unsupported("ボタン数"));
        }
        info.name = name;
        info.model = if info.name.to_ascii_lowercase().contains("g309") {
            "g309"
        } else {
            "pro-wireless"
        }
        .into();
        return Ok(Driver {
            dev: Mutex::new(dev),
            info,
            index,
            map,
            onboard,
            count,
        });
    }
    Err(HubError::Disconnected)
}

const FUNCTIONS: [MouseFn; 5] = [
    MouseFn::Left,
    MouseFn::Right,
    MouseFn::Middle,
    MouseFn::Back,
    MouseFn::Forward,
];
fn decode(value: u8) -> ButtonAction {
    match value {
        0 => ButtonAction::Disabled,
        1..=5 => ButtonAction::Mouse(FUNCTIONS[value as usize - 1]),
        ty => ButtonAction::Unsupported { ty, params: vec![] },
    }
}
impl DeviceDriver for Driver {
    fn info(&self) -> DeviceInfo {
        self.info.clone()
    }
    fn caps(&self) -> Capabilities {
        let mut caps = Capabilities::empty();
        caps.mouse = Some(MouseCaps {
            volatile_buttons: true,
            dpi_min: 0,
            dpi_max: 0,
            dpi_step: 1,
            min_stages: 0,
            max_stages: 0,
            xy_split: false,
            stage_colors: false,
            stage_toggle: false,
            buttons: (1..=self.count)
                .map(|n| ButtonSlot {
                    name: format!("G{n}"),
                })
                .collect(),
            button_kinds: vec![ButtonActionKind::Mouse, ButtonActionKind::Disabled],
            require_left_click: true,
            macros: None,
            live_buttons: vec![],
            adaptive_dpi: None,
        });
        caps
    }
    fn read(&self) -> HubResult<DeviceState> {
        let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
        let map = request(&dev, self.index, self.map, 3, &[])?;
        Ok(DeviceState {
            profile: Default::default(),
            settings: Default::default(),
            keyboard: None,
            lighting: Default::default(),
            peq: None,
            mouse: Some(MouseState {
                stages: vec![],
                active_stage: 0,
                buttons: map[..self.count].iter().map(|v| decode(*v)).collect(),
                macros: vec![],
                adaptive: None,
            }),
        })
    }
    fn apply(&self, change: &Change) -> HubResult<()> {
        let Change::Button { slot, action } = change else {
            return Err(HubError::Unsupported("ボタン割当のみ"));
        };
        if *slot >= self.count {
            return Err(HubError::Invalid("ボタン番号".into()));
        }
        let value = match action {
            ButtonAction::Disabled => 0,
            ButtonAction::Mouse(f) => {
                FUNCTIONS
                    .iter()
                    .position(|v| v == f)
                    .ok_or(HubError::Unsupported("マウス操作"))? as u8
                    + 1
            }
            ButtonAction::Unsupported { ty, params } if params.is_empty() => *ty,
            _ => return Err(HubError::Unsupported("ボタン割当")),
        };
        let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
        if self.index != 255 && identify(&dev, self.index)? != self.info.name {
            return Err(HubError::Disconnected);
        }
        let mut map = request(&dev, self.index, self.map, 3, &[])?;
        map[*slot] = value;
        if !map[..self.count].contains(&1) {
            return Err(HubError::Invalid(
                "左クリックを1個以上残してください".into(),
            ));
        }
        if request(&dev, self.index, self.onboard, 2, &[])?[0] != 2 {
            request(&dev, self.index, self.onboard, 1, &[2])?;
        }
        request(&dev, self.index, self.map, 4, &map)?;
        if request(&dev, self.index, self.map, 3, &[])?[..self.count] != map[..self.count] {
            return Err(HubError::Protocol("ボタン割当の読み戻し不一致".into()));
        }
        Ok(())
    }
}
