use crate::{START, WlClicker};
use calloop::timer::{TimeoutAction, Timer};
use common::Cps;
use rand::prelude::*;
use std::time::Duration;
use wayland_client::{globals::GlobalList, protocol::wl_pointer};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};

pub struct VirtualPointer(zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1);

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

        Self(virtual_pointer)
    }

    pub fn click(&self) {
        self.0.button(
            START.elapsed().as_millis() as u32,
            0x110,
            wl_pointer::ButtonState::Pressed,
        );
        self.0.frame();
        self.0.button(
            START.elapsed().as_millis() as u32,
            0x110,
            wl_pointer::ButtonState::Released,
        );
        self.0.frame();
    }

    pub fn schedule_clicks(
        &self,
        cps: Cps,
        handle: &calloop::LoopHandle<'_, WlClicker>,
    ) -> Option<calloop::RegistrationToken> {
        let mut rng = rand::rng();
        let cps_candidates = (cps.min..cps.max).collect::<Vec<_>>();
        let random = cps_candidates.choose(&mut rng);

        let delay = Duration::from_millis(1000 / random.unwrap_or(&cps.min));
        let timer = Timer::from_duration(delay);

        match handle.insert_source(timer, move |_, (), state| {
            state.virtual_pointer.click();

            match state.current_profile.as_ref() {
                Some(profile) => {
                    TimeoutAction::ToDuration(Duration::from_millis(1000 / profile.cps.min))
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
