//! bevy_enhanced_input action types and MappingContext for the key-mapping overlay.
//!
//! Updated for bevy_enhanced_input 0.19 (Bevy 0.17):
//!   - `MappingContext` is now `#[derive(Component)]` (plain component, not a special context)
//!   - Action structs use `#[action_output(T)]` (not `#[input_action(output = T)]`)
//!   - Conditions and modifiers are ECS Components on action/binding entities
//!   - Pull-style dispatch via `ActionEntityMap` resource + `Query<&ActionEvents>` / `Query<&ActionValue>`
//!   - `on_rebuild_input_bindings` observer rebuilds all action/binding entities on config reload

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
// Disambiguate from any bevy::prelude types of the same name:
use bevy_enhanced_input::prelude::{Press, Release};
use seq_macro::seq;

use crate::mask::mapping::{
    binding::{ButtonBinding, DirectionBinding, MergedButton},
    config::{ActiveMappingConfig, MappingAction},
};

// ── 480 InputAction unit structs ─────────────────────────────────────────────
//
// 15 types × 32 slots:
//   bool output (13): SingleTap, RepeatTap, MultipleTap, Swipe,
//                     MouseCastSpell, PadCastSpell, CancelCast, Observation,
//                     Fps, Fire, RawInput, Script, AutoRepeat
//   Vec2 output (2): DirectionPad, PadCastDirection
seq!(N in 1..=32 {
    #(
        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct SingleTap~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct RepeatTap~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct MultipleTap~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct Swipe~N;

        #[derive(Debug, InputAction)]
        #[action_output(Vec2)]
        pub struct DirectionPad~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct MouseCastSpell~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct PadCastSpell~N;

        #[derive(Debug, InputAction)]
        #[action_output(Vec2)]
        pub struct PadCastDirection~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct CancelCast~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct Observation~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct Fps~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct Fire~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct RawInput~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct Script~N;

        #[derive(Debug, InputAction)]
        #[action_output(bool)]
        pub struct AutoRepeat~N;
    )*
});

// ── Input context ─────────────────────────────────────────────────────────────
/// Marker component for the single global input-mapping context entity.
/// Registered as an input context in `MappingPlugins`.
#[derive(Component, Debug)]
pub struct MappingContext;

// ── Resources & events ────────────────────────────────────────────────────────

/// Maps each `MappingAction` variant to the entity of its spawned action.
///
/// Populated (and cleared) by `on_rebuild_input_bindings` whenever the active
/// mapping config changes.  Handler systems query this map together with
/// `Query<&ActionEvents>` / `Query<&ActionValue>` for pull-style dispatch.
#[derive(Resource, Default)]
pub struct ActionEntityMap(pub HashMap<MappingAction, Entity>);

/// Send this event (via `commands.trigger(RebuildInputBindings)`) to despawn the
/// old action/binding entities and respawn them from the current
/// `ActiveMappingConfig`.
#[derive(Event, Debug)]
pub struct RebuildInputBindings;

// ── Custom scroll-wheel conditions ───────────────────────────────────────────

/// Fires when the mouse wheel scrolls **downward** (Y < −0.1).
///
/// Must be registered with `app.add_input_condition::<ScrollDownCondition>()`.
#[derive(Debug, Clone, Default, Component)]
pub struct ScrollDownCondition;

impl InputCondition for ScrollDownCondition {
    fn evaluate(
        &mut self,
        _actions: &ActionsQuery,
        _time: &ContextTime,
        value: ActionValue,
    ) -> ActionState {
        if value.as_axis2d().y < -0.1 {
            ActionState::Fired
        } else {
            ActionState::None
        }
    }
}

/// Fires when the mouse wheel scrolls **upward** (Y > 0.1).
///
/// Must be registered with `app.add_input_condition::<ScrollUpCondition>()`.
#[derive(Debug, Clone, Default, Component)]
pub struct ScrollUpCondition;

impl InputCondition for ScrollUpCondition {
    fn evaluate(
        &mut self,
        _actions: &ActionsQuery,
        _time: &ContextTime,
        value: ActionValue,
    ) -> ActionState {
        if value.as_axis2d().y > 0.1 {
            ActionState::Fired
        } else {
            ActionState::None
        }
    }
}

// ── Internal binding helpers ─────────────────────────────────────────────────

/// Convert a `MergedButton` to a bevy_enhanced_input `Binding`.
/// Returns `None` for scroll variants (handled separately).
fn merged_to_binding(btn: &MergedButton) -> Option<Binding> {
    match btn {
        MergedButton::Mouse(mb) => Some(Binding::from(*mb)),
        MergedButton::Keyboard(kc) => Some(Binding::from(*kc)),
        MergedButton::GamePad(gb) => Some(Binding::from(*gb)),
        MergedButton::ScrollDown | MergedButton::ScrollUp => None,
    }
}

