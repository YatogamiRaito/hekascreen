//! bevy_enhanced_input action types and MappingContext for the key-mapping overlay.
//!
//! This module replaces bevy_ineffable with bevy_enhanced_input 0.11.0 (Bevy 0.16).
//! 480 action unit structs (15 mapping types × 32 slots) are generated via seq_macro.

use bevy::prelude::*;
use bevy_enhanced_input::{action_map::ActionMap, prelude::*};
use seq_macro::seq;

use crate::mask::mapping::{
    binding::{ButtonBinding, DirectionBinding, MergedButton},
    config::{ActiveMappingConfig, MappingAction},
};

// ── 480 InputAction unit structs ────────────────────────────────────────────
//
// 15 types × 32 slots:
//   bool output (13): SingleTap, RepeatTap, MultipleTap, Swipe,
//                     MouseCastSpell, PadCastSpell, CancelCast, Observation,
//                     Fps, Fire, RawInput, Script, AutoRepeat
//   Vec2 output (2): DirectionPad, PadCastDirection
seq!(N in 1..=32 {
    #(
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct SingleTap~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct RepeatTap~N;

        /// JustPress: fires once on key press.
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct MultipleTap~N;

        /// JustPress: fires once on key press.
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct Swipe~N;

        /// Vec2 output (screen coords: right=+X, down=+Y).
        #[derive(Debug, InputAction)]
        #[input_action(output = Vec2)]
        pub struct DirectionPad~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct MouseCastSpell~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct PadCastSpell~N;

        /// Vec2 output for the directional pad portion of PadCastSpell.
        #[derive(Debug, InputAction)]
        #[input_action(output = Vec2)]
        pub struct PadCastDirection~N;

        /// JustPress: fires once on key press.
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct CancelCast~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct Observation~N;

        /// JustPress: fires once on key press (toggle FPS mode).
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct Fps~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct Fire~N;

        /// Release: fires once on key *release* (enters raw-input mode).
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct RawInput~N;

        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct Script~N;

        /// JustPress: fires once on key press (toggle auto-repeat).
        #[derive(Debug, InputAction)]
        #[input_action(output = bool)]
        pub struct AutoRepeat~N;
    )*
});

// ── Input context ────────────────────────────────────────────────────────────
#[derive(Debug, InputContext)]
pub struct MappingContext;

// ── Custom scroll-wheel conditions ──────────────────────────────────────────

/// Fires (`ActionState::Fired`) when the mouse wheel scrolls downward (Y < −0.1).
#[derive(Debug, Clone, Default)]
pub struct ScrollDownCondition;

impl InputCondition for ScrollDownCondition {
    fn evaluate(
        &mut self,
        _action_map: &ActionMap,
        _time: &Time<Virtual>,
        value: ActionValue,
    ) -> ActionState {
        if value.as_axis2d().y < -0.1 {
            ActionState::Fired
        } else {
            ActionState::None
        }
    }
}

/// Fires (`ActionState::Fired`) when the mouse wheel scrolls upward (Y > 0.1).
#[derive(Debug, Clone, Default)]
pub struct ScrollUpCondition;

impl InputCondition for ScrollUpCondition {
    fn evaluate(
        &mut self,
        _action_map: &ActionMap,
        _time: &Time<Virtual>,
        value: ActionValue,
    ) -> ActionState {
        if value.as_axis2d().y > 0.1 {
            ActionState::Fired
        } else {
            ActionState::None
        }
    }
}

// ── Binding helpers ──────────────────────────────────────────────────────────

enum BindCondition {
    /// No condition: action fires every frame the button is held.
    Continuous,
    /// [`JustPress`]: fires exactly once on key press.
    JustPress,
    /// [`Release`]: fires exactly once on key release.
    Release,
}

/// Convert a `MergedButton` to a bevy_enhanced_input `Input`.
/// Returns `None` for scroll variants (handled separately).
fn merged_to_input(btn: &MergedButton) -> Option<Input> {
    match btn {
        MergedButton::Mouse(mb) => Some(Input::from(*mb)),
        MergedButton::Keyboard(kc) => Some(Input::from(*kc)),
        MergedButton::GamePad(gb) => Some(Input::from(*gb)),
        MergedButton::ScrollDown | MergedButton::ScrollUp => None,
    }
}

/// Bind the first button of `binding` to `ab` using `cond`.
///
/// **Chord support is not yet implemented** — only the first key in a chord is
/// used. A full chord implementation can be added later.
fn bind_button(ab: &mut ActionBinding, binding: &ButtonBinding, cond: BindCondition) {
    let first = match binding.0.first() {
        Some(b) => b,
        None => return,
    };

    match first {
        MergedButton::ScrollDown => {
            let base = Input::mouse_wheel().with_conditions(ScrollDownCondition);
            match cond {
                BindCondition::Continuous => {
                    ab.to(base);
                }
                BindCondition::JustPress => {
                    ab.to(base.with_conditions(JustPress::default()));
                }
                BindCondition::Release => {
                    ab.to(base.with_conditions(Release::default()));
                }
            }
        }
        MergedButton::ScrollUp => {
            let base = Input::mouse_wheel().with_conditions(ScrollUpCondition);
            match cond {
                BindCondition::Continuous => {
                    ab.to(base);
                }
                BindCondition::JustPress => {
                    ab.to(base.with_conditions(JustPress::default()));
                }
                BindCondition::Release => {
                    ab.to(base.with_conditions(Release::default()));
                }
            }
        }
        other => {
            if let Some(input) = merged_to_input(&other) {
                let ib = InputBinding::new(input);
                match cond {
                    BindCondition::Continuous => {
                        ab.to(ib);
                    }
                    BindCondition::JustPress => {
                        ab.to(ib.with_conditions(JustPress::default()));
                    }
                    BindCondition::Release => {
                        ab.to(ib.with_conditions(Release::default()));
                    }
                }
            }
        }
    }
}

