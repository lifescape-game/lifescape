pub(crate) mod spawning_wall;

use std::{f32::consts::PI, mem};

use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, VertexAttributeValues},
        render_resource::PrimitiveTopology,
        view::NoFrustumCulling,
    },
};
use bevy_rapier3d::prelude::*;
use bevy_replicon::prelude::*;
use itertools::{Itertools, MinMaxResult};
use oxidized_navigation::NavMeshAffector;
use serde::{Deserialize, Serialize};

use super::{collision_groups::HarmoniaGroupsExt, game_world::WorldName};
use spawning_wall::{SpawningWall, SpawningWallPlugin};

pub(super) struct WallPlugin;

impl Plugin for WallPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SpawningWallPlugin)
            .register_type::<Wall>()
            .register_type::<WallObject>()
            .replicate::<Wall>()
            .add_mapped_client_event::<WallSpawn>(EventType::Unordered)
            .add_systems(
                PreUpdate,
                (Self::wall_init_system, Self::collision_init_system)
                    .after(ClientSet::Receive)
                    .run_if(resource_exists::<WorldName>()),
            )
            .add_systems(
                Update,
                (
                    Self::spawn_system.run_if(resource_exists::<RenetServer>()),
                    (
                        Self::cleanup_system,
                        Self::connections_update_system,
                        Self::mesh_update_system,
                    )
                        .chain(),
                )
                    .run_if(resource_exists::<WorldName>()),
            );
    }
}