/// Spawn a binding entity for the first button in `binding`, associated with
/// `action_entity`.  Scroll buttons get `ScrollDownCondition`/`ScrollUpCondition`
/// as a component on the binding entity.
fn spawn_button_bindings(commands: &mut Commands, action_entity: Entity, binding: &ButtonBinding) {
    let first = match binding.0.first() {
        Some(b) => b,
        None => return,
    };
    match first {
        MergedButton::ScrollDown => {
            commands.spawn((
                Binding::mouse_wheel(),
                ScrollDownCondition,
                BindingOf(action_entity),
            ));
        }
        MergedButton::ScrollUp => {
            commands.spawn((
                Binding::mouse_wheel(),
                ScrollUpCondition,
                BindingOf(action_entity),
            ));
        }
        btn => {
            if let Some(b) = merged_to_binding(btn) {
                commands.spawn((b, BindingOf(action_entity)));
            }
        }
    }
}

/// Spawn binding entities for a `DirectionBinding`, associated with
/// `action_entity`.
///
/// Screen-coordinate convention (preserved from the old bevy_ineffable setup):
///   right → X = +1  (east)
///   left  → X = −1  (west)
///   down  → Y = +1  (screen-down; SwizzleAxis::YXZ maps Axis1D to Vec2.y)
///   up    → Y = −1  (screen-up;   Negate + SwizzleAxis::YXZ)
fn spawn_direction_bindings(
    commands: &mut Commands,
    action_entity: Entity,
    dir: &DirectionBinding,
) {
    match dir {
        DirectionBinding::Button { up, down, left, right } => {
            // down → +Y
            if let Some(b) = down.0.first().and_then(|b| merged_to_binding(b)) {
                commands.spawn((b, SwizzleAxis::YXZ, BindingOf(action_entity)));
            }
            // up → −Y
            if let Some(b) = up.0.first().and_then(|b| merged_to_binding(b)) {
                commands.spawn((b, Negate::all(), SwizzleAxis::YXZ, BindingOf(action_entity)));
            }
            // right → +X (no modifier)
            if let Some(b) = right.0.first().and_then(|b| merged_to_binding(b)) {
                commands.spawn((b, BindingOf(action_entity)));
            }
            // left → −X
            if let Some(b) = left.0.first().and_then(|b| merged_to_binding(b)) {
                commands.spawn((b, Negate::all(), BindingOf(action_entity)));
            }
        }
        DirectionBinding::JoyStick { x, y } => {
            // X axis stays on X
            commands.spawn((Binding::from(*x), BindingOf(action_entity)));
            // Y axis: SwizzleAxis::YXZ maps Axis1D value → Vec2.y
            commands.spawn((Binding::from(*y), SwizzleAxis::YXZ, BindingOf(action_entity)));
        }
    }
}

// ── Pull-style dispatch helpers on MappingAction ─────────────────────────────
//
// These are called from handler systems:
//   `action.just_activated(&entity_map, &events_q)`
//   `action.just_deactivated(&entity_map, &events_q)`
//   `action.just_pulsed(&entity_map, &events_q)`
//   `action.direction_2d(&entity_map, &value_q)`

impl MappingAction {
    /// Returns `true` on the first frame a continuous action's key is pressed.
    /// Checks `ActionEvents::STARTED` (set on None → Fired / None → Ongoing transitions).
    #[inline]
    pub fn just_activated(
        &self,
        entity_map: &ActionEntityMap,
        events_q: &Query<&ActionEvents>,
    ) -> bool {
        entity_map
            .0
            .get(self)
            .and_then(|&e| events_q.get(e).ok())
            .map(|ev| ev.contains(ActionEvents::STARTED))
            .unwrap_or(false)
    }

    /// Returns `true` on the first frame a continuous action's key is released.
    /// Checks `ActionEvents::COMPLETED` (set on Fired → None transition).
    #[inline]
    pub fn just_deactivated(
        &self,
        entity_map: &ActionEntityMap,
        events_q: &Query<&ActionEvents>,
    ) -> bool {
        entity_map
            .0
            .get(self)
            .and_then(|&e| events_q.get(e).ok())
            .map(|ev| ev.contains(ActionEvents::COMPLETED))
            .unwrap_or(false)
    }

    /// Returns the current 2-D directional value (screen coords: right=+X, down=+Y).
    #[inline]
    pub fn direction_2d(
        &self,
        entity_map: &ActionEntityMap,
        value_q: &Query<&ActionValue>,
    ) -> Vec2 {
        entity_map
            .0
            .get(self)
            .and_then(|&e| value_q.get(e).ok())
            .map(|v| v.as_axis2d())
            .unwrap_or(Vec2::ZERO)
    }
}

