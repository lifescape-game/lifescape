pub(super) mod placing_segment;
pub(super) mod ruler;

use std::{
    cmp::Ordering,
    f32::consts::PI,
    mem,
    ops::{Add, Sub},
};

use bevy::{ecs::system::QueryLens, prelude::*};
use bevy_replicon::prelude::*;
use itertools::{Itertools, MinMaxResult};
use serde::{Deserialize, Serialize};

use super::player_camera::CameraCaster;
use crate::core::GameState;
use placing_segment::PlacingSegmentPlugin;
use ruler::RulerPlugin;

pub(super) struct SegmentPlugin;

impl Plugin for SegmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RulerPlugin)
            .add_plugins(PlacingSegmentPlugin)
            .register_type::<Segment>()
            .replicate::<Segment>()
            .add_observer(cleanup_connections)
            .add_systems(
                PostUpdate,
                (update_transform, update_connections).run_if(in_state(GameState::InGame)),
            );
    }
}

fn update_transform(mut changed_segments: Query<(&mut Transform, &Segment), Changed<Segment>>) {
    for (mut transform, segment) in &mut changed_segments {
        transform.translation = Vec3::new(segment.start.x, 0.0, segment.start.y);
        transform.rotation = Quat::from_rotation_y(-segment.displacement().to_angle());
    }
}

/// Updates [`SegmentConnections`] between segments.
pub(super) fn update_connections(
    mut segments: Query<(Entity, &Visibility, &Segment, &mut SegmentConnections)>,
    children: Query<&Children>,
    changed_segments: Query<
        (Entity, &Parent, &Visibility, &Segment),
        (
            Or<(Changed<Segment>, Changed<Visibility>)>,
            With<SegmentConnections>,
        ),
    >,
) {
    for (segment_entity, parent, visibility, &segment) in &changed_segments {
        let mut taken_connections = disconnect_all(segment_entity, segments.transmute_lens());

        // If segment have zero length or hidden, exclude it from connections.
        if segment.start != segment.end && visibility != Visibility::Hidden {
            // Scan all segments from this lot for possible connections.
            let mut iter = segments.iter_many_mut(children.get(**parent).unwrap());
            while let Some((other_entity, visibility, &other_segment, mut other_connections)) =
                iter.fetch_next()
            {
                if visibility == Visibility::Hidden || segment_entity == other_entity {
                    // Don't connect to hidden segments or self.
                    continue;
                }

                let (from, to) = if segment.start == other_segment.start {
                    (PointKind::Start, PointKind::Start)
                } else if segment.start == other_segment.end {
                    (PointKind::Start, PointKind::End)
                } else if segment.end == other_segment.end {
                    (PointKind::End, PointKind::End)
                } else if segment.end == other_segment.start {
                    (PointKind::End, PointKind::Start)
                } else {
                    continue;
                };

                trace!(
                    "connecting `{from:?}` for `{segment_entity}` to `{to:?}` for `{other_entity}`"
                );
                taken_connections.get_mut(from).push(SegmentConnection {
                    entity: other_entity,
                    segment: other_segment,
                    kind: to,
                });
                other_connections.get_mut(to).push(SegmentConnection {
                    entity: segment_entity,
                    segment,
                    kind: from,
                });
            }
        }

        // Reinsert updated connections back.
        let (.., mut connections) = segments.get_mut(segment_entity).unwrap();
        *connections = taken_connections;
    }
}

fn cleanup_connections(
    trigger: Trigger<OnRemove, Segment>,
    mut segments: Query<&mut SegmentConnections>,
) {
    disconnect_all(trigger.entity(), segments.as_query_lens());
}

