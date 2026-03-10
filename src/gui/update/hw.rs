use super::*;

impl Maolan {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(super) fn apply_hw_selected(&self, hw: &AudioDeviceOption) {
        let mut state = self.state.blocking_write();
        let selected = Self::selected_output_device_for_platform(&mut state, hw);
        state.selected_hw = Some(selected);
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    pub(super) fn apply_hw_selected(&self, hw: &String) {
        self.state.blocking_write().selected_hw = Some(hw.to_string());
    }

    #[cfg(target_os = "windows")]
    pub(super) fn apply_hw_input_selected(&self, hw: &String) {
        self.state.blocking_write().selected_input_hw = Some(hw.to_string());
    }

    #[cfg(target_os = "freebsd")]
    pub(super) fn apply_hw_input_selected(&self, hw: &AudioDeviceOption) {
        let mut state = self.state.blocking_write();
        let selected = Self::select_refreshed_device(
            &mut state.available_hw,
            hw,
            crate::state::discover_freebsd_audio_devices,
        );
        state.selected_input_hw = Some(selected.clone());
        Self::update_bits_from_selected_device(&mut state, &selected);
    }

    #[cfg(target_os = "linux")]
    pub(super) fn apply_hw_input_selected(&self, hw: &AudioDeviceOption) {
        let mut state = self.state.blocking_write();
        let selected = Self::select_refreshed_device(
            &mut state.available_input_hw,
            hw,
            crate::state::discover_alsa_input_devices,
        );
        state.selected_input_hw = Some(selected);
        Self::update_bits_from_selected_device(&mut state, hw);
    }

    pub(super) fn apply_hw_backend_selected(&self, backend: &crate::state::AudioBackendOption) {
        let mut state = self.state.blocking_write();
        state.selected_backend = backend.clone();
        state.selected_hw = None;
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            state.selected_input_hw = None;
            state.oss_bits = 32;
            Self::apply_backend_device_defaults(&mut state, backend);
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(super) fn select_refreshed_device(
        available_devices: &mut Vec<AudioDeviceOption>,
        current: &AudioDeviceOption,
        discover: fn() -> Vec<AudioDeviceOption>,
    ) -> AudioDeviceOption {
        let refreshed = discover();
        let selected = refreshed
            .iter()
            .find(|candidate| candidate.id == current.id)
            .cloned()
            .unwrap_or_else(|| current.clone());
        if !refreshed.is_empty() {
            *available_devices = refreshed;
        }
        selected
    }

    #[cfg(target_os = "freebsd")]
    pub(super) fn selected_output_device_for_platform(
        state: &mut crate::state::StateData,
        hw: &AudioDeviceOption,
    ) -> AudioDeviceOption {
        let selected = Self::select_refreshed_device(
            &mut state.available_hw,
            hw,
            crate::state::discover_freebsd_audio_devices,
        );
        Self::update_bits_from_selected_device(state, &selected);
        selected
    }

    #[cfg(target_os = "linux")]
    pub(super) fn selected_output_device_for_platform(
        state: &mut crate::state::StateData,
        hw: &AudioDeviceOption,
    ) -> AudioDeviceOption {
        Self::update_bits_from_selected_device(state, hw);
        hw.clone()
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(super) fn update_bits_from_selected_device(
        state: &mut crate::state::StateData,
        selected: &AudioDeviceOption,
    ) {
        if let Some(bits) = selected.preferred_bits() {
            state.oss_bits = bits;
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub(super) fn select_first_backend_output_device(
        state: &mut crate::state::StateData,
        discover: fn() -> Vec<AudioDeviceOption>,
    ) -> Option<AudioDeviceOption> {
        let refreshed = discover();
        let selected = refreshed.first().cloned();
        if !refreshed.is_empty() {
            state.available_hw = refreshed;
        }
        if let Some(selected_ref) = selected.as_ref() {
            Self::update_bits_from_selected_device(state, selected_ref);
        }
        selected
    }

    #[cfg(target_os = "linux")]
    pub(super) fn select_first_backend_input_device(
        state: &mut crate::state::StateData,
        discover: fn() -> Vec<AudioDeviceOption>,
    ) -> Option<AudioDeviceOption> {
        let refreshed = discover();
        let selected = refreshed.first().cloned();
        if !refreshed.is_empty() {
            state.available_input_hw = refreshed;
        }
        if let Some(selected_ref) = selected.as_ref() {
            Self::update_bits_from_selected_device(state, selected_ref);
        }
        selected
    }

    #[cfg(target_os = "freebsd")]
    pub(super) fn apply_backend_device_defaults(
        state: &mut crate::state::StateData,
        backend: &crate::state::AudioBackendOption,
    ) {
        if !matches!(backend, crate::state::AudioBackendOption::Oss) {
            return;
        }
        state.selected_hw = Self::select_first_backend_output_device(
            state,
            crate::state::discover_freebsd_audio_devices,
        );
        state.selected_input_hw = state.selected_hw.clone();
    }

    #[cfg(target_os = "linux")]
    pub(super) fn apply_backend_device_defaults(
        state: &mut crate::state::StateData,
        backend: &crate::state::AudioBackendOption,
    ) {
        if !matches!(backend, crate::state::AudioBackendOption::Alsa) {
            return;
        }
        state.selected_hw = Self::select_first_backend_output_device(
            state,
            crate::state::discover_alsa_output_devices,
        );
        state.selected_input_hw = Self::select_first_backend_input_device(
            state,
            crate::state::discover_alsa_input_devices,
        );
    }
}
