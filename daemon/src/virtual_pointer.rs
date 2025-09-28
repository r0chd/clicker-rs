use crate::Clicker;
use calloop::timer::{TimeoutAction, Timer};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode, RelativeAxisCode};
use rand::prelude::*;
use rand_distr::{Distribution, Normal, Poisson};
use std::time::{Duration, Instant};

pub static POISSON_LAMBDA_FACTOR: f64 = 1.0;

pub struct VirtualPointer {
    virtual_device: VirtualDevice,
    last_window_start: Option<Instant>,
    clicks_in_current_window: u32,
    current_window_target: u32,
    rng: ThreadRng,
}

impl VirtualPointer {
    pub fn try_new() -> anyhow::Result<Self> {
        let mut keys = AttributeSet::new();
        keys.insert(KeyCode::BTN_LEFT);
        keys.insert(KeyCode::BTN_RIGHT);
        keys.insert(KeyCode::BTN_MIDDLE);

        let mut relative_axes = AttributeSet::new();
        relative_axes.insert(RelativeAxisCode::REL_X);
        relative_axes.insert(RelativeAxisCode::REL_Y);

        let virtual_device = VirtualDevice::builder()?
            .name("clicker-rs")
            .with_keys(&keys)?
            .with_relative_axes(&relative_axes)?
            .build()?;

        Ok(Self {
            virtual_device,
            last_window_start: None,
            clicks_in_current_window: 0,
            current_window_target: 0,
            rng: rand::rng(),
        })
    }

    pub fn click(&mut self, button: KeyCode) {
        self.virtual_device
            .emit(&[
                InputEvent::new_now(EventType::KEY.0, button.code(), 1),
                InputEvent::new_now(EventType::KEY.0, button.code(), 0),
            ])
            .unwrap();
    }

    pub fn schedule_clicks(
        &mut self,
        handle: &calloop::LoopHandle<'_, Clicker>,
    ) -> Option<calloop::RegistrationToken> {
        match handle.insert_source(Timer::immediate(), move |_, (), state| {
            let now = Instant::now();

            let should_start_new_window = match state.virtual_pointer.last_window_start {
                None => true,
                Some(start) => now.duration_since(start) >= Duration::from_millis(1000),
            };

            if should_start_new_window {
                if let Some(profile) = state.current_profile.as_ref() {
                    let mut rng = rand::rng();
                    let gaussian =
                        Normal::new(profile.cps.target as f64, profile.cps.std_dev as f64).unwrap();

                    let window_average_cps = loop {
                        let sample = gaussian.sample(&mut rng);
                        if sample > 0.5 {
                            break sample.round() as f32;
                        }
                    };

                    let poisson =
                        Poisson::new(window_average_cps as f64 * POISSON_LAMBDA_FACTOR).unwrap();
                    let clicks_this_window = poisson.sample(&mut rng) as u32;

                    state.virtual_pointer.last_window_start = Some(now);
                    state.virtual_pointer.clicks_in_current_window = 0;
                    state.virtual_pointer.current_window_target = clicks_this_window.max(1);
                }
            }

            match state.current_profile.as_ref() {
                Some(profile) => {
                    state.virtual_pointer.click(profile.repeat_key);
                    state.virtual_pointer.clicks_in_current_window += 1;

                    let remaining_clicks = state
                        .virtual_pointer
                        .current_window_target
                        .saturating_sub(state.virtual_pointer.clicks_in_current_window);

                    if remaining_clicks == 0 {
                        let elapsed = now
                            .duration_since(state.virtual_pointer.last_window_start.unwrap_or(now));
                        let remaining_window_time =
                            Duration::from_millis(1000).saturating_sub(elapsed);
                        TimeoutAction::ToDuration(
                            remaining_window_time.max(Duration::from_millis(10)),
                        )
                    } else {
                        let elapsed = now
                            .duration_since(state.virtual_pointer.last_window_start.unwrap_or(now));
                        let remaining_window_time =
                            Duration::from_millis(1000).saturating_sub(elapsed);
                        let base_interval =
                            remaining_window_time.as_millis() as u64 / remaining_clicks as u64;

                        let mut rng = rand::rng();
                        let jitter_range = (base_interval as f64 * 0.25) as i64;
                        let jitter = rng.random_range(-jitter_range..=jitter_range);
                        let final_interval = (base_interval as i64 + jitter).max(1) as u64;

                        TimeoutAction::ToDuration(Duration::from_millis(final_interval))
                    }
                }
                None => TimeoutAction::Drop,
            }
        }) {
            Ok(handle) => Some(handle),
            Err(e) => {
                log::warn!("{e}");
                None
            }
        }
    }
}