impl WallPlugin {
    fn wall_init_system(
        mut commands: Commands,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut meshes: ResMut<Assets<Mesh>>,
        asset_server: Res<AssetServer>,
        spawned_walls: Query<Entity, Added<Wall>>,
    ) {
        for entity in &spawned_walls {
            let material = StandardMaterial {
                base_color_texture: Some(
                    asset_server.load("base/walls/brick/brick_base_color.png"),
                ),
                metallic_roughness_texture: Some(
                    asset_server.load("base/walls/brick/brick_roughnes_metalic.png"),
                ),
                normal_map_texture: Some(asset_server.load("base/walls/brick/brick_normal.png")),
                occlusion_texture: Some(asset_server.load("base/walls/brick/brick_occlusion.png")),
                perceptual_roughness: 0.0,
                reflectance: 0.0,
                ..Default::default()
            };
            let mesh = Mesh::new(PrimitiveTopology::TriangleList)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<Vec3>::new())
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<Vec2>::new())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<Vec3>::new())
                .with_indices(Some(Indices::U32(Vec::new())));

            commands.entity(entity).insert((
                Name::new("Walls"),
                WallConnections::default(),
                CollisionGroups::new(Group::WALL, Group::ALL),
                NoFrustumCulling,
                PbrBundle {
                    material: materials.add(material),
                    mesh: meshes.add(mesh),
                    ..Default::default()
                },
            ));
        }
    }

    fn collision_init_system(
        mut commands: Commands,
        walls: Query<Entity, (Added<Wall>, Without<SpawningWall>)>,
        mut confirmed_walls: RemovedComponents<SpawningWall>,
    ) {
        // Spawning walls shouldn't affect navigation, so we initialize
        // it on confirmation or for regular walls.
        for entity in walls.iter().chain(confirmed_walls.read()) {
            if let Some(mut entity) = commands.get_entity(entity) {
                entity.insert((Collider::default(), NavMeshAffector));
            }
        }
    }

    fn spawn_system(
        mut commands: Commands,
        mut entity_map: ResMut<ClientEntityMap>,
        mut spawn_events: EventReader<FromClient<WallSpawn>>,
    ) {
        for FromClient { client_id, event } in spawn_events.read().copied() {
            commands.entity(event.lot_entity).with_children(|parent| {
                // TODO: validate if wall can be spawned.
                let server_entity = parent.spawn(WallBundle::new(event.wall)).id();
                entity_map.insert(
                    client_id,
                    ClientMapping {
                        client_entity: event.wall_entity,
                        server_entity,
                    },
                );
            });
        }
    }

    fn connections_update_system(
        mut walls: Query<(Entity, &Wall, &mut WallConnections)>,
        children: Query<&Children>,
        changed_walls: Query<(Entity, &Parent, &Wall), Changed<Wall>>,
    ) {
        for (wall_entity, parent, &wall) in &changed_walls {
            // Take changed connections to avoid mutability issues.
            let mut connections =
                mem::take(&mut *walls.component_mut::<WallConnections>(wall_entity));

            // Cleanup old connections.
            for other_entity in connections.drain() {
                let mut other_connections = walls.component_mut::<WallConnections>(other_entity);
                if let Some((point, index)) = other_connections.position(wall_entity) {
                    other_connections.remove(point, index);
                }
            }

            // If wall have zero length, exclude it from connections.
            if wall.start != wall.end {
                // Scan all walls from this lot for possible connections.
                let children = children.get(**parent).unwrap();
                let mut iter = walls.iter_many_mut(children);
                while let Some((other_entity, &other_wall, mut other_connections)) = iter
                    .fetch_next()
                    .filter(|&(entity, ..)| entity != wall_entity)
                {
                    if wall.start == other_wall.start {
                        connections.start.push(WallConnection {
                            wall_entity: other_entity,
                            point_kind: PointKind::Start,
                            wall: other_wall,
                        });
                        other_connections.start.push(WallConnection {
                            wall_entity,
                            point_kind: PointKind::Start,
                            wall,
                        });
                    } else if wall.start == other_wall.end {
                        connections.start.push(WallConnection {
                            wall_entity: other_entity,
                            point_kind: PointKind::End,
                            wall: other_wall,
                        });
                        other_connections.end.push(WallConnection {
                            wall_entity,
                            point_kind: PointKind::Start,
                            wall,
                        });
                    } else if wall.end == other_wall.end {
                        connections.end.push(WallConnection {
                            wall_entity: other_entity,
                            point_kind: PointKind::End,
                            wall: other_wall,
                        });
                        other_connections.end.push(WallConnection {
                            wall_entity,
                            point_kind: PointKind::End,
                            wall,
                        });
                    } else if wall.end == other_wall.start {
                        connections.end.push(WallConnection {
                            wall_entity: other_entity,
                            point_kind: PointKind::Start,
                            wall: other_wall,
                        });
                        other_connections.start.push(WallConnection {
                            wall_entity,
                            point_kind: PointKind::End,
                            wall,
                        });
                    }
                }
            }

            // Reinsert updated connections back.
            *walls.component_mut::<WallConnections>(wall_entity) = connections;
        }
    }

    fn mesh_update_system(
        mut meshes: ResMut<Assets<Mesh>>,
        mut changed_walls: Query<
            (
                &Handle<Mesh>,
                &Wall,
                &WallConnections,
                Option<&mut Collider>,
            ),
            Changed<WallConnections>,
        >,
    ) {
        for (mesh_handle, &wall, connections, collider) in &mut changed_walls {
            let mesh = meshes
                .get_mut(mesh_handle)
                .expect("wall handles should be valid");

            // Remove attributes to avoid mutability issues.
            let Some(VertexAttributeValues::Float32x3(mut positions)) =
                mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("all walls should be initialized with position attribute");
            };
            let Some(VertexAttributeValues::Float32x2(mut uvs)) =
                mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0)
            else {
                panic!("all walls should be initialized with UV attribute");
            };
            let Some(VertexAttributeValues::Float32x3(mut normals)) =
                mesh.remove_attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("all walls should be initialized with normal attribute");
            };
            let Some(Indices::U32(indices)) = mesh.indices_mut() else {
                panic!("all walls should have U32 indices");
            };

            positions.clear();
            uvs.clear();
            normals.clear();
            indices.clear();

            generate_wall(
                wall,
                connections,
                &mut positions,
                &mut uvs,
                &mut normals,
                indices,
            );

            // Reinsert removed attributes back.
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

            if let Some(mut collider) = collider {
                *collider = Collider::from_bevy_mesh(mesh, &ComputedColliderShape::TriMesh)
                    .expect("wall mesh should be in compatible format");
            }
        }
    }

    fn cleanup_system(
        mut removed_walls: RemovedComponents<Wall>,
        mut walls: Query<&mut WallConnections>,
    ) {
        for entity in removed_walls.read() {
            for mut connections in &mut walls {
                if let Some((point, index)) = connections.position(entity) {
                    connections.remove(point, index);
                }
            }
        }
    }
}

const WIDTH: f32 = 0.15;
pub(super) const HALF_WIDTH: f32 = WIDTH / 2.0;

