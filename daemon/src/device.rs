use evdev::Device;
use std::{
    fs,
    sync::{Arc, Mutex},
};

pub struct KeyboardDevices(Vec<Arc<Mutex<Device>>>);

impl KeyboardDevices {
    pub fn try_new() -> anyhow::Result<Self> {
        let devices = fs::read_dir("/dev/input")?
            .filter_map(|entry| entry.map(|entry| entry.path()).ok())
            .filter(|path| path.to_string_lossy().contains("event"))
            .filter_map(|path| Device::open(&path).ok())
            .filter(|device| {
                device
                    .supported_keys()
                    .is_some_and(|keys| keys.contains(evdev::KeyCode::KEY_A))
            })
            .map(|device| {
                if let Some(name) = device.name() {
                    log::info!("Keyboard registered {name}");
                }
                Arc::new(Mutex::new(device))
            })
            .collect::<Vec<_>>();

        Ok(Self(devices))
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Arc<Mutex<Device>>> {
        self.0.iter_mut()
    }
}

pub struct MouseDevices(Vec<Arc<Mutex<Device>>>);

impl MouseDevices {
    pub fn try_new() -> anyhow::Result<Self> {
        let devices = fs::read_dir("/dev/input")?
            .filter_map(|entry| entry.map(|entry| entry.path()).ok())
            .filter(|path| path.to_string_lossy().contains("event"))
            .filter_map(|path| Device::open(&path).ok())
            .filter(|device| {
                device.supported_keys().is_some_and(|keys| {
                    keys.contains(evdev::KeyCode::BTN_LEFT)
                        || keys.contains(evdev::KeyCode::BTN_RIGHT)
                        || keys.contains(evdev::KeyCode::BTN_MIDDLE)
                })
            })
            .map(|device| {
                if let Some(name) = device.name() {
                    log::info!("Mouse registered {name}");
                }
                Arc::new(Mutex::new(device))
            })
            .collect::<Vec<_>>();

        Ok(Self(devices))
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Arc<Mutex<Device>>> {
        self.0.iter_mut()
    }
}
