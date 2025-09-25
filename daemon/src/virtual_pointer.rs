use crate::{START, WlClicker};
use calloop::timer::{TimeoutAction, Timer};
use rand::prelude::*;
use rand_distr::{Distribution, Normal, Poisson};
use std::time::{Duration, Instant};
use wayland_client::{globals::GlobalList, protocol::wl_pointer};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

pub struct VirtualPointer {
    pointer: zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
    last_window_start: Option<Instant>,
    clicks_in_current_window: u32,
    current_window_target: u32,
}

impl VirtualPointer {
    pub fn new(globals: &GlobalList, qh: &wayland_client::QueueHandle<WlClicker>) -> Self {
        let virtual_pointer_manager = globals
            .bind::<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, _, _>(
                &qh,
                1..=2,
                (),
            )
            .expect("Compositor doesn't support zwlr_virtual_pointer_v1");
        let virtual_pointer = virtual_pointer_manager.create_virtual_pointer(None, &qh, ());
        Self {
            pointer: virtual_pointer,
            last_window_start: None,
            clicks_in_current_window: 0,
            current_window_target: 0,
        }
    }

    pub fn jitter(&self, jitter: f32) {
        let mut rng = rand::rng();
        let normal_x = Normal::new(0.0, jitter as f32 / 3.0).unwrap();
        let normal_y = Normal::new(0.0, jitter as f32 / 3.0).unwrap();
        let jitter_x = loop {
            let sample = normal_x.sample(&mut rng).round();
            if sample >= -(jitter) && sample <= jitter {
                break sample;
            }
        };
        let jitter_y = loop {
            let sample = normal_y.sample(&mut rng).round();
            if sample >= -(jitter) && sample <= jitter {
                break sample;
            }
        };
        self.pointer.motion(
            START.elapsed().as_millis() as u32,
            jitter_x as f64,
            jitter_y as f64,
        );
        self.pointer.frame();
    }

    pub fn click(&self) {
        self.pointer.button(
            START.elapsed().as_millis() as u32,
            0x110,
            wl_pointer::ButtonState::Pressed,
        );
        self.pointer.frame();
        self.pointer.button(
            START.elapsed().as_millis() as u32,
            0x110,
            wl_pointer::ButtonState::Released,
        );
        self.pointer.frame();
    }

    pub fn schedule_clicks(
        &mut self,
        handle: &calloop::LoopHandle<'_, WlClicker>,
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

                    let poisson = Poisson::new(window_average_cps as f64).unwrap();
                    let clicks_this_window = poisson.sample(&mut rng) as u32;

                    state.virtual_pointer.last_window_start = Some(now);
                    state.virtual_pointer.clicks_in_current_window = 0;
                    state.virtual_pointer.current_window_target = clicks_this_window.max(1);
                }
            }

            state.virtual_pointer.click();
            state.virtual_pointer.clicks_in_current_window += 1;

            match state.current_profile.as_ref() {
                Some(profile) => {
                    state.virtual_pointer.jitter(profile.jitter);

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