fn generate_wall(
    wall: Wall,
    connections: &WallConnections,
    positions: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    normals: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
) {
    if wall.start == wall.end {
        return;
    }

    const HEIGHT: f32 = 2.8;
    let dir = wall.dir();
    let width = wall.width();
    let rotation_mat = Mat2::from_angle(-dir.y.atan2(dir.x)); // TODO 0.13: Use `to_angle`.

    let start_walls = minmax_angles(dir, PointKind::Start, &connections.start);
    let (start_left, start_right) = offset_points(wall, start_walls, width);

    let end_walls = minmax_angles(-dir, PointKind::End, &connections.end);
    let (end_right, end_left) = offset_points(wall.inverse(), end_walls, -width);

    // Top
    positions.push([start_left.x, HEIGHT, start_left.y]);
    positions.push([start_right.x, HEIGHT, start_right.y]);
    positions.push([end_right.x, HEIGHT, end_right.y]);
    positions.push([end_left.x, HEIGHT, end_left.y]);
    uvs.push(position_to_uv(start_left, rotation_mat, wall.start));
    uvs.push(position_to_uv(start_right, rotation_mat, wall.start));
    uvs.push(position_to_uv(end_right, rotation_mat, wall.start));
    uvs.push(position_to_uv(end_left, rotation_mat, wall.start));
    normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
    indices.push(0);
    indices.push(3);
    indices.push(1);
    indices.push(1);
    indices.push(3);
    indices.push(2);

    // Right
    positions.push([start_right.x, 0.0, start_right.y]);
    positions.push([end_right.x, 0.0, end_right.y]);
    positions.push([end_right.x, HEIGHT, end_right.y]);
    positions.push([start_right.x, HEIGHT, start_right.y]);
    let start_right_uv = position_to_uv(start_right, rotation_mat, wall.start);
    let end_right_uv = position_to_uv(end_right, rotation_mat, wall.start);
    let start_right_top_uv = [start_right_uv[0], start_right_uv[1] + HEIGHT];
    let end_right_top_uv = [end_right_uv[0], end_right_uv[1] + HEIGHT];
    uvs.push(start_right_uv);
    uvs.push(end_right_uv);
    uvs.push(end_right_top_uv);
    uvs.push(start_right_top_uv);
    normals.extend_from_slice(&[[-width.x, 0.0, -width.y]; 4]);
    indices.push(4);
    indices.push(7);
    indices.push(5);
    indices.push(5);
    indices.push(7);
    indices.push(6);

    // Left
    positions.push([start_left.x, 0.0, start_left.y]);
    positions.push([end_left.x, 0.0, end_left.y]);
    positions.push([end_left.x, HEIGHT, end_left.y]);
    positions.push([start_left.x, HEIGHT, start_left.y]);
    let start_left_uv = position_to_uv(start_left, rotation_mat, wall.start);
    let end_left_uv = position_to_uv(end_left, rotation_mat, wall.start);
    let start_left_top_uv = [start_left_uv[0], start_left_uv[1] + HEIGHT];
    let end_left_top_uv = [end_left_uv[0], end_left_uv[1] + HEIGHT];
    uvs.push(start_left_uv);
    uvs.push(end_left_uv);
    uvs.push(end_left_top_uv);
    uvs.push(start_left_top_uv);
    normals.extend_from_slice(&[[width.x, 0.0, width.y]; 4]);
    indices.push(8);
    indices.push(9);
    indices.push(11);
    indices.push(9);
    indices.push(10);
    indices.push(11);

    match start_walls {
        MinMaxResult::OneElement(_) => (),
        MinMaxResult::NoElements => {
            // Front
            positions.push([start_left.x, 0.0, start_left.y]);
            positions.push([start_left.x, HEIGHT, start_left.y]);
            positions.push([start_right.x, HEIGHT, start_right.y]);
            positions.push([start_right.x, 0.0, start_right.y]);
            uvs.push([0.0, 0.0]);
            uvs.push([0.0, HEIGHT]);
            uvs.push([WIDTH, HEIGHT]);
            uvs.push([WIDTH, 0.0]);
            normals.extend_from_slice(&[[-dir.x, 0.0, -dir.y]; 4]);
            indices.push(12);
            indices.push(13);
            indices.push(15);
            indices.push(13);
            indices.push(14);
            indices.push(15);
        }
        MinMaxResult::MinMax(_, _) => {
            let start_index: u32 = positions
                .len()
                .try_into()
                .expect("start vertex index should fit u32");

            // Inside triangle to fill the gap between 3+ walls.
            positions.push([wall.start.x, HEIGHT, wall.start.y]);
            uvs.push(position_to_uv(wall.start, rotation_mat, wall.start));
            normals.push([0.0, 1.0, 0.0]);
            indices.push(1);
            indices.push(start_index);
            indices.push(0);
        }
    }

    match end_walls {
        MinMaxResult::OneElement(_) => (),
        MinMaxResult::NoElements => {
            let back_index: u32 = positions
                .len()
                .try_into()
                .expect("vertex back index should fit u32");

            // Back
            positions.push([end_left.x, 0.0, end_left.y]);
            positions.push([end_left.x, HEIGHT, end_left.y]);
            positions.push([end_right.x, HEIGHT, end_right.y]);
            positions.push([end_right.x, 0.0, end_right.y]);
            uvs.push([0.0, 0.0]);
            uvs.push([0.0, HEIGHT]);
            uvs.push([WIDTH, HEIGHT]);
            uvs.push([WIDTH, 0.0]);
            normals.extend_from_slice(&[[dir.x, 0.0, dir.y]; 4]);
            indices.push(back_index);
            indices.push(back_index + 3);
            indices.push(back_index + 1);
            indices.push(back_index + 1);
            indices.push(back_index + 3);
            indices.push(back_index + 2);
        }
        MinMaxResult::MinMax(_, _) => {
            let end_index: u32 = positions
                .len()
                .try_into()
                .expect("end vertex index should fit u32");

            // Inside triangle to fill the gap between 3+ walls.
            positions.push([wall.end.x, HEIGHT, wall.end.y]);
            uvs.push(position_to_uv(wall.end, rotation_mat, wall.start));
            normals.push([0.0, 1.0, 0.0]);
            indices.push(3);
            indices.push(end_index);
            indices.push(2);
        }
    }
}

