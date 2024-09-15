use bevy::prelude::*;

use super::TaskState;
use crate::game_world::actor::animation_state::AnimationState;

pub(super) struct LinkedTaskPlugin;

impl Plugin for LinkedTaskPlugin {
    fn build(&self, app: &mut App) {
        app.observe(Self::finish)
            .add_systems(PostUpdate, (Self::insert_links, Self::sync_states));
    }
}

impl LinkedTaskPlugin {
    fn insert_links(
        mut commands: Commands,
        tasks: Query<(Entity, &LinkedTask), Added<LinkedTask>>,
    ) {
        for (entity, linked_task) in &tasks {
            commands.entity(linked_task.0).insert(LinkedTask(entity));
        }
    }

    fn sync_states(
        mut query_cache: Local<Vec<(Entity, TaskState)>>,
        mut tasks: Query<(&mut TaskState, &LinkedTask)>,
    ) {
        for (task_state, linked_task) in &mut tasks {
            if task_state.is_changed() {
                query_cache.push((linked_task.0, *task_state));
            }
        }

        for &(linked_entity, task_state) in &query_cache {
            let (mut linked_state, _) = tasks
                .get_mut(linked_entity)
                .expect("linked task should have the same components");
            if *linked_state != task_state {
                *linked_state = task_state;
            }
        }

        query_cache.clear();
    }

    fn finish(
        trigger: Trigger<OnRemove, LinkedTask>,
        mut commands: Commands,
        tasks: Query<(Entity, &Parent, &TaskState)>,
        mut actors: Query<&mut AnimationState>,
    ) {
        if let Ok((entity, parent, &task_state)) = tasks.get(trigger.entity()) {
            if task_state == TaskState::Active {
                let mut animation_state = actors
                    .get_mut(**parent)
                    .expect("actor should have animator");
                animation_state.stop_montage();

                commands.entity(entity).despawn();
            }
        }
    }
}

/// Stores entity of another tasks and syncs [`TaskState`] between them.
///
/// Same component will be automatically added to the linked task too.
/// After insertion current task state changes to the linked state.
/// Current task will be considered finished after linked task despawn.
#[derive(Clone, Component, Copy)]
pub(super) struct LinkedTask(pub(super) Entity);