/// Bind a `DirectionBinding` to a Vec2 action.
///
/// Screen-coordinate convention preserved from bevy_ineffable:
///   right button → X = +1   (east)
///   left  button → X = −1   (west)
///   down  button → Y = +1   (screen-down = "north" in Cardinal math)
///   up    button → Y = −1   (screen-up   = "south" in Cardinal math)
fn bind_direction(ab: &mut ActionBinding, dir: &DirectionBinding) {
    match dir {
        DirectionBinding::Button {
            up,
            down,
            left,
            right,
        } => {
            // down → +Y (north): SwizzleAxis::YXZ moves Axis1D(1) → Vec2{0, 1}
            if let Some(input) = down.0.first().and_then(merged_to_input) {
                ab.to(InputBinding::new(input).with_modifiers(SwizzleAxis::YXZ));
            }
            // up → −Y (south): Negate then SwizzleAxis → Vec2{0, −1}
            if let Some(input) = up.0.first().and_then(merged_to_input) {
                ab.to(InputBinding::new(input).with_modifiers((Negate::all(), SwizzleAxis::YXZ)));
            }
            // right → +X (east): no modifier needed
            if let Some(input) = right.0.first().and_then(merged_to_input) {
                ab.to(InputBinding::new(input));
            }
            // left → −X (west): Negate all
            if let Some(input) = left.0.first().and_then(merged_to_input) {
                ab.to(InputBinding::new(input).with_modifiers(Negate::all()));
            }
        }
        DirectionBinding::JoyStick { x, y } => {
            // X axis stays on X; Y axis needs SwizzleAxis::YXZ to land on Y component
            ab.to(Input::from(*x));
            ab.to(Input::from(*y).with_modifiers(SwizzleAxis::YXZ));
        }
    }
}

// ── Runtime-dispatch helper methods on MappingAction ─────────────────────────
//
// These let handler systems call `action.just_activated(&actions)` etc. without
// knowing the concrete action type at compile time.
//
// State semantics:
//   just_activated  — continuous actions: ActionEvents::STARTED (None→Fired)
//   just_deactivated — continuous actions: ActionEvents::COMPLETED (Fired→None)
//   just_pulsed (JustPress) — ActionEvents::STARTED (None→Fired, once per press)
//   just_pulsed (Release)  — ActionEvents::FIRED   (Ongoing→Fired, once per release)
//   direction_2d — ActionValue::as_axis2d() of current value
seq!(N in 1..=32 {
    impl MappingAction {
        /// Returns `true` on the first frame a continuous action's key is pressed.
        pub fn just_activated(&self, actions: &Actions<MappingContext>) -> bool {
            match self {
                #(
                    MappingAction::SingleTap~N => actions
                        .get_action::<SingleTap~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::RepeatTap~N => actions
                        .get_action::<RepeatTap~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::MouseCastSpell~N => actions
                        .get_action::<MouseCastSpell~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::PadCastSpell~N => actions
                        .get_action::<PadCastSpell~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::Observation~N => actions
                        .get_action::<Observation~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::Fire~N => actions
                        .get_action::<Fire~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::Script~N => actions
                        .get_action::<Script~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                )*
                _ => false,
            }
        }

        /// Returns `true` on the first frame a continuous action's key is released.
        pub fn just_deactivated(&self, actions: &Actions<MappingContext>) -> bool {
            match self {
                #(
                    MappingAction::SingleTap~N => actions
                        .get_action::<SingleTap~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::RepeatTap~N => actions
                        .get_action::<RepeatTap~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::MouseCastSpell~N => actions
                        .get_action::<MouseCastSpell~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::PadCastSpell~N => actions
                        .get_action::<PadCastSpell~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::Observation~N => actions
                        .get_action::<Observation~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::Fire~N => actions
                        .get_action::<Fire~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                    MappingAction::Script~N => actions
                        .get_action::<Script~N>()
                        .map(|a| a.events().contains(ActionEvents::COMPLETED))
                        .unwrap_or(false),
                )*
                _ => false,
            }
        }

        /// Returns `true` on the trigger frame for a pulse-style action.
        ///
        /// - JustPress types (MultipleTap, Swipe, CancelCast, Fps, AutoRepeat):
        ///   fires on key press (STARTED, None→Fired).
        /// - Release type (RawInput):
        ///   fires on key release (FIRED, Ongoing→Fired).
        pub fn just_pulsed(&self, actions: &Actions<MappingContext>) -> bool {
            match self {
                #(
                    MappingAction::MultipleTap~N => actions
                        .get_action::<MultipleTap~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::Swipe~N => actions
                        .get_action::<Swipe~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::CancelCast~N => actions
                        .get_action::<CancelCast~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::Fps~N => actions
                        .get_action::<Fps~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    MappingAction::AutoRepeat~N => actions
                        .get_action::<AutoRepeat~N>()
                        .map(|a| a.events().contains(ActionEvents::STARTED))
                        .unwrap_or(false),
                    // RawInput uses Release condition: FIRED fires on key release
                    MappingAction::RawInput~N => actions
                        .get_action::<RawInput~N>()
                        .map(|a| a.events().contains(ActionEvents::FIRED))
                        .unwrap_or(false),
                )*
                _ => false,
            }
        }

        /// Returns the current 2D directional value (screen coords: right=+X, down=+Y).
        pub fn direction_2d(&self, actions: &Actions<MappingContext>) -> Vec2 {
            match self {
                #(
                    MappingAction::DirectionPad~N => actions
                        .get_action::<DirectionPad~N>()
                        .map(|a| a.value().as_axis2d())
                        .unwrap_or(Vec2::ZERO),
                    MappingAction::PadCastDirection~N => actions
                        .get_action::<PadCastDirection~N>()
                        .map(|a| a.value().as_axis2d())
                        .unwrap_or(Vec2::ZERO),
                )*
                _ => Vec2::ZERO,
            }
        }
    }
});

