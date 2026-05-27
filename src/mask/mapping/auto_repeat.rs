use bevy::ecs::system::{Query, Res};
use bevy_enhanced_input::prelude::ActionEvents;
use serde::{Deserialize, Serialize};

use crate::{
    mask::mapping::{
        binding::{ButtonBinding, ValidateMappingConfig},
        config::ActiveMappingConfig,
        input_actions::ActionEntityMap,
        utils::Position,
    },
    utils::ChannelSenderCS,
};

#[derive(Debug, Clone)]
pub struct BindMappingAutoRepeat {
    pub position: Position,
    pub note: String,
    pub target_key: String,
    pub interval: u32,
    pub bind: ButtonBinding,
}

impl From<MappingAutoRepeat> for BindMappingAutoRepeat {
    fn from(value: MappingAutoRepeat) -> Self {
        Self {
            position: value.position,
            note: value.note,
            target_key: value.target_key,
            interval: value.interval,
            bind: value.bind.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MappingAutoRepeat {
    pub position: Position,
    pub note: String,
    pub target_key: String,
    pub interval: u32,
    pub bind: ButtonBinding,
}

impl ValidateMappingConfig for MappingAutoRepeat {
    fn validate(&self) -> Result<(), String> {
        if self.target_key.is_empty() {
            return Err("Target key cannot be empty".to_string());
        }
        if self.interval == 0 {
            return Err("Interval must be a positive integer".to_string());
        }
        // Verify keyname parses correctly
        let _keycode = serde_json::from_str::<crate::scrcpy::constant::Keycode>(&format!(
            "\"{}\"",
            self.target_key
        ))
        .map_err(|_| format!("Invalid target key name '{}'", self.target_key))?;
        Ok(())
    }
}

pub fn handle_auto_repeat(
    entity_map: Res<ActionEntityMap>,
    events_q: Query<&ActionEvents>,
    active_mapping: Res<ActiveMappingConfig>,
    cs_tx_res: Res<ChannelSenderCS>,
) {
    if let Some(active_mapping) = &active_mapping.0 {
        for (action, mapping) in &active_mapping.mappings {
            if action.as_ref().starts_with("AutoRepeat") {
                let mapping = mapping.as_ref_autorepeat();
                if action.just_pulsed(&entity_map, &events_q) {
                    let target = &mapping.target_key;
                    if crate::mask::mapping::script_helper::is_repeating(target) {
                        crate::mask::mapping::script_helper::stop_repeat(target);
                    } else {
                        let _ = crate::mask::mapping::script_helper::start_repeat(
                            target.clone(),
                            mapping.interval as u64,
                            &cs_tx_res.0,
                        );
                    }
                }
            }
        }
    }
}