/// Removes all segment connections for the given entity.
///
/// During the process, the component will be taken from
/// the entity to avoid mutability issues.
///
/// Returns the taken component back for memory reuse.
fn disconnect_all(
    disconnect_entity: Entity,
    mut segments: QueryLens<&mut SegmentConnections>,
) -> SegmentConnections {
    debug!("removing connections for segment `{disconnect_entity}`");

    let mut segments = segments.query();
    let mut connections = segments.get_mut(disconnect_entity).unwrap();
    let mut taken_connections = mem::take(&mut *connections);

    for kind in [PointKind::Start, PointKind::End] {
        for connection in taken_connections.get_mut(kind).drain(..) {
            let mut other_connections = segments
                .get_mut(connection.entity)
                .expect("connected segment should also have connections");
            for other_kind in [PointKind::Start, PointKind::End] {
                let point_connections = other_connections.get_mut(other_kind);
                if let Some(index) = point_connections
                    .iter()
                    .position(|&SegmentConnection { entity, .. }| entity == disconnect_entity)
                {
                    point_connections.remove(index);
                    break; // A segment can connect to a single point of another segment, so stop at the first match.
                }
            }
        }
    }

    taken_connections
}

#[derive(Component, Clone, Copy, Default, Deserialize, Reflect, Serialize)]
#[reflect(Component)]
#[require(SegmentConnections)]
pub(crate) struct Segment {
    pub(super) start: Vec2,
    pub(super) end: Vec2,
}

impl Segment {
    /// Creates a new segment by endpoints.
    pub(super) fn new(start: Vec2, end: Vec2) -> Self {
        Self { start, end }
    }

    /// Creates a segment with the same start and end points.
    pub(super) fn splat(point: Vec2) -> Self {
        Self {
            start: point,
            end: point,
        }
    }

    pub(super) fn point(&self, kind: PointKind) -> Vec2 {
        match kind {
            PointKind::Start => self.start,
            PointKind::End => self.end,
        }
    }

    pub(super) fn set_point(&mut self, kind: PointKind, point: Vec2) {
        match kind {
            PointKind::Start => self.start = point,
            PointKind::End => self.end = point,
        }
    }

    /// Returns `true` if a point belongs to a segment.
    pub(super) fn contains(&self, point: Vec2) -> bool {
        let disp = self.displacement();
        let point_disp = point - self.start;
        if disp.perp_dot(point_disp).abs() > 0.1 {
            return false;
        }

        let dot = disp.dot(point_disp);
        if dot < 0.0 {
            return false;
        }

        dot <= disp.length_squared()
    }

    /// Returns the closest point on the segment to a point.
    pub(super) fn closest_point(&self, point: Vec2) -> Vec2 {
        let disp = self.displacement();
        let dir = disp.normalize();
        let point_dir = point - self.start;
        let dot = dir.dot(point_dir);

        if dot <= 0.0 {
            self.start
        } else if dot >= disp.length() {
            self.end
        } else {
            self.start + dir * dot
        }
    }

    /// Swaps end and start.
    pub(super) fn inverse(&self) -> Self {
        Self {
            start: self.end,
            end: self.start,
        }
    }

    pub(super) fn is_zero(&self) -> bool {
        self.start == self.end
    }

    /// Calculates displacement vector of the segment.
    pub(super) fn displacement(&self) -> Vec2 {
        self.end - self.start
    }

    /// Returns the intersection point of lines constructed from segments.
    pub(super) fn line_intersection(&self, other: Self) -> Option<Vec2> {
        let disp = self.displacement();
        let other_disp = other.displacement();

        let determinant = disp.perp_dot(other_disp);
        if determinant == 0.0 {
            // Lines are parallel or collinear.
            return None;
        }

        let t = (other.start - self.start).perp_dot(other_disp) / determinant;
        Some(self.start + t * disp)
    }

    /// Returns `true` if two segments intersect.
    pub(super) fn intersects(&self, other: Self) -> bool {
        let Some(intersection) = self.line_intersection(other) else {
            return false;
        };

        let distance1 = self.start.distance(intersection) + intersection.distance(self.end);
        let distance2 = other.start.distance(intersection) + intersection.distance(other.end);

        const TOLERANCE: f32 = 0.01;
        distance1 - self.len() < TOLERANCE && distance2 - other.len() < TOLERANCE
    }