// ── Binding observer ─────────────────────────────────────────────────────────

// Observer that fires when `Actions<MappingContext>` is inserted or `RebuildBindings` triggers.
// Sets up all action bindings from the current `ActiveMappingConfig`.
seq!(N in 1..=32 {
    pub fn setup_bindings(
        trigger: Trigger<Binding<MappingContext>>,
        active_mapping: Res<ActiveMappingConfig>,
        mut actions: Query<&mut Actions<MappingContext>>,
    ) {
        let Ok(mut actions) = actions.get_mut(trigger.target()) else {
            return;
        };
        let Some(mapping) = &active_mapping.0 else {
            return;
        };

        for (action, mapping_type) in &mapping.mappings {
            match action {
                #(
                    MappingAction::SingleTap~N => bind_button(
                        actions.bind::<SingleTap~N>(),
                        &mapping_type.as_ref_singletap().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::RepeatTap~N => bind_button(
                        actions.bind::<RepeatTap~N>(),
                        &mapping_type.as_ref_repeattap().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::MultipleTap~N => bind_button(
                        actions.bind::<MultipleTap~N>(),
                        &mapping_type.as_ref_multipletap().bind,
                        BindCondition::JustPress,
                    ),
                    MappingAction::Swipe~N => bind_button(
                        actions.bind::<Swipe~N>(),
                        &mapping_type.as_ref_swipe().bind,
                        BindCondition::JustPress,
                    ),
                    MappingAction::DirectionPad~N => bind_direction(
                        actions.bind::<DirectionPad~N>(),
                        &mapping_type.as_ref_directionpad().bind,
                    ),
                    MappingAction::MouseCastSpell~N => bind_button(
                        actions.bind::<MouseCastSpell~N>(),
                        &mapping_type.as_ref_mousecastspell().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::PadCastSpell~N => {
                        let m = mapping_type.as_ref_padcastspell();
                        bind_button(
                            actions.bind::<PadCastSpell~N>(),
                            &m.bind,
                            BindCondition::Continuous,
                        );
                        bind_direction(
                            actions.bind::<PadCastDirection~N>(),
                            &m.pad_bind,
                        );
                    }
                    MappingAction::PadCastDirection~N => {
                        // Bound as part of PadCastSpell~N above; skip.
                    }
                    MappingAction::CancelCast~N => bind_button(
                        actions.bind::<CancelCast~N>(),
                        &mapping_type.as_ref_cancelcast().bind,
                        BindCondition::JustPress,
                    ),
                    MappingAction::Observation~N => bind_button(
                        actions.bind::<Observation~N>(),
                        &mapping_type.as_ref_observation().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::Fps~N => bind_button(
                        actions.bind::<Fps~N>(),
                        &mapping_type.as_ref_fps().bind,
                        BindCondition::JustPress,
                    ),
                    MappingAction::Fire~N => bind_button(
                        actions.bind::<Fire~N>(),
                        &mapping_type.as_ref_fire().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::RawInput~N => bind_button(
                        actions.bind::<RawInput~N>(),
                        &mapping_type.as_ref_rawinput().bind,
                        BindCondition::Release,
                    ),
                    MappingAction::Script~N => bind_button(
                        actions.bind::<Script~N>(),
                        &mapping_type.as_ref_script().bind,
                        BindCondition::Continuous,
                    ),
                    MappingAction::AutoRepeat~N => bind_button(
                        actions.bind::<AutoRepeat~N>(),
                        &mapping_type.as_ref_autorepeat().bind,
                        BindCondition::JustPress,
                    ),
                )*
            }
        }
    }
});