/// Rotates a point using rotation matrix relatively to the specified origin point.
fn position_to_uv(position: Vec2, rotation_mat: Mat2, origin: Vec2) -> [f32; 2] {
    let translated_pos = position - origin;
    let rotated_point = rotation_mat * translated_pos;
    rotated_point.into()
}

/// Calculates the left and right wall points for the `start` point of the wall,
/// considering intersections with other walls.
fn offset_points(wall: Wall, min_max_walls: MinMaxResult<Wall>, width: Vec2) -> (Vec2, Vec2) {
    match min_max_walls {
        MinMaxResult::NoElements => (wall.start + width, wall.start - width),
        MinMaxResult::OneElement(other_wall) => {
            let other_width = other_wall.width();
            (
                wall_intersection(wall, width, other_wall, -other_width),
                wall_intersection(wall, -width, other_wall.inverse(), other_width),
            )
        }
        MinMaxResult::MinMax(min_wall, max_wall) => (
            wall_intersection(wall, width, max_wall, -max_wall.width()),
            wall_intersection(wall, -width, min_wall.inverse(), min_wall.width()),
        ),
    }
}

/// Returns the points with the maximum and minimum angle relative
/// to the direction vector.
fn minmax_angles(
    dir: Vec2,
    point_kind: PointKind,
    point_connections: &[WallConnection],
) -> MinMaxResult<Wall> {
    point_connections
        .iter()
        .map(|connection| {
            // Rotate points based on connection type.
            match (point_kind, connection.point_kind) {
                (PointKind::Start, PointKind::End) => connection.wall.inverse(),
                (PointKind::End, PointKind::Start) => connection.wall,
                (PointKind::Start, PointKind::Start) => connection.wall,
                (PointKind::End, PointKind::End) => connection.wall.inverse(),
            }
        })
        .minmax_by_key(|wall| {
            let angle = wall.dir().angle_between(dir);
            if angle < 0.0 {
                angle + 2.0 * PI
            } else {
                angle
            }
        })
}

/// Returns the intersection of the part of the wall that is `width` away
/// at the line constructed from `start` and `end` points with another part of the wall.
///
/// If the walls do not intersect, then returns a point that is a `width` away from the `start` point.
fn wall_intersection(wall: Wall, width: Vec2, other_wall: Wall, other_width: Vec2) -> Vec2 {
    let other_line = Line::with_offset(other_wall.start, other_wall.end, other_width);

    Line::with_offset(wall.start, wall.end, width)
        .intersection(other_line)
        .unwrap_or_else(|| wall.start + width)
}