// `just_pulsed` needs per-variant knowledge of the condition used:
//   - `RawInput~N` uses a `Release` condition → fires on key *release*
//     → `Ongoing → Fired` state transition → `ActionEvents::FIRED`
//   - All other pulse actions (MultipleTap, Swipe, CancelCast, Fps, AutoRepeat)
//     use a `Press` condition → fires on key *press*
//     → `None → Fired` state transition → `ActionEvents::STARTED`
seq!(N in 1..=32 {
    impl MappingAction {
        /// Returns `true` on the single trigger frame for a pulse-style action.
        pub fn just_pulsed(
            &self,
            entity_map: &ActionEntityMap,
            events_q: &Query<&ActionEvents>,
        ) -> bool {
            let flag = match self {
                #( MappingAction::RawInput~N => ActionEvents::FIRED, )*
                _ => ActionEvents::STARTED,
            };
            entity_map
                .0
                .get(self)
                .and_then(|&e| events_q.get(e).ok())
                .map(|ev| ev.contains(flag))
                .unwrap_or(false)
        }
    }
});

// ── Binding observer ──────────────────────────────────────────────────────────
//
// Triggered by `commands.trigger(RebuildInputBindings)`.
// Despawns old action/binding entities via `despawn_related`, then re-spawns
// them from the current `ActiveMappingConfig`, populating `ActionEntityMap`.
seq!(N in 1..=32 {
    pub fn on_rebuild_input_bindings(
        _trigger: On<RebuildInputBindings>,
        mut commands: Commands,
        context_q: Query<Entity, With<MappingContext>>,
        active_mapping: Res<ActiveMappingConfig>,
        mut entity_map: ResMut<ActionEntityMap>,
    ) {
        let Ok(context) = context_q.single() else { return; };

        // Despawn all action entities (and their linked binding entities) from the
        // previous config.  This is a no-op when called for the first time.
        commands.entity(context).despawn_related::<Actions<MappingContext>>();
        entity_map.0.clear();

        let Some(mapping) = &active_mapping.0 else { return; };

        for (action, mapping_type) in &mapping.mappings {
            match action {
                #(
                    MappingAction::SingleTap~N => {
                        let e = commands.spawn((
                            Action::<SingleTap~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_singletap().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::RepeatTap~N => {
                        let e = commands.spawn((
                            Action::<RepeatTap~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_repeattap().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::MultipleTap~N => {
                        let e = commands.spawn((
                            Action::<MultipleTap~N>::new(),
                            Press::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_multipletap().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::Swipe~N => {
                        let e = commands.spawn((
                            Action::<Swipe~N>::new(),
                            Press::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_swipe().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::DirectionPad~N => {
                        let e = commands.spawn((
                            Action::<DirectionPad~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_direction_bindings(&mut commands, e, &mapping_type.as_ref_directionpad().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::MouseCastSpell~N => {
                        let e = commands.spawn((
                            Action::<MouseCastSpell~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_mousecastspell().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::PadCastSpell~N => {
                        let m = mapping_type.as_ref_padcastspell();

                        // Spawn the trigger-button action for the spell.
                        let spell_e = commands.spawn((
                            Action::<PadCastSpell~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, spell_e, &m.bind);
                        entity_map.0.insert(MappingAction::PadCastSpell~N, spell_e);

                        // Spawn the directional action paired with this spell (always same slot N).
                        let dir_e = commands.spawn((
                            Action::<PadCastDirection~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_direction_bindings(&mut commands, dir_e, &m.pad_bind);
                        entity_map.0.insert(MappingAction::PadCastDirection~N, dir_e);
                    },
                    MappingAction::PadCastDirection~N => {
                        // Always spawned as part of PadCastSpell above; skip.
                    },
                    MappingAction::CancelCast~N => {
                        let e = commands.spawn((
                            Action::<CancelCast~N>::new(),
                            Press::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_cancelcast().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::Observation~N => {
                        let e = commands.spawn((
                            Action::<Observation~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_observation().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::Fps~N => {
                        let e = commands.spawn((
                            Action::<Fps~N>::new(),
                            Press::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_fps().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::Fire~N => {
                        let e = commands.spawn((
                            Action::<Fire~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_fire().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::RawInput~N => {
                        let e = commands.spawn((
                            Action::<RawInput~N>::new(),
                            Release::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_rawinput().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::Script~N => {
                        let e = commands.spawn((
                            Action::<Script~N>::new(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_script().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                    MappingAction::AutoRepeat~N => {
                        let e = commands.spawn((
                            Action::<AutoRepeat~N>::new(),
                            Press::default(),
                            ActionOf::<MappingContext>::new(context),
                        )).id();
                        spawn_button_bindings(&mut commands, e, &mapping_type.as_ref_autorepeat().bind);
                        entity_map.0.insert(action.clone(), e);
                    },
                )*
            }
        }
    }
});