    /// Calculates the left and right points for the `start` point of the segment based on `half_width`,
    /// considering intersections with other segments.
    ///
    /// `width_disp` is the width displacement vector of the segment.
    /// `half_width` is the half-width of the points for other segments.
    pub(super) fn offset_points(
        self,
        width_disp: Vec2,
        half_width: f32,
        connections: MinMaxResult<Segment>,
    ) -> (Vec2, Vec2) {
        match connections {
            MinMaxResult::NoElements => (self.start + width_disp, self.start - width_disp),
            MinMaxResult::OneElement(other_segment) => {
                let other_width = other_segment.displacement().perp().normalize() * half_width;
                let left = (self + width_disp)
                    .line_intersection(other_segment - other_width)
                    .unwrap_or_else(|| self.start + width_disp);
                let right = (self - width_disp)
                    .line_intersection(other_segment.inverse() + other_width)
                    .unwrap_or_else(|| self.start - width_disp);

                (left, right)
            }
            MinMaxResult::MinMax(min_segment, max_segment) => {
                let max_width = max_segment.displacement().perp().normalize() * half_width;
                let left = (self + width_disp)
                    .line_intersection(max_segment - max_width)
                    .unwrap_or_else(|| self.start + width_disp);
                let min_width = min_segment.displacement().perp().normalize() * half_width;
                let right = (self - width_disp)
                    .line_intersection(min_segment.inverse() + min_width)
                    .unwrap_or_else(|| self.start - width_disp);

                (left, right)
            }
        }
    }

    /// Returns distance from start to end.
    pub(super) fn len(&self) -> f32 {
        self.start.distance(self.end)
    }

    // Returns start and end points.
    pub(super) fn points(&self) -> [Vec2; 2] {
        [self.start, self.end]
    }
}

impl Add<Vec2> for Segment {
    type Output = Self;

    fn add(self, value: Vec2) -> Self {
        Segment {
            start: self.start + value,
            end: self.end + value,
        }
    }
}

impl Sub<Vec2> for Segment {
    type Output = Self;

    fn sub(self, value: Vec2) -> Self {
        Segment {
            start: self.start - value,
            end: self.end - value,
        }
    }
}

/// Dynamically updated component with precalculated connected entities for each segment point.
#[derive(Component, Default)]
pub(crate) struct SegmentConnections {
    start: Vec<SegmentConnection>,
    end: Vec<SegmentConnection>,
}

impl SegmentConnections {
    /// Returns closest left and right segments relative to the displacement vector.
    pub(super) fn side_segments(&self, point_kind: PointKind, disp: Vec2) -> MinMaxResult<Segment> {
        self.get_unified(point_kind).minmax_by_key(|segment| {
            let angle = segment.displacement().angle_to(disp);
            if angle < 0.0 {
                angle + 2.0 * PI
            } else {
                angle
            }
        })
    }

    /// Returns minimum angle for a point relative to the displacement vector.
    ///
    /// Angles compared by their absolute value.
    pub(super) fn min_angle(&self, point_kind: PointKind, disp: Vec2) -> Option<f32> {
        self.get_unified(point_kind)
            .map(|segment| segment.displacement().angle_to(disp))
            .min_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap_or(Ordering::Equal))
    }

    /// Returns iterator over segments that with unified direction based on point type.
    fn get_unified(&self, point_kind: PointKind) -> impl Iterator<Item = Segment> + '_ {
        self.get(point_kind)
            .iter()
            .map(move |connection| match (point_kind, connection.kind) {
                (PointKind::Start, PointKind::End) => connection.segment.inverse(),
                (PointKind::End, PointKind::Start) => connection.segment,
                (PointKind::Start, PointKind::Start) => connection.segment,
                (PointKind::End, PointKind::End) => connection.segment.inverse(),
            })
    }

    fn get(&self, kind: PointKind) -> &[SegmentConnection] {
        match kind {
            PointKind::Start => &self.start,
            PointKind::End => &self.end,
        }
    }

    fn get_mut(&mut self, kind: PointKind) -> &mut Vec<SegmentConnection> {
        match kind {
            PointKind::Start => &mut self.start,
            PointKind::End => &mut self.end,
        }
    }
}

pub(crate) struct SegmentConnection {
    entity: Entity,
    segment: Segment,
    kind: PointKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum PointKind {
    Start,
    End,
}

impl PointKind {
    pub(super) fn inverse(self) -> Self {
        match self {
            PointKind::Start => PointKind::End,
            PointKind::End => PointKind::Start,
        }
    }
}