#[derive(Clone, Copy, PartialEq)]
struct Line {
    a: f32,
    b: f32,
    c: f32,
}

impl Line {
    #[must_use]
    fn new(p1: Vec2, p2: Vec2) -> Self {
        let a = p2.y - p1.y;
        let b = p1.x - p2.x;
        let c = a * p1.x + b * p1.y;
        Self { a, b, c }
    }

    #[must_use]
    fn with_offset(p1: Vec2, p2: Vec2, offset: Vec2) -> Self {
        Self::new(p1 + offset, p2 + offset)
    }

    #[must_use]
    fn intersection(self, rhs: Self) -> Option<Vec2> {
        let det = self.a * rhs.b - rhs.a * self.b;
        if det == 0.0 {
            None
        } else {
            Some(Vec2 {
                x: (rhs.b * self.c - self.b * rhs.c) / det,
                y: (self.a * rhs.c - rhs.a * self.c) / det,
            })
        }
    }
}

/// Stores a handle for the lot line material.
#[derive(Resource)]
struct WallMaterial(Handle<StandardMaterial>);

impl FromWorld for WallMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let handle = materials.add(StandardMaterial::default());
        Self(handle)
    }
}

#[derive(Bundle)]
struct WallBundle {
    wall: Wall,
    parent_sync: ParentSync,
    replication: Replication,
}

impl WallBundle {
    fn new(wall: Wall) -> Self {
        Self {
            wall,
            parent_sync: Default::default(),
            replication: Replication,
        }
    }
}

#[derive(Clone, Component, Copy, Default, Deserialize, Reflect, Serialize)]
#[reflect(Component)]
pub(super) struct Wall {
    pub(super) start: Vec2,
    pub(super) end: Vec2,
}

impl Wall {
    fn inverse(&self) -> Self {
        Self {
            start: self.end,
            end: self.start,
        }
    }

    fn dir(&self) -> Vec2 {
        self.end - self.start
    }

    /// Calculates the wall thickness vector that faces to the left relative to the wall vector.
    fn width(&self) -> Vec2 {
        self.dir().perp().normalize() * HALF_WIDTH
    }
}

/// Dynamically updated component with precalculated connected entities for each wall point.
#[derive(Component, Default)]
struct WallConnections {
    start: Vec<WallConnection>,
    end: Vec<WallConnection>,
}

impl WallConnections {
    fn drain(&mut self) -> impl Iterator<Item = Entity> + '_ {
        self.start
            .drain(..)
            .chain(self.end.drain(..))
            .map(|WallConnection { wall_entity, .. }| wall_entity)
    }

    /// Returns position and point kind to which it connected for an entity.
    ///
    /// Used for [`Self::remove`] later.
    /// It's two different functions to avoid triggering change detection if there is no such entity.
    fn position(&self, entity: Entity) -> Option<(PointKind, usize)> {
        if let Some(index) = self
            .start
            .iter()
            .position(|&WallConnection { wall_entity, .. }| wall_entity == entity)
        {
            Some((PointKind::Start, index))
        } else {
            self.end
                .iter()
                .position(|&WallConnection { wall_entity, .. }| wall_entity == entity)
                .map(|index| (PointKind::End, index))
        }
    }

    /// Removes connection by its index from specific point.
    fn remove(&mut self, kind: PointKind, index: usize) {
        match kind {
            PointKind::Start => self.start.remove(index),
            PointKind::End => self.end.remove(index),
        };
    }
}

struct WallConnection {
    wall_entity: Entity,
    point_kind: PointKind,
    wall: Wall,
}

#[derive(Clone, Copy, Debug)]
enum PointKind {
    Start,
    End,
}

/// A component that marks that entity can be placed only on walls or inside them.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub(crate) enum WallObject {
    Fixture,
    Opening,
}

// To implement `Reflect`.
impl FromWorld for WallObject {
    fn from_world(_world: &mut World) -> Self {
        Self::Fixture
    }
}

/// Client event to request a wall creation.
#[derive(Clone, Copy, Deserialize, Event, Serialize)]
struct WallSpawn {
    lot_entity: Entity,
    wall_entity: Entity,
    wall: Wall,
}

impl MapNetworkEntities for WallSpawn {
    fn map_entities<T: Mapper>(&mut self, mapper: &mut T) {
        self.lot_entity = mapper.map(self.lot_entity);
    }
}
